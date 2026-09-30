//! Property tests: parsers must never panic on arbitrary, truncated or
//! corrupted input, and must keep their output bounded.

use collect::parse::{
    auth, containers, fs, listen, packages, procfs, procs, services, system, updates,
};
use collect::{Group, Sampler, ScriptRequest};
use proptest::prelude::*;

fn request() -> ScriptRequest {
    ScriptRequest {
        nonce: "fixture".into(),
        groups: Group::ALL.to_vec(),
        auth_since: 0,
    }
}

fn run_all_parsers(s: &str) {
    let _ = procfs::stat(s);
    let _ = procfs::meminfo(s);
    let _ = procfs::loadavg(s);
    let _ = procfs::uptime(s);
    let _ = procfs::diskstats(s);
    let _ = procfs::netdev(s);
    let _ = procfs::tcp_states(s);
    let m = fs::mounts(s);
    let _ = fs::filesystems(s, s, &m);
    let _ = procs::procstat(s);
    let _ = procs::ps(s);
    for prefix in ["", "#src ss\n", "#src netstat\n", "#src proc\n#file tcp6\n"] {
        let _ = listen::listening(&format!("{prefix}{s}"));
    }
    for prefix in ["", "#src systemd\n", "#src openrc\n"] {
        let _ = services::services(&format!("{prefix}{s}"));
        let _ = services::enabled_units(&format!("{prefix}{s}"));
    }
    for prefix in ["", "#src docker\n", "#src podman\n"] {
        let _ = containers::containers(&format!("{prefix}{s}\n#stats\n{s}"));
    }
    for prefix in ["", "#src dpkg\n", "#src rpm\n", "#src apk\n"] {
        let _ = packages::packages(&format!("{prefix}{s}"));
    }
    for prefix in ["", "#src journal\n", "tz=+0530\nnow=99\n#src file /x\n"] {
        let _ = auth::auth_failures(&format!("{prefix}{s}"), 0);
    }
    for prefix in [
        "",
        "#src apt\n#lists 1 1\n",
        "#src dnf\n",
        "#src zypper\n",
        "#src apk\n",
    ] {
        let _ = updates::updates(&format!("{prefix}{s}\n#security\n{s}"), 0);
    }
    let _ = system::os_release(s);
    let _ = system::kernel(s);
    let _ = system::cpu_model(s);
    let _ = system::virtualization(s);
    let _ = system::reboot(s);
    let _ = system::sessions(s);
    let _ = system::basics(s);
}

fn fixture_text() -> String {
    let dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let mut all = String::new();
    for name in ["debian-root", "rocky-root", "alpine-root", "opensuse-user"] {
        all.push_str(&std::fs::read_to_string(format!("{dir}/{name}.txt")).unwrap_or_default());
    }
    all
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text_never_panics(s in "\\PC{0,400}") {
        run_all_parsers(&s);
    }

    #[test]
    fn arbitrary_structured_lines_never_panic(
        lines in proptest::collection::vec(
            "[ a-zA-Z0-9:()\\[\\]{}\"%/,.=|<>_\\-]{0,80}", 0..40)
    ) {
        run_all_parsers(&lines.join("\n"));
    }

    #[test]
    fn truncated_fixtures_never_panic(cut in 0usize..200_000) {
        let text = fixture_text();
        let mut end = cut.min(text.len());
        while !text.is_char_boundary(end) { end -= 1; }
        let truncated = &text[..end];
        let mut s = Sampler::new();
        let c = s.ingest(truncated, &request(), 0.0, 0);
        let _ = s.ingest(truncated, &request(), 1.0, 0);
        // Truncated output must never claim to be complete unless the cut
        // happens after the first end marker.
        if !truncated.contains("@@HS-fixture@@ end") {
            prop_assert!(!c.complete);
        }
    }

    #[test]
    fn corrupted_fixtures_never_panic(
        edits in proptest::collection::vec((0usize..150_000, any::<u8>()), 1..50)
    ) {
        let mut bytes = fixture_text().into_bytes();
        for (pos, b) in edits {
            if !bytes.is_empty() {
                let i = pos % bytes.len();
                bytes[i] = b;
            }
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut s = Sampler::new();
        let _ = s.ingest(&text, &request(), 0.0, 0);
        let _ = s.ingest(&text, &request(), 5.0, 0);
        run_all_parsers(&text);
    }

    #[test]
    fn huge_numbers_never_panic(a in any::<u64>(), b in any::<u64>(), c in any::<i64>()) {
        let text = format!(
            "cpu {a} {b} {a} {b} {a} {b} {a} {b}\ncpu0 {a} {b} 1 1\nbtime {c}\n\
             MemTotal: {a} kB\nMemAvailable: {b} kB\nSwapTotal: {b} kB\nSwapFree: {a} kB\n"
        );
        let _ = procfs::stat(&text);
        if let Some(m) = procfs::meminfo(&text) {
            prop_assert!(m.used_pct >= 0.0 && m.used_pct <= 100.0);
            prop_assert!(m.swap_used_pct >= 0.0 && m.swap_used_pct <= 100.0);
        }
        let _ = containers::parse_size(&format!("{a}.{b}GiB"));
    }
}

#[test]
fn output_is_bounded() {
    let many_lines = "x (y) S 1 1 1 0 -1 0 0 0 0 0 1 1 0 0 20 0 1 0 1 1 1\n".repeat(10);
    let _ = procs::procstat(&many_lines);
    let pkgs = format!("#src apk\n{}", "pkg-1.0-r0\n".repeat(200_000));
    let p = packages::packages(&pkgs);
    assert!(p.data().map(|d| d.len()).unwrap_or(0) <= 50_000);
    let long = "a".repeat(10_000_000);
    run_all_parsers(&long);
}
