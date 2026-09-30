//! Authentication: password hashing, sessions, CSRF, rate limiting, TOTP
//! and OIDC.

pub mod oidc;
pub mod password;
pub mod ratelimit;
pub mod session;
pub mod totp;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// A random 256-bit token, base64url encoded.
pub fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(common::secrets::random_bytes::<32>())
}

/// SHA-256 hex digest of a token, as stored in the database.
pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Constant-time string comparison.
pub fn ct_eq(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
