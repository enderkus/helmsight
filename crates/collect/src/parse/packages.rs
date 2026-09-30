//! Installed packages from dpkg, rpm or apk.

use crate::model::{Package, Probe};
use crate::util::clip;

const MAX_PACKAGES: usize = 50_000;

pub fn packages(text: &str) -> Probe<Vec<Package>> {
    let mut lines = text.lines();
    let src = lines
        .next()
        .and_then(|l| l.strip_prefix("#src "))
        .unwrap_or("")
        .trim();
    let mut out = Vec::new();
    for line in lines.take(MAX_PACKAGES * 2) {
        let pkg = match src {
            "dpkg" => dpkg_line(line),
            "rpm" => rpm_line(line),
            "apk" => apk_line(line),
            _ => None,
        };
        if let Some(p) = pkg {
            out.push(p);
            if out.len() >= MAX_PACKAGES {
                break;
            }
        }
    }
    match src {
        "dpkg" | "rpm" | "apk" => {
            if out.is_empty() {
                return Probe::na(format!("{src} returned no packages"));
            }
            out.sort();
            out.dedup();
            Probe::ok(src, out)
        }
        "none" => Probe::na("no supported package manager (dpkg, rpm, apk) found"),
        _ => Probe::na("package data not collected"),
    }
}

fn dpkg_line(line: &str) -> Option<Package> {
    let mut f = line.split('\t');
    let status = f.next()?;
    // Second status character is the current state; 'i' means installed.
    if status.as_bytes().get(1) != Some(&b'i') {
        return None;
    }
    let name = f.next()?.trim();
    let version = f.next()?.trim();
    let arch = f.next().map(str::trim).filter(|a| !a.is_empty());
    valid(name, version).then(|| Package {
        name: clip(name, 128),
        version: clip(version, 128),
        arch: arch.map(|a| clip(a, 32)),
    })
}

fn rpm_line(line: &str) -> Option<Package> {
    let mut f = line.split('\t');
    let name = f.next()?.trim();
    let evr = f.next()?.trim();
    let arch = f
        .next()
        .map(str::trim)
        .filter(|a| !a.is_empty() && *a != "(none)");
    let version = evr.strip_prefix("(none):").unwrap_or(evr);
    valid(name, version).then(|| Package {
        name: clip(name, 128),
        version: clip(version, 128),
        arch: arch.map(|a| clip(a, 32)),
    })
}

/// Splits `name-version-rN` as printed by `apk info -v`.
fn apk_line(line: &str) -> Option<Package> {
    let line = line.trim();
    if line.starts_with("WARNING") || line.contains(' ') {
        return None;
    }
    let (rest, rel) = line.rsplit_once('-')?;
    if !rel.starts_with('r')
        || !rel
            .get(1..)
            .is_some_and(|r| r.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let bytes = rest.as_bytes();
    let idx = (1..bytes.len()).rev().find(|&i| {
        bytes.get(i - 1) == Some(&b'-') && bytes.get(i).is_some_and(u8::is_ascii_digit)
    })?;
    let name = rest.get(..idx - 1)?;
    let version = format!("{}-{rel}", rest.get(idx..)?);
    valid(name, &version).then(|| Package {
        name: clip(name, 128),
        version: clip(&version, 128),
        arch: None,
    })
}

fn valid(name: &str, version: &str) -> bool {
    !name.is_empty()
        && !version.is_empty()
        && name.len() <= 256
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpkg() {
        let Probe::Ok { data, .. } = packages(
            "#src dpkg\nii \tadduser\t3.134\tall\nrc \told-thing\t1.0\tamd64\nhi \tlibc6\t2.36-9+deb12u8\tarm64\n",
        ) else {
            panic!()
        };
        assert_eq!(data.len(), 2);
        assert_eq!(data[1].name, "libc6");
        assert_eq!(data[1].arch.as_deref(), Some("arm64"));
    }

    #[test]
    fn rpm() {
        let Probe::Ok { data, .. } = packages(
            "#src rpm\nlibgcc\t(none):11.4.1-2.1.el9\taarch64\nxz\t1:5.8.1-4.fc41\taarch64\ngpg-pubkey\t(none):abc-def\t(none)\n",
        ) else {
            panic!()
        };
        assert_eq!(data[1].version, "11.4.1-2.1.el9");
        assert_eq!(data[2].version, "1:5.8.1-4.fc41");
        assert_eq!(data[0].arch, None);
    }

    #[test]
    fn apk() {
        let Probe::Ok { data, .. } = packages(
            "#src apk\nmusl-1.2.5-r0\nca-certificates-bundle-20240705-r0\nlibssl3-3.3.2-r0\npy3-foo-2.0-r1\nWARNING: bad\nbroken\n",
        ) else {
            panic!()
        };
        let pairs: Vec<(&str, &str)> = data
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect();
        assert!(pairs.contains(&("musl", "1.2.5-r0")));
        assert!(pairs.contains(&("ca-certificates-bundle", "20240705-r0")));
        assert!(pairs.contains(&("libssl3", "3.3.2-r0")));
        assert!(pairs.contains(&("py3-foo", "2.0-r1")));
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn missing_manager() {
        assert!(!packages("#src none\n").is_ok());
        assert!(!packages("").is_ok());
        assert!(!packages("#src rpm\n").is_ok());
    }
}
