//! Pending package updates from apt, dnf/yum, zypper or apk.

use crate::model::{PendingUpdate, Probe, UpdateReport};
use crate::util::clip;
use std::collections::BTreeSet;

const MAX_UPDATES: usize = 10_000;

/// Parses the `updates` section. `now` (unix seconds) is used to describe
/// the age of package lists.
pub fn updates(text: &str, now: i64) -> Probe<UpdateReport> {
    let mut lines = text.lines();
    let src = lines
        .next()
        .and_then(|l| l.strip_prefix("#src "))
        .unwrap_or("")
        .trim()
        .to_string();
    let mut main: Vec<&str> = Vec::new();
    let mut security: Vec<&str> = Vec::new();
    let mut in_security = false;
    let mut lists: Option<(u64, i64)> = None;
    for line in lines.take(50_000) {
        if line == "#security" {
            in_security = true;
        } else if let Some(rest) = line.strip_prefix("#lists ") {
            let mut it = rest.split_ascii_whitespace();
            let n = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let m = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            lists = Some((n, m));
        } else if in_security {
            security.push(line);
        } else {
            main.push(line);
        }
    }
    let result = match src.as_str() {
        "apt" => apt(&main, lists, now),
        "dnf" | "yum" => dnf(&src, &main, &security),
        "zypper" => zypper(&main, &security),
        "apk" => apk(&main),
        "dnf-disabled" => Err(
            "dnf writes log files on the host; set `collect.dnf_updates = true` to allow this check"
                .to_string(),
        ),
        "none" => Err("no supported package manager found".to_string()),
        _ => Err("update data not collected".to_string()),
    };
    match result {
        Ok(mut r) => {
            r.packages.truncate(MAX_UPDATES);
            r.total = u32::try_from(r.packages.len()).unwrap_or(u32::MAX);
            Probe::ok(src, r)
        }
        Err(reason) => Probe::na(reason),
    }
}

fn first_error<'a>(lines: &[&'a str], prefixes: &[&str]) -> Option<&'a str> {
    lines
        .iter()
        .map(|l| l.trim())
        .find(|l| prefixes.iter().any(|p| l.starts_with(p)))
}

fn apt(lines: &[&str], lists: Option<(u64, i64)>, now: i64) -> Result<UpdateReport, String> {
    if let Some((0, _)) = lists {
        return Err(
            "apt package lists are empty; run `apt-get update` (e.g. via apt-daily.timer)".into(),
        );
    }
    let mut packages = Vec::new();
    for line in lines {
        let Some(rest) = line.strip_prefix("Inst ") else {
            continue;
        };
        // Inst name [current] (available origin [arch])
        let mut it = rest.splitn(2, ' ');
        let Some(name) = it.next() else { continue };
        let rest = it.next().unwrap_or("");
        let current = rest
            .strip_prefix('[')
            .and_then(|r| r.split_once(']'))
            .map(|(c, _)| clip(c, 128));
        let Some(paren) = rest.find('(').and_then(|i| rest.get(i + 1..)) else {
            continue;
        };
        let inner = paren.split(')').next().unwrap_or("");
        let mut parts = inner.split_ascii_whitespace();
        let available = parts.next().unwrap_or("");
        let origin = parts.next().map(|o| clip(o, 128));
        let is_security = inner.to_ascii_lowercase().contains("security");
        if name.is_empty() || available.is_empty() {
            continue;
        }
        packages.push(PendingUpdate {
            name: clip(name, 128),
            current,
            available: clip(available, 128),
            security: is_security,
            repo: origin,
        });
    }
    if packages.is_empty()
        && let Some(e) = first_error(lines, &["E: "])
    {
        return Err(format!("apt: {}", clip(e, 200)));
    }
    let mut notes = Vec::new();
    if let Some((_, mtime)) = lists
        && mtime > 0
        && now > mtime
    {
        let days = (now - mtime) / 86_400;
        if days >= 2 {
            notes.push(format!("package lists last refreshed {days} days ago"));
        }
    }
    let security = packages.iter().filter(|p| p.security).count();
    Ok(UpdateReport {
        total: 0,
        security: Some(u32::try_from(security).unwrap_or(u32::MAX)),
        packages,
        notes,
    })
}

