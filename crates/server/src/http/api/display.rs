//! Read-only data for wall displays, available with a session or with a
//! revocable display token scoped to groups.

use crate::app::App;
use crate::auth::session::Authed;
use crate::auth::token_hash;
use crate::http::error::{ApiError, ApiResult};
use crate::state::AlertCounts;
use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::header;
use axum::http::request::Parts;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

/// Either a signed-in user (all hosts) or a display token (its groups).
pub struct DisplayAccess {
    pub groups: Option<Vec<String>>,
    pub name: String,
}

impl FromRequestParts<Arc<App>> for DisplayAccess {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, ApiError> {
        if let Some(token) = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        {
            if token.len() > 128 {
                return Err(ApiError::unauthorized());
            }
            let t = app
                .store
                .use_display_token(token_hash(token.trim()))
                .await?
                .ok_or_else(|| {
                    ApiError::new(
                        axum::http::StatusCode::UNAUTHORIZED,
                        "invalid_token",
                        "This display token is invalid or was revoked.",
                    )
                })?;
            let all = t.groups.iter().any(|g| g == "*");
            return Ok(DisplayAccess {
                groups: (!all).then_some(t.groups),
                name: t.name,
            });
        }
        let auth = Authed::from_request_parts(parts, app).await?;
        Ok(DisplayAccess {
            groups: None,
            name: auth.user.username,
        })
    }
}

#[derive(Serialize, ToSchema)]
pub struct DisplayHost {
    pub name: String,
    pub status: String,
    pub groups: Vec<String>,
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub disk_pct: Option<f64>,
    pub load1: Option<f64>,
    pub alerts: AlertCounts,
    pub stale: bool,
    pub last_success: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct DisplayAlert {
    pub severity: String,
    pub host: Option<String>,
    pub summary: String,
    pub started_at: i64,
    pub acknowledged: bool,
}

#[derive(Serialize, ToSchema)]
pub struct DisplayState {
    pub product: String,
    pub viewer: String,
    pub generated_at: i64,
    pub hosts: Vec<DisplayHost>,
    pub alerts: Vec<DisplayAlert>,
}

/// Host tiles and firing alerts for a wall display.
#[utoipa::path(get, path = "/display/state", tag = "display",
    responses((status = 200, body = DisplayState), (status = 401, body = crate::http::error::ErrorBody)),
    security(("display_token" = [])))]
pub async fn state(
    State(app): State<Arc<App>>,
    access: DisplayAccess,
) -> ApiResult<Json<DisplayState>> {
    let now = store::now();
    let visible = |groups: &[String]| match &access.groups {
        None => true,
        Some(allowed) => groups.iter().any(|g| allowed.contains(g)),
    };
    let hosts: Vec<DisplayHost> = app
        .fleet
        .summaries(now, app.stale_after(), false)
        .into_iter()
        .filter(|h| visible(&h.groups))
        .map(|h| DisplayHost {
            disk_pct: h.disk.as_ref().map(|d| d.used_pct),
            name: h.name,
            status: h.status,
            groups: h.groups,
            cpu_pct: h.cpu_pct,
            mem_pct: h.mem_pct,
            load1: h.load1,
            alerts: h.alerts,
            stale: h.stale,
            last_success: h.last_success,
        })
        .collect();
    let names: Vec<&str> = hosts.iter().map(|h| h.name.as_str()).collect();
    let alerts = app
        .store
        .firing_alerts()
        .await?
        .into_iter()
        .filter(|a| match &a.host {
            Some(h) => names.contains(&h.as_str()),
            None => access.groups.is_none(),
        })
        .map(|a| DisplayAlert {
            acknowledged: a.ack_at.is_some(),
            severity: a.severity,
            host: a.host,
            summary: a.summary,
            started_at: a.started_at,
        })
        .collect();
    Ok(Json(DisplayState {
        product: common::PRODUCT_NAME.to_string(),
        viewer: access.name,
        generated_at: now,
        hosts,
        alerts,
    }))
}
