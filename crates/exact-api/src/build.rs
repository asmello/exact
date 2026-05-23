// Build worker: compile a user submission for a Cortex-M target and pack
// the resulting ELF into a monoexec `.bin`.
//
// The user's source ships as a bare freestanding function (LeetCode-style),
// not a full no_std/no_main program. This module wraps it in:
//   1. The mandatory crate attributes (`#![no_std]`, `#![no_main]`).
//   2. Per-problem I/O glue derived from `io_spec` (decode input bytes →
//      typed args, call `solve(...)`, encode return → output bytes).
//   3. The `userlib::entry!(__exact_main)` macro that pulls in the
//      cortex-m vector table and panic handler.
//
// Sandboxing is intentionally light here: just a wall-clock timeout via
// tokio. The single-`.rs` user shape means no `build.rs`/proc-macros from
// user code (we control both Cargo.toml and build.rs), so the only
// realistic attack is rustc resource exhaustion — which the timeout caps.
// Linux `setrlimit` / namespaces land alongside the Railway Dockerfile.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use exact_proto::Board;
use serde::Deserialize;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::info;

const CARGO_TOML_IN: &str = include_str!("../../../build-template/Cargo.toml.in");
const BUILD_RS: &str = include_str!("../../../build-template/build.rs");

/// Wall-clock cap on `cargo build`. The first build of userlib in a cold
/// CARGO_TARGET_DIR is the worst case; incrementals are sub-second.
const BUILD_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug)]
pub enum BuildOutcome {
    Success { bin: Vec<u8> },
    Failure { log: String },
}

/// `io_spec` JSON shape: `{ "input": "u32_le", "output": "u64_le" }`.
///
/// For v1 each side is a single scalar; multi-arg / byte-array / variable-
/// length shapes can extend the enum without breaking existing problems.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct IoSpec {
    pub input: ScalarSpec,
    pub output: ScalarSpec,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum ScalarSpec {
    #[serde(rename = "u8")]
    U8,
    #[serde(rename = "i8")]
    I8,
    #[serde(rename = "u16_le")]
    U16Le,
    #[serde(rename = "u16_be")]
    U16Be,
    #[serde(rename = "i16_le")]
    I16Le,
    #[serde(rename = "i16_be")]
    I16Be,
    #[serde(rename = "u32_le")]
    U32Le,
    #[serde(rename = "u32_be")]
    U32Be,
    #[serde(rename = "i32_le")]
    I32Le,
    #[serde(rename = "i32_be")]
    I32Be,
    #[serde(rename = "u64_le")]
    U64Le,
    #[serde(rename = "u64_be")]
    U64Be,
    #[serde(rename = "i64_le")]
    I64Le,
    #[serde(rename = "i64_be")]
    I64Be,
}

impl ScalarSpec {
    pub fn rust_type(self) -> &'static str {
        use ScalarSpec::*;
        match self {
            U8 => "u8",
            I8 => "i8",
            U16Le | U16Be => "u16",
            I16Le | I16Be => "i16",
            U32Le | U32Be => "u32",
            I32Le | I32Be => "i32",
            U64Le | U64Be => "u64",
            I64Le | I64Be => "i64",
        }
    }

    pub fn size(self) -> usize {
        use ScalarSpec::*;
        match self {
            U8 | I8 => 1,
            U16Le | U16Be | I16Le | I16Be => 2,
            U32Le | U32Be | I32Le | I32Be => 4,
            U64Le | U64Be | I64Le | I64Be => 8,
        }
    }

    /// Rust expression that turns the input buf (a `[u8; N]` named `buf`)
    /// into a value of `rust_type()`.
    fn decode_expr(self, buf: &str) -> String {
        use ScalarSpec::*;
        let ty = self.rust_type();
        match self {
            U8 => format!("{buf}[0]"),
            I8 => format!("{buf}[0] as i8"),
            U16Le | U32Le | U64Le | I16Le | I32Le | I64Le => {
                format!("{ty}::from_le_bytes({buf})")
            }
            U16Be | U32Be | U64Be | I16Be | I32Be | I64Be => {
                format!("{ty}::from_be_bytes({buf})")
            }
        }
    }

    /// Rust expression that turns a `rust_type()` value (named `val`) into
    /// a `[u8; N]`.
    fn encode_expr(self, val: &str) -> String {
        use ScalarSpec::*;
        match self {
            U8 | I8 => format!("[{val} as u8]"),
            U16Le | U32Le | U64Le | I16Le | I32Le | I64Le => format!("{val}.to_le_bytes()"),
            U16Be | U32Be | U64Be | I16Be | I32Be | I64Be => format!("{val}.to_be_bytes()"),
        }
    }
}

