//! Turns raw section text into [`Collection`]s, keeping the previous counters
//! of a host so that rates (CPU %, bytes/s, ...) can be computed.

use crate::model::*;
use crate::parse::{
    auth, containers, fs, listen, packages, procfs, procs, services, system, updates,
};
use crate::script::{Group, ScriptRequest};
use crate::sections::Sections;
use crate::util::{pct, round2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

const TOP_PROCESSES: usize = 15;

/// Everything parsed from one script execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Collection {
    /// The script ran to completion.
    pub complete: bool,
    /// Sections that were requested but are absent from the output.
    pub missing: Vec<String>,
    pub metrics: Option<Metrics>,
    pub medium: Option<Medium>,
    pub inventory: Option<Inventory>,
    pub auth: Option<Probe<Vec<AuthFailure>>>,
    pub updates: Option<Probe<UpdateReport>>,
    pub basics: Option<Basics>,
}

impl Collection {
    /// True when some requested data is missing or the output was cut short.
    pub fn is_partial(&self) -> bool {
        !self.complete || !self.missing.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
struct Previous {
    uptime: Option<f64>,
    mono: f64,
    stat: procfs::Stat,
    disks: BTreeMap<String, procfs::DiskCounters>,
    net: BTreeMap<String, procfs::NetCounters>,
    /// pid -> (start time, cpu ticks)
    procs: HashMap<u32, (u64, u64)>,
}

/// Per-host parsing state.
#[derive(Debug, Clone, Default)]
pub struct Sampler {
    prev: Option<Previous>,
    basics: Basics,
}

impl Sampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets previous counters, e.g. after a reconnect.
    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// Parses one execution. `mono` is a monotonic clock reading in seconds
    /// (used when the remote uptime is unavailable) and `now` the current
    /// unix time in seconds.
    pub fn ingest(&mut self, raw: &str, req: &ScriptRequest, mono: f64, now: i64) -> Collection {
        let sec = Sections::split(raw, &req.marker());
        let mut out = Collection {
            complete: sec.complete,
            ..Collection::default()
        };
        let wants = |g: Group| req.groups.contains(&g);
        let require = |names: &[&str], out: &mut Collection| {
            for n in names {
                if !sec.has(n) {
                    out.missing.push((*n).to_string());
                }
            }
        };

        if wants(Group::Basics) {
            require(&["basics"], &mut out);
            if let Some(t) = sec.get("basics") {
                self.basics = system::basics(t);
                out.basics = Some(self.basics.clone());
            }
        }
        if wants(Group::Metrics) {
            require(
                &[
                    "stat",
                    "meminfo",
                    "loadavg",
                    "uptime",
                    "diskstats",
                    "netdev",
                    "df",
                ],
                &mut out,
            );
            if sec.has("stat") || sec.has("meminfo") {
                out.metrics = Some(self.metrics(&sec, mono));
            }
        }
        if wants(Group::Medium) {
            require(&["listen", "services", "containers"], &mut out);
            if sec.has("listen") || sec.has("services") {
                out.medium = Some(Medium {
                    listening: listen::listening(sec.get("listen").unwrap_or("")),
                    services: services::services(sec.get("services").unwrap_or("")),
                    containers: containers::containers(sec.get("containers").unwrap_or("")),
                    sessions: system::sessions(sec.get("who").unwrap_or("")),
                });
            }
        }
        if wants(Group::Inventory) {
            require(&["osrelease", "uname", "packages", "units"], &mut out);
            if sec.has("osrelease") || sec.has("uname") {
                out.inventory = Some(Inventory {
                    os: system::os_release(sec.get("osrelease").unwrap_or("")),
                    kernel: system::kernel(sec.get("uname").unwrap_or("")),
                    cpu_model: system::cpu_model(sec.get("cpuinfo").unwrap_or("")),
                    virtualization: system::virtualization(sec.get("virt").unwrap_or("")),
                    reboot: system::reboot(sec.get("reboot").unwrap_or("")),
                    packages: packages::packages(sec.get("packages").unwrap_or("")),
                    enabled_units: services::enabled_units(sec.get("units").unwrap_or("")),
                });
            }
        }
        if wants(Group::Auth) {
            require(&["auth"], &mut out);
            if let Some(t) = sec.get("auth") {
                out.auth = Some(auth::auth_failures(t, req.auth_since));
            }
        }
        if wants(Group::Updates) {
            require(&["updates"], &mut out);
            if let Some(t) = sec.get("updates") {
                out.updates = Some(updates::updates(t, now));
            }
        }
        out
    }

    fn metrics(&mut self, sec: &Sections, mono: f64) -> Metrics {
        let stat = procfs::stat(sec.get("stat").unwrap_or(""));
        let mem = procfs::meminfo(sec.get("meminfo").unwrap_or(""));
        let uptime = procfs::uptime(sec.get("uptime").unwrap_or(""));
        let disks = procfs::diskstats(sec.get("diskstats").unwrap_or(""));
        let net = procfs::netdev(sec.get("netdev").unwrap_or(""));
        let proc_list = procs::procstat(sec.get("procstat").unwrap_or(""));
        let ps = procs::ps(sec.get("ps").unwrap_or(""));
        let mount_types = fs::mounts(sec.get("mounts").unwrap_or(""));

        let prev = self.prev.take();
        let elapsed = prev.as_ref().and_then(|p| {
            let by_uptime = match (p.uptime, uptime) {
                (Some(a), Some(b)) if b > a => Some(b - a),
                _ => None,
            };
            by_uptime
                .or(Some(mono - p.mono))
                .filter(|e| e.is_finite() && *e > 0.05)
        });
        // A decreasing uptime means the host rebooted: counters restarted.
        let rebooted = matches!(
            (prev.as_ref().and_then(|p| p.uptime), uptime),
            (Some(a), Some(b)) if b < a
        );
        let prev = if rebooted { None } else { prev };

        let cpu = prev.as_ref().and_then(|p| cpu_metrics(&p.stat, &stat));

        let mut disk_io = Vec::new();
        let mut net_io = Vec::new();
        if let (Some(p), Some(dt)) = (prev.as_ref(), elapsed) {
            for (name, c) in &disks {
                let Some(o) = p.disks.get(name) else { continue };
                let d = |a: u64, b: u64| a.checked_sub(b).map(|v| v as f64 / dt);
                let (Some(r), Some(w), Some(rs), Some(ws), Some(io)) = (
                    d(c.reads, o.reads),
                    d(c.writes, o.writes),
                    d(c.read_sectors, o.read_sectors),
                    d(c.write_sectors, o.write_sectors),
                    d(c.io_ms, o.io_ms),
                ) else {
                    continue;
                };
                disk_io.push(DiskIo {
                    device: name.clone(),
                    read_bps: round2(rs * 512.0),
                    write_bps: round2(ws * 512.0),
                    read_iops: round2(r),
                    write_iops: round2(w),
                    util_pct: round2((io / 10.0).clamp(0.0, 100.0)),
                });
            }
            for (name, c) in &net {
                let Some(o) = p.net.get(name) else { continue };
                let d = |a: u64, b: u64| a.checked_sub(b).map(|v| round2(v as f64 / dt));
                let (Some(rx), Some(tx), Some(rxp), Some(txp)) = (
                    d(c.rx_bytes, o.rx_bytes),
                    d(c.tx_bytes, o.tx_bytes),
                    d(c.rx_packets, o.rx_packets),
                    d(c.tx_packets, o.tx_packets),
                ) else {
                    continue;
                };
                net_io.push(NetIface {
                    name: name.clone(),
                    rx_bps: rx,
                    tx_bps: tx,
                    rx_pps: rxp,
                    tx_pps: txp,
                    rx_errors: d(c.rx_errors, o.rx_errors).unwrap_or(0.0),
                    tx_errors: d(c.tx_errors, o.tx_errors).unwrap_or(0.0),
                    rx_drops: d(c.rx_drops, o.rx_drops).unwrap_or(0.0),
                    tx_drops: d(c.tx_drops, o.tx_drops).unwrap_or(0.0),
                });
            }
        }

        // Jiffies elapsed per core between samples, for per-process CPU %.
        let per_core_jiffies = match (prev.as_ref().and_then(|p| p.stat.total), stat.total) {
            (Some(a), Some(b)) => {
                let cores = stat.cores.len().max(1) as f64;
                b.total()
                    .checked_sub(a.total())
                    .map(|d| d as f64 / cores)
                    .filter(|d| *d > 0.0)
            }
            _ => None,
        };
        let procs = (!proc_list.is_empty()).then(|| {
            self.process_summary(
                &proc_list,
                &ps,
                prev.as_ref(),
                per_core_jiffies,
                mem.as_ref(),
                &stat,
            )
        });

        let filesystems = fs::filesystems(
            sec.get("df").unwrap_or(""),
            sec.get("dfi").unwrap_or(""),
            &mount_types,
        );

        let metrics = Metrics {
            remote_time: sec.get("time").and_then(|t| t.trim().parse().ok()),
            cpu,
            mem,
            load: procfs::loadavg(sec.get("loadavg").unwrap_or("")),
            uptime_secs: uptime,
            boot_time: stat.boot_time,
            filesystems,
            disks: disk_io,
            net: net_io,
            tcp: procfs::tcp_states(sec.get("tcp").unwrap_or("")),
            procs,
        };

        self.prev = Some(Previous {
            uptime,
            mono,
            procs: proc_list
                .iter()
                .map(|p| (p.pid, (p.start_time, p.cpu_ticks)))
                .collect(),
            stat,
            disks,
            net,
        });
        metrics
    }

    fn process_summary(
        &self,
        list: &[procs::ProcStat],
        ps: &BTreeMap<u32, procs::PsEntry>,
        prev: Option<&Previous>,
        per_core_jiffies: Option<f64>,
        mem: Option<&Memory>,
        stat: &procfs::Stat,
    ) -> ProcSummary {
        let page = self.basics.page_size.unwrap_or(4096);
        let mem_total = mem.map(|m| m.total as f64).unwrap_or(0.0);
        let mut all: Vec<Process> = list
            .iter()
            .map(|p| {
                let cpu_pct = match (prev, per_core_jiffies) {
                    (Some(prev), Some(j)) => prev
                        .procs
                        .get(&p.pid)
                        .filter(|(start, _)| *start == p.start_time)
                        .and_then(|(_, ticks)| p.cpu_ticks.checked_sub(*ticks))
                        .map(|d| round2(d as f64 / j * 100.0)),
                    _ => None,
                };
                let rss = p.rss_pages.saturating_mul(page);
                let entry = ps.get(&p.pid);
                Process {
                    pid: p.pid,
                    name: p.comm.clone(),
                    user: entry.map(|e| e.user.clone()),
                    command: entry.map(|e| e.command.clone()).filter(|c| !c.is_empty()),
                    state: p.state.to_string(),
                    threads: p.threads,
                    rss_bytes: rss,
                    cpu_pct,
                    mem_pct: (mem_total > 0.0).then(|| round2(pct(rss as f64, mem_total))),
                }
            })
            .collect();
        let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
        let running = stat.procs_running.unwrap_or_else(|| {
            u32::try_from(list.iter().filter(|p| p.state == 'R').count()).unwrap_or(0)
        });
        let blocked = stat.procs_blocked.unwrap_or_else(|| {
            u32::try_from(list.iter().filter(|p| p.state == 'D').count()).unwrap_or(0)
        });

        all.sort_by_key(|p| std::cmp::Reverse(p.rss_bytes));
        let top_mem: Vec<Process> = all.iter().take(TOP_PROCESSES).cloned().collect();
        let top_cpu: Vec<Process> = if per_core_jiffies.is_some() {
            all.sort_by(|a, b| {
                b.cpu_pct
                    .unwrap_or(0.0)
                    .total_cmp(&a.cpu_pct.unwrap_or(0.0))
                    .then(b.rss_bytes.cmp(&a.rss_bytes))
            });
            all.into_iter().take(TOP_PROCESSES).collect()
        } else {
            Vec::new()
        };
        ProcSummary {
            total,
            running,
            blocked,
            top_cpu,
            top_mem,
        }
    }
}

fn cpu_metrics(prev: &procfs::Stat, cur: &procfs::Stat) -> Option<CpuMetrics> {
    let total = breakdown(prev.total.as_ref()?, cur.total.as_ref()?)?;
    let per_core = cur
        .cores
        .iter()
        .map(|(i, c)| {
            prev.cores
                .get(i)
                .and_then(|p| breakdown(p, c))
                .map(|b| b.busy)
                .unwrap_or(0.0)
        })
        .collect::<Vec<_>>();
    Some(CpuMetrics {
        cores: u32::try_from(cur.cores.len().max(1)).unwrap_or(1),
        total,
        per_core,
    })
}

fn breakdown(a: &procfs::CpuTimes, b: &procfs::CpuTimes) -> Option<CpuBreakdown> {
    let dt = b.total().checked_sub(a.total())? as f64;
    if dt <= 0.0 {
        return None;
    }
    let d = |x: u64, y: u64| round2(pct(y.saturating_sub(x) as f64, dt));
    let idle = d(a.idle, b.idle);
    let iowait = d(a.iowait, b.iowait);
    Some(CpuBreakdown {
        user: d(a.user, b.user),
        nice: d(a.nice, b.nice),
        system: d(a.system, b.system),
        iowait,
        irq: d(a.irq, b.irq),
        softirq: d(a.softirq, b.softirq),
        steal: d(a.steal, b.steal),
        idle,
        busy: round2((100.0 - idle - iowait).clamp(0.0, 100.0)),
    })
}
