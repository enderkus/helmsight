//! Secrets at rest: a key file created on first run and XChaCha20-Poly1305
//! authenticated encryption, plus references to secrets from the config.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use std::fmt;
use std::path::Path;
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("{0}")]
    Io(String),
    #[error("key file {path} is accessible by other users; run `chmod 600 {path}`")]
    Permissions { path: String },
    #[error("key file {0} is not 32 bytes; it may be corrupted")]
    BadKey(String),
    #[error("secret could not be decrypted (wrong key file or tampered data)")]
    Decrypt,
    #[error("secret `{0}` is not stored; set it with `secret set {0}`")]
    Missing(String),
    #[error("environment variable `{0}` is not set")]
    MissingEnv(String),
}

/// 256-bit key used to encrypt stored secrets.
pub struct SecretKey([u8; 32]);

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(..)")
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    // The OS random source failing is unrecoverable for a security tool.
    #[allow(clippy::expect_used)]
    getrandom::fill(&mut b).expect("operating system random number generator failed");
    b
}

impl SecretKey {
    pub fn generate() -> Self {
        SecretKey(random_bytes())
    }

    /// Loads the key file, creating it with mode 0600 if it does not exist.
    /// Refuses key files readable by group or others.
    pub fn load_or_create(path: &Path) -> Result<Self, SecretError> {
        let display = path.display().to_string();
        match std::fs::read(path) {
            Ok(mut bytes) => {
                check_permissions(path)?;
                let key: [u8; 32] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| SecretError::BadKey(display.clone()))?;
                bytes.zeroize();
                Ok(SecretKey(key))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| SecretError::Io(format!("creating {}: {e}", dir.display())))?;
                }
                let key = SecretKey::generate();
                write_private(path, &key.0)
                    .map_err(|e| SecretError::Io(format!("writing {display}: {e}")))?;
                Ok(key)
            }
            Err(e) => Err(SecretError::Io(format!("reading {display}: {e}"))),
        }
    }

    /// Encrypts `plaintext`. `context` binds the ciphertext to its purpose
    /// (for example the secret name) so it cannot be swapped with another.
    pub fn seal(&self, plaintext: &[u8], context: &str) -> String {
        let cipher = XChaCha20Poly1305::new(&Key::from(self.0));
        let nonce_bytes: [u8; 24] = random_bytes();
        let nonce = XNonce::from(nonce_bytes);
        // Encryption with a valid key and nonce cannot fail.
        let ct = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: context.as_bytes(),
                },
            )
            .unwrap_or_default();
        let mut out = Vec::with_capacity(24 + ct.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        format!("v1:{}", B64.encode(out))
    }

    pub fn open(&self, sealed: &str, context: &str) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        let data = sealed
            .strip_prefix("v1:")
            .and_then(|b| B64.decode(b).ok())
            .ok_or(SecretError::Decrypt)?;
        if data.len() < 24 + 16 {
            return Err(SecretError::Decrypt);
        }
        let (nonce, ct) = data.split_at(24);
        let nonce: [u8; 24] = nonce.try_into().map_err(|_| SecretError::Decrypt)?;
        let cipher = XChaCha20Poly1305::new(&Key::from(self.0));
        cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ct,
                    aad: context.as_bytes(),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| SecretError::Decrypt)
    }
}

#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), SecretError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path)
        .map_err(|e| SecretError::Io(format!("reading {}: {e}", path.display())))?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(SecretError::Permissions {
            path: path.display().to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_permissions(_path: &Path) -> Result<(), SecretError> {
    Ok(())
}

/// Writes a file readable only by the current user.
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(data)?;
    f.sync_all()
}

/// Lookup of named secrets stored in the database.
pub trait SecretLookup {
    fn lookup(&self, name: &str) -> Option<Zeroizing<String>>;
}

/// A value in the config that may refer to a secret:
/// `secret:<name>` (stored encrypted), `env:<VAR>`, `file:<path>`, or a
/// literal value.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SecretRef(pub String);

impl fmt::Debug for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_literal() {
            f.write_str("SecretRef(<literal>)")
        } else {
            write!(f, "SecretRef({})", self.0)
        }
    }
}

impl SecretRef {
    pub fn is_literal(&self) -> bool {
        !(self.0.starts_with("secret:")
            || self.0.starts_with("env:")
            || self.0.starts_with("file:"))
    }

    pub fn stored_name(&self) -> Option<&str> {
        self.0.strip_prefix("secret:")
    }

    pub fn resolve(&self, store: &dyn SecretLookup) -> Result<Zeroizing<String>, SecretError> {
        if let Some(name) = self.0.strip_prefix("secret:") {
            return store
                .lookup(name)
                .ok_or_else(|| SecretError::Missing(name.to_string()));
        }
        if let Some(var) = self.0.strip_prefix("env:") {
            return std::env::var(var)
                .map(Zeroizing::new)
                .map_err(|_| SecretError::MissingEnv(var.to_string()));
        }
        if let Some(path) = self.0.strip_prefix("file:") {
            return std::fs::read_to_string(path)
                .map(|s| Zeroizing::new(s.trim_end_matches(['\n', '\r']).to_string()))
                .map_err(|e| SecretError::Io(format!("reading {path}: {e}")));
        }
        Ok(Zeroizing::new(self.0.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open() {
        let key = SecretKey::generate();
        let sealed = key.seal(b"hunter2", "smtp");
        assert!(sealed.starts_with("v1:"));
        assert_eq!(key.open(&sealed, "smtp").unwrap().as_slice(), b"hunter2");
        assert!(key.open(&sealed, "other").is_err(), "context must be bound");
        assert!(SecretKey::generate().open(&sealed, "smtp").is_err());
        let mut tampered = sealed.clone();
        tampered.pop();
        tampered.push('A');
        assert!(key.open(&tampered, "smtp").is_err());
        assert!(key.open("garbage", "smtp").is_err());
    }

    #[test]
    fn key_file_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/secret.key");
        let k1 = SecretKey::load_or_create(&path).unwrap();
        let sealed = k1.seal(b"x", "c");
        let k2 = SecretKey::load_or_create(&path).unwrap();
        assert_eq!(k2.open(&sealed, "c").unwrap().as_slice(), b"x");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(
                SecretKey::load_or_create(&path),
                Err(SecretError::Permissions { .. })
            ));
        }
    }

    #[test]
    fn references() {
        struct Store;
        impl SecretLookup for Store {
            fn lookup(&self, name: &str) -> Option<Zeroizing<String>> {
                (name == "a").then(|| Zeroizing::new("va".to_string()))
            }
        }
        assert_eq!(
            SecretRef("secret:a".into())
                .resolve(&Store)
                .unwrap()
                .as_str(),
            "va"
        );
        assert!(SecretRef("secret:b".into()).resolve(&Store).is_err());
        assert_eq!(
            SecretRef("plain".into()).resolve(&Store).unwrap().as_str(),
            "plain"
        );
        assert!(SecretRef("plain".into()).is_literal());
        assert!(format!("{:?}", SecretRef("plain".into())).contains("<literal>"));
    }
}
