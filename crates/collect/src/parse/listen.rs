//! Listening sockets from `ss`, `netstat` or raw /proc/net tables.

use crate::model::{ListenSocket, Probe};
use crate::util::clip;
use std::net::{Ipv4Addr, Ipv6Addr};

const MAX_SOCKETS: usize = 4096;

pub fn listening(text: &str) -> Probe<Vec<ListenSocket>> {
    let mut lines = text.lines();
    let src = lines
        .next()
        .and_then(|l| l.strip_prefix("#src "))
        .unwrap_or("")
        .trim()
        .to_string();
    let body: Vec<&str> = lines.take(50_000).collect();
    let mut socks = match src.as_str() {
        "ss" => from_ss(&body),
        "netstat" => from_netstat(&body),
        "proc" => from_proc(&body),
        _ => return Probe::na("no socket listing tool available"),
    };
    socks.sort();
    socks.dedup();
    socks.truncate(MAX_SOCKETS);
    if socks.is_empty() && body.iter().all(|l| l.trim().is_empty()) && src != "proc" {
        return Probe::na(format!("{src} returned no output"));
    }
    Probe::ok(src, socks)
}

fn split_addr(s: &str) -> Option<(String, u16)> {
    let (addr, port) = s.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    let addr = addr.trim_start_matches('[').trim_end_matches(']');
    let addr = if addr.is_empty() { "*" } else { addr };
    Some((clip(addr, 64), port))
}

fn from_ss(lines: &[&str]) -> Vec<ListenSocket> {
    let mut out = Vec::new();
    for line in lines {
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        let (Some(proto), Some(state), Some(local)) = (f.first(), f.get(1), f.get(4)) else {
            continue;
        };
        let proto = match *proto {
            "tcp" | "udp" => *proto,
            _ => continue,
        };
        if *proto == *"tcp" && *state != "LISTEN" {
            continue;
        }
        let Some((address, port)) = split_addr(local) else {
            continue;
        };
        let (process, pid) = f
            .iter()
            .skip(6)
            .find_map(|t| t.strip_prefix("users:"))
            .map(ss_users)
            .unwrap_or((None, None));
        out.push(ListenSocket {
            proto: proto.to_string(),
            address,
            port,
            process,
            pid,
        });
    }
    out
}

/// Extracts the first process name and pid from `(("nginx",pid=1,fd=5),...)`.
fn ss_users(s: &str) -> (Option<String>, Option<u32>) {
    let name = s
        .split('"')
        .nth(1)
        .filter(|n| !n.is_empty())
        .map(|n| clip(n, 64));
    let pid = s
        .split("pid=")
        .nth(1)
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|p| p.parse().ok());
    (name, pid)
}

fn from_netstat(lines: &[&str]) -> Vec<ListenSocket> {
    let mut out = Vec::new();
    for line in lines {
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        let Some(proto) = f.first() else { continue };
        let (base, prog_idx) = match *proto {
            "tcp" | "tcp6" => {
                if f.get(5) != Some(&"LISTEN") {
                    continue;
                }
                ("tcp", 6)
            }
            "udp" | "udp6" => ("udp", 5),
            _ => continue,
        };
        let Some((address, port)) = f.get(3).and_then(|a| split_addr(a)) else {
            continue;
        };
        let (pid, process) = match f.get(prog_idx).and_then(|p| p.split_once('/')) {
            Some((pid, name)) => (pid.parse().ok(), Some(clip(name.trim_end_matches(':'), 64))),
            None => (None, None),
        };
        out.push(ListenSocket {
            proto: base.to_string(),
            address,
            port,
            process: process.filter(|p| !p.is_empty()),
            pid,
        });
    }
    out
}

fn from_proc(lines: &[&str]) -> Vec<ListenSocket> {
    let mut out = Vec::new();
    let mut file = "";
    for line in lines {
        if let Some(f) = line.strip_prefix("#file ") {
            file = f.trim();
            continue;
        }
        let (proto, listen_state, v6) = match file {
            "tcp" => ("tcp", "0A", false),
            "tcp6" => ("tcp", "0A", true),
            "udp" => ("udp", "07", false),
            "udp6" => ("udp", "07", true),
            _ => continue,
        };
        let f: Vec<&str> = line.split_ascii_whitespace().collect();
        let (Some(local), Some(state)) = (f.get(1), f.get(3)) else {
            continue;
        };
        if !state.eq_ignore_ascii_case(listen_state) {
            continue;
        }
        let Some((hex_addr, hex_port)) = local.split_once(':') else {
            continue;
        };
        let Ok(port) = u16::from_str_radix(hex_port, 16) else {
            continue;
        };
        let Some(address) = decode_proc_addr(hex_addr, v6) else {
            continue;
        };
        out.push(ListenSocket {
            proto: proto.to_string(),
            address,
            port,
            process: None,
            pid: None,
        });
    }
    out
}

