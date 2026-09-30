//! Host key verification with OpenSSH `known_hosts` semantics.
//!
//! Keys approved in the application are stored in a private known_hosts
//! file in the data directory. Additional known_hosts files (for example
//! `~/.ssh/known_hosts`) are consulted read-only.

use russh::keys::{HashAlg, PublicKey};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// A host key presented by a server that is not (yet) trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PresentedKey {
    /// `address:port` as used in known_hosts.
    pub endpoint: String,
    pub algorithm: String,
    /// `SHA256:...` fingerprint, as printed by `ssh-keygen -l`.
    pub fingerprint: String,
    #[serde(skip)]
    pub key: PublicKey,
    /// Unix seconds when the key was first presented.
    pub seen_at: i64,
    /// Fingerprint of the previously trusted key when the key changed.
    pub previous: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Trusted,
    /// Trusted now because `accept_new_host_keys` is enabled.
    Learned,
    Unknown(PresentedKey),
    Changed(PresentedKey),
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn known_hosts_name(address: &str, port: u16) -> String {
    if port == 22 {
        address.to_string()
    } else {
        format!("[{address}]:{port}")
    }
}

/// Trust store shared by all SSH connections.
#[derive(Debug)]
pub struct HostKeys {
    own_file: PathBuf,
    extra_files: Vec<PathBuf>,
    accept_new: bool,
    /// Keys awaiting admin approval, by endpoint.
    pending: Mutex<HashMap<String, PresentedKey>>,
    write_lock: Mutex<()>,
}

impl HostKeys {
    pub fn new(own_file: PathBuf, extra_files: Vec<PathBuf>, accept_new: bool) -> Self {
        Self {
            own_file,
            extra_files,
            accept_new,
            pending: Mutex::new(HashMap::new()),
            write_lock: Mutex::new(()),
        }
    }

    pub fn own_file(&self) -> &PathBuf {
        &self.own_file
    }

    /// Checks a presented key against all known_hosts files.
    pub fn verify(&self, address: &str, port: u16, key: &PublicKey, now: i64) -> Verdict {
        let endpoint = known_hosts_name(address, port);
        let mut changed_from: Option<String> = None;
        let files = std::iter::once(&self.own_file).chain(self.extra_files.iter());
        for file in files {
            match russh::keys::check_known_hosts_path(address, port, key, file) {
                Ok(true) => {
                    self.pending_remove(&endpoint);
                    return Verdict::Trusted;
                }
                Ok(false) => {}
                Err(russh::keys::Error::KeyChanged { line }) => {
                    let prev = russh::keys::known_hosts::known_host_keys_path(address, port, file)
                        .ok()
                        .and_then(|keys| {
                            keys.into_iter()
                                .find(|(l, k)| *l == line && k.algorithm() == key.algorithm())
                                .map(|(_, k)| fingerprint(&k))
                        });
                    changed_from =
                        Some(prev.unwrap_or_else(|| format!("{} line {line}", file.display())));
                }
                Err(e) => {
                    tracing::warn!(file = %file.display(), error = %e, "cannot read known_hosts file");
                }
            }
        }
        let presented = PresentedKey {
            endpoint: endpoint.clone(),
            algorithm: key.algorithm().as_str().to_string(),
            fingerprint: fingerprint(key),
            key: key.clone(),
            seen_at: now,
            previous: changed_from.clone(),
        };
        if changed_from.is_some() {
            self.pending_insert(presented.clone());
            return Verdict::Changed(presented);
        }
        if self.accept_new {
            match self.append(&endpoint, key) {
                Ok(()) => return Verdict::Learned,
                Err(e) => tracing::error!(error = %e, "cannot write known_hosts"),
            }
        }
        self.pending_insert(presented.clone());
        Verdict::Unknown(presented)
    }

    fn pending_insert(&self, key: PresentedKey) {
        if let Ok(mut p) = self.pending.lock() {
            let keep_seen = p
                .get(&key.endpoint)
                .filter(|old| old.fingerprint == key.fingerprint)
                .map(|old| old.seen_at);
            let mut key = key;
            if let Some(seen) = keep_seen {
                key.seen_at = seen;
            }
            p.insert(key.endpoint.clone(), key);
        }
    }

    fn pending_remove(&self, endpoint: &str) {
        if let Ok(mut p) = self.pending.lock() {
            p.remove(endpoint);
        }
    }

    /// Keys waiting for a decision.
    pub fn pending(&self) -> Vec<PresentedKey> {
        let mut v: Vec<PresentedKey> = self
            .pending
            .lock()
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default();
        v.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
        v
    }

    pub fn pending_for(&self, address: &str, port: u16) -> Option<PresentedKey> {
        let endpoint = known_hosts_name(address, port);
        self.pending.lock().ok()?.get(&endpoint).cloned()
    }

