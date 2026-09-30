//! TLS for the web server: provided certificate files or a self-signed
//! certificate generated on first run.

use common::config::{Config, TlsMode};
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Generates (once) a self-signed certificate in `dir` and returns the paths.
pub fn self_signed(dir: &Path, listen: &str) -> Result<(PathBuf, PathBuf), String> {
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    if cert.is_file() && key.is_file() {
        return Ok((cert, key));
    }
    crate::app::create_private_dir(dir)?;
    let mut names = vec!["localhost".to_string()];
    if let Some((host, _)) = listen.rsplit_once(':') {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if !host.is_empty() && host != "0.0.0.0" && host != "::" {
            names.push(host.to_string());
        }
    }
    if let Ok(h) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
        let h = h.trim();
        if !h.is_empty() {
            names.push(h.to_string());
        }
    }
    names.dedup();
    let mut params = rcgen::CertificateParams::new(names).map_err(|e| e.to_string())?;
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        format!("{} self-signed", common::PRODUCT_NAME),
    );
    let now = std::time::SystemTime::now();
    params.not_before = now.into();
    params.not_after = (now + std::time::Duration::from_secs(825 * 86_400)).into();
    let key_pair = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
    let c = params.self_signed(&key_pair).map_err(|e| e.to_string())?;
    common::secrets::write_private(&key, key_pair.serialize_pem().as_bytes())
        .map_err(|e| format!("writing {}: {e}", key.display()))?;
    std::fs::write(&cert, c.pem()).map_err(|e| format!("writing {}: {e}", cert.display()))?;
    Ok((cert, key))
}

pub fn server_config(cert: &Path, key: &Path) -> Result<ServerConfig, String> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
        .map_err(|e| format!("reading {}: {e}", cert.display()))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("parsing {}: {e}", cert.display()))?;
    if certs.is_empty() {
        return Err(format!("{} contains no certificate", cert.display()));
    }
    let key =
        PrivateKeyDer::from_pem_file(key).map_err(|e| format!("reading {}: {e}", key.display()))?;
    let mut cfg =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| format!("invalid certificate or key: {e}"))?;
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(cfg)
}

/// SHA-256 fingerprint of the first certificate in a PEM file.
pub fn fingerprint(cert: &Path) -> Option<String> {
    let der = CertificateDer::pem_file_iter(cert).ok()?.next()?.ok()?;
    let d = Sha256::digest(der.as_ref());
    Some(
        d.iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// Returns the TLS configuration to use, or `None` for plain HTTP.
pub fn configure(
    cfg: &Config,
    data_dir: &Path,
    config_dir: &Path,
) -> Result<Option<(ServerConfig, PathBuf)>, String> {
    if !cfg.tls_enabled() {
        return Ok(None);
    }
    let (cert, key) = match cfg.server.tls.mode {
        TlsMode::Files => (
            common::config::resolve_path(
                config_dir,
                cfg.server.tls.cert.as_deref().unwrap_or(Path::new("")),
            ),
            common::config::resolve_path(
                config_dir,
                cfg.server.tls.key.as_deref().unwrap_or(Path::new("")),
            ),
        ),
        _ => self_signed(&data_dir.join("tls"), &cfg.server.listen)?,
    };
    Ok(Some((server_config(&cert, &key)?, cert)))
}
