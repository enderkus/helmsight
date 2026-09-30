//! Per-process data from `cat /proc/[0-9]*/stat` and `ps`.

use crate::util::clip;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStat {
    pub pid: u32,
    pub comm: String,
    pub state: char,
    pub ppid: u32,
    /// utime + stime in clock ticks.
    pub cpu_ticks: u64,
    pub threads: u32,
    pub start_time: u64,
    pub rss_pages: u64,
}

const MAX_PROCS: usize = 100_000;

/// Parses concatenated /proc/<pid>/stat records. `comm` may contain spaces,
/// parentheses and even newlines, so records are anchored on the last `)`.
pub fn procstat(text: &str) -> Vec<ProcStat> {
    let mut out = Vec::new();
    let mut pending = String::new();
    for line in text.lines() {
        if out.len() >= MAX_PROCS {
            break;
        }
        if !pending.is_empty() {
            pending.push('\n');
            pending.push_str(line);
            if pending.len() > 4096 {
                pending.clear();
                continue;
            }
            if let Some(p) = parse_one(&pending) {
                out.push(p);
                pending.clear();
            }
            continue;
        }
        match parse_one(line) {
            Some(p) => out.push(p),
            None if line.contains('(') && !line.contains(')') => pending = line.to_string(),
            None => {}
        }
    }
    out
}

fn parse_one(line: &str) -> Option<ProcStat> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }
    let pid: u32 = line.get(..open)?.trim().parse().ok()?;
    let comm = line.get(open + 1..close)?;
    let rest: Vec<&str> = line.get(close + 1..)?.split_ascii_whitespace().collect();
    if rest.len() < 22 {
        return None;
    }
    let n = |i: usize| rest.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let state = rest.first()?.chars().next().unwrap_or('?');
    Some(ProcStat {
        pid,
        comm: clip(comm, 64),
        state,
        ppid: u32::try_from(n(1)).unwrap_or(0),
        cpu_ticks: n(11).saturating_add(n(12)),
        threads: u32::try_from(n(17)).unwrap_or(0),
        start_time: n(19),
        rss_pages: n(21),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsEntry {
    pub user: String,
    pub command: String,
}

/// Parses `ps -o pid,user,args` (procps or BusyBox), with or without header.
pub fn ps(text: &str) -> BTreeMap<u32, PsEntry> {
    let mut out = BTreeMap::new();
    for line in text.lines().take(MAX_PROCS) {
        let line = line.trim_start();
        let Some((pid, rest)) = line.split_once(|c: char| c.is_ascii_whitespace()) else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        let rest = rest.trim_start();
        let (user, cmd) = rest
            .split_once(|c: char| c.is_ascii_whitespace())
            .unwrap_or((rest, ""));
        out.insert(
            pid,
            PsEntry {
                user: clip(user, 64),
                command: clip(cmd.trim(), 512),
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "1234 (my (odd) proc) S 1 1234 1234 0 -1 4194560 100 0 0 0 150 50 0 0 20 0 3 0 5000 100000 2560 18446744073709551615";

    #[test]
    fn parses_tricky_comm() {
        let p = procstat(LINE);
        assert_eq!(p.len(), 1);
        let p = &p[0];
        assert_eq!(p.pid, 1234);
        assert_eq!(p.comm, "my (odd) proc");
        assert_eq!(p.state, 'S');
        assert_eq!(p.cpu_ticks, 200);
        assert_eq!(p.threads, 3);
        assert_eq!(p.start_time, 5000);
        assert_eq!(p.rss_pages, 2560);
    }

    #[test]
    fn joins_comm_with_newline() {
        let text = "7 (evil\nname) R 1 1 1 0 -1 0 0 0 0 0 1 1 0 0 20 0 1 0 1 1 1\n8 (ok) S 1 1 1 0 -1 0 0 0 0 0 1 1 0 0 20 0 1 0 1 1 1\n";
        let p = procstat(text);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].comm, "evil name");
    }

    #[test]
    fn ps_formats() {
        let busybox =
            "PID   USER     COMMAND\n    1 root     sh -s\n   26 www-data ps -o pid,user,args\n";
        let m = ps(busybox);
        assert_eq!(m.len(), 2);
        assert_eq!(m[&26].user, "www-data");
        assert_eq!(m[&26].command, "ps -o pid,user,args");
        let procps = "      1 root                             /sbin/init\n     33 root                             sshd: /usr/sbin/sshd -D [listener]\n";
        let m = ps(procps);
        assert_eq!(m[&33].command, "sshd: /usr/sbin/sshd -D [listener]");
    }
}
