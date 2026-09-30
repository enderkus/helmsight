//! Parses real script output captured from each supported distribution
//! (see scripts/capture-fixtures.sh) and checks the extracted values.

use collect::model::Probe;
use collect::{Collection, Group, Sampler, ScriptRequest};

const DISTROS: [&str; 6] = ["debian", "ubuntu", "rocky", "fedora", "opensuse", "alpine"];

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn request() -> ScriptRequest {
    ScriptRequest {
        nonce: "fixture".into(),
        groups: Group::ALL.to_vec(),
        auth_since: 0,
    }
}

/// Parses a fixture twice so that rate-based metrics are computed. The
/// second sample has its CPU counters advanced by 100 jiffies per field.
fn parse(name: &str) -> Collection {
    let raw = fixture(name);
    let mut s = Sampler::new();
    let _ = s.ingest(&raw, &request(), 0.0, 1_790_787_400);
    s.ingest(&advance_cpu(&raw), &request(), 5.0, 1_790_787_405)
}

fn advance_cpu(raw: &str) -> String {
    raw.lines()
        .map(|l| {
            if l.starts_with("cpu") {
                l.split_ascii_whitespace()
                    .enumerate()
                    .map(|(i, t)| match (i, t.parse::<u64>()) {
                        (0, _) | (_, Err(_)) => t.to_string(),
                        (_, Ok(v)) => (v + 100).to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_fixture_is_complete() {
    for d in DISTROS {
        for who in ["root", "user"] {
            let c = parse(&format!("{d}-{who}"));
            assert!(c.complete, "{d}-{who} incomplete");
            assert!(c.missing.is_empty(), "{d}-{who} missing {:?}", c.missing);
            let m = c.metrics.as_ref().expect("metrics");
            let mem = m.mem.as_ref().expect("mem");
            assert!(mem.total > 0 && mem.used_pct >= 0.0 && mem.used_pct <= 100.0);
            let cpu = m.cpu.as_ref().expect("cpu");
            assert!(cpu.cores >= 1);
            assert_eq!(cpu.per_core.len(), cpu.cores as usize);
            assert!(m.load.is_some());
            assert!(m.uptime_secs.unwrap() > 0.0);
            assert!(!m.filesystems.is_empty(), "{d}-{who} has no filesystems");
            assert!(m.filesystems.iter().any(|f| f.mount == "/"));
            let procs = m.procs.as_ref().expect("procs");
            assert!(procs.total >= 1);
            assert!(!procs.top_mem.is_empty());
            let basics = c.basics.as_ref().expect("basics");
            assert!(basics.page_size.is_some());
            let expected_user = if who == "root" { "root" } else { "monitor" };
            assert_eq!(basics.user.as_deref(), Some(expected_user));
        }
    }
}

#[test]
fn os_release_per_distro() {
    let expect = [
        ("debian", "debian"),
        ("ubuntu", "ubuntu"),
        ("rocky", "rocky"),
        ("fedora", "fedora"),
        ("opensuse", "opensuse-leap"),
        ("alpine", "alpine"),
    ];
    for (d, id) in expect {
        let inv = parse(&format!("{d}-root")).inventory.expect("inventory");
        assert_eq!(inv.os.id.as_deref(), Some(id), "{d}");
        assert!(inv.os.pretty_name.is_some());
        assert_eq!(inv.kernel.name.as_deref(), Some("Linux"));
        assert!(inv.kernel.release.is_some());
        assert!(
            inv.virtualization
                .as_deref()
                .unwrap_or("")
                .starts_with("container")
        );
    }
}

#[test]
fn packages_per_manager() {
    let expect = [
        ("debian", "dpkg", "base-files"),
        ("ubuntu", "dpkg", "base-files"),
        ("rocky", "rpm", "bash"),
        ("fedora", "rpm", "bash"),
        ("opensuse", "rpm", "bash"),
        ("alpine", "apk", "busybox"),
    ];
    for (d, mgr, pkg) in expect {
        let inv = parse(&format!("{d}-user")).inventory.expect("inventory");
        let Probe::Ok { source, data } = inv.packages else {
            panic!("{d}: packages unavailable")
        };
        assert_eq!(source, mgr, "{d}");
        assert!(data.len() > 10, "{d}: only {} packages", data.len());
        assert!(data.iter().any(|p| p.name == pkg), "{d}: {pkg} missing");
    }
}

#[test]
fn listening_ports() {
    for d in DISTROS {
        let medium = parse(&format!("{d}-root")).medium.expect("medium");
        let Probe::Ok { data, source } = medium.listening else {
            panic!("{d}: listening unavailable")
        };
        let ssh = data
            .iter()
            .find(|s| s.port == 22 && s.proto == "tcp")
            .unwrap_or_else(|| panic!("{d}: sshd not listening ({source}) {data:?}"));
        assert!(
            ssh.process.as_deref().unwrap_or("").starts_with("sshd"),
            "{d}: {ssh:?}"
        );
    }
    // Unprivileged users see the sockets but not the owning process.
    let medium = parse("debian-user").medium.expect("medium");
    let socks = medium.listening.data().expect("listening");
    assert!(socks.iter().any(|s| s.port == 80));
    assert!(socks.iter().all(|s| s.process.is_none()));
}

#[test]
fn failed_units_are_detected() {
    for d in ["debian", "ubuntu", "rocky", "fedora", "opensuse"] {
        let medium = parse(&format!("{d}-root")).medium.expect("medium");
        let Probe::Ok { data, source } = medium.services else {
            panic!("{d}: services unavailable")
        };
        assert_eq!(source, "systemd");
        let broken = data
            .iter()
            .find(|s| s.unit == "broken.service")
            .unwrap_or_else(|| panic!("{d}: broken.service missing"));
        assert!(broken.is_failed());
        assert!(
            data.iter()
                .any(|s| s.active == "active" && s.sub == "running")
        );
    }
    // Without D-Bus, systemctl fails for unprivileged users on minimal images.
    let medium = parse("debian-user").medium.expect("medium");
    assert!(matches!(medium.services, Probe::Na { .. }));
    // Alpine uses OpenRC.
    let medium = parse("alpine-root").medium.expect("medium");
    assert!(matches!(medium.services, Probe::Ok { ref source, .. } if source == "openrc"));
}

#[test]
fn enabled_units() {
    let inv = parse("rocky-root").inventory.expect("inventory");
    let units = inv.enabled_units.data().expect("units");
    assert!(units.iter().any(|u| u == "sshd.service"));
    let inv = parse("alpine-root").inventory.expect("inventory");
    assert!(inv.enabled_units.is_ok());
}

#[test]
fn failed_ssh_logins() {
    for d in ["debian", "ubuntu", "rocky", "fedora", "alpine"] {
        let auth = parse(&format!("{d}-root")).auth.expect("auth");
        let Probe::Ok { data, .. } = auth else {
            panic!("{d}: auth unavailable: {auth:?}")
        };
        // admin (invalid, password), root (password), oracle (invalid,
        // password), ghost (invalid, publickey only).
        assert_eq!(data.len(), 4, "{d}: {data:#?}");
        assert!(data.iter().any(|e| e.user == "root" && !e.invalid_user));
        assert!(data.iter().any(|e| e.user == "ghost" && e.invalid_user));
        assert!(data.iter().all(|e| e.ip.as_deref() == Some("127.0.0.1")));
    }
    // Journal is not readable for unprivileged users on systemd hosts.
    for d in ["debian", "ubuntu", "rocky", "fedora"] {
        let auth = parse(&format!("{d}-user")).auth.expect("auth");
        assert!(matches!(auth, Probe::Na { .. }), "{d}: {auth:?}");
    }
    // BusyBox syslog writes a world-readable /var/log/messages.
    let auth = parse("alpine-user").auth.expect("auth");
    assert!(auth.is_ok());
}

#[test]
fn pending_updates() {
    let up = parse("rocky-root").updates.expect("updates");
    let Probe::Ok { data, .. } = up else {
        panic!("{up:?}")
    };
    assert!(data.total > 10);
    assert!(data.security.unwrap() > 0);

    let up = parse("debian-root").updates.expect("updates");
    let Probe::Ok { data, .. } = up else {
        panic!("{up:?}")
    };
    assert_eq!(data.total, 1);
    assert_eq!(data.security, Some(1));
    assert_eq!(data.packages[0].name, "tzdata");

    let up = parse("ubuntu-root").updates.expect("updates");
    assert!(
        matches!(up, Probe::Na { .. }),
        "empty apt lists must be n/a"
    );

    let up = parse("fedora-root").updates.expect("updates");
    assert!(up.data().unwrap().total >= 1);

    let up = parse("alpine-root").updates.expect("updates");
    assert!(
        matches!(up, Probe::Na { .. }),
        "missing apk index must be n/a"
    );

    let up = parse("opensuse-root").updates.expect("updates");
    assert!(up.is_ok());
}

#[test]
fn process_cpu_rates_after_second_sample() {
    let c = parse("rocky-root");
    let procs = c.metrics.unwrap().procs.unwrap();
    // Identical samples: every process used 0 % CPU, but values exist.
    assert!(!procs.top_cpu.is_empty());
    assert!(procs.top_cpu.iter().all(|p| p.cpu_pct.is_some()));
    assert!(procs.top_mem.iter().any(|p| p.command.is_some()));
}
