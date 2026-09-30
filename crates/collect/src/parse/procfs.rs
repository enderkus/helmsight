//! Parsers for kernel files under /proc.

use crate::model::{LoadAvg, Memory, TcpSummary};
use crate::util::{clip, pct};
use std::collections::BTreeMap;

/// Raw CPU jiffy counters of one `cpu` line of /proc/stat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    /// Sum of all accounted states. Guest time is already part of user time.
    pub fn total(&self) -> u64 {
        self.user
            .saturating_add(self.nice)
            .saturating_add(self.system)
            .saturating_add(self.idle)
            .saturating_add(self.iowait)
            .saturating_add(self.irq)
            .saturating_add(self.softirq)
            .saturating_add(self.steal)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stat {
    pub total: Option<CpuTimes>,
    pub cores: BTreeMap<u32, CpuTimes>,
    pub boot_time: Option<i64>,
    pub procs_running: Option<u32>,
    pub procs_blocked: Option<u32>,
}

const MAX_CORES: u32 = 4096;

pub fn stat(text: &str) -> Stat {
    let mut out = Stat::default();
    for line in text.lines().take(20_000) {
        let mut it = line.split_ascii_whitespace();
        let Some(key) = it.next() else { continue };
        if let Some(idx) = key.strip_prefix("cpu") {
            let nums: Vec<u64> = it.take(10).map_while(|v| v.parse().ok()).collect();
            if nums.len() < 4 {
                continue;
            }
            let get = |i: usize| nums.get(i).copied().unwrap_or(0);
            let t = CpuTimes {
                user: get(0),
                nice: get(1),
                system: get(2),
                idle: get(3),
                iowait: get(4),
                irq: get(5),
                softirq: get(6),
                steal: get(7),
            };
            if idx.is_empty() {
                out.total = Some(t);
            } else if let Ok(n) = idx.parse::<u32>()
                && n < MAX_CORES
            {
                out.cores.insert(n, t);
            }
            continue;
        }
        let val = it.next();
        match key {
            "btime" => out.boot_time = val.and_then(|v| v.parse().ok()),
            "procs_running" => out.procs_running = val.and_then(|v| v.parse().ok()),
            "procs_blocked" => out.procs_blocked = val.and_then(|v| v.parse().ok()),
            _ => {}
        }
    }
    out
}

pub fn meminfo(text: &str) -> Option<Memory> {
    let mut m: BTreeMap<&str, u64> = BTreeMap::new();
    for line in text.lines().take(1000) {
        let Some((k, rest)) = line.split_once(':') else {
            continue;
        };
        let mut parts = rest.split_ascii_whitespace();
        let Some(v) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
            continue;
        };
        let v = match parts.next() {
            Some("kB") => v.saturating_mul(1024),
            _ => v,
        };
        m.entry(k.trim()).or_insert(v);
    }
    let total = *m.get("MemTotal")?;
    if total == 0 {
        return None;
    }
    let free = m.get("MemFree").copied().unwrap_or(0);
    let buffers = m.get("Buffers").copied().unwrap_or(0);
    let cached = m
        .get("Cached")
        .copied()
        .unwrap_or(0)
        .saturating_add(m.get("SReclaimable").copied().unwrap_or(0));
    let available = m
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| free.saturating_add(buffers).saturating_add(cached))
        .min(total);
    let used = total.saturating_sub(available);
    let swap_total = m.get("SwapTotal").copied().unwrap_or(0);
    let swap_free = m.get("SwapFree").copied().unwrap_or(0).min(swap_total);
    let swap_used = swap_total.saturating_sub(swap_free);
    Some(Memory {
        total,
        free,
        available,
        used,
        buffers,
        cached,
        used_pct: pct(used as f64, total as f64),
        swap_total,
        swap_used,
        swap_used_pct: pct(swap_used as f64, swap_total as f64),
    })
}

