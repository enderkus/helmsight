//! Fleet, host detail, history, changes and comparison.

use super::time_range;
use crate::app::App;
use crate::auth::session::Authed;
use crate::http::error::{ApiError, ApiResult};
use crate::state::{AuthSource, HostSummary, Timed};
use axum::Json;
use axum::extract::{Path, Query, State};
use collect::model::{Basics, Medium, Metrics, Probe, RebootStatus, UpdateReport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use store::inventory::{Change, diff};
use store::metrics::{QueryResult, SeriesQuery};
use store::security::{AuthBucket, TopSource};
use utoipa::{IntoParams, ToSchema};

/// All hosts with their current status.
#[utoipa::path(get, path = "/hosts", tag = "hosts", responses((status = 200, body = Vec<HostSummary>)))]
pub async fn list(State(app): State<Arc<App>>, _auth: Authed) -> Json<Vec<HostSummary>> {
    Json(app.fleet.summaries(store::now(), app.stale_after(), true))
}

#[derive(Serialize, ToSchema)]
pub struct InventoryView {
    pub ts: i64,
    pub os: collect::model::OsRelease,
    pub kernel: collect::model::Kernel,
    pub cpu_model: Option<String>,
    pub virtualization: Option<String>,
    pub reboot: RebootStatus,
    pub package_manager: Option<String>,
    pub package_count: Option<usize>,
    pub packages_unavailable: Option<String>,
    pub enabled_units: Probe<Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionInfo {
    pub last_attempt: Option<i64>,
    pub last_success: Option<i64>,
    pub duration_ms: Option<u64>,
    pub missing: Vec<String>,
    pub truncated: bool,
    pub interval: u64,
    pub clock_skew: Option<f64>,
}

#[derive(Serialize, ToSchema)]
pub struct HostDetail {
    pub summary: HostSummary,
    pub collection: CollectionInfo,
    pub metrics: Option<Timed<Metrics>>,
    pub medium: Option<Timed<Medium>>,
    pub inventory: Option<InventoryView>,
    pub updates: Option<Timed<Probe<UpdateReport>>>,
    pub auth: Option<Timed<AuthSource>>,
    pub basics: Option<Basics>,
}

/// Everything currently known about one host.
#[utoipa::path(get, path = "/hosts/{name}", tag = "hosts",
    params(("name" = String, Path, description = "Host name")),
    responses((status = 200, body = HostDetail), (status = 404, body = crate::http::error::ErrorBody)))]
pub async fn detail(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Path(name): Path<String>,
) -> ApiResult<Json<HostDetail>> {
    let h = app
        .fleet
        .get(&name)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let now = store::now();
    let inventory = h.inventory.as_ref().map(|t| {
        let inv = &t.data;
        let (pm, count, unavailable) = match &inv.packages {
            Probe::Ok { source, data } => (Some(source.clone()), Some(data.len()), None),
            Probe::Na { reason } => (None, None, Some(reason.clone())),
        };
        InventoryView {
            ts: t.ts,
            os: inv.os.clone(),
            kernel: inv.kernel.clone(),
            cpu_model: inv.cpu_model.clone(),
            virtualization: inv.virtualization.clone(),
            reboot: inv.reboot.clone(),
            package_manager: pm,
            package_count: count,
            packages_unavailable: unavailable,
            enabled_units: inv.enabled_units.clone(),
        }
    });
    Ok(Json(HostDetail {
        summary: h.summary(now, app.stale_after(), true),
        collection: CollectionInfo {
            last_attempt: h.last_attempt,
            last_success: h.last_success,
            duration_ms: h.last_duration_ms,
            missing: h.missing.clone(),
            truncated: h.truncated,
            interval: app.cfg.collect.interval.as_secs(),
            clock_skew: h.clock_skew,
        },
        metrics: h.metrics,
        medium: h.medium,
        inventory,
        updates: h.updates,
        auth: h.auth,
        basics: h.basics,
    }))
}

#[derive(Deserialize, IntoParams)]
pub struct MetricsQuery {
    /// Comma-separated series keys; a trailing `*` matches a prefix
    /// (e.g. `cpu.*,mem.used_pct,fs.used_pct:*`).
    pub series: String,
    /// Unix seconds; defaults to one hour ago.
    pub from: Option<i64>,
    /// Unix seconds; defaults to now.
    pub to: Option<i64>,
    /// Maximum points per series (default 600, max 2000).
    pub points: Option<usize>,
}

/// Historical metric series for a host.
#[utoipa::path(get, path = "/hosts/{name}/metrics", tag = "hosts",
    params(("name" = String, Path), MetricsQuery),
    responses((status = 200, body = QueryResult)))]
