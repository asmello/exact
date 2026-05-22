//! exact-api: HTTP + WebSocket service for the exact judging system.
//!
//! This is the step-2 skeleton — a single `GET /api/healthz` route and the
//! tracing scaffold. Real routes (auth, problems, submissions, runner WS)
//! land in subsequent steps.

use anyhow::{Context, Result};
use axum::Router;
use axum::routing::get;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = Router::new().route("/api/healthz", get(|| async { "ok" }));

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    info!(%addr, "exact-api listening");
    axum::serve(listener, app).await.context("serving")?;
    Ok(())
}
