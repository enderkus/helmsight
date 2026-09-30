//! Failed SSH login attempts from the journal or syslog files.

use crate::model::{AuthFailure, Probe};
use crate::util::{clip, days_from_civil, key_values, year_of};
use std::collections::BTreeSet;

const MAX_EVENTS: usize = 5000;

#[derive(Debug, Default)]
struct Source<'a> {
    kind: &'a str,
    lines: Vec<&'a str>,
}

/// Parses the `auth` section. Only events strictly after `since` are kept.
pub fn auth_failures(text: &str, since: i64) -> Probe<Vec<AuthFailure>> {
    let head: String = text
        .lines()
        .take_while(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let kv = key_values(&head);
    let tz_offset = kv.get("tz").and_then(|v| parse_tz(v)).unwrap_or(0);
    let now = kv.get("now").and_then(|v| v.parse::<i64>().ok());

    let mut sources: Vec<Source> = Vec::new();
    let mut denied: Vec<String> = Vec::new();
    for line in text.lines().take(100_000) {
        if let Some(rest) = line.strip_prefix("#src ") {
            sources.push(Source {
                kind: rest.trim(),
                lines: Vec::new(),
            });
        } else if let Some(path) = line.strip_prefix("#denied ") {
            denied.push(clip(path.trim(), 128));
        } else if let Some(src) = sources.last_mut()
            && !line.trim().is_empty()
        {
            src.lines.push(line);
        }
    }

    let mut problems = Vec::new();
    for src in &sources {
        if src.kind == "journal" {
            let unreadable = src.lines.iter().any(|l| {
                l.starts_with("Hint:")
                    || l.contains("No journal files")
                    || l.contains("insufficient permissions")
                    || l.contains("not seeing messages")
            });
            if unreadable {
                problems.push(
                    "system journal not readable (add the SSH user to the systemd-journal or adm group)"
                        .to_string(),
                );
                continue;
            }
            let events = parse_lines(&src.lines, since, |l| parse_journal_prefix(l));
            return Probe::ok("journal", events);
        }
        if let Some(path) = src.kind.strip_prefix("file ") {
            let events = parse_lines(&src.lines, since, |l| {
                parse_syslog_prefix(l, tz_offset, now)
            });
            return Probe::ok(clip(path, 128), events);
        }
    }
    for d in denied {
        problems.push(format!("{d} not readable"));
    }
    if problems.is_empty() {
        problems.push("no readable authentication log found".into());
    }
    Probe::na(problems.join("; "))
}

/// Returns (timestamp, rest-of-line after the program tag) for a journal
/// `short-unix` line: `1790787068.760486 host sshd[190]: message`.
fn parse_journal_prefix(line: &str) -> Option<(i64, &str)> {
    let (ts, rest) = line.split_once(' ')?;
    let ts = ts.split('.').next()?.parse::<i64>().ok()?;
    Some((ts, rest))
}

/// Parses syslog timestamps: `Sep 30 16:56:11 host ...` (no year, local
/// time) or RFC 3339 `2026-09-30T16:56:11.123456+00:00 host ...`.
fn parse_syslog_prefix(line: &str, tz_offset: i64, now: Option<i64>) -> Option<(i64, &str)> {
    if line.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        let (stamp, rest) = line.split_once(' ')?;
        return Some((parse_rfc3339(stamp)?, rest));
    }
    let month = match line.get(..3)? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let rest = line.get(3..)?.trim_start();
    let (day, rest) = rest.split_once(' ')?;
    let day: u32 = day.parse().ok()?;
    let (time, rest) = rest.trim_start().split_once(' ')?;
    let secs = parse_hms(time)?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let now = now.unwrap_or(0);
    let mut year = year_of(now + tz_offset);
    let mut ts = days_from_civil(year, month, day) * 86_400 + secs - tz_offset;
    // Logs have no year; a date in the future belongs to the previous year.
    if now > 0 && ts > now + 86_400 {
        year -= 1;
        ts = days_from_civil(year, month, day) * 86_400 + secs - tz_offset;
    }
    Some((ts, rest))
}