/// Strips `-version-release.arch` from an RPM NEVRA.
fn rpm_name(nevra: &str) -> Option<&str> {
    let (without_arch, _) = nevra.rsplit_once('.')?;
    let (without_rel, _) = without_arch.rsplit_once('-')?;
    let (name, _) = without_rel.rsplit_once('-')?;
    Some(name)
}

fn dnf(tool: &str, main: &[&str], security: &[&str]) -> Result<UpdateReport, String> {
    let mut packages: Vec<PendingUpdate> = Vec::new();
    let mut carry: Option<String> = None;
    for line in main {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("Obsoleting") || t.starts_with("Security:") {
            break;
        }
        let toks: Vec<&str> = t.split_ascii_whitespace().collect();
        // Long package names wrap onto the next line.
        let toks: Vec<String> = match (carry.take(), toks.len()) {
            (None, 1) if !line.starts_with(' ') => {
                carry = toks.first().map(|s| s.to_string());
                continue;
            }
            (Some(c), 2) => std::iter::once(c)
                .chain(toks.iter().map(|s| s.to_string()))
                .collect(),
            (_, _) => toks.iter().map(|s| s.to_string()).collect(),
        };
        let [name_arch, version, repo] = toks.as_slice() else {
            continue;
        };
        let Some((name, _arch)) = name_arch.rsplit_once('.') else {
            continue;
        };
        if name.is_empty()
            || !version.bytes().next().is_some_and(|b| b.is_ascii_digit())
            || name.contains(':')
        {
            continue;
        }
        packages.push(PendingUpdate {
            name: clip(name, 128),
            current: None,
            available: clip(version, 128),
            security: false,
            repo: Some(clip(repo, 128)),
        });
    }
    if packages.is_empty()
        && let Some(e) = first_error(main, &["Error:", "Failed", "Cache-only"])
    {
        let hint = if e.contains("Cache-only") || e.contains("no cache") {
            "; the metadata cache is empty, enable dnf-makecache.timer"
        } else {
            ""
        };
        return Err(format!("{tool}: {}{hint}", clip(e, 200)));
    }
    let mut sec_names: BTreeSet<String> = BTreeSet::new();
    let mut security_ok = true;
    for line in security {
        let toks: Vec<&str> = line.split_ascii_whitespace().collect();
        if toks.first().is_some_and(|t| t.starts_with("Error")) {
            security_ok = false;
            break;
        }
        // dnf4: ADVISORY SEVERITY/Sec. NEVRA
        // dnf5: ADVISORY security SEVERITY NEVRA ISSUED
        let nevra = match toks.as_slice() {
            [_, sev, nevra] if sev.ends_with("/Sec.") => Some(*nevra),
            [_, kind, _, nevra, ..] if *kind == "security" => Some(*nevra),
            _ => None,
        };
        if let Some(name) = nevra.and_then(rpm_name) {
            sec_names.insert(name.to_string());
        }
    }
    for p in &mut packages {
        p.security = sec_names.contains(&p.name);
    }
    let sec = packages.iter().filter(|p| p.security).count();
    Ok(UpdateReport {
        total: 0,
        security: security_ok.then(|| u32::try_from(sec).unwrap_or(u32::MAX)),
        packages,
        notes: Vec::new(),
    })
}

fn table_rows<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    lines
        .iter()
        .filter(|l| l.contains('|'))
        .map(|l| l.split('|').map(str::trim).collect::<Vec<_>>())
        .filter(|cols| cols.len() >= 5)
        .collect()
}

