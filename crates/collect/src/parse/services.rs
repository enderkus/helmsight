//! Service state from systemd or OpenRC.

use crate::model::{Probe, Service};
use crate::util::clip;

const MAX_SERVICES: usize = 5000;

pub fn services(text: &str) -> Probe<Vec<Service>> {
    let mut lines = text.lines();
    let src = lines
        .next()
        .and_then(|l| l.strip_prefix("#src "))
        .unwrap_or("")
        .trim();
    let body: Vec<&str> = lines.take(MAX_SERVICES * 2).collect();
    match src {
        "systemd" => systemd(&body),
        "openrc" => Probe::ok("openrc", openrc(&body)),
        "none" => Probe::na("no supported service manager (systemd or OpenRC) found"),
        _ => Probe::na("service data not collected"),
    }
}

fn systemd(lines: &[&str]) -> Probe<Vec<Service>> {
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim_start_matches(['●', '*', '×', ' ']);
        let mut it = line.split_ascii_whitespace();
        let (Some(unit), Some(load), Some(active), Some(sub)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        if !unit.contains('.') || !is_state_word(load) || !is_state_word(active) {
            continue;
        }
        if load == "not-found" && active == "inactive" {
            continue;
        }
        let description = it.collect::<Vec<_>>().join(" ");
        out.push(Service {
            unit: clip(unit, 256),
            load: clip(load, 32),
            active: clip(active, 32),
            sub: clip(sub, 32),
            description: clip(&description, 256),
        });
        if out.len() >= MAX_SERVICES {
            break;
        }
    }
    if out.is_empty()
        && let Some(err) = lines.iter().find(|l| !l.trim().is_empty())
    {
        return Probe::na(format!("systemctl: {}", clip(err.trim(), 200)));
    }
    Probe::ok("systemd", out)
}

fn is_state_word(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

fn openrc(lines: &[&str]) -> Vec<Service> {
    let mut out: Vec<Service> = Vec::new();
    for line in lines {
        let Some((name, rest)) = line.split_once('[') else {
            continue;
        };
        let name = name.trim();
        let state = rest.trim_end().trim_end_matches(']').trim();
        if name.is_empty() || name.contains(' ') || name.starts_with('*') {
            continue;
        }
        let (active, sub) = match state {
            "started" => ("active", "running"),
            "crashed" => ("failed", "crashed"),
            "failed" => ("failed", "failed"),
            "starting" => ("activating", "starting"),
            "stopping" => ("deactivating", "stopping"),
            "inactive" => ("inactive", "inactive"),
            _ => ("inactive", "stopped"),
        };
        if out.iter().any(|s| s.unit == name) {
            continue;
        }
        out.push(Service {
            unit: clip(name, 256),
            load: "loaded".into(),
            active: active.into(),
            sub: sub.into(),
            description: String::new(),
        });
        if out.len() >= MAX_SERVICES {
            break;
        }
    }
    out
}

/// Enabled units (`systemctl list-unit-files --state=enabled`) or OpenRC
/// services with at least one runlevel (`rc-update show -v`).
pub fn enabled_units(text: &str) -> Probe<Vec<String>> {
    let mut lines = text.lines();
    let src = lines
        .next()
        .and_then(|l| l.strip_prefix("#src "))
        .unwrap_or("")
        .trim();
    let mut out = Vec::new();
    match src {
        "systemd" => {
            let body: Vec<&str> = lines.take(20_000).collect();
            for line in &body {
                let mut it = line.split_ascii_whitespace();
                if let (Some(unit), Some("enabled")) = (it.next(), it.next()) {
                    out.push(clip(unit, 256));
                }
            }
            if out.is_empty()
                && let Some(err) = body.iter().find(|l| !l.trim().is_empty())
            {
                return Probe::na(format!("systemctl: {}", clip(err.trim(), 200)));
            }
        }
        "openrc" => {
            for line in lines.take(20_000) {
                if let Some((name, levels)) = line.split_once('|') {
                    let levels = levels
                        .split_ascii_whitespace()
                        .collect::<Vec<_>>()
                        .join(",");
                    let name = name.trim();
                    if !levels.is_empty() && !name.is_empty() {
                        out.push(clip(&format!("{name}@{levels}"), 256));
                    }
                }
            }
        }
        "none" => return Probe::na("no supported service manager (systemd or OpenRC) found"),
        _ => return Probe::na("unit data not collected"),
    }
    out.sort();
    out.dedup();
    out.truncate(MAX_SERVICES);
    Probe::ok(src, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_units() {
        let text = "#src systemd\n\
auditd.service                       not-found inactive dead    auditd.service\n\
broken.service                       loaded    failed   failed  Deliberately failing unit\n\
● old.service                        loaded    failed   failed  Old style bullet\n\
cron.service                         loaded    active   running Regular background program processing daemon\n";
        let Probe::Ok { data, .. } = services(text) else {
            panic!()
        };
        assert_eq!(data.len(), 3);
        assert!(data[0].is_failed());
        assert_eq!(data[1].unit, "old.service");
        assert_eq!(
            data[2].description,
            "Regular background program processing daemon"
        );
    }

    #[test]
    fn systemd_error_is_na() {
        let p = services("#src systemd\nFailed to connect to bus: No such file or directory\n");
        assert_eq!(
            p,
            Probe::na("systemctl: Failed to connect to bus: No such file or directory")
        );
    }

    #[test]
    fn openrc_status() {
        let text = "#src openrc\nRunlevel: default\n sshd                [  started  ]\n crond   [  crashed  ]\n nginx [ stopped ]\nDynamic Runlevel: manual\n";
        let Probe::Ok { data, .. } = services(text) else {
            panic!()
        };
        assert_eq!(data.len(), 3);
        assert_eq!(data[0].active, "active");
        assert!(data[1].is_failed());
    }

    #[test]
    fn enabled_units_both_managers() {
        let Probe::Ok { data, .. } = enabled_units(
            "#src systemd\ncron.service enabled enabled\nnginx.service enabled disabled\n",
        ) else {
            panic!()
        };
        assert_eq!(data, ["cron.service", "nginx.service"]);
        let Probe::Ok { data, .. } = enabled_units(
            "#src openrc\n   acpid |  \n   sshd | default \n  crond | boot default\n",
        ) else {
            panic!()
        };
        assert_eq!(data, ["crond@boot,default", "sshd@default"]);
    }
}