    /// Trusts the pending key for `address:port`, but only if its
    /// fingerprint equals the one the administrator confirmed. Any previous
    /// key for the endpoint in the private file is replaced.
    pub fn approve(&self, address: &str, port: u16, fingerprint: &str) -> Result<(), String> {
        let endpoint = known_hosts_name(address, port);
        let pending = self
            .pending
            .lock()
            .map_err(|_| "internal lock error".to_string())?
            .get(&endpoint)
            .cloned()
            .ok_or_else(|| format!("no pending host key for {endpoint}"))?;
        if pending.fingerprint != fingerprint {
            return Err(format!(
                "fingerprint mismatch: the host now presents {}; review it again",
                pending.fingerprint
            ));
        }
        if pending.previous.is_some() {
            self.remove_endpoint(&endpoint)?;
        }
        self.append(&endpoint, &pending.key)
            .map_err(|e| format!("cannot write {}: {e}", self.own_file.display()))?;
        self.pending_remove(&endpoint);
        Ok(())
    }

    /// Discards a pending key without trusting it.
    pub fn reject(&self, address: &str, port: u16) {
        self.pending_remove(&known_hosts_name(address, port));
    }

    fn append(&self, endpoint: &str, key: &PublicKey) -> std::io::Result<()> {
        let _guard = self.write_lock.lock();
        if let Some(dir) = self.own_file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&self.own_file)?;
        let line = key
            .to_openssh()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        // Drop the comment; known_hosts lines are "<host> <algo> <base64>".
        let mut parts = line.split_whitespace();
        let algo = parts.next().unwrap_or("");
        let b64 = parts.next().unwrap_or("");
        writeln!(f, "{endpoint} {algo} {b64}")?;
        f.sync_all()
    }

    fn remove_endpoint(&self, endpoint: &str) -> Result<(), String> {
        let _guard = self.write_lock.lock();
        let text = match std::fs::read_to_string(&self.own_file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        let kept: Vec<&str> = text
            .lines()
            .filter(|l| {
                l.split_whitespace()
                    .next()
                    .is_none_or(|hosts| !hosts.split(',').any(|h| h == endpoint))
            })
            .collect();
        let tmp = self.own_file.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp);
        common::secrets::write_private(
            &tmp,
            format!("{}\n", kept.join("\n")).trim_start().as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.own_file).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> PublicKey {
        let private = russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[seed; 32]),
        );
        private.public_key().clone()
    }

    #[test]
    fn unknown_then_approved() {
        let dir = tempfile::tempdir().unwrap();
        let hk = HostKeys::new(dir.path().join("known_hosts"), vec![], false);
        let k = key(1);
        let Verdict::Unknown(p) = hk.verify("10.0.0.1", 22, &k, 100) else {
            panic!()
        };
        assert_eq!(p.endpoint, "10.0.0.1");
        assert!(p.fingerprint.starts_with("SHA256:"));
        assert_eq!(hk.pending().len(), 1);
        assert!(hk.approve("10.0.0.1", 22, "SHA256:wrong").is_err());
        hk.approve("10.0.0.1", 22, &p.fingerprint).unwrap();
        assert_eq!(hk.verify("10.0.0.1", 22, &k, 101), Verdict::Trusted);
        assert!(hk.pending().is_empty());
    }

    #[test]
    fn changed_key_is_refused_until_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let hk = HostKeys::new(dir.path().join("known_hosts"), vec![], true);
        assert_eq!(hk.verify("h", 2222, &key(1), 0), Verdict::Learned);
        let text = std::fs::read_to_string(dir.path().join("known_hosts")).unwrap();
        assert!(text.starts_with("[h]:2222 ssh-ed25519 "));
        let Verdict::Changed(p) = hk.verify("h", 2222, &key(2), 0) else {
            panic!()
        };
        assert_eq!(p.previous.as_deref(), Some(fingerprint(&key(1)).as_str()));
        hk.approve("h", 2222, &p.fingerprint).unwrap();
        assert_eq!(hk.verify("h", 2222, &key(2), 0), Verdict::Trusted);
        assert!(matches!(
            hk.verify("h", 2222, &key(1), 0),
            Verdict::Changed(_)
        ));
    }

    #[test]
    fn extra_files_are_read_only_sources() {
        let dir = tempfile::tempdir().unwrap();
        let extra = dir.path().join("user_known_hosts");
        let line = key(3).to_openssh().unwrap();
        std::fs::write(&extra, format!("# comment\nweb-1,10.0.0.5 {line}\n")).unwrap();
        let hk = HostKeys::new(dir.path().join("own"), vec![extra], false);
        assert_eq!(hk.verify("10.0.0.5", 22, &key(3), 0), Verdict::Trusted);
        assert!(matches!(
            hk.verify("10.0.0.5", 22, &key(4), 0),
            Verdict::Changed(_)
        ));
    }
}
