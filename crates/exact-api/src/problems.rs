// HTTP routes for /api/problems and the nested /cases sub-resource.
//
// Visibility model (mirrors plan):
//   - public: anyone can read
//   - shared: anyone with the share_token in `?t=`, plus owner/admin
//   - private: owner/admin only
// List endpoint never surfaces shared problems — they're discovered via
// direct URLs from the owner.

use anyhow::Result;
use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use base64::Engine;
use exact_proto::{b64_bytes, b64_bytes_opt};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::AppState;
use crate::auth::{AdminUser, AuthUser};
use crate::db::{self, NewProblem, NewTestCase, Problem, ProblemUpdate, TestCase, TestCaseUpdate};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/problems", get(list).post(create))
        .route("/api/problems/{id}", get(detail).put(update).delete(remove))
        .route(
            "/api/problems/{id}/cases",
            get(list_cases).post(create_case),
        )
        .route(
            "/api/problems/{id}/cases/{ord}",
            put(update_case).delete(delete_case),
        )
}

// ---- DTOs ----------------------------------------------------------------

#[derive(Deserialize)]
struct CreateProblemBody {
    id: String,
    title: String,
    description_md: String,
    starter_code: String,
    io_spec: serde_json::Value,
    visibility: String,
    default_timeout_ms: i32,
    allowed_boards: Vec<String>,
}

#[derive(Deserialize, Default)]
struct UpdateProblemBody {
    title: Option<String>,
    description_md: Option<String>,
    starter_code: Option<String>,
    io_spec: Option<serde_json::Value>,
    visibility: Option<String>,
    default_timeout_ms: Option<i32>,
    allowed_boards: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CreateCaseBody {
    ord: i32,
    #[serde(default)]
    name: Option<String>,
    #[serde(with = "b64_bytes")]
    input: Vec<u8>,
    #[serde(default, with = "b64_bytes_opt")]
    expected_output: Option<Vec<u8>>,
    #[serde(default = "default_weight")]
    weight: f32,
    #[serde(default)]
    hidden: bool,
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Deserialize, Default)]
struct UpdateCaseBody {
    // `Option<Option<_>>`: outer None = leave alone; outer Some(None) = set NULL.
    #[serde(default, deserialize_with = "deserialize_some")]
    name: Option<Option<String>>,
    #[serde(default, with = "b64_bytes_opt")]
    input: Option<Vec<u8>>,
    #[serde(default, deserialize_with = "deserialize_some_b64")]
    expected_output: Option<Option<Vec<u8>>>,
    weight: Option<f32>,
    hidden: Option<bool>,
}

/// `Option<Option<T>>` deserializer that distinguishes "field absent" from
/// "field explicitly null". Standard serde collapses both to None.
fn deserialize_some<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    use serde::Deserialize;
    Option::<T>::deserialize(de).map(Some)
}

/// Same as `deserialize_some` but routes through the b64 helper.
fn deserialize_some_b64<'de, D>(de: D) -> Result<Option<Option<Vec<u8>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    b64_bytes_opt::deserialize(de).map(Some)
}

#[derive(Serialize)]
struct CaseView {
    id: i64,
    problem_id: String,
    ord: i32,
    name: Option<String>,
    #[serde(with = "b64_bytes")]
    input: Vec<u8>,
    #[serde(with = "b64_bytes_opt")]
    expected_output: Option<Vec<u8>>,
    weight: f32,
    hidden: bool,
}

impl CaseView {
    fn from_full(c: TestCase) -> Self {
        Self {
            id: c.id,
            problem_id: c.problem_id,
            ord: c.ord,
            name: c.name,
            input: c.input,
            expected_output: c.expected_output,
            weight: c.weight,
            hidden: c.hidden,
        }
    }

    /// Non-owner view: hide `expected_output`.
    fn from_redacted(c: TestCase) -> Self {
        Self {
            id: c.id,
            problem_id: c.problem_id,
            ord: c.ord,
            name: c.name,
            input: c.input,
            expected_output: None,
            weight: c.weight,
            hidden: c.hidden,
        }
    }
}

#[derive(Deserialize)]
struct AccessQuery {
    #[serde(default)]
    t: Option<String>,
}

// ---- Helpers -------------------------------------------------------------

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_visibility(v: &str) -> bool {
    matches!(v, "private" | "shared" | "public")
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, msg.into()).into_response()
}

