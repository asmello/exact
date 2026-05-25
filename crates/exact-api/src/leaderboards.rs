// Per-(problem, board) leaderboards.
//
// Visibility mirrors GET /api/problems/:id: public always; shared requires
// `?t=<share_token>`; private 404s. Owner/admin always allowed.
//
// Ranking: a submission is eligible iff status='done', total_cycles IS NOT
// NULL (every case OK — no fault/timeout), and passed = total_cases (every
// case matched expected_output). Per-user best wins ties via earliest
// finished_at. Synthetic runs are ranked alongside real ones but flagged so
// the UI can distinguish them.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/problems/{id}/leaderboard", get(leaderboard))
}

#[derive(Deserialize)]
struct LeaderboardQuery {
    board: String,
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    limit: Option<i32>,
}

#[derive(Serialize, Clone)]
struct LeaderboardEntry {
    rank: i64,
    submission_id: uuid::Uuid,
    user_id: i64,
    github_login: String,
    avatar_url: Option<String>,
    total_cycles: i64,
    finished_at: chrono::DateTime<chrono::Utc>,
    synthetic: bool,
}

#[derive(Serialize)]
struct LeaderboardResponse {
    problem_id: String,
    board: String,
    entries: Vec<LeaderboardEntry>,
    /// Viewer's own best entry, included whether or not it appears in
    /// `entries`. `None` if the viewer is anonymous or hasn't submitted a
    /// fully-passing run on this (problem, board).
    you: Option<LeaderboardEntry>,
}

fn err(s: StatusCode, m: impl Into<String>) -> Response {
    (s, m.into()).into_response()
}

async fn leaderboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<LeaderboardQuery>,
    viewer: Option<AuthUser>,
) -> Response {
    let problem = match db::get_problem(&state.db, &id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => {
            warn!(error=%e, id, "load problem for leaderboard");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // Same gate as problems::check_read — 404 for no permission so private
    // problem ids don't leak via this endpoint.
    let is_admin = viewer.as_ref().map(|AuthUser(u)| u.is_admin).unwrap_or(false);
    let is_owner = viewer
        .as_ref()
        .map(|AuthUser(u)| u.id == problem.owner_id)
        .unwrap_or(false);
    let allowed = is_admin
        || is_owner
        || match problem.visibility.as_str() {
            "public" => true,
            "shared" => problem.share_token.as_deref() == q.t.as_deref(),
            _ => false,
        };
    if !allowed {
        return err(StatusCode::NOT_FOUND, "not found");
    }

    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let viewer_id = viewer.as_ref().map(|AuthUser(u)| u.id);

    let rows = match db::leaderboard(&state.db, &id, &q.board, limit, viewer_id).await {
        Ok(rs) => rs,
        Err(e) => {
            warn!(error=%e, id, board=%q.board, "leaderboard query");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    let mut entries: Vec<LeaderboardEntry> = Vec::with_capacity(rows.len());
    let mut you: Option<LeaderboardEntry> = None;
    for r in rows {
        let entry = LeaderboardEntry {
            rank: r.rank,
            submission_id: r.submission_id,
            user_id: r.user_id,
            github_login: r.github_login.clone(),
            avatar_url: r.avatar_url.clone(),
            total_cycles: r.total_cycles,
            finished_at: r.finished_at,
            synthetic: r.synthetic,
        };
        let is_viewer = viewer_id == Some(r.user_id);
        if is_viewer {
            you = Some(entry.clone());
        }
        if r.rank as i32 <= limit {
            entries.push(entry);
        }
    }

    Json(LeaderboardResponse {
        problem_id: id,
        board: q.board,
        entries,
        you,
    })
    .into_response()
}
