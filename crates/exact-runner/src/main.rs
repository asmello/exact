//! exact-runner: Pi-side agent that drives one Cortex-M device for exact-api.
//!
//! Step-2 skeleton: parses CLI args, prints them. The WebSocket reconnect
//! loop, `Loader` integration, and protocol bridging land in step 6.

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(about = "exact runner agent — bridges a Cortex-M device to exact-api")]
struct Cli {
    /// Backend WSS URL, e.g. wss://exact.run/api/runner/ws
    #[arg(long, env = "BACKEND_URL")]
    backend_url: String,

    /// Path to a file containing the per-runner bearer token, mode 0400.
    #[arg(long, env = "API_KEY_FILE")]
    api_key_file: std::path::PathBuf,

    /// Stable device identifier registered in the API, e.g. lpc1768-pi-asm.
    #[arg(long, env = "DEVICE_ID")]
    device_id: String,

    /// Serial port path, e.g. /dev/ttyACM0 or a QEMU `-serial pty`.
    #[arg(long, env = "SERIAL_PORT")]
    serial_port: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();
    info!(
        backend_url = %cli.backend_url,
        device_id = %cli.device_id,
        serial_port = %cli.serial_port,
        "exact-runner starting (WS loop unimplemented)",
    );

    Ok(())
}