pub fn loadavg(text: &str) -> Option<LoadAvg> {
    let mut it = text.split_ascii_whitespace();
    let mut f = || {
        it.next()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    let one = f()?;
    let five = f()?;
    let fifteen = f()?;
    let (runnable, entities) = text
        .split_ascii_whitespace()
        .nth(3)
        .and_then(|v| v.split_once('/'))
        .map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0)))
        .unwrap_or((0, 0));
    Some(LoadAvg {
        one: one.max(0.0),
        five: five.max(0.0),
        fifteen: fifteen.max(0.0),
        runnable,
        entities,
    })
}

pub fn uptime(text: &str) -> Option<f64> {
    text.split_ascii_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}

/// Counters of one block device from /proc/diskstats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiskCounters {
    pub reads: u64,
    pub read_sectors: u64,
    pub writes: u64,
    pub write_sectors: u64,
    pub io_ms: u64,
}

const MAX_DEVICES: usize = 512;

/// Parses /proc/diskstats, keeping whole disks and device-mapper devices and
/// skipping partitions and virtual devices.
pub fn diskstats(text: &str) -> BTreeMap<String, DiskCounters> {
    let mut all = BTreeMap::new();
    for line in text.lines().take(10_000) {
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        if f.len() < 14 {
            continue;
        }
        let Some(name) = f.get(2) else { continue };
        let n = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let c = DiskCounters {
            reads: n(3),
            read_sectors: n(5),
            writes: n(7),
            write_sectors: n(9),
            io_ms: n(12),
        };
        if all.len() < MAX_DEVICES * 4 {
            all.insert(clip(name, 64), c);
        }
    }
    let names: Vec<String> = all.keys().cloned().collect();
    all.retain(|name, c| {
        let virt = ["loop", "ram", "fd", "sr", "zram", "nbd"]
            .iter()
            .any(|p| name.starts_with(p));
        let idle = c.reads == 0 && c.writes == 0;
        !virt && !idle && !is_partition(name, &names)
    });
    while all.len() > MAX_DEVICES {
        all.pop_last();
    }
    all
}

