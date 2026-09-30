//! The web server and background workers.

pub mod alerting;
pub mod app;
pub mod auth;
pub mod certs;
pub mod engine;
pub mod http;
mod maintenance;
mod prom;
pub mod state;
pub mod tls;

pub use app::{App, BuildOptions};

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Installs the ring crypto provider as the process default for rustls.
pub fn init_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Starts background workers and serves HTTP(S) until `shutdown` resolves.
pub async fn serve(
    app: Arc<App>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), String> {
    init_crypto();
    engine::restore(&app).await;

    if app.store.user_count().await.map_err(|e| e.to_string())? == 0 {
        let token = auth::random_token();
        if let Ok(mut t) = app.setup_token.lock() {
            *t = Some(auth::token_hash(&token));
        }
        let scheme = if app.tls { "https" } else { "http" };
        let base = app.cfg.server.public_url.clone().unwrap_or_else(|| {
            format!(
                "{scheme}://{}",
                app.cfg.server.listen.replace("0.0.0.0", "127.0.0.1")
            )
        });
        tracing::warn!(
            "no users exist yet; create the first administrator at {}/setup#token={token}",
            base.trim_end_matches('/')
        );
        eprintln!(
            "\n  No users exist yet. Create the first administrator:\n\n    {}/setup#token={token}\n\n  (or run `{} user add <name> --role admin`)\n",
            base.trim_end_matches('/'),
            common::PRODUCT_NAME
        );
    }

    engine::start(app.clone());
    alerting::start(app.clone());
    certs::start(app.clone());
    maintenance::start(app.clone());

    let router = http::router(app.clone());
    let addr: SocketAddr = app
        .cfg
        .server
        .listen
        .parse()
        .map_err(|_| format!("invalid listen address `{}`", app.cfg.server.listen))?;
    let config_dir = app
        .config_path
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let tls = tls::configure(&app.cfg, &app.data_dir, &config_dir)?;
    let service = router.into_make_service_with_connect_info::<SocketAddr>();

    let token = app.shutdown.clone();
    let handle = axum_server::Handle::new();
    let h2 = handle.clone();
    tokio::spawn(async move {
        shutdown.await;
        tracing::info!("shutting down");
        token.cancel();
        h2.graceful_shutdown(Some(Duration::from_secs(10)));
    });

    match tls {
        Some((cfg, cert_path)) => {
            if let Some(fp) = tls::fingerprint(&cert_path) {
                tracing::info!(certificate = %cert_path.display(), sha256 = %fp, "TLS enabled");
            }
            tracing::info!(%addr, "listening (https)");
            let rustls = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(cfg));
            axum_server::bind_rustls(addr, rustls)
                .handle(handle)
                .serve(service)
                .await
                .map_err(|e| format!("server error on {addr}: {e}"))
        }
        None => {
            if !addr.ip().is_loopback() {
                tracing::warn!(%addr, "serving plain HTTP on a non-loopback address; use a TLS-terminating proxy");
            }
            tracing::info!(%addr, "listening (http)");
            axum_server::bind(addr)
                .handle(handle)
                .serve(service)
                .await
                .map_err(|e| format!("server error on {addr}: {e}"))
        }
    }
}
