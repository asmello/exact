// /api/runner/ws — persistent runner channel.
//
// Auth happens BEFORE the WebSocket upgrade: an attacker shouldn't be
// able to start a long-lived TCP read just by knowing the URL.
// The bearer token is matched by 8-char prefix (constant-time DB lookup)
// then verified with argon2; the X-Device-Id header must match the
// token's bound device. Mismatch → 401, no upgrade.
//
// Once upgraded the runner sends `Hello` to declare its board/clock, gets
// registered in the in-process hub, and is told about any submissions that
// were waiting in `ready` for its board. Subsequent CaseResult / RunResult
// messages flow into the submissions + case_results tables.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use exact_proto::{Board, CaseStatus, RunnerToServer, ServerToRunner};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::AppState;
use crate::admin::verify_token;
use crate::db;
use crate::dispatcher;
use crate::hub::RunnerEntry;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/runner/ws", get(handler))
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let token = match headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        Some(t) if t.len() >= 8 => t.to_string(),
        _ => return unauthorized(),
    };
    let claimed_device_id = match headers.get("x-device-id").and_then(|h| h.to_str().ok()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return unauthorized(),
    };

    let prefix = &token[..8];
    let auth = match db::find_runner_for_auth(&state.db, prefix).await {
        Ok(Some(r)) => r,
        Ok(None) => return unauthorized(),
        Err(e) => {
            warn!(error=%e, "runner auth lookup");
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
        }
    };

    if auth.revoked_at.is_some() {
        return unauthorized();
    }
    if !verify_token(&token, auth.token_hash.as_bytes()) {
        return unauthorized();
    }
    if auth.device_id != claimed_device_id {
        warn!(
            runner_id = auth.id,
            claimed = %claimed_device_id,
            bound = %auth.device_id,
            "runner token<->device mismatch"
        );
        return unauthorized();
    }

    let _ = db::touch_runner(&state.db, auth.id).await;
    let device_id = auth.device_id.clone();
    let state = state.clone();
    ws.on_upgrade(move |socket| run_socket(socket, state, auth.id, device_id))
}

async fn run_socket(socket: WebSocket, state: AppState, runner_id: i64, device_id: String) {
    info!(runner_id, %device_id, "runner connected");
    let (mut sink, mut stream) = socket.split();

    // First message must be Hello.
    let hello = match stream.next().await {
        Some(Ok(Message::Text(t))) => parse_msg(t.as_str()),
        _ => {
            warn!(%device_id, "no Hello frame");
            return;
        }
    };
    let (board, cclk_hz, version) = match hello {
        Some(RunnerToServer::Hello {
            board,
            cclk_hz,
            version,
            device_id: hello_device_id,
        }) => {
            if hello_device_id != device_id {
                warn!(%device_id, %hello_device_id, "Hello device_id mismatch");
                return;
            }
            (board, cclk_hz, version)
        }
        _ => {
            warn!(%device_id, "first message was not Hello");
            return;
        }
    };
    info!(%device_id, ?board, cclk_hz, %version, "runner Hello");

    // Mirror declared device state into the devices row. lm3s6965evb in
    // particular gets synthetic=true so the dispatcher knows to ask the
    // runner to fabricate cycles.
    let synthetic = matches!(board, Board::Lm3s6965evb);
    if let Err(e) = db::upsert_device(
        &state.db,
        &device_id,
        board_str(board),
        cclk_hz as i64,
        None,
        synthetic,
    )
    .await
    {
        warn!(error=%e, %device_id, "upsert device on Hello");
    }
    let _ = db::touch_device(&state.db, &device_id).await;

    // Register in hub + start writer task.
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerToRunner>();
    let entry = RunnerEntry {
        device_id: device_id.clone(),
        board,
        cclk_hz,
        synthetic,
        tx: tx.clone(),
    };
    state.hub.insert(entry);

    // Drain any ready submissions for this board.
    let pool = state.db.clone();
    let board_str_owned = board_str(board);
    let drain_tx = tx.clone();
    let device_id_for_drain = device_id.clone();
    tokio::spawn(async move {
        dispatcher::drain_ready_for_board(&pool, board_str_owned, &device_id_for_drain, &drain_tx)
            .await;
    });

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(s) => s,
                Err(e) => {
                    warn!(error=%e, "serializing ServerToRunner");
                    continue;
                }
            };
            if sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Reader loop.
    while let Some(frame) = stream.next().await {
        match frame {
            Ok(Message::Text(t)) => {
                if let Some(msg) = parse_msg(t.as_str()) {
                    handle_runner_msg(&state, &device_id, msg).await;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(p)) => {
                // axum's WS layer auto-replies to pings, but if we wanted
                // app-level keepalive logic this is where it'd go.
                let _ = p;
            }
            Err(e) => {
                warn!(error=%e, %device_id, "ws read error");
                break;
            }
            _ => {}
        }
    }

    writer.abort();
    state.hub.remove(&device_id);
    info!(%device_id, "runner disconnected");
}

