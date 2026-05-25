//! exact-api: HTTP + WebSocket service for the exact judging system.
//!
//! Step 3: Postgres pool, GitHub OAuth, signed-cookie sessions, /api/me.
//! Routes for problems, submissions, runner WS land in later steps.

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::FromRef;
use axum::routing::get;
use axum_extra::extract::cookie::Key;
use oauth2::basic::BasicClient;
use sqlx::PgPool;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod admin;
mod auth;
mod build;
mod config;
mod db;
mod dispatcher;
mod events;
mod hub;
mod leaderboards;
mod problems;
mod runner_ws;
mod submissions;

use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub oauth: BasicClient,
    pub key: Key,
    pub hub: Arc<hub::RunnerHub>,
    pub events: Arc<events::EventBus>,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.key.clone()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env if present (CWD walk). Done before tracing init so RUST_LOG
    // in .env applies. Missing file is fine — prod sets env vars directly.
    let dotenv_path = match dotenvy::dotenv() {
        Ok(p) => Some(p),
        Err(e) if e.not_found() => None,
        Err(e) => return Err(e).context("loading .env"),
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    if let Some(p) = dotenv_path {
        info!(path = %p.display(), "loaded .env");
    }

    let config = Config::from_env().context("loading config")?;
    let key = Key::from(&config.session_secret);
    let oauth = auth::build_oauth_client(
        &config.backend_public_url,
        &config.github_client_id,
        &config.github_client_secret,
    )?;

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    let bind_addr = config.bind_addr.clone();
    let state = AppState {
        config: Arc::new(config),
        db: pool,
        oauth,
        key,
        hub: hub::RunnerHub::new(),
        events: events::EventBus::new(),
    };

    let app = Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .route("/api/me", get(auth::me))
        .merge(auth::router())
        .merge(problems::router())
        .merge(leaderboards::router())
        .merge(submissions::router())
        .merge(admin::router())
        .merge(runner_ws::router())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("binding {bind_addr}"))?;
    info!(%bind_addr, "exact-api listening");
    axum::serve(listener, app).await.context("serving")?;
    Ok(())
}
