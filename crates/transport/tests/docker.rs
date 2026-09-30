//! Integration tests against real sshd containers (tests/docker).
//!
//! Enabled with `HELMSIGHT_DOCKER_TESTS=1`. The distributions default to
//! Debian, Rocky and Alpine; override with
//! `HELMSIGHT_DOCKER_DISTROS="debian ubuntu rocky fedora opensuse alpine"`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use collect::{Group, Sampler, ScriptRequest};
use russh::keys::PrivateKey;
use russh::keys::ssh_key::LineEnding;
use russh::keys::ssh_key::private::Ed25519Keypair;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use transport::{Credentials, Executor, HostKeys, SshTarget, SshTargetConfig, TransportError};

fn enabled() -> bool {
    std::env::var("HELMSIGHT_DOCKER_TESTS").is_ok_and(|v| v == "1")
}

fn distros() -> Vec<String> {
    std::env::var("HELMSIGHT_DOCKER_DISTROS")
        .unwrap_or_else(|_| "debian rocky alpine".into())
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker").args(args).output().expect("docker");
    assert!(
        out.status.success(),
        "docker {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn docker_stdin(args: &[&str], input: &str) {
    use std::io::Write;
    let mut child = Command::new("docker")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("docker");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

struct Container {
    name: String,
    port: u16,
}

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
    }
}

fn start(distro: &str, authorized_key: &str) -> Container {
    let image = format!("helmsight-test-{distro}");
    let ctx = repo_root().join("tests/docker").join(distro);
    docker(&["build", "-q", "-t", &image, ctx.to_str().unwrap()]);
    let name = format!("helmsight-it-{distro}-{}", std::process::id());
    let _ = Command::new("docker").args(["rm", "-f", &name]).output();
    let mut args = vec!["run", "-d", "--name", &name, "-p", "127.0.0.1::22", &image];
    if distro != "alpine" {
        args.extend(["/usr/sbin/sshd", "-D", "-e"]);
    }
    docker(&args);
    let c = Container {
        port: docker(&["port", &name, "22/tcp"])
            .lines()
            .next()
            .and_then(|l| l.rsplit(':').next())
            .and_then(|p| p.parse().ok())
            .expect("mapped port"),
        name,
    };
    docker_stdin(
        &[
            "exec",
            "-i",
            &c.name,
            "sh",
            "-c",
            "cat > /home/monitor/.ssh/authorized_keys && chown monitor /home/monitor/.ssh/authorized_keys && chmod 600 /home/monitor/.ssh/authorized_keys",
        ],
        authorized_key,
    );
    // Wait for sshd.
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", c.port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(500));
    c
}

fn keypair(dir: &Path, seed: u8, name: &str) -> (PathBuf, String) {
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]));
    let path = dir.join(name);
    let pem = key.to_openssh(LineEnding::LF).unwrap();
    common::secrets::write_private(&path, pem.as_bytes()).unwrap();
    (path, key.public_key().to_openssh().unwrap())
}

fn target(port: u16, key: &Path, hostkeys: Arc<HostKeys>) -> SshTarget {
    let (creds, warnings) = Credentials::load(&[key.to_path_buf()], false);
    assert!(warnings.is_empty(), "{warnings:?}");
    SshTarget::new(
        SshTargetConfig {
            address: "127.0.0.1".into(),
            port,
            user: "monitor".into(),
            identity_file: None,
            connect_timeout: Duration::from_secs(10),
            keepalive: Duration::from_secs(30),
        },
        Arc::new(creds),
        hostkeys,
    )
    .0
}

fn request() -> ScriptRequest {
    ScriptRequest::new(Group::ALL.to_vec(), 0, [7; 16])
}

