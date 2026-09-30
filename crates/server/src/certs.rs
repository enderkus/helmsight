//! TLS certificate expiry checks, performed from the central server.

use crate::app::App;
use rustls::DigitallySignedStruct;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::security::CertStatus;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

pub fn start(app: Arc<App>) {
    for cert in app.cfg.certs.clone() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(cert.interval.0.max(Duration::from_secs(60)));
            loop {
                ticker.tick().await;
                if app.shutdown.is_cancelled() {
                    return;
                }
                let status = check(&cert.endpoint, cert.server_name()).await;
                if let Some(e) = &status.error {
                    tracing::warn!(endpoint = %cert.endpoint, error = %e, "certificate check failed");
                }
                if let Err(e) = app.store.put_cert_status(status).await {
                    tracing::error!(error = %e, "storing certificate status failed");
                }
            }
        });
    }
}

/// Validates like a browser would, but records the result instead of
/// aborting, so the certificate can be inspected even when untrusted. No
/// application data is exchanged over this connection.
#[derive(Debug)]
struct Recording {
    inner: Arc<WebPkiServerVerifier>,
    result: Mutex<Option<String>>,
}

impl ServerCertVerifier for Recording {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let Err(e) =
            self.inner
                .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
            && let Ok(mut r) = self.result.lock()
        {
            *r = Some(e.to_string());
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

pub async fn check(endpoint: &str, server_name: &str) -> CertStatus {
    let mut status = CertStatus {
        endpoint: endpoint.to_string(),
        checked_at: store::now(),
        not_after: None,
        subject: None,
        issuer: None,
        error: None,
        trust_error: None,
    };
    match inspect(endpoint, server_name).await {
        Ok((not_after, subject, issuer, trust)) => {
            status.not_after = Some(not_after);
            status.subject = Some(subject);
            status.issuer = Some(issuer);
            status.trust_error = trust;
        }
        Err(e) => status.error = Some(e),
    }
    status
}

async fn inspect(
    endpoint: &str,
    server_name: &str,
) -> Result<(i64, String, String, Option<String>), String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let inner = WebPkiServerVerifier::builder_with_provider(
        Arc::new(alerts::notify::root_store()),
        provider.clone(),
    )
    .build()
    .map_err(|e| e.to_string())?;
    let verifier = Arc::new(Recording {
        inner,
        result: Mutex::new(None),
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let name = ServerName::try_from(server_name.to_string())
        .map_err(|e| format!("invalid server name: {e}"))?;
    let tcp = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(endpoint))
        .await
        .map_err(|_| "connection timed out".to_string())?
        .map_err(|e| format!("connection failed: {e}"))?;
    let tls = tokio::time::timeout(
        Duration::from_secs(10),
        TlsConnector::from(Arc::new(config)).connect(name, tcp),
    )
    .await
    .map_err(|_| "TLS handshake timed out".to_string())?
    .map_err(|e| format!("TLS handshake failed: {e}"))?;
    let (_, conn) = tls.get_ref();
    let leaf = conn
        .peer_certificates()
        .and_then(|c| c.first())
        .ok_or("server sent no certificate")?;
    let (_, cert) = x509_parser::parse_x509_certificate(leaf.as_ref())
        .map_err(|e| format!("cannot parse certificate: {e}"))?;
    let trust = verifier.result.lock().ok().and_then(|r| r.clone());
    Ok((
        cert.validity().not_after.timestamp(),
        cert.subject().to_string(),
        cert.issuer().to_string(),
        trust,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn refused_connection_is_reported() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let s = check(&format!("127.0.0.1:{port}"), "localhost").await;
        assert!(s.error.unwrap().contains("connection failed"));
    }

    #[tokio::test]
    async fn self_signed_certificate_is_read_with_trust_error() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let (cert, key) = crate::tls::self_signed(&dir.path().join("tls"), "127.0.0.1:0").unwrap();
        let config = crate::tls::server_config(&cert, &key).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
            if let Ok((s, _)) = listener.accept().await {
                let _ = acceptor.accept(s).await;
            }
        });
        let s = check(&addr.to_string(), "localhost").await;
        assert!(s.error.is_none(), "{:?}", s.error);
        assert!(s.not_after.unwrap() > store::now() + 86_400 * 300);
        assert!(s.trust_error.is_some());
    }
}
