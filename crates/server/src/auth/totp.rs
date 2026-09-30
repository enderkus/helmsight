//! RFC 6238 time-based one-time passwords (SHA-1, 6 digits, 30 s steps),
//! with replay protection.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::collections::HashMap;
use std::sync::Mutex;

const STEP: u64 = 30;
const DIGITS: u32 = 6;

/// HOTP value for a counter (RFC 4226).
fn hotp(secret: &[u8], counter: u64) -> u32 {
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(secret) else {
        return u32::MAX;
    };
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest.get(19).copied().unwrap_or(0) & 0x0f);
    let bytes: [u8; 4] = digest
        .get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .unwrap_or([0; 4]);
    (u32::from_be_bytes(bytes) & 0x7fff_ffff) % 10u32.pow(DIGITS)
}

pub fn code_at(secret: &[u8], unix: u64) -> String {
    format!("{:06}", hotp(secret, unix / STEP))
}

/// RFC 4648 base32 without padding.
pub fn base32(data: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &b in data {
        buffer = (buffer << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            let idx = ((buffer >> (bits - 5)) & 31) as usize;
            out.push(char::from(ALPHABET.get(idx).copied().unwrap_or(b'A')));
            bits -= 5;
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 31) as usize;
        out.push(char::from(ALPHABET.get(idx).copied().unwrap_or(b'A')));
    }
    out
}

/// `otpauth://` URL understood by authenticator apps.
pub fn otpauth_url(secret: &[u8], issuer: &str, account: &str) -> String {
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    let i = utf8_percent_encode(issuer, NON_ALPHANUMERIC);
    let a = utf8_percent_encode(account, NON_ALPHANUMERIC);
    format!(
        "otpauth://totp/{i}:{a}?secret={}&issuer={i}&algorithm=SHA1&digits={DIGITS}&period={STEP}",
        base32(secret)
    )
}

/// QR code for a URL as an SVG document.
pub fn qr_svg(data: &str) -> Option<String> {
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    Some(
        code.render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .build(),
    )
}

/// Remembers the last accepted step per user so a code cannot be reused.
#[derive(Debug, Default)]
pub struct ReplayGuard {
    last: Mutex<HashMap<i64, u64>>,
}

impl ReplayGuard {
    /// Verifies `code` for `user_id`, allowing one step of clock drift.
    pub fn verify(&self, user_id: i64, secret: &[u8], code: &str, unix: u64) -> bool {
        let code = code.trim().replace(' ', "");
        if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let now_step = unix / STEP;
        let mut matched = None;
        for step in [now_step.saturating_sub(1), now_step, now_step + 1] {
            let expected = format!("{:06}", hotp(secret, step));
            if super::ct_eq(&expected, &code) {
                matched = Some(step);
            }
        }
        let Some(step) = matched else { return false };
        let Ok(mut last) = self.last.lock() else {
            return false;
        };
        if last.get(&user_id).is_some_and(|l| step <= *l) {
            return false;
        }
        last.insert(user_id, step);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vectors() {
        // RFC 6238 appendix B, SHA-1, truncated to 6 digits.
        let secret = b"12345678901234567890";
        assert_eq!(code_at(secret, 59), "287082");
        assert_eq!(code_at(secret, 1_111_111_109), "081804");
        assert_eq!(code_at(secret, 2_000_000_000), "279037");
    }

    #[test]
    fn base32_encoding() {
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(
            base32(b"12345678901234567890"),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
        );
    }

    #[test]
    fn replay_is_rejected() {
        let g = ReplayGuard::default();
        let secret = b"12345678901234567890";
        let t = 1_111_111_109;
        let code = code_at(secret, t);
        assert!(g.verify(1, secret, &code, t));
        assert!(!g.verify(1, secret, &code, t), "same code twice");
        assert!(g.verify(2, secret, &code, t), "other user unaffected");
        assert!(!g.verify(3, secret, "000000", t));
        assert!(!g.verify(3, secret, "12345", t));
        // Next step is accepted.
        assert!(g.verify(1, secret, &code_at(secret, t + 30), t + 30));
    }

    #[test]
    fn otpauth() {
        let u = otpauth_url(b"12345678901234567890", "helm sight", "alice@x");
        assert!(u.starts_with("otpauth://totp/helm%20sight:alice%40x?secret=GEZDGNBV"));
        assert!(qr_svg(&u).unwrap().starts_with("<?xml"));
    }
}
