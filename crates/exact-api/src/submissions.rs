// POST /api/submissions: kick off a build for a user-supplied snippet.
// GET  /api/submissions/:id: poll the status + result.
//
// For step 5 the lifecycle is: queued → building → done|failed. The runner
// stage that adds → running → done lands in step 6.

use anyhow::Context;
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use exact_proto::Board;
use exact_proto::b64_bytes_opt;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::build::{self, BuildOutcome, IoSpec};
use crate::db;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/submissions", post(create))
        .route("/api/submissions/{id}", get(detail))
}

#[derive(Deserialize)]
struct CreateBody {
    /// Optional: omit to submit raw against the board with no problem
    /// (playground). For step 5 we require it — io_spec drives the glue.
    problem_id: Option<String>,
    source_code: String,
    board: Board,
}

fn board_str(b: Board) -> &'static str {
    match b {
        Board::Lm3s6965evb => "lm3s6965evb",
        Board::Lpc1768 => "lpc1768",
        Board::Stm32f429zi => "stm32f429zi",
    }
}

fn err(s: StatusCode, m: impl Into<String>) -> Response {
    (s, m.into()).into_response()
}

async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateBody>,
) -> Response {
    let problem_id = match body.problem_id {
        Some(ref id) => id.clone(),
        None => {
            return err(
                StatusCode::BAD_REQUEST,
                "problem_id is required (playground submissions land in a later step)",
            );
        }
    };
    let problem = match db::get_problem(&state.db, &problem_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "problem not found"),
        Err(e) => {
            error!(error=%e, "load problem for submission");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    let board_s = board_str(body.board);
    if !problem.allowed_boards.iter().any(|b| b == board_s) {
        return err(
            StatusCode::BAD_REQUEST,
            format!("board {board_s} is not allowed for this problem"),
        );
    }

    let spec: IoSpec = match serde_json::from_value(problem.io_spec.clone()) {
        Ok(s) => s,
        Err(e) => {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("problem io_spec is not a valid IoSpec: {e}"),
            );
        }
    };

    let id = Uuid::new_v4();
    let submission = match db::create_submission(
        &state.db,
        id,
        user.id,
        Some(&problem.id),
        &body.source_code,
        board_s,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            error!(error=%e, "create submission");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // Drive the build off the request thread so the client gets the id
    // back immediately and polls /api/submissions/:id for status.
    let task_state = state.clone();
    let task_source = body.source_code.clone();
    let task_board = body.board;
    let task_timeout = problem.default_timeout_ms.max(1) as u32;
    tokio::spawn(async move {
        if let Err(e) = run_build(task_state, id, task_source, task_board, spec, task_timeout).await
        {
            error!(error=?e, %id, "build task crashed");
        }
    });

    (StatusCode::ACCEPTED, Json(submission)).into_response()
}

async fn run_build(
    state: AppState,
    id: Uuid,
    source: String,
    board: Board,
    spec: IoSpec,
    pack_timeout_ms: u32,
) -> anyhow::Result<()> {
    db::set_submission_status(&state.db, id, "building")
        .await
        .context("status=building")?;
    info!(%id, "build starting");

    let outcome = build::build(
        &source,
        board,
        &spec,
        &state.config.userlib_path,
        pack_timeout_ms,
    )
    .await;
    match outcome {
        Ok(BuildOutcome::Success { bin }) => {
            info!(%id, bytes = bin.len(), "build done");
            db::set_submission_built(&state.db, id, &bin)
                .await
                .context("status=ready")?;
            // Try to dispatch to a runner immediately. If no runner is
            // online for this board, the submission stays at 'ready' and
            // the next runner to Hello will drain it.
            crate::dispatcher::dispatch_or_queue(&state, id, board).await;
        }
        Ok(BuildOutcome::Failure { log }) => {
            info!(%id, log_len = log.len(), "build failed");
            db::set_submission_failed(&state.db, id, &log)
                .await
                .context("status=failed")?;
        }
        Err(e) => {
            // Infra failure (couldn't spawn cargo, missing userlib, etc).
            // Surface as a failed build with the chain rendered.
            let msg = format!("build infra error: {e:#}");
            error!(%id, "{msg}");
            db::set_submission_failed(&state.db, id, &msg)
                .await
                .context("status=failed (infra)")?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct CaseResultView {
    case_ord: i32,
    status: String,
    exit_code: Option<i32>,
    cycles: Option<i64>,
    #[serde(with = "b64_bytes_opt")]
    output: Option<Vec<u8>>,
    passed: Option<bool>,
    synthetic: bool,
}

#[derive(Serialize)]
struct SubmissionDetail {
    #[serde(flatten)]
    submission: db::Submission,
    case_results: Vec<CaseResultView>,
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AuthUser(user): AuthUser,
) -> Response {
    let sub = match db::get_submission(&state.db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => return err(StatusCode::NOT_FOUND, "submission not found"),
        Err(e) => {
            error!(error=%e, "get submission");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    if sub.user_id != user.id && !user.is_admin {
        return err(StatusCode::NOT_FOUND, "submission not found");
    }
    let case_results = match db::list_case_results(&state.db, id).await {
        Ok(r) => r,
        Err(e) => {
            error!(error=%e, %id, "list case_results");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    let cases_view = case_results
        .into_iter()
        .map(|r| CaseResultView {
            case_ord: r.case_ord,
            status: r.status,
            exit_code: r.exit_code,
            cycles: r.cycles,
            output: r.output,
            passed: r.passed,
            synthetic: r.synthetic,
        })
        .collect();
    Json(SubmissionDetail {
        submission: sub,
        case_results: cases_view,
    })
    .into_response()
}
