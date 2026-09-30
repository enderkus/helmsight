//! Alerts, acknowledgements, silences and rule listing.

use crate::app::App;
use crate::auth::session::Authed;
use crate::http::error::{ApiError, ApiResult};
use crate::state::Event;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use common::Role;
use common::rules::METRICS;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use store::alerts::{AlertRecord, Delivery, Silence};
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, ToSchema)]
pub struct AlertView {
    #[serde(flatten)]
    pub alert: AlertRecord,
    /// An active silence suppresses notifications for this alert.
    pub silenced: bool,
}

#[derive(Deserialize, IntoParams)]
pub struct AlertsQuery {
    /// Include alerts resolved since this unix time (default: 24 h ago).
    pub since: Option<i64>,
    pub limit: Option<u32>,
}

fn silenced(a: &AlertRecord, silences: &[Silence], now: i64) -> bool {
    silences
        .iter()
        .any(|s| s.matches(&a.rule_id, a.host.as_deref(), now))
}

/// Firing alerts and recently resolved ones.
#[utoipa::path(get, path = "/alerts", tag = "alerts", params(AlertsQuery),
    responses((status = 200, body = Vec<AlertView>)))]
pub async fn list(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Query(q): Query<AlertsQuery>,
) -> ApiResult<Json<Vec<AlertView>>> {
    let now = store::now();
    let since = q.since.unwrap_or(now - 86_400);
    let alerts = app
        .store
        .recent_alerts(since, q.limit.unwrap_or(500).min(5000))
        .await?;
    let silences = app.store.silences(0).await?;
    Ok(Json(
        alerts
            .into_iter()
            .map(|a| AlertView {
                silenced: a.state == "firing" && silenced(&a, &silences, now),
                alert: a,
            })
            .collect(),
    ))
}

#[derive(Serialize, ToSchema)]
pub struct AlertDetail {
    #[serde(flatten)]
    pub alert: AlertView,
    pub deliveries: Vec<Delivery>,
}

#[utoipa::path(get, path = "/alerts/{id}", tag = "alerts", params(("id" = i64, Path)),
    responses((status = 200, body = AlertDetail), (status = 404, body = crate::http::error::ErrorBody)))]
