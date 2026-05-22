//! Wire types shared between `exact-api`, `exact-runner`, and the frontend.
//!
//! Kept dependency-light (serde + uuid + base64) so both the Rust backend
//! and the Pi agent can pull it in cheaply. Byte fields ship as base64-
//! encoded strings over JSON so payloads stay compact compared to serde's
//! default array-of-integers form for `Vec<u8>`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which Cortex-M board a submission targets.
///
/// Boards diverge in clock rate, MPU region sizes, and DWT cycle-counter
/// availability, so this enum drives both the build worker's target
/// selection and the runner dispatcher's device matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Board {
    Lm3s6965evb,
    Lpc1768,
    Stm32f429zi,
}

/// Per-case status reported by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Ok,
    Timeout,
    Memfault,
    Busfault,
    Usagefault,
    LoadError,
}

/// Overall run status (sum of all cases plus protocol-level outcomes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Timeout,
    Memfault,
    Busfault,
    Usagefault,
    LoadError,
}

/// One case's input bytes plus its ordinal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseInput {
    pub ord: u32,
    #[serde(with = "b64_bytes")]
    pub input: Vec<u8>,
}

// ---- Runner WebSocket protocol --------------------------------------------

/// Messages the runner sends up to the API over its persistent WS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunnerToServer {
    /// Sent immediately after the WS connects.
    Hello {
        device_id: String,
        board: Board,
        cclk_hz: u64,
        version: String,
    },
    Heartbeat,
    CaseOutput {
        job_id: Uuid,
        case_ord: u32,
        #[serde(with = "b64_bytes")]
        output: Vec<u8>,
    },
    CaseResult {
        job_id: Uuid,
        case_ord: u32,
        status: CaseStatus,
        exit_code: u32,
        cycles: u64,
        /// True if the runner fabricated `cycles` (QEMU dev mode).
        synthetic: bool,
    },
    RunResult {
        job_id: Uuid,
        overall: RunStatus,
        cclk_hz: u32,
    },
    Error {
        job_id: Option<Uuid>,
        reason: String,
    },
}

/// Messages the API sends down to a runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerToRunner {
    AssignJob {
        job_id: Uuid,
        #[serde(with = "b64_bytes")]
        bin: Vec<u8>,
        cases: Vec<CaseInput>,
        total_timeout_ms: u32,
    },
    Cancel {
        job_id: Uuid,
    },
    Ping,
}

mod b64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let s: String = Deserialize::deserialize(de)?;
        STANDARD.decode(s.as_bytes()).map_err(de::Error::custom)
    }
}
