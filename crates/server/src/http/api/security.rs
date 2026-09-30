//! Fleet-wide security overview.

use super::hosts::{AuthReport, RangeQuery, auth_report, baseline_ports};
use super::time_range;
use crate::app::App;
use crate::auth::session::Authed;
use crate::http::error::ApiResult;
use axum::Json;
use axum::extract::{Query, State};
use collect::model::Probe;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct HostSecurityRow {
    pub name: String,
    pub status: String,
    pub groups: Vec<String>,
    pub tags: Vec<String>,
    pub failed_logins_24h: Option<u64>,
    pub auth_source: Option<String>,
    pub auth_unavailable: Option<String>,
    pub pending_updates: Option<u32>,
    pub security_updates: Option<u32>,
    pub updates_unavailable: Option<String>,
    pub updates_checked_at: Option<i64>,
    pub reboot_required: Option<bool>,
    pub reboot_reasons: Vec<String>,
    pub baseline: Option<String>,
    /// Listening ports not present on the baseline host.
    pub new_ports: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CertView {
    pub endpoint: String,
    pub checked_at: i64,
    pub not_after: Option<i64>,
    pub days_left: Option<f64>,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub error: Option<String>,
    pub trust_error: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SecurityOverview {
    pub auth: AuthReport,
    pub hosts: Vec<HostSecurityRow>,
    pub certs: Vec<CertView>,
    /// Endpoints configured but not checked yet.
    pub certs_pending: Vec<String>,
}

/// Failed logins, pending updates, reboot state, unexpected ports and
/// certificate expiry across the fleet.
#[utoipa::path(get, path = "/security", tag = "security", params(RangeQuery),
    responses((status = 200, body = SecurityOverview)))]
pub async fn overview(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<SecurityOverview>> {
    let (from, _) = time_range(q.from, q.to, 7 * 86_400);
    let auth = auth_report(&app, None, from, None).await?;
    let now = store::now();
    let stale = app.stale_after();
    let states: Vec<_> = app
        .fleet
        .with(|m| m.values().cloned().collect())
        .unwrap_or_default();
    let mut hosts = Vec::with_capacity(states.len());
    for h in states {
        let s = h.summary(now, stale, false);
        let (pending, security, unavailable, checked) = match &h.updates {
            Some(t) => match &t.data {
                Probe::Ok { data, .. } => (Some(data.total), data.security, None, Some(t.ts)),
                Probe::Na { reason } => (None, None, Some(reason.clone()), Some(t.ts)),
            },
            None => (None, None, None, None),
        };
        let reboot = h.inventory.as_ref().map(|i| i.data.reboot.clone());
        let new_ports = baseline_ports(&app, &h.cfg.name, h.cfg.baseline.as_deref())
            .await?
            .into_iter()
            .map(|c| match c.new {
                Some(v) => format!("{} ({v})", c.subject),
                None => c.subject,
            })
            .collect();
        hosts.push(HostSecurityRow {
            name: h.cfg.name.clone(),
            status: s.status,
            groups: h.cfg.groups.clone(),
            tags: h.cfg.tags.clone(),
            failed_logins_24h: h.failed_logins_24h,
            auth_source: h.auth.as_ref().and_then(|a| a.data.source.clone()),
            auth_unavailable: h.auth.as_ref().and_then(|a| a.data.unavailable.clone()),
            pending_updates: pending,
            security_updates: security,
            updates_unavailable: unavailable,
            updates_checked_at: checked,
            reboot_required: reboot.as_ref().and_then(|r| r.required),
            reboot_reasons: reboot.map(|r| r.reasons).unwrap_or_default(),
            baseline: h.cfg.baseline.clone(),
            new_ports,
        });
    }
    let statuses = app.store.cert_statuses().await?;
    let certs: Vec<CertView> = statuses
        .into_iter()
        .filter(|c| app.cfg.certs.iter().any(|x| x.endpoint == c.endpoint))
        .map(|c| CertView {
            days_left: c.not_after.map(|na| (na - now) as f64 / 86_400.0),
            endpoint: c.endpoint,
            checked_at: c.checked_at,
            not_after: c.not_after,
            subject: c.subject,
            issuer: c.issuer,
            error: c.error,
            trust_error: c.trust_error,
        })
        .collect();
    let certs_pending = app
        .cfg
        .certs
        .iter()
        .filter(|c| !certs.iter().any(|v| v.endpoint == c.endpoint))
        .map(|c| c.endpoint.clone())
        .collect();
    Ok(Json(SecurityOverview {
        auth,
        hosts,
        certs,
        certs_pending,
    }))
}