/// Wrap the user's source in a complete `no_std`/`no_main` program that
/// reads one input, calls `solve(...)`, and writes the encoded output.
pub fn render_main_rs(user_source: &str, spec: &IoSpec) -> String {
    let in_size = spec.input.size();
    let in_type = spec.input.rust_type();
    let decode = spec.input.decode_expr("__exact_input_buf");
    let out_type = spec.output.rust_type();
    let encode = spec.output.encode_expr("__exact_output");

    format!(
        r#"#![no_std]
#![no_main]

extern crate userlib;

// === User source (verbatim) ============================================

{user_source}

// === Generated entry glue ==============================================

fn __exact_main() {{
    let mut __exact_input_buf = [0u8; {in_size}];
    let __exact_n = userlib::read(&mut __exact_input_buf);
    if __exact_n < {in_size} {{
        // Malformed input: too few bytes from the host. Sentinel exit
        // code distinguishes this from a clean run.
        userlib::exit(0xDEAD_BEEF);
    }}
    let __exact_input: {in_type} = {decode};
    let __exact_output: {out_type} = solve(__exact_input);
    let __exact_out_bytes = {encode};
    userlib::write(&__exact_out_bytes);
}}

userlib::entry!(__exact_main);
"#
    )
}

fn board_feature(board: Board) -> Result<&'static str> {
    match board {
        Board::Lm3s6965evb => Ok("lm3s6965evb"),
        Board::Lpc1768 => Ok("lpc1768"),
        Board::Stm32f429zi => Err(anyhow!(
            "stm32f429zi: userlib has no board feature for this target yet"
        )),
    }
}

/// Materialize the temp project (Cargo.toml + build.rs + src/main.rs) and
/// compile it. Returns the packed `.bin` bytes on success.
pub async fn build(
    source_code: &str,
    board: Board,
    spec: &IoSpec,
    userlib_path: &Path,
    pack_timeout_ms: u32,
) -> Result<BuildOutcome> {
    let feature = board_feature(board)?;

    let tmp = TempDir::new().context("creating build tempdir")?;
    let root = tmp.path();
    info!(dir = %root.display(), %feature, "build worker: starting");

    // Cargo.toml with the userlib path substituted in.
    let userlib_str = userlib_path
        .to_str()
        .ok_or_else(|| anyhow!("userlib path is not valid UTF-8"))?;
    let cargo_toml = CARGO_TOML_IN.replace("__USERLIB_PATH__", userlib_str);
    tokio::fs::write(root.join("Cargo.toml"), cargo_toml)
        .await
        .context("writing Cargo.toml")?;
    tokio::fs::write(root.join("build.rs"), BUILD_RS)
        .await
        .context("writing build.rs")?;
    tokio::fs::create_dir(root.join("src"))
        .await
        .context("mkdir src")?;
    let main_rs = render_main_rs(source_code, spec);
    tokio::fs::write(root.join("src/main.rs"), &main_rs)
        .await
        .context("writing src/main.rs")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--release")
        .arg("--target")
        .arg("thumbv7m-none-eabi")
        .arg("--no-default-features")
        .arg("--features")
        .arg(feature)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = match tokio::time::timeout(BUILD_TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(e).context("spawning cargo build"),
        Err(_) => {
            return Ok(BuildOutcome::Failure {
                log: format!("build timed out after {BUILD_TIMEOUT:?}"),
            });
        }
    };

    if !output.status.success() {
        let mut log = String::new();
        log.push_str(&String::from_utf8_lossy(&output.stdout));
        log.push_str(&String::from_utf8_lossy(&output.stderr));
        return Ok(BuildOutcome::Failure { log });
    }

    let elf_path = root.join("target/thumbv7m-none-eabi/release/exact-job");
    let elf = tokio::fs::read(&elf_path)
        .await
        .with_context(|| format!("reading {}", elf_path.display()))?;

    let mut bin = Vec::with_capacity(8 * 1024);
    let summary = monolink::pack_into(&elf, &mut bin, pack_timeout_ms).context("packing ELF")?;
    info!(
        text = summary.text_size,
        data = summary.data_size,
        bss = summary.bss_size,
        bytes = summary.total_bytes,
        "build worker: pack ok"
    );

    Ok(BuildOutcome::Success { bin })
}
