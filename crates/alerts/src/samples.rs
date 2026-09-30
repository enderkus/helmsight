//! Converts collected data into the metric values alert rules refer to.

use collect::model::{Inventory, Medium, Metrics, Probe, UpdateReport};

/// One value of a rule metric, optionally for an instance (mount, device,
/// interface ...).
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub metric: &'static str,
    pub instance: Option<String>,
    pub value: f64,
}

fn s(metric: &'static str, value: f64) -> Sample {
    Sample {
        metric,
        instance: None,
        value,
    }
}

fn si(metric: &'static str, instance: &str, value: f64) -> Sample {
    Sample {
        metric,
        instance: Some(instance.to_string()),
        value,
    }
}

/// Builds rule samples from the latest data of a host. Missing inputs
/// simply produce no samples.
pub fn samples(
    metrics: Option<&Metrics>,
    medium: Option<&Medium>,
    inventory: Option<&Inventory>,
    updates: Option<&Probe<UpdateReport>>,
    ssh_failed_logins_1h: Option<u64>,
    clock_skew_secs: Option<f64>,
) -> Vec<Sample> {
    let mut out = Vec::new();
    if let Some(m) = metrics {
        if let Some(cpu) = &m.cpu {
            out.push(s("cpu_pct", cpu.total.busy));
            out.push(s("cpu_iowait_pct", cpu.total.iowait));
            out.push(s("cpu_steal_pct", cpu.total.steal));
        }
        if let Some(mem) = &m.mem {
            out.push(s("mem_used_pct", mem.used_pct));
            out.push(s("swap_used_pct", mem.swap_used_pct));
        }
        if let Some(l) = &m.load {
            out.push(s("load1", l.one));
            out.push(s("load5", l.five));
            out.push(s("load15", l.fifteen));
            let cores = m.cpu.as_ref().map(|c| c.cores).unwrap_or(1).max(1);
            out.push(s("load1_per_core", l.one / f64::from(cores)));
        }
        for f in &m.filesystems {
            out.push(si("disk_used_pct", &f.mount, f.used_pct));
            if let Some(p) = f.inodes_used_pct {
                out.push(si("disk_inodes_used_pct", &f.mount, p));
            }
        }
        for d in &m.disks {
            out.push(si("disk_util_pct", &d.device, d.util_pct));
        }
        for n in &m.net {
            out.push(si("net_rx_bytes", &n.name, n.rx_bps));
            out.push(si("net_tx_bytes", &n.name, n.tx_bps));
            out.push(si("net_errors", &n.name, n.rx_errors + n.tx_errors));
        }
        if let Some(t) = &m.tcp {
            out.push(s("tcp_established", t.established as f64));
        }
        if let Some(p) = &m.procs {
            out.push(s("process_count", f64::from(p.total)));
        }
        if let Some(u) = m.uptime_secs {
            out.push(s("uptime_secs", u));
        }
    }
    if let Some(med) = medium {
        if let Probe::Ok { data, .. } = &med.services {
            let failed = data.iter().filter(|s| s.is_failed()).count();
            out.push(s("failed_units", failed as f64));
        }
        if let Probe::Ok { data, .. } = &med.containers {
            let stopped = data.iter().filter(|c| c.state != "running").count();
            out.push(s("containers_not_running", stopped as f64));
        }
    }
    if let Some(inv) = inventory
        && let Some(r) = inv.reboot.required
    {
        out.push(s("reboot_required", if r { 1.0 } else { 0.0 }));
    }
    if let Some(Probe::Ok { data, .. }) = updates {
        out.push(s("pending_updates", f64::from(data.total)));
        if let Some(sec) = data.security {
            out.push(s("pending_security_updates", f64::from(sec)));
        }
    }
    if let Some(n) = ssh_failed_logins_1h {
        out.push(s("ssh_failed_logins_1h", n as f64));
    }
    if let Some(skew) = clock_skew_secs {
        out.push(s("clock_skew_secs", skew.abs()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use collect::model::{Filesystem, Memory};

    #[test]
    fn builds_samples() {
        let m = Metrics {
            mem: Some(Memory {
                used_pct: 42.0,
                ..Default::default()
            }),
            filesystems: vec![Filesystem {
                mount: "/var".into(),
                used_pct: 91.0,
                inodes_used_pct: Some(5.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let v = samples(Some(&m), None, None, None, Some(7), Some(-3.0));
        assert!(v.contains(&s("mem_used_pct", 42.0)));
        assert!(v.contains(&si("disk_used_pct", "/var", 91.0)));
        assert!(v.contains(&s("ssh_failed_logins_1h", 7.0)));
        assert!(v.contains(&s("clock_skew_secs", 3.0)));
        // Every sample refers to a documented metric.
        for x in &v {
            assert!(common::rules::metric(x.metric).is_some(), "{}", x.metric);
        }
    }
}
