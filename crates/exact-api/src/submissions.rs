// POST /api/submissions: kick off a build for a user-supplied snippet.
// GET  /api/submissions/:id: poll the status + result.
//
// For step 5 the lifecycle is: queued → building → done|failed. The runner
// stage that adds → running → done lands in step 6.

use anyhow::Context;
use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use exact_proto::Board;
use exact_proto::b64_bytes_opt;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::build::{self, BuildOutcome, IoSpec};
use crate::db;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/submissions", post(create).get(history))
        .route("/api/submissions/{id}", get(detail))
        .route("/api/submissions/{id}/events", get(events_stream))
        .route("/api/me/best", get(my_best))
}

/// GET /api/me/best — viewer's best (problem, board) entries across the
/// catalog, with global rank. Frontend uses this to badge the problem list.
async fn my_best(State(state): State<AppState>, AuthUser(user): AuthUser) -> Response {
    match db::user_best_per_problem(&state.db, user.id).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            error!(error=%e, user=user.id, "user_best_per_problem");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
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
    state.events.publish(
        id,
        crate::events::SubmissionEvent::Status {
            status: "building".into(),
        },
    );
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
            state.events.publish(
                id,
                crate::events::SubmissionEvent::Status {
                    status: "ready".into(),
                },
            );
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
            state
                .events
                .publish(id, crate::events::SubmissionEvent::Failed { log });
        }
        Err(e) => {
            // Infra failure (couldn't spawn cargo, missing userlib, etc).
            // Surface as a failed build with the chain rendered.
            let msg = format!("build infra error: {e:#}");
            error!(%id, "{msg}");
            db::set_submission_failed(&state.db, id, &msg)
                .await
                .context("status=failed (infra)")?;
            state
                .events
                .publish(id, crate::events::SubmissionEvent::Failed { log: msg });
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

/// Build the full detail (submission + case_results) without any visibility
/// check. Callers are responsible for the gate.
async fn load_detail_raw(
    state: &AppState,
    sub: db::Submission,
) -> Result<SubmissionDetail, anyhow::Error> {
    let case_results = db::list_case_results(&state.db, sub.id).await?;
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
    Ok(SubmissionDetail {
        submission: sub,
        case_results: cases_view,
    })
}

/// Returns true if `viewer` may see this submission. Owner/admin always can.
/// Otherwise the rule mirrors problems::check_read against the submission's
/// problem (public always; shared requires matching `?t=`; private no).
/// Playground submissions (no problem_id) are owner-only.
async fn submission_visible(
    state: &AppState,
    sub: &db::Submission,
    viewer: Option<&AuthUser>,
    share_token: Option<&str>,
) -> Result<bool, anyhow::Error> {
    let is_admin = viewer.map(|AuthUser(u)| u.is_admin).unwrap_or(false);
    let is_owner = viewer.map(|AuthUser(u)| u.id == sub.user_id).unwrap_or(false);
    if is_admin || is_owner {
        return Ok(true);
    }
    let Some(problem_id) = sub.problem_id.as_deref() else {
        return Ok(false); // playground submissions are owner-only.
    };
    let Some(problem) = db::get_problem(&state.db, problem_id).await? else {
        return Ok(false);
    };
    Ok(match problem.visibility.as_str() {
        "public" => true,
        "shared" => problem.share_token.as_deref() == share_token,
        _ => false,
    })
}

#[derive(Deserialize)]
struct DetailQuery {
    #[serde(default)]
    t: Option<String>,
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<DetailQuery>,
    viewer: Option<AuthUser>,
) -> Response {
    let sub = match db::get_submission(&state.db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => return err(StatusCode::NOT_FOUND, "submission not found"),
        Err(e) => {
            error!(error=%e, %id, "get submission");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    let visible = match submission_visible(&state, &sub, viewer.as_ref(), q.t.as_deref()).await {
        Ok(v) => v,
        Err(e) => {
            error!(error=%e, %id, "check submission visibility");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    if !visible {
        return err(StatusCode::NOT_FOUND, "submission not found");
    }
    match load_detail_raw(&state, sub).await {
        Ok(d) => Json(d).into_response(),
        Err(e) => {
            error!(error=%e, %id, "load submission detail");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

#[derive(Deserialize)]
struct HistoryQuery {
    problem_id: String,
    board: String,
    #[serde(default)]
    limit: Option<i32>,
}

#[derive(Serialize)]
struct HistoryRow {
    id: Uuid,
    board: String,
    status: String,
    total_cycles: Option<i64>,
    passed: Option<i32>,
    total_cases: Option<i32>,
    created_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// GET /api/submissions?problem_id=&board=&limit= — viewer's own submission
/// history for one (problem, board). Always scoped to the authenticated user;
/// admins viewing other users' history would use a different (future) admin
/// endpoint.
async fn history(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<HistoryQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let rows = match db::list_user_submissions(&state.db, user.id, &q.problem_id, &q.board, limit)
        .await
    {
        Ok(rs) => rs,
        Err(e) => {
            error!(error=%e, user=user.id, problem=%q.problem_id, "list user submissions");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    let view: Vec<HistoryRow> = rows
        .into_iter()
        .map(|s| HistoryRow {
            id: s.id,
            board: s.board,
            status: s.status,
            total_cycles: s.total_cycles,
            passed: s.passed,
            total_cases: s.total_cases,
            created_at: s.created_at,
            finished_at: s.finished_at,
        })
        .collect();
    Json(view).into_response()
}

/// SSE: emits a one-shot `snapshot` event with the full detail on connect,
/// then per-delta events (`status`, `case_result`, `finalized`, `failed`)
/// as the submission progresses. Each event variant maps to a small JSON
/// payload — clients merge incrementally rather than refetching.
async fn events_stream(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AuthUser(user): AuthUser,
) -> Response {
    // Live events are owner-only: only the submitter is watching their own
    // build/run progress. Non-owner read access is through the detail
    // endpoint, which returns a final snapshot.
    let sub = match db::get_submission(&state.db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => return err(StatusCode::NOT_FOUND, "submission not found"),
        Err(e) => {
            error!(error=%e, %id, "load submission for SSE");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    if sub.user_id != user.id && !user.is_admin {
        return err(StatusCode::NOT_FOUND, "submission not found");
    }
    let snapshot = match load_detail_raw(&state, sub).await {
        Ok(d) => d,
        Err(e) => {
            error!(error=%e, %id, "snapshot for SSE");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    let snapshot_json = match serde_json::to_string(&snapshot) {
        Ok(j) => j,
        Err(e) => {
            error!(error=%e, "serialize snapshot");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "encode error");
        }
    };

    let initial = tokio_stream::once(Event::default().event("snapshot").data(snapshot_json));

    let rx = state.events.subscribe(id);
    let deltas = BroadcastStream::new(rx)
        .filter_map(|res| res.ok())
        .map(|ev| {
            let name = match &ev {
                crate::events::SubmissionEvent::Status { .. } => "status",
                crate::events::SubmissionEvent::CaseResult { .. } => "case_result",
                crate::events::SubmissionEvent::Finalized { .. } => "finalized",
                crate::events::SubmissionEvent::Failed { .. } => "failed",
            };
            let data = serde_json::to_string(&ev).unwrap_or_else(|_| String::from("{}"));
            Event::default().event(name).data(data)
        });

    let stream = initial.chain(deltas).map(Ok::<Event, Infallible>);

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

// Ensure tokio_stream::Stream import isn't elided when only used by signature.
#[allow(dead_code)]
fn _stream_anchor<S: Stream<Item = ()>>(_: S) {}