#[allow(clippy::result_large_err)] // Response is sized but used only for early-return errors.
async fn load_problem(state: &AppState, id: &str) -> Result<Problem, Response> {
    match db::get_problem(&state.db, id).await {
        Ok(Some(p)) => Ok(p),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "not found")),
        Err(e) => {
            warn!(error=%e, id, "load_problem");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "db error"))
        }
    }
}

/// Resolves access for a non-mutating read. Returns the problem and whether
/// the viewer is owner/admin (used to decide test-case redaction). 404 if no
/// permission, to prevent enumeration of private problems.
#[allow(clippy::result_large_err)]
fn check_read(
    problem: &Problem,
    viewer: Option<&AuthUser>,
    token: Option<&str>,
) -> Result<bool, Response> {
    let is_admin = viewer.map(|AuthUser(u)| u.is_admin).unwrap_or(false);
    let is_owner = viewer
        .map(|AuthUser(u)| u.id == problem.owner_id)
        .unwrap_or(false);
    if is_admin || is_owner {
        return Ok(true);
    }
    let allow = match problem.visibility.as_str() {
        "public" => true,
        "shared" => problem.share_token.as_deref() == token,
        _ => false,
    };
    if allow {
        Ok(false)
    } else {
        Err(err(StatusCode::NOT_FOUND, "not found"))
    }
}

/// Returns Ok if viewer can mutate; 403 otherwise (404 would leak existence).
#[allow(clippy::result_large_err)]
fn check_write(problem: &Problem, viewer: &AuthUser) -> Result<(), Response> {
    if viewer.0.is_admin || viewer.0.id == problem.owner_id {
        Ok(())
    } else {
        Err(err(StatusCode::FORBIDDEN, "not allowed"))
    }
}

