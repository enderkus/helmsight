//! Login rate limiting by client IP and by user name.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_PER_IP: usize = 20;
const MAX_PER_USER: usize = 8;

#[derive(Debug, Default)]
pub struct RateLimiter {
    failures: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    fn prune(list: &mut Vec<Instant>, now: Instant) {
        list.retain(|t| now.duration_since(*t) < WINDOW);
    }

    /// Returns the time to wait before another attempt is allowed.
    pub fn check(&self, ip: &str, user: &str) -> Option<Duration> {
        let now = Instant::now();
        let mut map = self.failures.lock().ok()?;
        let mut wait = None;
        for (key, max) in [
            (format!("ip:{ip}"), MAX_PER_IP),
            (format!("user:{}", user.to_lowercase()), MAX_PER_USER),
        ] {
            if let Some(list) = map.get_mut(&key) {
                Self::prune(list, now);
                if list.len() >= max {
                    let oldest = list.first().copied().unwrap_or(now);
                    let w = WINDOW.saturating_sub(now.duration_since(oldest));
                    wait = Some(wait.map_or(w, |x: Duration| x.max(w)));
                }
            }
        }
        wait
    }

    pub fn failure(&self, ip: &str, user: &str) {
        let now = Instant::now();
        if let Ok(mut map) = self.failures.lock() {
            for key in [format!("ip:{ip}"), format!("user:{}", user.to_lowercase())] {
                let list = map.entry(key).or_default();
                Self::prune(list, now);
                list.push(now);
            }
            // Bound memory under attack from many addresses.
            if map.len() > 100_000 {
                map.retain(|_, v| {
                    Self::prune(v, now);
                    !v.is_empty()
                });
            }
        }
    }

    pub fn success(&self, user: &str) {
        if let Ok(mut map) = self.failures.lock() {
            map.remove(&format!("user:{}", user.to_lowercase()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_user_then_per_ip() {
        let r = RateLimiter::default();
        for _ in 0..MAX_PER_USER {
            assert!(r.check("1.1.1.1", "alice").is_none());
            r.failure("1.1.1.1", "alice");
        }
        assert!(r.check("1.1.1.1", "Alice").is_some());
        assert!(
            r.check("2.2.2.2", "alice").is_some(),
            "per user regardless of IP"
        );
        assert!(r.check("1.1.1.1", "bob").is_none());
        r.success("alice");
        assert!(r.check("2.2.2.2", "alice").is_none());
        for i in 0..MAX_PER_IP {
            r.failure("3.3.3.3", &format!("u{i}"));
        }
        assert!(r.check("3.3.3.3", "new").is_some());
    }
}
