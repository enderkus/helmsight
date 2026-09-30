//! Executing the collection script on hosts: over SSH with one persistent
//! session per host, or locally for `serve --local`.

pub mod hostkeys;
mod local;
mod ssh;

pub use hostkeys::{HostKeys, PresentedKey, Verdict};
pub use local::LocalExecutor;
pub use ssh::{Credentials, SshTarget, SshTargetConfig};

use serde::Serialize;
use std::time::Duration;

/// Output of one remote execution.
#[derive(Debug, Clone, Default)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    /// First bytes of stderr, for diagnostics.
    pub stderr: Vec<u8>,
    pub exit_status: Option<u32>,
    /// Output exceeded the limit and was cut.
    pub truncated: bool,
    pub duration: Duration,
}

/// Why a host could not be reached. These map directly to host states shown
/// in the UI, so each variant must be actionable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportError {
    #[error("connection failed: {message}")]
    Unreachable { message: String },
    #[error("host key for {} is not trusted yet (fingerprint {})", .key.endpoint, .key.fingerprint)]
    HostKeyUnknown { key: HostKeyInfo },
    #[error("host key for {} changed (now {}); possible man-in-the-middle", .key.endpoint, .key.fingerprint)]
    HostKeyChanged { key: HostKeyInfo },
    #[error("SSH authentication failed: {message}")]
    AuthFailed { message: String },
    #[error("command timed out after {secs}s")]
    Timeout { secs: u64 },
    #[error("SSH error: {message}")]
    Protocol { message: String },
}

/// Serializable view of a presented host key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HostKeyInfo {
    pub endpoint: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub previous: Option<String>,
}

impl From<&PresentedKey> for HostKeyInfo {
    fn from(k: &PresentedKey) -> Self {
        Self {
            endpoint: k.endpoint.clone(),
            algorithm: k.algorithm.clone(),
            fingerprint: k.fingerprint.clone(),
            previous: k.previous.clone(),
        }
    }
}

impl TransportError {
    /// Short machine-readable state used by the API.
    pub fn state(&self) -> &'static str {
        match self {
            TransportError::Unreachable { .. } | TransportError::Timeout { .. } => "unreachable",
            TransportError::HostKeyUnknown { .. } => "host_key_unknown",
            TransportError::HostKeyChanged { .. } => "host_key_changed",
            TransportError::AuthFailed { .. } => "auth_failed",
            TransportError::Protocol { .. } => "unreachable",
        }
    }
}

/// Runs commands on one host.
#[async_trait::async_trait]
pub trait Executor: Send + Sync {
    /// Runs `command`, writing `stdin` (if any) and closing it. Stdout is
    /// capped at `limit` bytes; the whole execution at `timeout`.
    async fn exec(
        &self,
        command: &str,
        stdin: Option<&[u8]>,
        limit: usize,
        timeout: Duration,
    ) -> Result<ExecOutput, TransportError>;

    /// Drops any cached connection.
    async fn reset(&self) {}
}

/// Maximum stderr kept for diagnostics.
pub(crate) const STDERR_LIMIT: usize = 16 * 1024;
