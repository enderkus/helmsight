//! System information: os-release, uname, CPU model, virtualization,
//! reboot status, sessions and basic facts.

use crate::model::{Basics, Kernel, OsRelease, RebootStatus, Session};
use crate::util::{clip, clip_opt, key_values};

pub fn os_release(text: &str) -> OsRelease {
    let kv = key_values(text);
    let get = |k: &str| {
        kv.get(k)
            .map(|v| unquote(v))
            .and_then(|v| clip_opt(&v, 256))
    };
    OsRelease {
        id: get("ID"),
        id_like: get("ID_LIKE"),
        name: get("NAME"),
        version_id: get("VERSION_ID"),
        pretty_name: get("PRETTY_NAME"),
    }
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    let inner = v
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(v);
    inner.replace("\\\"", "\"").replace("\\\\", "\\")
}

pub fn kernel(text: &str) -> Kernel {
    let kv = key_values(text);
    let get = |k: &str| kv.get(k).and_then(|v| clip_opt(v, 256));
    Kernel {
        name: get("kernel_name"),
        release: get("kernel_release"),
        version: get("kernel_version"),
        machine: get("machine"),
    }
}

/// CPU model from `lscpu` (preferred) or the first model line of /proc/cpuinfo.
pub fn cpu_model(text: &str) -> Option<String> {
    let mut from_cpuinfo = None;
    let mut lscpu_model = None;
    let mut vendor = None;
    for line in text.lines().take(64) {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        if v.is_empty() || v == "-" {
            continue;
        }
        match k.trim() {
            "Model name" => lscpu_model = Some(v),
            "Vendor ID" => vendor = Some(v),
            "model name" | "Hardware" | "cpu model" | "Processor" => {
                from_cpuinfo = from_cpuinfo.or(Some(v));
            }
            _ => {}
        }
    }
    lscpu_model
        .or(from_cpuinfo)
        .or(vendor)
        .and_then(|v| clip_opt(v, 256))
}

pub fn virtualization(text: &str) -> Option<String> {
    let kv = key_values(text);
    let get = |k: &str| {
        kv.get(k)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty() && *v != "none")
    };
    if let Some(c) = get("detect_container") {
        return clip_opt(&format!("container ({c})"), 64);
    }
    if kv.contains_key("dockerenv") {
        return Some("container (docker)".into());
    }
    if kv.contains_key("containerenv") {
        return Some("container (podman)".into());
    }
    if let Some(v) = get("detect_virt") {
        return clip_opt(v, 64);
    }
    let product = get("dmi_product").unwrap_or("");
    let vendor = get("dmi_vendor").unwrap_or("");
    let known = [
        ("KVM", "kvm"),
        ("QEMU", "qemu"),
        ("VirtualBox", "oracle"),
        ("VMware", "vmware"),
        ("Virtual Machine", "microsoft"),
        ("HVM domU", "xen"),
        ("Google Compute Engine", "google"),
        ("Amazon EC2", "amazon"),
        ("Droplet", "kvm"),
    ];
    for (needle, name) in known {
        if product.contains(needle) || vendor.contains(needle) {
            return Some(name.to_string());
        }
    }
    if kv.contains_key("cpu_hypervisor") {
        return Some("vm".into());
    }
    if kv.contains_key("detect_virt") || !product.is_empty() {
        return Some("none".into());
    }
    None
}

pub fn reboot(text: &str) -> RebootStatus {
    let mut reasons = Vec::new();
    let mut checked = false;
    let mut pkgs = Vec::new();
    let mut running: Option<String> = None;
    let mut rpm_kernel: Option<String> = None;
    for line in text.lines().take(512) {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k {
            "flag" => reasons.push(match v {
                "reboot-required" => "/var/run/reboot-required exists".to_string(),
                "reboot-needed" => "/run/reboot-needed exists".to_string(),
                other => clip(other, 128),
            }),
            "pkg" if pkgs.len() < 50 => pkgs.push(clip(v, 128)),
            "modules" => {
                checked = true;
                if v == "missing" {
                    reasons.push("running kernel has no module directory (kernel upgraded)".into());
                }
            }
            "running" if !v.is_empty() => running = Some(v.to_string()),
            "rpm_kernel" if !v.is_empty() => rpm_kernel = Some(v.to_string()),
            _ => {}
        }
    }
    if let (Some(run), Some(pkg)) = (&running, &rpm_kernel) {
        let newest = ["kernel-core-", "kernel-"]
            .iter()
            .find_map(|p| pkg.strip_prefix(p))
            .unwrap_or(pkg);
        checked = true;
        if newest != run {
            reasons.push(clip(
                &format!("newest installed kernel {newest} is not running (running {run})"),
                256,
            ));
        }
    }
    if !pkgs.is_empty() {
        reasons.push(format!("packages: {}", pkgs.join(", ")));
    }
    let required = if !reasons.is_empty() {
        Some(true)
    } else if checked {
        Some(false)
    } else {
        None
    };
    RebootStatus { required, reasons }
}