/// Decodes the host-endian hex address format of /proc/net/{tcp,udp}{,6}.
/// Linux prints each 32-bit word in native (little-endian on all supported
/// architectures) byte order.
fn decode_proc_addr(hex: &str, v6: bool) -> Option<String> {
    let words: Vec<u32> = (0..hex.len() / 8)
        .map(|i| {
            hex.get(i * 8..i * 8 + 8)
                .and_then(|w| u32::from_str_radix(w, 16).ok())
        })
        .collect::<Option<_>>()?;
    match (v6, words.as_slice()) {
        (false, [w]) => Some(Ipv4Addr::from(w.swap_bytes()).to_string()),
        (true, [a, b, c, d]) => {
            let mut bytes = [0u8; 16];
            for (chunk, w) in bytes.chunks_mut(4).zip([a, b, c, d]) {
                chunk.copy_from_slice(&w.to_le_bytes());
            }
            Some(Ipv6Addr::from(bytes).to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ss_with_processes() {
        let text = "#src ss\n\
tcp LISTEN 0      511    0.0.0.0:80 0.0.0.0:* users:((\"nginx\",pid=59,fd=5),(\"nginx\",pid=58,fd=5))\n\
tcp LISTEN 0      128       [::]:22    [::]:* users:((\"sshd\",pid=48,fd=4))\n\
udp UNCONN 0      0      127.0.0.53%lo:53   0.0.0.0:*\n\
tcp ESTAB 0 0 10.0.0.1:22 10.0.0.2:5555\n";
        let Probe::Ok { data, source } = listening(text) else {
            panic!()
        };
        assert_eq!(source, "ss");
        assert_eq!(data.len(), 3);
        let http = data.iter().find(|s| s.port == 80).unwrap();
        assert_eq!(http.process.as_deref(), Some("nginx"));
        assert_eq!(http.pid, Some(59));
        let dns = data.iter().find(|s| s.port == 53).unwrap();
        assert_eq!(dns.address, "127.0.0.53%lo");
        assert_eq!(data.iter().find(|s| s.port == 22).unwrap().address, "::");
    }

    #[test]
    fn busybox_netstat() {
        let text = "#src netstat\nActive Internet connections (only servers)\n\
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name\n\
tcp        0      0 0.0.0.0:22              0.0.0.0:*               LISTEN      1/sshd -D [liste\n\
tcp        0      0 :::22                   :::*                    LISTEN      -\n\
udp        0      0 0.0.0.0:68              0.0.0.0:*                           312/udhcpc\n";
        let Probe::Ok { data, .. } = listening(text) else {
            panic!()
        };
        assert_eq!(data.len(), 3);
        assert!(
            data.iter()
                .any(|s| s.address == "::" && s.process.is_none())
        );
        let udp = data.iter().find(|s| s.proto == "udp").unwrap();
        assert_eq!(udp.process.as_deref(), Some("udhcpc"));
    }

    #[test]
    fn raw_proc_tables() {
        let text = "#src proc\n#file tcp\n  sl  local_address rem_address   st\n\
   0: 0100007F:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000   999        0 1 1\n\
   1: 0100007F:0CEA 0200007F:1234 01 00000000:00000000 00:00000000 00000000   999        0 1 1\n\
#file tcp6\n\
   0: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 0 0 0 0 0 1\n\
   1: 00000000000000000000000001000000:1F90 00000000000000000000000000000000:0000 0A 0 0 0 0 0 1\n";
        let Probe::Ok { data, .. } = listening(text) else {
            panic!()
        };
        let v: Vec<(String, u16)> = data.iter().map(|s| (s.address.clone(), s.port)).collect();
        assert!(v.contains(&("127.0.0.1".into(), 3306)));
        assert!(v.contains(&("::".into(), 22)));
        assert!(v.contains(&("::1".into(), 8080)));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn missing_tool() {
        assert!(!listening("").is_ok());
    }
}