fn random_share_token() -> String {
    // 16 url-safe bytes ~ 21 chars. Enough entropy that brute force is not
    // economical; not so long that the URL becomes unwieldy.
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("getrandom");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ---- Handlers ------------------------------------------------------------

async fn list(State(state): State<AppState>, viewer: Option<AuthUser>) -> Response {
    let (id, is_admin) = match &viewer {
        Some(AuthUser(u)) => (Some(u.id), u.is_admin),
        None => (None, false),
    };
    match db::list_problems(&state.db, id, is_admin).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            warn!(error=%e, "list problems");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

async fn create(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Json(body): Json<CreateProblemBody>,
) -> Response {
    if !valid_slug(&body.id) {
        return err(
            StatusCode::BAD_REQUEST,
            "id must be lowercase kebab-case (a-z0-9-), 1-64 chars",
        );
    }
    if !valid_visibility(&body.visibility) {
        return err(
            StatusCode::BAD_REQUEST,
            "visibility must be private|shared|public",
        );
    }
    if body.default_timeout_ms <= 0 {
        return err(StatusCode::BAD_REQUEST, "default_timeout_ms must be > 0");
    }

    let res = db::create_problem(
        &state.db,
        NewProblem {
            id: &body.id,
            title: &body.title,
            description_md: &body.description_md,
            starter_code: &body.starter_code,
            io_spec: &body.io_spec,
            visibility: &body.visibility,
            default_timeout_ms: body.default_timeout_ms,
            allowed_boards: &body.allowed_boards,
            owner_id: user.id,
        },
    )
    .await;

    let mut problem = match res {
        Ok(p) => p,
        Err(e) => {
            warn!(error=?e, "create problem");
            // sqlx wraps a postgres error; surface the common cases.
            let msg = format!("{e:#}");
            if msg.contains("duplicate key") {
                return err(StatusCode::CONFLICT, "problem id already exists");
            }
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // Shared problems get an auto-generated token so the owner can copy the
    // /p/:id?t=... URL right after creating.
    if problem.visibility == "shared" && problem.share_token.is_none() {
        let token = random_share_token();
        if let Err(e) = sqlx::query("UPDATE problems SET share_token = $1 WHERE id = $2")
            .bind(&token)
            .bind(&problem.id)
            .execute(&state.db)
            .await
        {
            warn!(error=%e, id=%problem.id, "set share_token");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
        problem.share_token = Some(token);
    }

    (StatusCode::CREATED, Json(problem)).into_response()
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AccessQuery>,
    viewer: Option<AuthUser>,
) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = check_read(&problem, viewer.as_ref(), q.t.as_deref()) {
        return r;
    }
    Json(problem).into_response()
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    user: AuthUser,
    Json(body): Json<UpdateProblemBody>,
) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = check_write(&problem, &user) {
        return r;
    }
    if let Some(v) = &body.visibility
        && !valid_visibility(v)
    {
        return err(
            StatusCode::BAD_REQUEST,
            "visibility must be private|shared|public",
        );
    }

    let mut updated = match db::update_problem(
        &state.db,
        &id,
        ProblemUpdate {
            title: body.title.as_deref(),
            description_md: body.description_md.as_deref(),
            starter_code: body.starter_code.as_deref(),
            io_spec: body.io_spec.as_ref(),
            visibility: body.visibility.as_deref(),
            default_timeout_ms: body.default_timeout_ms,
            allowed_boards: body.allowed_boards.as_deref(),
        },
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            warn!(error=%e, id, "update problem");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // If the visibility flipped to "shared" and we don't already have a
    // token, mint one so the owner can immediately copy a /p/:id?t=... URL.
    if updated.visibility == "shared" && updated.share_token.is_none() {
        let token = random_share_token();
        if let Err(e) = sqlx::query("UPDATE problems SET share_token = $1 WHERE id = $2")
            .bind(&token)
            .bind(&updated.id)
            .execute(&state.db)
            .await
        {
            warn!(error=%e, id=%updated.id, "set share_token");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
        updated.share_token = Some(token);
    }

    Json(updated).into_response()
}

async fn remove(State(state): State<AppState>, Path(id): Path<String>, user: AuthUser) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = check_write(&problem, &user) {
        return r;
    }
    match db::delete_problem(&state.db, &id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!(error=%e, id, "delete problem");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

// ---- Cases ---------------------------------------------------------------

async fn list_cases(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AccessQuery>,
    viewer: Option<AuthUser>,
) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let owns = match check_read(&problem, viewer.as_ref(), q.t.as_deref()) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let cases = match db::list_test_cases(&state.db, &id).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error=%e, id, "list cases");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    let views: Vec<CaseView> = if owns {
        cases.into_iter().map(CaseView::from_full).collect()
    } else {
        // Non-owners see only non-hidden cases, with expected_output redacted.
        cases
            .into_iter()
            .filter(|c| !c.hidden)
            .map(CaseView::from_redacted)
            .collect()
    };
    Json(views).into_response()
}

async fn create_case(
    State(state): State<AppState>,
    Path(id): Path<String>,
    user: AuthUser,
    Json(body): Json<CreateCaseBody>,
) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = check_write(&problem, &user) {
        return r;
    }
    if body.ord < 0 {
        return err(StatusCode::BAD_REQUEST, "ord must be >= 0");
    }
    if body.weight < 0.0 {
        return err(StatusCode::BAD_REQUEST, "weight must be >= 0");
    }
    match db::create_test_case(
        &state.db,
        NewTestCase {
            problem_id: &id,
            ord: body.ord,
            name: body.name.as_deref(),
            input: &body.input,
            expected_output: body.expected_output.as_deref(),
            weight: body.weight,
            hidden: body.hidden,
        },
    )
    .await
    {
        Ok(c) => (StatusCode::CREATED, Json(CaseView::from_full(c))).into_response(),
        Err(e) => {
            warn!(error=%e, id, "create case");
            let msg = format!("{e:#}");
            if msg.contains("duplicate key") {
                return err(StatusCode::CONFLICT, "case with that ord already exists");
            }
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

async fn update_case(
    State(state): State<AppState>,
    Path((id, ord)): Path<(String, i32)>,
    user: AuthUser,
    Json(body): Json<UpdateCaseBody>,
) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = check_write(&problem, &user) {
        return r;
    }
    match db::update_test_case(
        &state.db,
        &id,
        ord,
        TestCaseUpdate {
            name: body.name.as_ref().map(|o| o.as_deref()),
            input: body.input.as_deref(),
            expected_output: body.expected_output.as_ref().map(|o| o.as_deref()),
            weight: body.weight,
            hidden: body.hidden,
        },
    )
    .await
    {
        Ok(Some(c)) => Json(CaseView::from_full(c)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "case not found"),
        Err(e) => {
            warn!(error=%e, id, ord, "update case");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

async fn delete_case(
    State(state): State<AppState>,
    Path((id, ord)): Path<(String, i32)>,
    user: AuthUser,
) -> Response {
    let problem = match load_problem(&state, &id).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = check_write(&problem, &user) {
        return r;
    }
    match db::delete_test_case(&state.db, &id, ord).await {
        Ok(0) => err(StatusCode::NOT_FOUND, "case not found"),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!(error=%e, id, ord, "delete case");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}
