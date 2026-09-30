//! Local execution for monitoring the machine the server runs on.

use crate::{ExecOutput, Executor, STDERR_LIMIT, TransportError};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// Runs commands with `/bin/sh` on the local machine.
#[derive(Debug, Default)]
pub struct LocalExecutor;

#[async_trait::async_trait]
impl Executor for LocalExecutor {
    async fn exec(
        &self,
        command: &str,
        stdin: Option<&[u8]>,
        limit: usize,
        timeout: Duration,
    ) -> Result<ExecOutput, TransportError> {
        let start = Instant::now();
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| TransportError::Protocol {
                message: format!("cannot start /bin/sh: {e}"),
            })?;

        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            let input = input.to_vec();
            tokio::spawn(async move {
                let _ = pipe.write_all(&input).await;
                let _ = pipe.shutdown().await;
            });
        }
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let run = async {
            let mut out = ExecOutput::default();
            let mut buf = vec![0u8; 64 * 1024];
            if let Some(so) = stdout.as_mut() {
                loop {
                    let n = so.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    let room = limit.saturating_sub(out.stdout.len());
                    out.stdout
                        .extend_from_slice(buf.get(..n.min(room)).unwrap_or(&[]));
                    if n > room {
                        out.truncated = true;
                        break;
                    }
                }
            }
            if out.truncated {
                let _ = child.start_kill();
            }
            if let Some(se) = stderr.as_mut() {
                let mut err = Vec::new();
                let _ = se.take(STDERR_LIMIT as u64).read_to_end(&mut err).await;
                out.stderr = err;
            }
            out
        };
        let result = tokio::time::timeout(timeout, run).await;
        match result {
            Ok(mut out) => {
                let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                out.exit_status = status
                    .ok()
                    .and_then(|s| s.ok())
                    .and_then(|s| s.code())
                    .and_then(|c| u32::try_from(c).ok());
                out.duration = start.elapsed();
                Ok(out)
            }
            Err(_) => {
                let _ = child.kill().await;
                Err(TransportError::Timeout {
                    secs: timeout.as_secs(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_script_from_stdin() {
        let out = LocalExecutor
            .exec(
                "sh -s",
                Some(b"echo hello; echo oops >&2; exit 3"),
                1024,
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert_eq!(out.stdout, b"hello\n");
        assert_eq!(out.stderr, b"oops\n");
        assert_eq!(out.exit_status, Some(3));
    }

    #[tokio::test]
    async fn truncates_and_times_out() {
        let out = LocalExecutor
            .exec("yes", None, 1000, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(out.truncated);
        assert_eq!(out.stdout.len(), 1000);
        let err = LocalExecutor
            .exec("sleep 5", None, 1000, Duration::from_millis(200))
            .await
            .unwrap_err();
        assert!(matches!(err, TransportError::Timeout { .. }));
    }
}