fn parse_msg(s: &str) -> Option<RunnerToServer> {
    match serde_json::from_str::<RunnerToServer>(s) {
        Ok(m) => Some(m),
        Err(e) => {
            warn!(error=%e, "malformed RunnerToServer");
            None
        }
    }
}

async fn handle_runner_msg(state: &AppState, device_id: &str, msg: RunnerToServer) {
    match msg {
        RunnerToServer::Hello { .. } => {
            warn!(%device_id, "duplicate Hello after handshake");
        }
        RunnerToServer::Heartbeat => {
            let _ = db::touch_device(&state.db, device_id).await;
        }
        RunnerToServer::CaseResult {
            job_id,
            case_ord,
            status,
            exit_code,
            cycles,
            output,
            synthetic,
        } => {
            let (status_str, passed) = case_status_str(status);
            // `passed` = correctness of the result; only meaningful for OK runs
            // and once we've compared against expected_output below.
            let final_passed = match status {
                CaseStatus::Ok => match expected_for(state, job_id, case_ord as i32).await {
                    Some(Some(exp)) => Some(exp == output),
                    Some(None) => None, // benchmark-only case
                    None => None,
                },
                _ => Some(false),
            };
            if let Err(e) = db::insert_case_result(
                &state.db,
                db::NewCaseResult {
                    submission_id: job_id,
                    case_ord: case_ord as i32,
                    status: status_str,
                    exit_code: Some(exit_code as i32),
                    cycles: Some(cycles as i64),
                    output: Some(&output),
                    passed: final_passed.or(passed),
                    synthetic,
                },
            )
            .await
            {
                warn!(error=%e, %job_id, case_ord, "insert case_result");
            }
        }
        RunnerToServer::RunResult {
            job_id,
            overall,
            cclk_hz: _,
        } => {
            // Sum cycles + count pass/fail from case_results.
            if let Err(e) = finalize_run(state, job_id).await {
                warn!(error=%e, %job_id, "finalize run");
            }
            info!(%job_id, ?overall, "RunResult");
        }
        RunnerToServer::Error { job_id, reason } => {
            warn!(?job_id, %reason, "runner reported Error");
            if let Some(id) = job_id {
                let _ = db::set_submission_failed(&state.db, id, &reason).await;
            }
        }
    }
}

async fn expected_for(
    state: &AppState,
    submission_id: uuid::Uuid,
    case_ord: i32,
) -> Option<Option<Vec<u8>>> {
    let sub = db::get_submission(&state.db, submission_id)
        .await
        .ok()
        .flatten()?;
    let problem_id = sub.problem_id?;
    let cases = db::list_test_cases(&state.db, &problem_id).await.ok()?;
    let c = cases.into_iter().find(|c| c.ord == case_ord)?;
    Some(c.expected_output)
}

async fn finalize_run(state: &AppState, submission_id: uuid::Uuid) -> anyhow::Result<()> {
    let rows = db::list_case_results(&state.db, submission_id).await?;
    let total_cases = rows.len() as i32;
    let passed = rows.iter().filter(|r| r.passed.unwrap_or(false)).count() as i32;
    let total_cycles: Option<i64> = if rows.iter().all(|r| r.status == "OK") {
        Some(rows.iter().filter_map(|r| r.cycles).sum())
    } else {
        None
    };
    db::finalize_submission(&state.db, submission_id, total_cycles, passed, total_cases).await?;
    Ok(())
}

fn case_status_str(s: CaseStatus) -> (&'static str, Option<bool>) {
    match s {
        CaseStatus::Ok => ("OK", None),
        CaseStatus::Timeout => ("TIMEOUT", Some(false)),
        CaseStatus::Memfault => ("MEMFAULT", Some(false)),
        CaseStatus::Busfault => ("BUSFAULT", Some(false)),
        CaseStatus::Usagefault => ("USAGEFAULT", Some(false)),
        CaseStatus::LoadError => ("LOAD_ERROR", Some(false)),
    }
}

pub fn board_str(b: Board) -> &'static str {
    match b {
        Board::Lm3s6965evb => "lm3s6965evb",
        Board::Lpc1768 => "lpc1768",
        Board::Stm32f429zi => "stm32f429zi",
    }
}

// Stub to silence unused-warning for things still being wired.
#[allow(dead_code)]
fn _ensure_arc_state(_s: Arc<AppState>) {}
