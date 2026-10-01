//! SSH key generation and public key export for the monitoring account.

use russh::keys::PrivateKey;
use russh::keys::ssh_key::LineEnding;
use russh::keys::ssh_key::private::Ed25519Keypair;
use std::io::Write;
use std::path::{Path, PathBuf};

fn pub_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".pub");
    PathBuf::from(p)
}

fn write_new(path: &Path, data: &[u8], mode: u32) -> Result<(), String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut f = opts
        .open(path)
        .map_err(|e| format!("creating {}: {e}", path.display()))?;
    f.write_all(data)
        .map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Creates a new ed25519 key at `path` (mode 0600) and its public key at
/// `<path>.pub`. Existing files are never overwritten. Returns the public
/// key in OpenSSH format.
pub fn generate_ed25519(path: &Path, comment: &str) -> Result<String, String> {
    let pubfile = pub_path(path);
    for p in [path, pubfile.as_path()] {
        if p.exists() {
            return Err(format!("{} already exists", p.display()));
        }
    }
    let seed: [u8; 32] = common::secrets::random_bytes();
    let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
    key.set_comment(comment);
    let private = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| format!("encoding the private key: {e}"))?;
    let public = key
        .public_key()
        .to_openssh()
        .map_err(|e| format!("encoding the public key: {e}"))?;
    write_new(path, private.as_bytes(), 0o600)?;
    write_new(&pubfile, format!("{public}\n").as_bytes(), 0o644)?;
    Ok(public)
}

/// Returns `<algorithm> <base64>` for the key at `path`: derived from the
/// private key when it is readable, otherwise read from `<path>.pub`.
pub fn public_key(path: &Path) -> Result<String, String> {
    let line = match russh::keys::load_secret_key(path, None) {
        Ok(k) => k
            .public_key()
            .to_openssh()
            .map_err(|e| format!("encoding the public key: {e}"))?,
        Err(private_err) => {
            let pubfile = pub_path(path);
            std::fs::read_to_string(&pubfile).map_err(|_| {
                format!(
                    "cannot read the key {} ({private_err}) or {}",
                    path.display(),
                    pubfile.display()
                )
            })?
        }
    };
    let mut parts = line.split_whitespace();
    let (Some(algo), Some(blob)) = (parts.next(), parts.next()) else {
        return Err(format!("{} does not contain a public key", path.display()));
    };
    let algo_ok = !algo.is_empty()
        && algo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@.-".contains(&b));
    let blob_ok = !blob.is_empty()
        && blob
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b));
    if !algo_ok || !blob_ok {
        return Err(format!("{} does not contain a public key", path.display()));
    }
    Ok(format!("{algo} {blob}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_loads_and_matches_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id_ed25519");
        let public = generate_ed25519(&path, "helmsight").unwrap();
        assert!(public.starts_with("ssh-ed25519 "));
        assert!(public.ends_with(" helmsight"));
        let derived = public_key(&path).unwrap();
        assert!(public.starts_with(&derived));
        assert!(russh::keys::load_secret_key(&path, None).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn existing_keys_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id_ed25519");
        std::fs::write(&path, "keep").unwrap();
        assert!(generate_ed25519(&path, "helmsight").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
    }

    #[test]
    fn public_key_falls_back_to_pub_file_and_rejects_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id");
        std::fs::write(
            pub_path(&path),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB4= comment\n",
        )
        .unwrap();
        assert_eq!(
            public_key(&path).unwrap(),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB4="
        );
        std::fs::write(pub_path(&path), "ssh-ed25519 'x; rm -rf /'\n").unwrap();
        assert!(public_key(&path).is_err());
    }
}