pub async fn metrics(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Path(name): Path<String>,
    Query(q): Query<MetricsQuery>,
) -> ApiResult<Json<QueryResult>> {
    let h = app
        .fleet
        .get(&name)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let patterns: Vec<String> = q
        .series
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 600)
        .take(50)
        .map(str::to_string)
        .collect();
    if patterns.is_empty() {
        return Err(ApiError::bad_request("Specify at least one series."));
    }
    let (from, to) = time_range(q.from, q.to, 3600);
    let r = &app.cfg.retention;
    let result = app
        .store
        .query(SeriesQuery {
            host_id: h.id,
            patterns,
            from,
            to,
            max_points: q.points.unwrap_or(600).clamp(10, 2000),
            retention_raw: i64::try_from(r.raw.as_secs()).unwrap_or(86_400),
            retention_minute: i64::try_from(r.minute.as_secs()).unwrap_or(7 * 86_400),
        })
        .await?;
    Ok(Json(result))
}

#[derive(Serialize, ToSchema)]
pub struct ChangeView {
    pub id: i64,
    pub host: String,
    pub ts: i64,
    #[serde(flatten)]
    pub change: Change,
}

#[derive(Deserialize, IntoParams)]
pub struct ChangesQuery {
    /// Unix seconds; defaults to 7 days ago.
    pub since: Option<i64>,
    pub limit: Option<u32>,
}

fn host_names(app: &App) -> BTreeMap<i64, String> {
    app.fleet
        .with(|m| m.values().map(|h| (h.id, h.cfg.name.clone())).collect())
        .unwrap_or_default()
}

/// Inventory changes of one host, newest first.
#[utoipa::path(get, path = "/hosts/{name}/changes", tag = "hosts",
    params(("name" = String, Path), ChangesQuery), responses((status = 200, body = Vec<ChangeView>)))]
pub async fn changes(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Path(name): Path<String>,
    Query(q): Query<ChangesQuery>,
) -> ApiResult<Json<Vec<ChangeView>>> {
    let h = app
        .fleet
        .get(&name)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let since = q.since.unwrap_or(store::now() - 7 * 86_400);
    let rows = app
        .store
        .changes(Some(h.id), since, q.limit.unwrap_or(1000).min(5000))
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ChangeView {
                id: r.id,
                host: name.clone(),
                ts: r.ts,
                change: r.change,
            })
            .collect(),
    ))
}

/// Inventory changes across all hosts, newest first.
#[utoipa::path(get, path = "/changes", tag = "hosts", params(ChangesQuery),
    responses((status = 200, body = Vec<ChangeView>)))]
pub async fn fleet_changes(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Query(q): Query<ChangesQuery>,
) -> ApiResult<Json<Vec<ChangeView>>> {
    let names = host_names(&app);
    let since = q.since.unwrap_or(store::now() - 86_400);
    let rows = app
        .store
        .changes(None, since, q.limit.unwrap_or(1000).min(5000))
        .await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|r| {
                Some(ChangeView {
                    id: r.id,
                    host: names.get(&r.host_id)?.clone(),
                    ts: r.ts,
                    change: r.change,
                })
            })
            .collect(),
    ))
}

#[derive(Serialize, ToSchema)]
pub struct AuthReport {
    pub source: Option<String>,
    pub unavailable: Option<String>,
    pub total: i64,
    pub histogram: Vec<AuthBucket>,
    pub top_ips: Vec<TopSource>,
    pub top_users: Vec<TopSource>,
}

#[derive(Serialize, ToSchema)]
pub struct HostSecurity {
    pub auth: AuthReport,
    pub updates: Option<Timed<Probe<UpdateReport>>>,
    pub reboot: Option<RebootStatus>,
    pub baseline: Option<String>,
    /// Listening ports present on this host but not on its baseline.
    pub new_ports: Vec<Change>,
}

#[derive(Deserialize, IntoParams)]
pub struct RangeQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

pub(crate) async fn auth_report(
    app: &App,
    host_id: Option<i64>,
    from: i64,
    source: Option<&AuthSource>,
) -> ApiResult<AuthReport> {
    let span = store::now() - from;
    let bucket = if span <= 2 * 86_400 { 3600 } else { 6 * 3600 };
    let histogram = app.store.auth_histogram(host_id, from, bucket).await?;
    let total = histogram.iter().map(|b| b.count).sum();
    Ok(AuthReport {
        source: source.and_then(|s| s.source.clone()),
        unavailable: source.and_then(|s| s.unavailable.clone()),
        total,
        histogram,
        top_ips: app.store.auth_top(host_id, from, "ip", 10).await?,
        top_users: app.store.auth_top(host_id, from, "user", 10).await?,
    })
}

