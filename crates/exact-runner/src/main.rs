//! exact-runner: Pi-side agent that drives one Cortex-M device for exact-api.
//!
//! Architecture: three tasks.
//!   - WS reader: deserializes ServerToRunner messages; pushes AssignJob
//!     onto a job queue.
//!   - WS writer: drains an outgoing RunnerToServer channel into the WS sink.
//!   - Loader worker (blocking thread): pulls AssignJob, drives
//!     `monolink::Loader::upload_and_run`, pushes each per-case event
//!     back onto the outgoing channel.
//!
//! Reconnect with exponential backoff (1s → 30s). On reconnect we send
//! Hello again — server treats it as fresh state. In-flight jobs are
//! abandoned (client/runner crash isn't part of v1's recovery story).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use exact_proto::{Board, CaseInput, CaseStatus, RunStatus, RunnerToServer, ServerToRunner};
use futures_util::{SinkExt, StreamExt};
use monolink::FileTransport;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod qemu;
mod synthetic;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(about = "exact runner agent — bridges a Cortex-M device to exact-api")]
struct Cli {
    /// Backend WSS URL, e.g. wss://exact.run/api/runner/ws
    #[arg(long, env = "BACKEND_URL")]
    backend_url: String,

    /// Path to a file containing the per-runner bearer token, mode 0400.
    #[arg(long, env = "API_KEY_FILE")]
    api_key_file: PathBuf,

    /// Stable device identifier registered in the API, e.g. lpc1768-pi-asm.
    #[arg(long, env = "DEVICE_ID")]
    device_id: String,

    /// Board this runner serves. Defaults to lm3s6965evb under `--qemu`,
    /// lpc1768 otherwise.
    #[arg(long, env = "BOARD")]
    board: Option<String>,

    /// Reported core clock. Defaults: 12 MHz (qemu) / 96 MHz (lpc1768).
    #[arg(long, env = "CCLK_HZ")]
    cclk_hz: Option<u64>,

    /// Serial port path for real hardware, e.g. /dev/ttyACM0. Mutually
    /// exclusive with `--qemu`.
    #[arg(long, env = "SERIAL_PORT", conflicts_with = "qemu")]
    serial_port: Option<String>,

    /// Spawn `qemu-system-arm` (lm3s6965evb) and use its `-serial pty`.
    #[arg(long, env = "QEMU", requires = "kernel")]
    qemu: bool,

    /// Path to the mono-os kernel.elf for `--qemu` mode.
    #[arg(long, env = "KERNEL")]
    kernel: Option<PathBuf>,

    /// Baud rate for real serial ports. Ignored under `--qemu`.
    #[arg(long, env = "BAUD", default_value = "115200")]
    baud: u32,
}

fn default_board(qemu: bool) -> Board {
    if qemu {
        Board::Lm3s6965evb
    } else {
        Board::Lpc1768
    }
}