fn zypper(main: &[&str], security: &[&str]) -> Result<UpdateReport, String> {
    let mut notes: Vec<String> = main
        .iter()
        .filter(|l| l.starts_with("Warning:"))
        .take(5)
        .map(|l| clip(l, 200))
        .collect();
    let rows = table_rows(main);
    let mut packages = Vec::new();
    // S | Repository | Name | Current Version | Available Version | Arch
    for cols in rows {
        let (Some(status), Some(repo), Some(name), Some(cur), Some(avail)) = (
            cols.first(),
            cols.get(1),
            cols.get(2),
            cols.get(3),
            cols.get(4),
        ) else {
            continue;
        };
        if *status == "S" || name.is_empty() || *name == "Name" {
            continue;
        }
        packages.push(PendingUpdate {
            name: clip(name, 128),
            current: Some(clip(cur, 128)),
            available: clip(avail, 128),
            security: false,
            repo: Some(clip(repo, 128)),
        });
    }
    if packages.is_empty()
        && let Some(e) = first_error(
            main,
            &["Root privileges", "Error", "System management is locked"],
        )
    {
        return Err(format!("zypper: {}", clip(e, 200)));
    }
    // Repository | Name | Category | Severity | Interactive | Status | Summary
    let patches = table_rows(security)
        .into_iter()
        .filter(|c| {
            c.get(2) == Some(&"security") && c.get(5).is_some_and(|s| s.starts_with("needed"))
        })
        .count();
    if patches > 0 {
        notes.push(format!("{patches} security patches needed"));
    }
    Ok(UpdateReport {
        total: 0,
        security: Some(u32::try_from(patches).unwrap_or(u32::MAX)),
        packages,
        notes,
    })
}

fn apk(lines: &[&str]) -> Result<UpdateReport, String> {
    let no_index = lines
        .iter()
        .any(|l| l.starts_with("WARNING") && (l.contains("No such file") || l.contains("cache")));
    let mut packages = Vec::new();
    for line in lines {
        // name-1.2-r0  < 1.3-r0
        let Some((installed, available)) = line.split_once('<') else {
            continue;
        };
        let installed = installed.trim();
        let available = available.trim();
        if installed.is_empty() || available.is_empty() || installed.starts_with("Installed") {
            continue;
        }
        let (name, current) = split_apk(installed);
        packages.push(PendingUpdate {
            name: clip(name, 128),
            current: current.map(|c| clip(c, 128)),
            available: clip(available, 128),
            security: false,
            repo: None,
        });
    }
    if packages.is_empty() && no_index {
        return Err("apk package index is not cached; run `apk update`".into());
    }
    Ok(UpdateReport {
        total: 0,
        security: None,
        packages,
        notes: Vec::new(),
    })
}

