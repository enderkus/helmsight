//! Human-friendly durations such as `5s`, `10m`, `1h30m` or `7d`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::time::Duration;

/// A duration written as a string in the configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Dur(pub Duration);

impl Dur {
    pub const fn secs(s: u64) -> Self {
        Dur(Duration::from_secs(s))
    }

    pub fn as_secs(self) -> u64 {
        self.0.as_secs()
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty duration".into());
        }
        let mut total: u64 = 0;
        let mut num = String::new();
        let mut saw_unit = false;
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                num.push(c);
                continue;
            }
            if c.is_whitespace() {
                continue;
            }
            let mut unit = c.to_string();
            if c == 'm' && chars.peek() == Some(&'s') {
                chars.next();
                unit = "ms".into();
            }
            let n: u64 = num.parse().map_err(|_| {
                format!("invalid duration `{s}`: expected a number before `{unit}`")
            })?;
            num.clear();
            let ms: u64 = match unit.as_str() {
                "ms" => 1,
                "s" => 1_000,
                "m" => 60_000,
                "h" => 3_600_000,
                "d" => 86_400_000,
                "w" => 604_800_000,
                _ => {
                    return Err(format!(
                        "invalid duration `{s}`: unknown unit `{unit}` (use ms, s, m, h, d or w)"
                    ));
                }
            };
            total = n
                .checked_mul(ms)
                .and_then(|v| total.checked_add(v))
                .ok_or_else(|| format!("duration `{s}` is too large"))?;
            saw_unit = true;
        }
        if !num.is_empty() || !saw_unit {
            return Err(format!(
                "invalid duration `{s}`: missing unit (for example `30s`, `5m`, `1h`)"
            ));
        }
        Ok(Dur(Duration::from_millis(total)))
    }
}

impl fmt::Display for Dur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = self.0.as_millis();
        if ms == 0 {
            return write!(f, "0s");
        }
        if !ms.is_multiple_of(1000) {
            return write!(f, "{ms}ms");
        }
        let mut s = ms / 1000;
        for (unit, size) in [("d", 86_400), ("h", 3_600), ("m", 60), ("s", 1)] {
            if s >= size {
                write!(f, "{}{unit}", s / size)?;
                s %= size;
            }
        }
        Ok(())
    }
}

impl Serialize for Dur {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Dur {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Dur::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        assert_eq!(Dur::parse("5s").unwrap(), Dur::secs(5));
        assert_eq!(Dur::parse("1h30m").unwrap(), Dur::secs(5400));
        assert_eq!(Dur::parse("7d").unwrap(), Dur::secs(604_800));
        assert_eq!(Dur::parse("250ms").unwrap().0, Duration::from_millis(250));
        assert_eq!(Dur::parse("1h 30m").unwrap().to_string(), "1h30m");
        assert!(Dur::parse("5").is_err());
        assert!(Dur::parse("5x").is_err());
        assert!(Dur::parse("").is_err());
        assert!(Dur::parse("99999999999999999999d").is_err());
    }
}
