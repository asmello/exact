// Bridge between "build finished" and "send the bin to a runner."
//
// Called from two places:
//   1. The build worker (after a successful pack) calls
//      `dispatch_or_queue` for that one submission.
//   2. The runner WS endpoint calls `drain_ready_for_board` when a
//      runner says Hello, to pick up anything that was waiting.
//
// Uses `db::try_claim_submission` for the actual 'ready' → 'running'
// transition so two dispatchers racing on the same runner can't both
// claim the same job.

use anyhow::{Context, Result};
use exact_proto::{Board, CaseInput, ServerToRunner};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::events::EventBus;

/// Build the AssignJob payload for `submission_id` and send it to `tx`.
/// Atomically claims 'ready' → 'running' (binding the row to `device_id`)
/// before reading the bin, so a parallel dispatcher can't double-assign.
async fn build_assign_job(
    pool: &PgPool,
    submission_id: Uuid,
    device_id: &str,
) -> Result<Option<ServerToRunner>> {
    if !crate::db::try_claim_submission(pool, submission_id, device_id).await? {
        return Ok(None);
    }
    let bin = match crate::db::get_submission_bin(pool, submission_id).await? {
        Some(b) => b,
        None => {
            warn!(%submission_id, "claimed submission but bin_blob is null");
            return Ok(None);
        }
    };
    let sub = crate::db::get_submission(pool, submission_id)
        .await?
        .context("claimed submission vanished")?;
    let problem_id = match sub.problem_id {
        Some(p) => p,
        None => return Ok(None),
    };
    let problem = crate::db::get_problem(pool, &problem_id)
        .await?
        .context("problem referenced by submission is gone")?;
    let cases = crate::db::list_test_cases(pool, &problem_id).await?;
    let case_inputs: Vec<CaseInput> = cases
        .into_iter()
        .map(|c| CaseInput {
            ord: c.ord as u32,
            input: c.input,
        })
        .collect();

    Ok(Some(ServerToRunner::AssignJob {
        job_id: submission_id,
        bin,
        cases: case_inputs,
        total_timeout_ms: problem.default_timeout_ms as u32,
    }))
}

/// Try to dispatch one submission to a runner online for its board. If no
/// runner is online, leave it at status='ready' for later pickup.
pub async fn dispatch_or_queue(state: &AppState, submission_id: Uuid, board: Board) {
    let entry = match state.hub.find_for_board(board) {
        Some(e) => e,
        None => {
            info!(%submission_id, ?board, "no runner online; left in 'ready'");
            return;
        }
    };
    let job = match build_assign_job(&state.db, submission_id, &entry.device_id).await {
        Ok(Some(j)) => j,
        Ok(None) => return,
        Err(e) => {
            warn!(error=%e, %submission_id, "build AssignJob");
            return;
        }
    };
    if entry.tx.send(job).is_err() {
        warn!(%submission_id, device_id=%entry.device_id, "runner channel closed mid-dispatch");
    } else {
        info!(%submission_id, device_id=%entry.device_id, "AssignJob queued");
        state.events.publish(
            submission_id,
            crate::events::SubmissionEvent::Status {
                status: "running".into(),
            },
        );
    }
}

/// On runner connect: send AssignJob for every submission stuck in
/// 'ready' for this runner's board, routing them all to this specific
/// runner's tx.
pub async fn drain_ready_for_board(
    pool: &PgPool,
    events: &EventBus,
    board: &str,
    device_id: &str,
    tx: &mpsc::UnboundedSender<ServerToRunner>,
) {
    let subs = match crate::db::list_ready_for_board(pool, board).await {
        Ok(s) => s,
        Err(e) => {
            warn!(error=%e, board, "list ready");
            return;
        }
    };
    if subs.is_empty() {
        return;
    }
    info!(board, count = subs.len(), "draining ready submissions");

    for sub in subs {
        match build_assign_job(pool, sub.id, device_id).await {
            Ok(Some(job)) => {
                if tx.send(job).is_err() {
                    warn!(submission_id=%sub.id, "runner tx closed during drain");
                    return;
                }
                events.publish(
                    sub.id,
                    crate::events::SubmissionEvent::Status {
                        status: "running".into(),
                    },
                );
            }
            Ok(None) => {
                // Race: another runner claimed it. Skip.
            }
            Err(e) => {
                warn!(error=%e, submission_id=%sub.id, "drain build_assign_job");
            }
        }
    }
}