fn split_apk(s: &str) -> (&str, Option<&str>) {
    let Some((rest, _rel)) = s.rsplit_once('-') else {
        return (s, None);
    };
    let bytes = rest.as_bytes();
    let idx = (1..bytes.len())
        .rev()
        .find(|&i| bytes.get(i - 1) == Some(&b'-') && bytes.get(i).is_some_and(u8::is_ascii_digit));
    match idx {
        Some(i) => (s.get(..i - 1).unwrap_or(s), s.get(i..)),
        None => (s, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_simulation() {
        let text = "#src apt\n#lists 8 1790000000\n\
Inst tzdata [2026b-0+deb12u1] (2026c-0+deb12u1 Debian-Security:12/oldstable-security [all])\n\
Inst curl [7.88.1-10+deb12u7] (7.88.1-10+deb12u8 Debian:12.7/stable [arm64]) []\n\
Inst linux-image-6.1.0-27-amd64 (6.1.115-1 Debian:12.8/stable [amd64])\n";
        let Probe::Ok { data, .. } = updates(text, 1_790_787_000) else {
            panic!()
        };
        assert_eq!(data.total, 3);
        assert_eq!(data.security, Some(1));
        assert_eq!(data.packages[0].current.as_deref(), Some("2026b-0+deb12u1"));
        assert_eq!(data.packages[2].current, None);
        assert_eq!(data.notes, ["package lists last refreshed 9 days ago"]);
    }

    #[test]
    fn apt_without_lists() {
        assert!(!updates("#src apt\n#lists 0 0\n", 0).is_ok());
    }

    #[test]
    fn dnf_with_security() {
        let text = "#src dnf\n\n\
curl-minimal.aarch64                     7.76.1-40.el9_8.7                baseos\n\
a-very-long-package-name-that-wraps.x86_64\n\
                                         1.0-1.el9                        appstream\n\
bash.aarch64                             5.1.8-9.el9                      baseos\n\
#security\n\
RLSA-2026:69126 Important/Sec. curl-minimal-7.76.1-40.el9_8.7.aarch64\n\
RLSA-2025:0925  Moderate/Sec.  bzip2-libs-1.0.8-10.el9_5.aarch64\n";
        let Probe::Ok { data, .. } = updates(text, 0) else {
            panic!()
        };
        assert_eq!(data.total, 3, "{data:#?}");
        assert_eq!(data.security, Some(1));
        assert!(data.packages[0].security);
        assert_eq!(data.packages[1].name, "a-very-long-package-name-that-wraps");
    }

    #[test]
    fn dnf5_security_format() {
        let text = "#src dnf\nxz.aarch64      1:5.8.1-4.fc41 updates\n#security\n\
Name                 Type     Severity  Package               Issued\n\
FEDORA-2026-abc      security Moderate  xz-1:5.8.1-4.fc41.aarch64 2026-09-01 00:00:00\n";
        let Probe::Ok { data, .. } = updates(text, 0) else {
            panic!()
        };
        assert_eq!(data.security, Some(1));
    }

    #[test]
    fn dnf_disabled_explains_why() {
        let Probe::Na { reason } = updates("#src dnf-disabled\n", 0) else {
            panic!()
        };
        assert!(reason.contains("collect.dnf_updates"));
    }

    #[test]
    fn dnf_without_cache() {
        let Probe::Na { reason } = updates(
            "#src dnf\nError: Cache-only enabled but no cache for 'baseos'\n#security\n",
            0,
        ) else {
            panic!()
        };
        assert!(reason.contains("dnf-makecache.timer"));
    }

    #[test]
    fn zypper_tables() {
        let text = "#src zypper\n\
S | Repository | Name | Current Version | Available Version | Arch\n\
--+------------+------+-----------------+-------------------+-------\n\
v | Main Update | curl | 8.0.1-1.1 | 8.0.1-2.1 | x86_64\n\
#security\n\
Repository | Name | Category | Severity | Interactive | Status | Summary\n\
Main Update | openSUSE-2026-1 | security | important | --- | needed | Security update for curl\n\
Main Update | openSUSE-2026-2 | recommended | low | --- | needed | Bugfix\n";
        let Probe::Ok { data, .. } = updates(text, 0) else {
            panic!()
        };
        assert_eq!(data.total, 1);
        assert_eq!(data.security, Some(1));
        assert_eq!(data.packages[0].current.as_deref(), Some("8.0.1-1.1"));
    }

    #[test]
    fn apk_versions() {
        let text = "#src apk\nInstalled:                                Available:\nmusl-1.2.5-r0                           < 1.2.5-r1\n";
        let Probe::Ok { data, .. } = updates(text, 0) else {
            panic!()
        };
        assert_eq!(data.total, 1);
        assert_eq!(data.packages[0].name, "musl");
        assert_eq!(data.packages[0].current.as_deref(), Some("1.2.5-r0"));
        assert_eq!(data.security, None);
        let text = "#src apk\nWARNING: opening from cache https://x/main: No such file or directory\nInstalled:  Available:\n";
        assert!(!updates(text, 0).is_ok());
    }
}