/// Ports listening on `name` that its baseline host does not expose.
pub(crate) async fn baseline_ports(
    app: &App,
    name: &str,
    baseline: Option<&str>,
) -> ApiResult<Vec<Change>> {
    let (Some(h), Some(b)) = (app.fleet.get(name), baseline.and_then(|b| app.fleet.get(b))) else {
        return Ok(Vec::new());
    };
    let (Some((_, snap)), Some((_, base))) = (
        app.store.snapshot(h.id).await?,
        app.store.snapshot(b.id).await?,
    ) else {
        return Ok(Vec::new());
    };
    Ok(diff(&base, &snap)
        .into_iter()
        .filter(|c| c.kind == "port" && c.action != "removed")
        .collect())
}

/// Security view of one host: failed logins, updates, reboot, new ports.
#[utoipa::path(get, path = "/hosts/{name}/security", tag = "hosts",
    params(("name" = String, Path), RangeQuery), responses((status = 200, body = HostSecurity)))]
pub async fn host_security(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Path(name): Path<String>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<HostSecurity>> {
    let h = app
        .fleet
        .get(&name)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let (from, _) = time_range(q.from, q.to, 7 * 86_400);
    let auth = auth_report(&app, Some(h.id), from, h.auth.as_ref().map(|a| &a.data)).await?;
    let new_ports = baseline_ports(&app, &name, h.cfg.baseline.as_deref()).await?;
    Ok(Json(HostSecurity {
        auth,
        updates: h.updates.clone(),
        reboot: h.inventory.as_ref().map(|i| i.data.reboot.clone()),
        baseline: h.cfg.baseline.clone(),
        new_ports,
    }))
}

#[derive(Deserialize, IntoParams)]
pub struct CompareQuery {
    /// Host to inspect.
    pub a: String,
    /// Reference host. Defaults to the baseline configured for `a`.
    pub b: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Comparison {
    pub a: String,
    pub b: String,
    pub a_ts: Option<i64>,
    pub b_ts: Option<i64>,
    /// Differences from `b` (reference) to `a`: `added` means present only
    /// on `a`, `removed` only on `b`, `changed` differs.
    pub differences: Vec<Change>,
    pub counts: BTreeMap<String, usize>,
    /// Parts that could not be compared (unknown on one side).
    pub incomplete: Vec<String>,
}

/// Compares the inventory of two hosts, or a host with its baseline.
#[utoipa::path(get, path = "/compare", tag = "hosts", params(CompareQuery),
    responses((status = 200, body = Comparison), (status = 400, body = crate::http::error::ErrorBody)))]
pub async fn compare(
    State(app): State<Arc<App>>,
    _auth: Authed,
    Query(q): Query<CompareQuery>,
) -> ApiResult<Json<Comparison>> {
    let a = app
        .fleet
        .get(&q.a)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let b_name =
        q.b.clone()
            .or_else(|| a.cfg.baseline.clone())
            .ok_or_else(|| {
                ApiError::bad_request(
                    "Choose a host to compare with; this host has no baseline configured.",
                )
            })?;
    let b = app
        .fleet
        .get(&b_name)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let sa = app.store.snapshot(a.id).await?;
    let sb = app.store.snapshot(b.id).await?;
    let (Some((a_ts, sa)), Some((b_ts, sb))) = (sa, sb) else {
        return Ok(Json(Comparison {
            a: q.a,
            b: b_name,
            a_ts: None,
            b_ts: None,
            differences: Vec::new(),
            counts: BTreeMap::new(),
            incomplete: vec!["inventory not collected yet".into()],
        }));
    };
    let mut incomplete = Vec::new();
    for (part, x, y) in [
        ("packages", sa.has_packages, sb.has_packages),
        ("ports", sa.has_ports, sb.has_ports),
        ("units", sa.has_units, sb.has_units),
    ] {
        if !(x && y) {
            incomplete.push(part.to_string());
        }
    }
    let differences = diff(&sb, &sa);
    let mut counts = BTreeMap::new();
    for d in &differences {
        *counts.entry(d.kind.clone()).or_insert(0) += 1;
    }
    Ok(Json(Comparison {
        a: q.a,
        b: b_name,
        a_ts: Some(a_ts),
        b_ts: Some(b_ts),
        differences,
        counts,
        incomplete,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct Labels {
    pub groups: BTreeMap<String, usize>,
    pub tags: BTreeMap<String, usize>,
}

/// Groups and tags in use, with host counts.
#[utoipa::path(get, path = "/groups", tag = "hosts", responses((status = 200, body = Labels)))]
pub async fn groups(State(app): State<Arc<App>>, _auth: Authed) -> Json<Labels> {
    let mut l = Labels {
        groups: BTreeMap::new(),
        tags: BTreeMap::new(),
    };
    app.fleet.with(|m| {
        for h in m.values() {
            for g in &h.cfg.groups {
                *l.groups.entry(g.clone()).or_insert(0) += 1;
            }
            for t in &h.cfg.tags {
                *l.tags.entry(t.clone()).or_insert(0) += 1;
            }
        }
    });
    Json(l)
}
