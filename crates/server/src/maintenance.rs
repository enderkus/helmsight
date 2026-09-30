//! Periodic housekeeping: rollups, retention and expired sessions.

use crate::app::App;
use std::sync::Arc;
use std::time::Duration;

pub fn start(app: Arc<App>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        let mut runs: u64 = 0;
        loop {
            ticker.tick().await;
            if app.shutdown.is_cancelled() {
                return;
            }
            let now = store::now();
            if let Err(e) = app.store.rollup(now).await {
                tracing::error!(error = %e, "rollup failed");
            }
            if runs.is_multiple_of(60) {
                let r = &app.cfg.retention;
                let secs = |d: common::Dur| i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
                match app
                    .store
                    .apply_retention(
                        now,
                        secs(r.raw),
                        secs(r.minute),
                        secs(r.five_minute),
                        secs(r.events),
                    )
                    .await
                {
                    Ok(n) if n > 0 => tracing::info!(rows = n, "applied retention"),
                    Ok(_) => {}
                    Err(e) => tracing::error!(error = %e, "retention failed"),
                }
                let _ = app.store.purge_sessions().await;
            }
            runs += 1;
        }
    });
}
