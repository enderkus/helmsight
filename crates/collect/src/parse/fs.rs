//! Filesystem usage from `df -Pk`, `df -Pi` and /proc/mounts.

use crate::model::Filesystem;
use crate::util::{clip, pct};
use std::collections::BTreeMap;

const MAX_FILESYSTEMS: usize = 256;

/// Filesystem types that never represent persistent storage.
const PSEUDO_FS: &[&str] = &[
    "autofs",
    "binfmt_misc",
    "bpf",
    "cgroup",
    "cgroup2",
    "configfs",
    "debugfs",
    "devfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fusectl",
    "fuse.gvfsd-fuse",
    "fuse.lxcfs",
    "fuse.portal",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "proc",
    "pstore",
    "ramfs",
    "rpc_pipefs",
    "securityfs",
    "squashfs",
    "sysfs",
    "tmpfs",
    "tracefs",
];

const PSEUDO_DEVICES: &[&str] = &["tmpfs", "devtmpfs", "udev", "shm", "none", "proc", "sysfs"];

/// Maps mount point to filesystem type, decoding octal escapes.
pub fn mounts(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines().take(20_000) {
        let mut it = line.split_ascii_whitespace();
        let (Some(_dev), Some(mnt), Some(fstype)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        // Later entries shadow earlier ones mounted at the same place.
        out.insert(unescape_octal(mnt), clip(fstype, 32));
    }
    out
}

fn unescape_octal(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'\\'
            && let Some(oct) = bytes.get(i + 1..i + 4)
            && oct.iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = oct
                .iter()
                .fold(0u32, |acc, c| acc * 8 + u32::from(c - b'0'));
            if let Ok(v) = u8::try_from(v) {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    clip(&String::from_utf8_lossy(&out), 1024)
}

#[derive(Debug, Clone, PartialEq)]
struct DfRow {
    device: String,
    a: u64,
    b: u64,
    c: u64,
    mount: String,
}

/// Parses POSIX `df -P` output. Device and mount names may contain spaces;
/// the row is anchored on the four numeric columns.
fn df_rows(text: &str) -> Vec<DfRow> {
    let mut out = Vec::new();
    for line in text.lines().take(20_000) {
        let toks: Vec<&str> = line.split_ascii_whitespace().collect();
        if toks.first() == Some(&"Filesystem") {
            continue;
        }
        let Some(i) = (1..toks.len()).find(|&i| {
            let num = |k: usize| toks.get(k).is_some_and(|t| t.parse::<u64>().is_ok());
            num(i)
                && num(i + 1)
                && num(i + 2)
                && toks
                    .get(i + 3)
                    .is_some_and(|t| t.ends_with('%') || *t == "-")
                && i + 4 < toks.len()
        }) else {
            continue;
        };
        let n = |k: usize| toks.get(k).and_then(|t| t.parse::<u64>().ok()).unwrap_or(0);
        let device = toks.get(..i).map(|t| t.join(" ")).unwrap_or_default();
        let mount = toks.get(i + 4..).map(|t| t.join(" ")).unwrap_or_default();
        out.push(DfRow {
            device: clip(&device, 256),
            a: n(i),
            b: n(i + 1),
            c: n(i + 2),
            mount: clip(&mount, 1024),
        });
    }
    out
}

/// Combines block and inode usage with mount types. Pseudo filesystems,
/// bind-mounted files and duplicates are removed.
pub fn filesystems(
    df_k: &str,
    df_i: &str,
    mount_types: &BTreeMap<String, String>,
) -> Vec<Filesystem> {
    let inodes: BTreeMap<String, (u64, u64)> = df_rows(df_i)
        .into_iter()
        .map(|r| (r.mount, (r.a, r.b)))
        .collect();
    let mut out: Vec<Filesystem> = Vec::new();
    for r in df_rows(df_k) {
        let fstype = mount_types.get(&r.mount).cloned();
        let t = fstype.as_deref().unwrap_or("");
        if PSEUDO_FS.contains(&t) || (t == "overlay" && r.mount != "/") {
            continue;
        }
        if fstype.is_none() && PSEUDO_DEVICES.contains(&r.device.as_str()) {
            continue;
        }
        // Files bind-mounted by container runtimes.
        let runtime_file =
            ["/etc/hosts", "/etc/hostname", "/etc/resolv.conf"].contains(&r.mount.as_str());
        if r.a == 0
            || runtime_file
            || [
                "/proc",
                "/sys",
                "/dev",
                "/run",
                "/snap/",
                "/var/lib/docker/",
                "/var/lib/containers/",
            ]
            .iter()
            .any(|p| r.mount.starts_with(p))
        {
            continue;
        }
        let total = r.a.saturating_mul(1024);
        let used = r.b.saturating_mul(1024);
        let avail = r.c.saturating_mul(1024);
        // Same device and figures mounted twice (bind mounts, btrfs
        // subvolumes): keep the shortest mount path.
        if let Some(existing) = out
            .iter_mut()
            .find(|f| f.device == r.device && f.total == total && f.used == used)
        {
            if r.mount.len() < existing.mount.len() {
                existing.mount = r.mount.clone();
            }
            continue;
        }
        let (it, iu) = inodes.get(&r.mount).copied().unwrap_or((0, 0));
        let has_inodes = it > 0;
        // Match `df`: used / (used + available), which excludes reserved blocks.
        let used_pct = pct(used as f64, used.saturating_add(avail) as f64);
        out.push(Filesystem {
            device: r.device,
            mount: r.mount,
            fstype,
            total,
            used,
            avail,
            used_pct,
            inodes_total: has_inodes.then_some(it),
            inodes_used: has_inodes.then_some(iu),
            inodes_used_pct: has_inodes.then(|| pct(iu as f64, it as f64)),
        });
        if out.len() >= MAX_FILESYSTEMS {
            break;
        }
    }
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DF: &str = "Filesystem     1024-blocks      Used Available Capacity Mounted on
/dev/sda1         41152736  20576368  18463224      53% /
tmpfs              4046452         0   4046452       0% /dev/shm
/dev/sdb1        103081248  92773080   5048456      95% /var/lib/my data
/dev/sda1         41152736  20576368  18463224      53% /etc/hostname
/dev/sda2         41152736  20576368  18463224      53% /etc/resolv.conf
/dev/loop0           56064     56064         0     100% /snap/core18/1
";
    const DFI: &str = "Filesystem      Inodes  IUsed   IFree IUse% Mounted on
/dev/sda1      2621440 300000 2321440   12% /
/dev/sdb1      6553600 100 6553500    1% /var/lib/my data
";
    const MOUNTS: &str = "/dev/sda1 / ext4 rw,relatime 0 0
tmpfs /dev/shm tmpfs rw 0 0
/dev/sdb1 /var/lib/my\\040data xfs rw 0 0
/dev/sda1 /etc/hostname ext4 rw 0 0
/dev/loop0 /snap/core18/1 squashfs ro 0 0
";

    #[test]
    fn filters_and_joins() {
        let fs = filesystems(DF, DFI, &mounts(MOUNTS));
        assert_eq!(fs.len(), 2, "{fs:#?}");
        let root = &fs[0];
        assert_eq!(root.mount, "/");
        assert_eq!(root.fstype.as_deref(), Some("ext4"));
        assert_eq!(root.inodes_total, Some(2_621_440));
        let data = &fs[1];
        assert_eq!(data.mount, "/var/lib/my data");
        assert_eq!(data.fstype.as_deref(), Some("xfs"));
        assert!((data.used_pct - 94.84).abs() < 0.01);
    }

    #[test]
    fn keeps_overlay_root_in_containers() {
        let df =
            "Filesystem 1024-blocks Used Available Capacity Mounted on\noverlay 100 50 50 50% /\n";
        let fs = filesystems(df, "", &mounts("overlay / overlay rw 0 0\n"));
        assert_eq!(fs.len(), 1);
    }
}
