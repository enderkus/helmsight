//! Small helpers shared by the parsers.

/// Truncates `s` to at most `max` bytes on a char boundary and removes
/// control characters.
pub fn clip(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max));
    for c in s.chars() {
        if out.len() + c.len_utf8() > max {
            break;
        }
        if c.is_control() {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

pub fn clip_opt(s: &str, max: usize) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(clip(t, max))
    }
}

/// Parses `key=value` lines. First occurrence wins.
pub fn key_values(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    for line in text.lines().take(10_000) {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k.is_empty() || k.len() > 64 {
                continue;
            }
            map.entry(k.to_string())
                .or_insert_with(|| clip(v.trim(), 1024));
        }
    }
    map
}

/// Percentage `part / whole * 100`, 0 when `whole` is 0, clamped to 0..=100.
pub fn pct(part: f64, whole: f64) -> f64 {
    if whole <= 0.0 || !part.is_finite() || !whole.is_finite() {
        return 0.0;
    }
    (part / whole * 100.0).clamp(0.0, 100.0)
}

/// Rounds to two decimals to keep JSON payloads small.
pub fn round2(v: f64) -> f64 {
    if v.is_finite() {
        (v * 100.0).round() / 100.0
    } else {
        0.0
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let d = i64::from(d);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Year of a unix timestamp (UTC).
pub fn year_of(ts: i64) -> i64 {
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 { y + 1 } else { y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_respects_char_boundaries() {
        assert_eq!(clip("héllo", 2), "h");
        assert_eq!(clip("a\u{1b}[31mb", 10), "a [31mb");
    }

    #[test]
    fn calendar_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 9, 30), 20_726);
        assert_eq!(year_of(1_790_787_372), 2026);
        assert_eq!(year_of(0), 1970);
        assert_eq!(year_of(-1), 1969);
    }
}
