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
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg(command)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Own process group, so that children of the shell are killed too.
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd.spawn().map_err(|e| TransportError::Protocol {
            message: format!("cannot start /bin/sh: {e}"),
        })?;
        let pid = child.id();

        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            let input = input.to_vec();
            tokio::spawn(async move {
                let _ = pipe.write_all(&input).await;
                let _ = pipe.shutdown().await;
            });
        }
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        // Read both pipes concurrently so that a chatty stderr cannot block.
        let read_out = async {
            let mut data = Vec::new();
            let mut truncated = false;
            let mut buf = vec![0u8; 64 * 1024];
            if let Some(so) = stdout.as_mut() {
                loop {
                    let n = so.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    let room = limit.saturating_sub(data.len());
                    data.extend_from_slice(buf.get(..n.min(room)).unwrap_or(&[]));
                    if n > room {
                        truncated = true;
                        kill_group(pid);
                        break;
                    }
                }
            }
            (data, truncated)
        };
        let read_err = async {
            let mut err = Vec::new();
            if let Some(se) = stderr.as_mut() {
                let _ = se.take(STDERR_LIMIT as u64).read_to_end(&mut err).await;
            }
            err
        };
        let run = async { tokio::join!(read_out, read_err) };
        match tokio::time::timeout(timeout, run).await {
            Ok(((stdout, truncated), stderr)) => {
                let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                Ok(ExecOutput {
                    stdout,
                    stderr,
                    exit_status: status
                        .ok()
                        .and_then(|s| s.ok())
                        .and_then(|s| s.code())
                        .and_then(|c| u32::try_from(c).ok()),
                    truncated,
                    duration: start.elapsed(),
                })
            }
            Err(_) => {
                kill_group(pid);
                let _ = child.kill().await;
                Err(TransportError::Timeout {
                    secs: timeout.as_secs(),
                })
            }
        }
    }
}

/// Kills the whole process group started for a command.
#[cfg(unix)]
fn kill_group(pid: Option<u32>) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    if let Some(pid) = pid.and_then(|p| i32::try_from(p).ok()) {
        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_group(_pid: Option<u32>) {}

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

    #[tokio::test]
    async fn kills_children_of_the_shell() {
        // The shell forks `yes` instead of exec'ing it; truncation must still
        // terminate it, otherwise stderr never reaches end-of-file.
        let out = LocalExecutor
            .exec("yes; true", None, 1000, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(out.truncated);
        let err = LocalExecutor
            .exec("sleep 30; true", None, 1000, Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(matches!(err, TransportError::Timeout { .. }));
    }
}
