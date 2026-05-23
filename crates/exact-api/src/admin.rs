// Admin-only endpoints: device registration + runner provisioning.
//
// Runner tokens are returned plaintext exactly once (on create) and only
// stored as argon2 hashes thereafter, so a DB dump cannot impersonate any
// runner. The first 8 chars of the token serve as a lookup prefix to keep
// argon2 verifies O(1) — see auth flow in runner_ws.rs.

use anyhow::{Context, Result};
use argon2::Argon2;
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
};
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use base64::Engine;
use exact_proto::Board;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::AppState;
use crate::auth::AdminUser;
use crate::db;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/devices", get(list_devices_public))
        .route("/api/admin/devices", post(create_device))
        .route("/api/admin/runners", get(list_runners).post(create_runner))
        .route("/api/admin/runners/{id}", delete(revoke_runner))
}

fn err(s: StatusCode, m: impl Into<String>) -> Response {
    (s, m.into()).into_response()
}

// ---- Devices -------------------------------------------------------------

#[derive(Deserialize)]
struct CreateDeviceBody {
    id: String,
    board: Board,
    cclk_hz: i64,
    description: Option<String>,
    #[serde(default)]
    synthetic: bool,
}

fn board_str(b: Board) -> &'static str {
    match b {
        Board::Lm3s6965evb => "lm3s6965evb",
        Board::Lpc1768 => "lpc1768",
        Board::Stm32f429zi => "stm32f429zi",
    }
}

#[derive(Serialize)]
struct DeviceView {
    #[serde(flatten)]
    inner: db::Device,
    online: bool,
}

async fn list_devices_public(State(state): State<AppState>) -> Response {
    let devices = match db::list_devices(&state.db).await {
        Ok(d) => d,
        Err(e) => {
            error!(error=%e, "list devices");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    let views: Vec<DeviceView> = devices
        .into_iter()
        .map(|d| {
            let online = state.hub.get(&d.id).is_some();
            DeviceView { inner: d, online }
        })
        .collect();
    Json(views).into_response()
}

async fn create_device(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateDeviceBody>,
) -> Response {
    if body.id.is_empty() || body.cclk_hz <= 0 {
        return err(
            StatusCode::BAD_REQUEST,
            "id must be non-empty and cclk_hz > 0",
        );
    }
    match db::upsert_device(
        &state.db,
        &body.id,
        board_str(body.board),
        body.cclk_hz,
        body.description.as_deref(),
        body.synthetic,
    )
    .await
    {
        Ok(d) => (StatusCode::CREATED, Json(d)).into_response(),
        Err(e) => {
            error!(error=?e, "create device");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

// ---- Runners -------------------------------------------------------------

#[derive(Deserialize)]
struct CreateRunnerBody {
    device_id: String,
    label: String,
}

#[derive(Serialize)]
struct CreateRunnerResponse {
    runner: db::Runner,
    /// Plaintext bearer token. Shown once; never returned again.
    token: String,
}

fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).context("getrandom for runner token")?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

pub fn hash_token(token: &str) -> Result<Vec<u8>> {
    let salt = SaltString::generate(&mut OsRng);
    let phc = Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))?
        .to_string();
    Ok(phc.into_bytes())
}

pub fn verify_token(token: &str, hash_bytes: &[u8]) -> bool {
    let s = match std::str::from_utf8(hash_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let parsed = match PasswordHash::new(s) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(token.as_bytes(), &parsed)
        .is_ok()
}

async fn list_runners(State(state): State<AppState>, _admin: AdminUser) -> Response {
    match db::list_runners(&state.db).await {
        Ok(rs) => Json(rs).into_response(),
        Err(e) => {
            error!(error=?e, "list runners");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

async fn create_runner(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(body): Json<CreateRunnerBody>,
) -> Response {
    if body.device_id.is_empty() || body.label.is_empty() {
        return err(StatusCode::BAD_REQUEST, "device_id and label are required");
    }
    match db::get_device(&state.db, &body.device_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return err(StatusCode::NOT_FOUND, "device not found"),
        Err(e) => {
            error!(error=%e, "device lookup");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    }

    let token = match random_token() {
        Ok(t) => t,
        Err(e) => {
            error!(error=%e, "random_token");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "rng error");
        }
    };
    let hash = match hash_token(&token) {
        Ok(h) => h,
        Err(e) => {
            error!(error=%e, "hash_token");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "hash error");
        }
    };
    let prefix = &token[..8];

    match db::create_runner(
        &state.db,
        &body.device_id,
        &body.label,
        &hash,
        prefix,
        admin.0.id,
    )
    .await
    {
        Ok(runner) => (
            StatusCode::CREATED,
            Json(CreateRunnerResponse { runner, token }),
        )
            .into_response(),
        Err(e) => {
            error!(error=?e, "create runner");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}

async fn revoke_runner(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _admin: AdminUser,
) -> Response {
    match db::revoke_runner(&state.db, id).await {
        Ok(0) => err(StatusCode::NOT_FOUND, "runner not found or already revoked"),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            error!(error=?e, "revoke runner");
            err(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        }
    }
}