fn parse_hms(s: &str) -> Option<i64> {
    let mut it = s.split(':');
    let h: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let sec: i64 = it.next()?.split('.').next()?.parse().ok()?;
    ((0..24).contains(&h) && (0..60).contains(&m) && (0..=60).contains(&sec))
        .then_some(h * 3600 + m * 60 + sec)
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let mo: u32 = d.next()?.parse().ok()?;
    let da: u32 = d.next()?.parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        return None;
    }
    let (clock, offset) = match time.find(['+', '-', 'Z']) {
        Some(i) => (time.get(..i)?, time.get(i..)?),
        None => (time, "Z"),
    };
    let secs = parse_hms(clock)?;
    let off = if offset == "Z" {
        0
    } else {
        parse_tz(&offset.replace(':', ""))?
    };
    Some(days_from_civil(y, mo, da) * 86_400 + secs - off)
}

/// Parses `+0200` / `-0530` into seconds east of UTC.
fn parse_tz(s: &str) -> Option<i64> {
    let s = s.trim();
    let sign = match s.get(..1)? {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hh: i64 = s.get(1..3)?.parse().ok()?;
    let mm: i64 = s.get(3..5)?.parse().ok()?;
    Some(sign * (hh * 3600 + mm * 60))
}

fn parse_lines<'a, F>(lines: &[&'a str], since: i64, prefix: F) -> Vec<AuthFailure>
where
    F: Fn(&'a str) -> Option<(i64, &'a str)>,
{
    struct Raw {
        ts: i64,
        pid: String,
        user: String,
        ip: Option<String>,
        invalid: bool,
        method: String,
        is_invalid_user_line: bool,
    }
    let mut raw = Vec::new();
    for line in lines {
        let Some((ts, rest)) = prefix(line) else {
            continue;
        };
        if ts <= since {
            continue;
        }
        let Some((pid, msg)) = split_sshd_tag(rest) else {
            continue;
        };
        if let Some(r) = msg.strip_prefix("Invalid user ") {
            let (user, ip) = user_and_ip(r);
            raw.push(Raw {
                ts,
                pid,
                user,
                ip,
                invalid: true,
                method: "none".into(),
                is_invalid_user_line: true,
            });
        } else if let Some(r) = msg.strip_prefix("Failed ") {
            let Some((method, r)) = r.split_once(" for ") else {
                continue;
            };
            let (invalid, r) = match r.strip_prefix("invalid user ") {
                Some(r) => (true, r),
                None => (false, r),
            };
            let (user, ip) = user_and_ip(r);
            raw.push(Raw {
                ts,
                pid,
                user,
                ip,
                invalid,
                method: clip(method, 32),
                is_invalid_user_line: false,
            });
        }
    }
    // A connection that tried a password for an unknown user logs both
    // "Invalid user" and "Failed password"; count it once.
    let with_failed: BTreeSet<String> = raw
        .iter()
        .filter(|r| !r.is_invalid_user_line)
        .map(|r| r.pid.clone())
        .collect();
    let mut out: Vec<AuthFailure> = raw
        .into_iter()
        .filter(|r| !(r.is_invalid_user_line && with_failed.contains(&r.pid)))
        .map(|r| AuthFailure {
            ts: r.ts,
            user: r.user,
            ip: r.ip,
            invalid_user: r.invalid,
            method: r.method,
        })
        .collect();
    if out.len() > MAX_EVENTS {
        out.drain(..out.len() - MAX_EVENTS);
    }
    out
}

