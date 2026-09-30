//! Argon2id password hashing.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use std::sync::OnceLock;

pub const MIN_LENGTH: usize = 12;
pub const MAX_LENGTH: usize = 256;

fn hasher() -> Argon2<'static> {
    // OWASP recommendation: m=19 MiB, t=2, p=1.
    let params = Params::new(19 * 1024, 2, 1, None).unwrap_or_default();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Checks the password policy. Returns a user-facing message on failure.
pub fn check_policy(password: &str, username: &str) -> Result<(), String> {
    let n = password.chars().count();
    if n < MIN_LENGTH {
        return Err(format!(
            "Password must be at least {MIN_LENGTH} characters."
        ));
    }
    if n > MAX_LENGTH {
        return Err(format!("Password must be at most {MAX_LENGTH} characters."));
    }
    if !username.is_empty() && password.to_lowercase().contains(&username.to_lowercase()) {
        return Err("Password must not contain the user name.".into());
    }
    Ok(())
}

/// Hashes a password. CPU-heavy: call from a blocking task.
pub fn hash(password: &str) -> Result<String, String> {
    let salt = common::secrets::random_bytes::<16>();
    let h: PasswordHash = hasher()
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?;
    Ok(h.to_string())
}

fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash("dummy password for timing").unwrap_or_default())
}

/// Verifies a password. With `None`, a dummy hash is verified so that
/// unknown users take as long as known ones.
pub fn verify(stored: Option<&str>, password: &str) -> bool {
    let is_dummy = stored.is_none();
    let stored: &str = match stored {
        Some(s) => s,
        None => dummy_hash(),
    };
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    let ok = hasher()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok();
    ok && !is_dummy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify(Some(&h), "correct horse battery"));
        assert!(!verify(Some(&h), "wrong"));
        assert!(!verify(None, "dummy password for timing"));
        assert!(!verify(Some("garbage"), "x"));
    }

    #[test]
    fn policy() {
        assert!(check_policy("short", "a").is_err());
        assert!(check_policy("alice-password-123", "alice").is_err());
        assert!(check_policy("a long enough passphrase", "bob").is_ok());
    }
}