fn parse_board(s: &str) -> Result<Board> {
    Ok(match s {
        "lm3s6965evb" => Board::Lm3s6965evb,
        "lpc1768" => Board::Lpc1768,
        "stm32f429zi" => Board::Stm32f429zi,
        other => return Err(anyhow!("unknown board {other}")),
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();
    let board = match cli.board.as_deref() {
        Some(b) => parse_board(b)?,
        None => default_board(cli.qemu),
    };
    let cclk_hz = cli.cclk_hz.unwrap_or(match board {
        Board::Lm3s6965evb => 12_000_000,
        Board::Lpc1768 => 96_000_000,
        Board::Stm32f429zi => 168_000_000,
    });

    let token = std::fs::read_to_string(&cli.api_key_file)
        .with_context(|| format!("reading token from {}", cli.api_key_file.display()))?
        .trim()
        .to_string();

    // Bring the device up exactly once. For QEMU we keep one process
    // around for the whole runner lifetime — the kernel inside it loops
    // listening for EXEC frames, so we don't need to restart between
    // submissions. _qemu_session lives in main; Drop kills the child.
    let (transport, _qemu_session) = open_transport(&cli)?;

    // Hand the transport to a dedicated blocking thread so the synchronous
    // monolink::Loader API can run without blocking the tokio reactor.
    let (job_tx, job_rx) = std::sync::mpsc::channel::<JobReq>();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<RunnerToServer>();
    std::thread::Builder::new()
        .name("monolink-loader".into())
        .spawn(move || loader_worker(transport, job_rx, out_tx))
        .context("spawning loader worker thread")?;

    // Reconnect loop.
    let mut backoff = Duration::from_secs(1);
    loop {
        let attempt = run_session(
            &cli.backend_url,
            &token,
            &cli.device_id,
            board,
            cclk_hz,
            &mut out_rx,
            &job_tx,
        )
        .await;
        match attempt {
            Ok(()) => {
                info!("session ended cleanly; reconnecting after {backoff:?}");
            }
            Err(e) => {
                warn!(error=?e, "session error; reconnecting after {backoff:?}");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

fn open_transport(cli: &Cli) -> Result<(FileTransport, Option<qemu::QemuSession>)> {
    if cli.qemu {
        let kernel = cli
            .kernel
            .as_ref()
            .ok_or_else(|| anyhow!("--qemu requires --kernel"))?;
        let session = qemu::spawn_qemu(kernel).context("spawn qemu")?;
        let ft = monolink::open_serial(&session.pty_path, cli.baud)
            .with_context(|| format!("open qemu PTY {}", session.pty_path))?;
        Ok((ft, Some(session)))
    } else {
        let port = cli
            .serial_port
            .as_deref()
            .ok_or_else(|| anyhow!("--serial-port required without --qemu"))?;
        let ft =
            monolink::open_serial(port, cli.baud).with_context(|| format!("open serial {port}"))?;
        Ok((ft, None))
    }
}

/// One AssignJob worth of work, pushed from the WS layer to the loader
/// worker thread. `total_timeout_ms` is already encoded into the bin's
/// monoexec header (the build worker baked it in), so the runner doesn't
/// need to enforce it separately.
struct JobReq {
    job_id: uuid::Uuid,
    bin: Vec<u8>,
    cases: Vec<CaseInput>,
    #[allow(dead_code)]
    total_timeout_ms: u32,
    synthetic: bool,
}

/// One round of: connect WS → send Hello → reader/writer loops. Returns
/// when either side errors or the connection closes.
async fn run_session(
    url: &str,
    token: &str,
    device_id: &str,
    board: Board,
    cclk_hz: u64,
    out_rx: &mut mpsc::UnboundedReceiver<RunnerToServer>,
    job_tx: &std::sync::mpsc::Sender<JobReq>,
) -> Result<()> {
    let mut req = url.into_client_request().context("parsing backend URL")?;
    req.headers_mut().insert(
        http::header::AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .context("authorization header")?,
    );
    req.headers_mut().insert(
        "x-device-id",
        device_id.parse().context("x-device-id header")?,
    );

    info!(%url, %device_id, "connecting to backend");
    let (ws, _resp) = tokio_tungstenite::connect_async(req)
        .await
        .context("WS connect")?;
    info!("connected");

    let (mut sink, mut stream) = ws.split();

    // Hello.
    let hello = RunnerToServer::Hello {
        device_id: device_id.to_string(),
        board,
        cclk_hz,
        version: VERSION.to_string(),
    };
    sink.send(Message::text(serde_json::to_string(&hello)?))
        .await
        .context("send Hello")?;

    // Writer task: outgoing channel → sink. Aborted when this fn returns.
    let writer = tokio::spawn(async move {
        // out_rx is held by the caller; we use a relay through another
        // channel to allow this task ownership.
        // (Simpler: relay in the caller via select! — see below.)
    });
    writer.abort();

    // Combined reader/writer via select!.
    loop {
        tokio::select! {
            // Outgoing: drain results from the worker thread.
            maybe_msg = out_rx.recv() => {
                match maybe_msg {
                    Some(msg) => {
                        let json = serde_json::to_string(&msg).context("serialize outgoing")?;
                        sink.send(Message::text(json)).await.context("send outgoing")?;
                    }
                    None => return Err(anyhow!("outgoing channel closed")),
                }
            }
            // Incoming: deserialize, dispatch.
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Text(t))) => {
                        match serde_json::from_str::<ServerToRunner>(t.as_str()) {
                            Ok(ServerToRunner::AssignJob { job_id, bin, cases, total_timeout_ms }) => {
                                info!(%job_id, bin_bytes = bin.len(), cases = cases.len(), "AssignJob");
                                let synthetic = matches!(board, Board::Lm3s6965evb);
                                let req = JobReq { job_id, bin, cases, total_timeout_ms, synthetic };
                                if job_tx.send(req).is_err() {
                                    return Err(anyhow!("loader worker channel closed"));
                                }
                            }
                            Ok(ServerToRunner::Cancel { job_id }) => {
                                warn!(%job_id, "Cancel received (not yet implemented)");
                            }
                            Ok(ServerToRunner::Ping) => {}
                            Err(e) => {
                                warn!(error=%e, payload=%t.as_str(), "malformed ServerToRunner");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WS closed by server");
                        return Ok(());
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e).context("ws read"),
                }
            }
        }
    }
}

/// Blocking worker: owns the Transport + Loader for the whole runner
/// lifetime, processes one AssignJob at a time, sends results back over
/// `out_tx`. Synthetic-cycle overlay applied if the inbound JobReq says so.
fn loader_worker(
    transport: FileTransport,
    job_rx: std::sync::mpsc::Receiver<JobReq>,
    out_tx: mpsc::UnboundedSender<RunnerToServer>,
) {
    let mut loader = monolink::Loader::new(transport);
    while let Ok(req) = job_rx.recv() {
        run_one(&mut loader, req, &out_tx);
    }
}

fn run_one(
    loader: &mut monolink::Loader<FileTransport>,
    req: JobReq,
    out_tx: &mpsc::UnboundedSender<RunnerToServer>,
) {
    let JobReq {
        job_id,
        bin,
        cases,
        total_timeout_ms: _,
        synthetic,
    } = req;

    let bin_sha = if synthetic {
        Some(synthetic::sha256(&bin))
    } else {
        None
    };

    let case_inputs: Vec<Vec<u8>> = cases.iter().map(|c| c.input.clone()).collect();
    let ords: Vec<u32> = cases.iter().map(|c| c.ord).collect();

    let report = loader.upload_and_run(&bin, &case_inputs, |i, frame, output| {
        let case_ord = *ords.get(i).unwrap_or(&(i as u32));
        let status = map_case_status(frame.status);
        let cycles = if let Some(ref sha) = bin_sha {
            synthetic::synthetic_cycles(
                sha,
                case_inputs.get(i).map(|v| v.as_slice()).unwrap_or(&[]),
            )
        } else {
            frame.cycles
        };
        let msg = RunnerToServer::CaseResult {
            job_id,
            case_ord,
            status,
            exit_code: frame.exit_code,
            cycles,
            output: output.to_vec(),
            synthetic,
        };
        if out_tx.send(msg).is_err() {
            warn!(%job_id, "out_tx closed mid-case");
        }
    });

    match report {
        Ok(report) => {
            let overall = map_run_status_from_byte(report.overall.status);
            let _ = out_tx.send(RunnerToServer::RunResult {
                job_id,
                overall,
                cclk_hz: report.overall.cclk_hz,
            });
        }
        Err(e) => {
            error!(error=%e, %job_id, "Loader::upload_and_run failed");
            let _ = out_tx.send(RunnerToServer::Error {
                job_id: Some(job_id),
                reason: format!("{e:#}"),
            });
        }
    }
}

fn map_case_status(byte: u8) -> CaseStatus {
    use monolink::proto::{
        STATUS_BUSFAULT, STATUS_LOAD_ERROR, STATUS_MEMFAULT, STATUS_OK, STATUS_TIMEOUT,
        STATUS_USAGEFAULT,
    };
    match byte {
        STATUS_OK => CaseStatus::Ok,
        STATUS_TIMEOUT => CaseStatus::Timeout,
        STATUS_MEMFAULT => CaseStatus::Memfault,
        STATUS_BUSFAULT => CaseStatus::Busfault,
        STATUS_USAGEFAULT => CaseStatus::Usagefault,
        STATUS_LOAD_ERROR => CaseStatus::LoadError,
        _ => CaseStatus::LoadError,
    }
}

fn map_run_status_from_byte(byte: u8) -> RunStatus {
    use monolink::proto::{
        STATUS_BUSFAULT, STATUS_LOAD_ERROR, STATUS_MEMFAULT, STATUS_OK, STATUS_TIMEOUT,
        STATUS_USAGEFAULT,
    };
    match byte {
        STATUS_OK => RunStatus::Ok,
        STATUS_TIMEOUT => RunStatus::Timeout,
        STATUS_MEMFAULT => RunStatus::Memfault,
        STATUS_BUSFAULT => RunStatus::Busfault,
        STATUS_USAGEFAULT => RunStatus::Usagefault,
        STATUS_LOAD_ERROR => RunStatus::LoadError,
        _ => RunStatus::LoadError,
    }
}