/// Finds `sshd[123]: ` or `sshd-session[123]: ` and returns (pid, message).
fn split_sshd_tag(rest: &str) -> Option<(String, &str)> {
    let start = rest.find("sshd")?;
    let after = rest.get(start..)?;
    let open = after.find('[')?;
    let close = after.find("]: ")?;
    if close < open {
        return None;
    }
    let pid = after.get(open + 1..close)?;
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((pid.to_string(), after.get(close + 3..)?))
}

/// Splits `<user> from <ip> port <n> ...`. The user name is attacker
/// controlled and may contain spaces, so split on the last " from ".
fn user_and_ip(s: &str) -> (String, Option<String>) {
    match s.rfind(" from ") {
        Some(i) => {
            let user = s.get(..i).unwrap_or("");
            let ip = s
                .get(i + 6..)
                .and_then(|r| r.split_ascii_whitespace().next())
                .filter(|ip| {
                    ip.len() <= 64
                        && ip
                            .chars()
                            .all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':')
                })
                .map(str::to_string);
            (clip(user, 64), ip)
        }
        None => (clip(s.split(' ').next().unwrap_or(""), 64), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOURNAL: &str = "tz=+0000\nnow=1790787090\n#src journal\n\
1790787082.427860 host sshd-session[180]: Invalid user admin from 127.0.0.1 port 59540\n\
1790787084.250448 host sshd-session[180]: Failed password for invalid user admin from 127.0.0.1 port 59540 ssh2\n\
1790787086.242489 host sshd-session[184]: Failed password for root from 10.1.2.3 port 59548 ssh2\n\
1790787089.971949 host sshd-session[191]: Invalid user ghost user from 127.0.0.1 port 37650\n";

    #[test]
    fn journal_events_deduplicated() {
        let Probe::Ok { data, source } = auth_failures(JOURNAL, 0) else {
            panic!()
        };
        assert_eq!(source, "journal");
        assert_eq!(data.len(), 3);
        assert_eq!(data[0].user, "admin");
        assert!(data[0].invalid_user);
        assert_eq!(data[0].method, "password");
        assert_eq!(data[1].ip.as_deref(), Some("10.1.2.3"));
        assert_eq!(data[2].user, "ghost user");
    }

    #[test]
    fn since_filters() {
        let Probe::Ok { data, .. } = auth_failures(JOURNAL, 1_790_787_086) else {
            panic!()
        };
        assert_eq!(data.len(), 1);
    }

    #[test]
    fn unreadable_journal_falls_back_to_files() {
        let text = "tz=+0200\nnow=1790787372\n#src journal\nHint: You are currently not seeing messages from other users and the system.\n\
#src file /var/log/auth.log\n\
Sep 30 18:56:11 host sshd[15]: Failed password for root from 1.2.3.4 port 45894 ssh2\n\
2026-09-30T18:56:12.123456+02:00 host sshd[16]: Failed publickey for bob from 1.2.3.5 port 1 ssh2: RSA SHA256:x\n";
        let Probe::Ok { data, source } = auth_failures(text, 0) else {
            panic!()
        };
        assert_eq!(source, "/var/log/auth.log");
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].ts, 1_790_787_371);
        assert_eq!(data[1].ts, 1_790_787_372);
        assert_eq!(data[1].method, "publickey");
    }

    #[test]
    fn syslog_year_rollover() {
        // Log line from December read in January belongs to the previous year.
        let now = days_from_civil(2027, 1, 2) * 86_400;
        let (ts, _) = parse_syslog_prefix("Dec 31 23:00:00 h x", 0, Some(now)).unwrap();
        assert_eq!(ts, days_from_civil(2026, 12, 31) * 86_400 + 23 * 3600);
    }

    #[test]
    fn nothing_readable() {
        let Probe::Na { reason } = auth_failures(
            "tz=+0000\n#src journal\nNo journal files were opened due to insufficient permissions.\n#denied /var/log/secure\n",
            0,
        ) else {
            panic!()
        };
        assert!(reason.contains("journal not readable"));
        assert!(reason.contains("/var/log/secure not readable"));
    }
}