#[tokio::test]
async fn collects_over_ssh_from_real_distributions() {
    if !enabled() {
        eprintln!("skipped: set HELMSIGHT_DOCKER_TESTS=1");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (key, public) = keypair(dir.path(), 11, "id_ed25519");
    let (wrong_key, _) = keypair(dir.path(), 12, "id_wrong");

    for distro in distros() {
        let c = start(&distro, &public);
        let hostkeys = Arc::new(HostKeys::new(
            dir.path().join(format!("kh-{distro}")),
            vec![],
            false,
        ));
        let t = target(c.port, &key, hostkeys.clone());
        let req = request();
        let script = req.render();

        // Unknown host key: refused, fingerprint reported.
        let err = t
            .exec(
                "sh -s",
                Some(script.as_bytes()),
                collect::MAX_OUTPUT_BYTES,
                Duration::from_secs(60),
            )
            .await
            .unwrap_err();
        let TransportError::HostKeyUnknown { key: info } = err else {
            panic!("{distro}: expected unknown host key, got {err:?}")
        };
        let actual = docker(&[
            "exec",
            &c.name,
            "ssh-keygen",
            "-lf",
            "/etc/ssh/ssh_host_ed25519_key.pub",
        ]);
        assert!(
            actual.contains(&info.fingerprint),
            "{distro}: {actual} vs {}",
            info.fingerprint
        );

        // Approve the exact fingerprint, then collect.
        hostkeys
            .approve("127.0.0.1", c.port, &info.fingerprint)
            .unwrap();
        let marker = "/tmp/.helmsight-it-marker";
        docker(&[
            "exec",
            &c.name,
            "sh",
            "-c",
            &format!("touch {marker}; sleep 1"),
        ]);
        let out = t
            .exec(
                "sh -s",
                Some(script.as_bytes()),
                collect::MAX_OUTPUT_BYTES,
                Duration::from_secs(120),
            )
            .await
            .unwrap_or_else(|e| panic!("{distro}: {e}"));
        assert!(!out.truncated);
        let text = String::from_utf8_lossy(&out.stdout);
        let mut sampler = Sampler::new();
        let c1 = sampler.ingest(&text, &req, 0.0, 0);
        assert!(
            c1.complete && c1.missing.is_empty(),
            "{distro}: {:?}",
            c1.missing
        );
        assert_eq!(c1.basics.unwrap().user.as_deref(), Some("monitor"));
        assert!(c1.inventory.unwrap().packages.is_ok());

        // Second run reuses the session and yields rates.
        let out2 = t
            .exec(
                "sh -s",
                Some(script.as_bytes()),
                collect::MAX_OUTPUT_BYTES,
                Duration::from_secs(120),
            )
            .await
            .unwrap();
        let c2 = sampler.ingest(&String::from_utf8_lossy(&out2.stdout), &req, 5.0, 0);
        assert!(c2.metrics.unwrap().cpu.is_some(), "{distro}: no CPU rates");

        // Agentless: nothing was written outside logs and runtime state.
        // The only exception is the host's own login bookkeeping: Ubuntu's
        // pam_motd records the first login of a user in ~/.cache.
        let written = docker(&[
            "exec",
            &c.name,
            "sh",
            "-c",
            &format!(
                "find / -xdev -type f -newer {marker} 2>/dev/null \\
                 | grep -vE '^/(proc|sys|dev|run|var/log|var/run)/' \\
                 | grep -vx '/home/monitor/.cache/motd.legal-displayed' || true"
            ),
        ]);
        assert!(
            written.is_empty(),
            "{distro}: files written on remote host: {written}"
        );

        // Wrong key: authentication failure.
        let bad = target(c.port, &wrong_key, hostkeys.clone());
        let err = bad
            .exec("true", None, 1024, Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(
            matches!(err, TransportError::AuthFailed { .. }),
            "{distro}: {err:?}"
        );

        // Changed key: a trust store that knows a different key refuses.
        let other = dir.path().join(format!("kh-other-{distro}"));
        let fake = PrivateKey::from(Ed25519Keypair::from_seed(&[99; 32]));
        std::fs::write(
            &other,
            format!(
                "[127.0.0.1]:{} {}\n",
                c.port,
                fake.public_key().to_openssh().unwrap()
            ),
        )
        .unwrap();
        let changed = target(c.port, &key, Arc::new(HostKeys::new(other, vec![], true)));
        let err = changed
            .exec("true", None, 1024, Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(
            matches!(err, TransportError::HostKeyChanged { .. }),
            "{distro}: {err:?}"
        );
    }
}

#[tokio::test]
async fn closed_port_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let (key, _) = keypair(dir.path(), 13, "id");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let t = target(
        port,
        &key,
        Arc::new(HostKeys::new(dir.path().join("kh"), vec![], false)),
    );
    let err = t
        .exec("true", None, 10, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert_eq!(err.state(), "unreachable", "{err:?}");
}
