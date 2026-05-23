// Spawn `qemu-system-arm -machine lm3s6965evb -serial pty` and recover the
// PTY path QEMU prints to stderr at startup. The runner then opens that
// PTY with `monolink::open_serial` exactly as it would a USB-CDC device,
// so the same wire-protocol code paths exercise QEMU and real hardware.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use std::io::{BufRead, BufReader};
use tracing::info;

pub struct QemuSession {
    pub pty_path: String,
    pub child: Child,
}

impl Drop for QemuSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start qemu, wait until it prints its PTY path on stdout, return the
/// path along with the child handle (so the caller can keep it alive).
///
/// We deliberately avoid `-nographic`: it both mutes the graphical
/// display *and* attaches an interactive monitor to stdio, which on
/// macOS produces a `(qemu)` prompt that competes with our announcement
/// parsing. `-display none -monitor none` does what we actually want.
/// The PTY announcement itself goes to qemu's stdout in this config.
pub fn spawn_qemu(kernel: &Path) -> Result<QemuSession> {
    let mut cmd = Command::new("qemu-system-arm");
    cmd.arg("-cpu")
        .arg("cortex-m3")
        .arg("-machine")
        .arg("lm3s6965evb")
        .arg("-display")
        .arg("none")
        .arg("-monitor")
        .arg("none")
        .arg("-semihosting-config")
        .arg("enable=on,target=native")
        .arg("-kernel")
        .arg(kernel)
        .arg("-serial")
        .arg("pty")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    info!(?kernel, "spawning qemu");
    let mut child = cmd.spawn().context("spawning qemu-system-arm")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let pty_path = parse_pty(stdout).context("parsing qemu PTY path from stdout")?;
    info!(%pty_path, "qemu PTY ready");
    Ok(QemuSession { pty_path, child })
}

/// QEMU emits "char device redirected to /dev/ttysXX (label serial0)"
/// for the first `-serial pty`. We grep that out and return the path.
fn parse_pty<R: std::io::Read>(stream: R) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    while Instant::now() < deadline {
        line.clear();
        let n = reader.read_line(&mut line).context("reading qemu output")?;
        if n == 0 {
            return Err(anyhow!("qemu stream closed before PTY announcement"));
        }
        if let Some(path) = extract_pty(&line) {
            return Ok(path);
        }
    }
    Err(anyhow!("timed out waiting for qemu PTY announcement"))
}

fn extract_pty(line: &str) -> Option<String> {
    // "char device redirected to /dev/ttys032 (label compat_monitor0)"
    let needle = "char device redirected to ";
    let idx = line.find(needle)?;
    let after = &line[idx + needle.len()..];
    let path: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
    if path.is_empty() { None } else { Some(path) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_macos_announcement() {
        let line = "char device redirected to /dev/ttys032 (label compat_monitor0)\n";
        assert_eq!(extract_pty(line), Some("/dev/ttys032".to_string()));
    }
}