fn is_partition(name: &str, all: &[String]) -> bool {
    all.iter().any(|parent| {
        parent != name
            && name.starts_with(parent.as_str())
            && name.get(parent.len()..).is_some_and(|rest| {
                let digits = rest.strip_prefix('p').unwrap_or(rest);
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            })
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetCounters {
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub rx_errors: u64,
    pub rx_drops: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub tx_errors: u64,
    pub tx_drops: u64,
}

const MAX_IFACES: usize = 128;

/// Parses /proc/net/dev, skipping loopback and veth pairs.
pub fn netdev(text: &str) -> BTreeMap<String, NetCounters> {
    let mut out = BTreeMap::new();
    for line in text.lines().take(10_000) {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.contains(' ') || name == "lo" || name.starts_with("veth") {
            continue;
        }
        let f: Vec<u64> = rest
            .split_ascii_whitespace()
            .map_while(|v| v.parse().ok())
            .collect();
        if f.len() < 12 {
            continue;
        }
        let n = |i: usize| f.get(i).copied().unwrap_or(0);
        if out.len() < MAX_IFACES {
            out.insert(
                clip(name, 64),
                NetCounters {
                    rx_bytes: n(0),
                    rx_packets: n(1),
                    rx_errors: n(2),
                    rx_drops: n(3),
                    tx_bytes: n(8),
                    tx_packets: n(9),
                    tx_errors: n(10),
                    tx_drops: n(11),
                },
            );
        }
    }
    out
}

/// Parses `<hex state> <count>` lines produced by the awk summary of
/// /proc/net/tcp and /proc/net/tcp6.
pub fn tcp_states(text: &str) -> Option<TcpSummary> {
    let mut s = TcpSummary::default();
    let mut seen = false;
    for line in text.lines().take(64) {
        let mut it = line.split_ascii_whitespace();
        let (Some(state), Some(count)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(count) = count.parse::<u64>() else {
            continue;
        };
        seen = true;
        match state.to_ascii_uppercase().as_str() {
            "01" => s.established = s.established.saturating_add(count),
            "0A" => s.listen = s.listen.saturating_add(count),
            "06" => s.time_wait = s.time_wait.saturating_add(count),
            "08" => s.close_wait = s.close_wait.saturating_add(count),
            _ => s.other = s.other.saturating_add(count),
        }
        s.total = s.total.saturating_add(count);
    }
    seen.then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stat() {
        let s = stat(
            "cpu  100 5 50 1000 20 1 2 3 0 0\ncpu0 50 2 25 500 10 0 1 1 0 0\ncpu1 50 3 25 500 10 1 1 2 0 0\n\
             intr 1 2 3\nctxt 1234\nbtime 1700000000\nprocesses 99\nprocs_running 3\nprocs_blocked 1\n",
        );
        let t = s.total.unwrap();
        assert_eq!(t.user, 100);
        assert_eq!(t.steal, 3);
        assert_eq!(t.total(), 1181);
        assert_eq!(s.cores.len(), 2);
        assert_eq!(s.boot_time, Some(1_700_000_000));
        assert_eq!(s.procs_running, Some(3));
    }

    #[test]
    fn parses_old_kernel_stat_with_fewer_fields() {
        let s = stat("cpu 1 2 3 4\n");
        assert_eq!(s.total.unwrap().iowait, 0);
    }

    #[test]
    fn meminfo_without_memavailable() {
        let m = meminfo("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 250 kB\n")
            .unwrap();
        assert_eq!(m.available, 400 * 1024);
        assert_eq!(m.used, 600 * 1024);
        assert!((m.used_pct - 60.0).abs() < 1e-9);
        assert_eq!(m.swap_used_pct, 0.0);
    }

    #[test]
    fn loadavg_parses() {
        let l = loadavg("0.52 0.58 0.59 2/613 12345\n").unwrap();
        assert_eq!(l.one, 0.52);
        assert_eq!(l.runnable, 2);
        assert_eq!(l.entities, 613);
        assert!(loadavg("nan nan nan").is_none());
    }

    #[test]
    fn diskstats_skips_partitions_and_loops() {
        let d = diskstats(
            "   8       0 sda 100 0 800 10 50 0 400 20 0 30 30 0 0 0 0\n\
                8       1 sda1 90 0 700 10 50 0 400 20 0 30 30 0 0 0 0\n\
              259       0 nvme0n1 10 0 80 1 5 0 40 2 0 3 3\n\
              259       1 nvme0n1p1 10 0 80 1 5 0 40 2 0 3 3\n\
                7       0 loop0 10 0 80 1 5 0 40 2 0 3 3\n\
              253       0 dm-0 10 0 80 1 5 0 40 2 0 3 3\n\
                8      16 sdb 0 0 0 0 0 0 0 0 0 0 0\n",
        );
        let names: Vec<&str> = d.keys().map(String::as_str).collect();
        assert_eq!(names, ["dm-0", "nvme0n1", "sda"]);
        assert_eq!(d["sda"].read_sectors, 800);
        assert_eq!(d["sda"].io_ms, 30);
    }

    #[test]
    fn netdev_parses_joined_numbers() {
        let n = netdev(
            "Inter-|   Receive                                                |  Transmit\n \
             face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    \
             lo: 1 2 0 0 0 0 0 0 1 2 0 0 0 0 0 0\n  \
             eth0:12345678901 100 1 2 0 0 0 0 999 50 3 4 0 0 0 0\n",
        );
        assert_eq!(n.len(), 1);
        assert_eq!(n["eth0"].rx_bytes, 12_345_678_901);
        assert_eq!(n["eth0"].tx_drops, 4);
    }

    #[test]
    fn tcp_state_counts() {
        let t = tcp_states("01 10\n0A 4\n06 2\n08 1\n02 1\n").unwrap();
        assert_eq!(t.established, 10);
        assert_eq!(t.listen, 4);
        assert_eq!(t.total, 18);
        assert!(tcp_states("").is_none());
    }
}