/// Parses `who` output (coreutils or BusyBox).
pub fn sessions(text: &str) -> Vec<Session> {
    let mut out = Vec::new();
    for line in text.lines().take(500) {
        let mut it = line.split_ascii_whitespace();
        let (Some(user), Some(tty)) = (it.next(), it.next()) else {
            continue;
        };
        let rest: Vec<&str> = it.collect();
        let (from, login): (Option<String>, Vec<&str>) = match rest
            .last()
            .filter(|l| l.starts_with('(') && l.ends_with(')'))
        {
            Some(last) => (
                clip_opt(last.trim_start_matches('(').trim_end_matches(')'), 128),
                rest.get(..rest.len().saturating_sub(1))
                    .unwrap_or(&[])
                    .to_vec(),
            ),
            None => (None, rest),
        };
        out.push(Session {
            user: clip(user, 64),
            line: clip(tty, 64),
            from,
            login: clip(&login.join(" "), 64),
        });
    }
    out
}

pub fn basics(text: &str) -> Basics {
    let kv = key_values(text);
    Basics {
        page_size: kv
            .get("pagesize")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| v.is_power_of_two() && (512..=1 << 20).contains(v)),
        hostname: kv.get("hostname").and_then(|v| clip_opt(v, 256)),
        user: kv.get("user").and_then(|v| clip_opt(v, 64)),
        uid: kv.get("uid").and_then(|v| v.parse().ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_quotes() {
        let o = os_release(
            "PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\nNAME='Debian'\nVERSION_ID=\"12\"\nID=debian\n",
        );
        assert_eq!(
            o.pretty_name.as_deref(),
            Some("Debian GNU/Linux 12 (bookworm)")
        );
        assert_eq!(o.name.as_deref(), Some("Debian"));
        assert_eq!(o.id.as_deref(), Some("debian"));
        assert_eq!(o.id_like, None);
    }

    #[test]
    fn cpu_model_sources() {
        assert_eq!(
            cpu_model("model name\t: Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz\n").as_deref(),
            Some("Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz")
        );
        assert_eq!(
            cpu_model("Vendor ID:                               Apple\nModel name:                              -\n")
                .as_deref(),
            Some("Apple")
        );
        assert_eq!(cpu_model(""), None);
    }

    #[test]
    fn virtualization_detection() {
        assert_eq!(
            virtualization("detect_virt=kvm\ndetect_container=none\n").as_deref(),
            Some("kvm")
        );
        assert_eq!(
            virtualization("dockerenv=1\ndmi_vendor=\ndmi_product=\n").as_deref(),
            Some("container (docker)")
        );
        assert_eq!(
            virtualization("dmi_vendor=QEMU\ndmi_product=Standard PC\n").as_deref(),
            Some("qemu")
        );
        assert_eq!(
            virtualization("dmi_vendor=Dell Inc.\ndmi_product=PowerEdge R640\n").as_deref(),
            Some("none")
        );
    }

    #[test]
    fn reboot_signals() {
        assert_eq!(reboot("").required, None);
        assert_eq!(reboot("modules=ok\n").required, Some(false));
        let r = reboot("flag=reboot-required\npkg=linux-image-6.1.0-26-amd64\nmodules=ok\n");
        assert_eq!(r.required, Some(true));
        assert_eq!(r.reasons.len(), 2);
        let el = "running=5.14.0-427.el9.x86_64\nrpm_kernel=kernel-core-5.14.0-503.el9.x86_64\n";
        assert_eq!(reboot(el).required, Some(true));
        let el = "running=5.14.0-503.el9.x86_64\nrpm_kernel=kernel-core-5.14.0-503.el9.x86_64\n";
        assert_eq!(reboot(el).required, Some(false));
    }

    #[test]
    fn who_formats() {
        let s = sessions(
            "root     pts/0        2026-09-30 17:40 (10.0.0.1)\nalice tty1 2026-09-30 08:00\n",
        );
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].from.as_deref(), Some("10.0.0.1"));
        assert_eq!(s[0].login, "2026-09-30 17:40");
        assert_eq!(s[1].from, None);
    }

    #[test]
    fn basics_validates_page_size() {
        assert_eq!(basics("pagesize=65536\n").page_size, Some(65536));
        assert_eq!(basics("pagesize=3\n").page_size, None);
        assert_eq!(
            basics("uid=1000\nuser=monitor\n").user.as_deref(),
            Some("monitor")
        );
    }
}