pub async fn get(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertDetail>> {
    let a = app.store.alert(id).await?;
    let silences = app.store.silences(0).await?;
    let deliveries = app.store.deliveries(id).await?;
    Ok(Json(AlertDetail {
        alert: AlertView {
            silenced: a.state == "firing" && silenced(&a, &silences, store::now()),
            alert: a,
        },
        deliveries,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct AckRequest {
    pub note: Option<String>,
}

/// Acknowledges a firing alert (operator). Reminders stop.
#[utoipa::path(post, path = "/alerts/{id}/ack", tag = "alerts", params(("id" = i64, Path)),
    request_body = AckRequest, responses((status = 200, body = AlertRecord)))]
pub async fn ack(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(id): Path<i64>,
    Json(req): Json<AckRequest>,
) -> ApiResult<Json<AlertRecord>> {
    auth.require(Role::Operator)?;
    let note = req
        .note
        .map(|n| n.trim().chars().take(500).collect::<String>())
        .filter(|n| !n.is_empty());
    let a = app
        .store
        .ack_alert(id, auth.name().to_string(), note.clone(), store::now())
        .await?;
    app.store
        .audit(
            auth.name(),
            "alert.ack",
            Some(&format!("alert:{id}")),
            serde_json::json!({"note": note, "summary": a.summary}),
        )
        .await?;
    let _ = app.events.send(Event::Alerts);
    Ok(Json(a))
}

/// Active silences and those that ended in the last 7 days.
#[utoipa::path(get, path = "/silences", tag = "alerts", responses((status = 200, body = Vec<Silence>)))]
pub async fn silences(State(app): State<Arc<App>>, _auth: Authed) -> ApiResult<Json<Vec<Silence>>> {
    Ok(Json(app.store.silences(7 * 86_400).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct SilenceRequest {
    /// Rule to silence; empty silences every rule.
    pub rule_id: Option<String>,
    /// Host to silence; empty silences every host.
    pub host: Option<String>,
    /// Duration such as `2h` or `1d` (max 30 days).
    pub duration: String,
    pub reason: String,
}

/// Creates a silence (operator).
#[utoipa::path(post, path = "/silences", tag = "alerts", request_body = SilenceRequest,
    responses((status = 200, body = Silence), (status = 400, body = crate::http::error::ErrorBody)))]
pub async fn create_silence(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<SilenceRequest>,
) -> ApiResult<Json<Silence>> {
    auth.require(Role::Operator)?;
    let d = common::Dur::parse(&req.duration).map_err(ApiError::bad_request)?;
    if d.as_secs() < 60 || d.as_secs() > 30 * 86_400 {
        return Err(ApiError::bad_request(
            "Duration must be between 1m and 30d.",
        ));
    }
    let reason = req.reason.trim().chars().take(500).collect::<String>();
    if reason.is_empty() {
        return Err(ApiError::bad_request(
            "Give a reason so others know why alerts are silenced.",
        ));
    }
    let rule = req.rule_id.filter(|r| !r.trim().is_empty());
    let host = req.host.filter(|h| !h.trim().is_empty());
    if let Some(h) = &host
        && app.fleet.get(h).is_none()
    {
        return Err(ApiError::bad_request(format!("Unknown host `{h}`.")));
    }
    let ends = store::now() + i64::try_from(d.as_secs()).unwrap_or(0);
    let s = app
        .store
        .create_silence(
            rule.clone(),
            host.clone(),
            reason.clone(),
            auth.name().to_string(),
            ends,
        )
        .await?;
    app.store
        .audit(
            auth.name(),
            "silence.create",
            Some(&format!("silence:{}", s.id)),
            serde_json::json!({"rule": rule, "host": host, "reason": reason, "ends_at": ends}),
        )
        .await?;
    let _ = app.events.send(Event::Alerts);
    Ok(Json(s))
}

/// Ends a silence now (operator).
#[utoipa::path(delete, path = "/silences/{id}", tag = "alerts", params(("id" = i64, Path)),
    responses((status = 204)))]
pub async fn expire_silence(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    app.store
        .expire_silence(id, auth.name().to_string())
        .await?;
    app.store
        .audit(
            auth.name(),
            "silence.expire",
            Some(&format!("silence:{id}")),
            serde_json::json!({}),
        )
        .await?;
    let _ = app.events.send(Event::Alerts);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct RuleView {
    pub id: String,
    pub expr: String,
    pub severity: String,
    pub summary: Option<String>,
    pub builtin: bool,
    pub hosts: Vec<String>,
    pub groups: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MetricInfo {
    pub name: String,
    pub unit: String,
    pub description: String,
    pub instance: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct RulesView {
    pub rules: Vec<RuleView>,
    pub metrics: Vec<MetricInfo>,
}

/// Configured and built-in alert rules, and the metrics rules can use.
#[utoipa::path(get, path = "/rules", tag = "alerts", responses((status = 200, body = RulesView)))]
pub async fn rules(State(app): State<Arc<App>>, _auth: Authed) -> Json<RulesView> {
    let a = &app.cfg.alerts;
    let builtin = |id: &str, expr: String, severity: &str| RuleView {
        id: id.into(),
        expr,
        severity: severity.into(),
        summary: None,
        builtin: true,
        hosts: vec![],
        groups: vec![],
        tags: vec![],
    };
    let mut rules = vec![
        builtin(
            alerts::HOST_UNREACHABLE,
            format!(
                "host unreachable or SSH authentication failing for {}",
                a.unreachable_after
            ),
            "critical",
        ),
        builtin(
            alerts::HOST_KEY_CHANGED,
            "SSH host key differs from the trusted key".into(),
            "critical",
        ),
        builtin(
            alerts::HOST_KEY_UNKNOWN,
            "SSH host key not trusted yet".into(),
            "warning",
        ),
        builtin(
            alerts::CERT_EXPIRY,
            format!(
                "certificate expires within {} days (critical within {})",
                a.cert_warning_days, a.cert_critical_days
            ),
            "warning",
        ),
    ];
    if a.failed_units {
        rules.push(builtin(
            alerts::FAILED_UNITS,
            "a systemd unit or OpenRC service has failed".into(),
            "warning",
        ));
    }
    rules.extend(a.rules.iter().map(|r| RuleView {
        id: r.id.clone(),
        expr: r.expr.clone(),
        severity: r.severity.as_str().into(),
        summary: r.summary.clone(),
        builtin: false,
        hosts: r.scope.hosts.clone(),
        groups: r.scope.groups.clone(),
        tags: r.scope.tags.clone(),
    }));
    Json(RulesView {
        rules,
        metrics: METRICS
            .iter()
            .map(|m| MetricInfo {
                name: m.name.into(),
                unit: m.unit.into(),
                description: m.description.into(),
                instance: m.instance.map(str::to_string),
            })
            .collect(),
    })
}
