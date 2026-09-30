//! Administration: users, host keys, display tokens and notification tests.

use crate::app::App;
use crate::auth::session::Authed;
use crate::auth::{password, random_token, token_hash};
use crate::http::error::{ApiError, ApiResult};
use crate::state::Event;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use common::Role;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use store::users::{DisplayToken, User, UserUpdate};
use utoipa::ToSchema;

/// All users (admin).
#[utoipa::path(get, path = "/users", tag = "admin", responses((status = 200, body = Vec<User>)))]
pub async fn users(State(app): State<Arc<App>>, auth: Authed) -> ApiResult<Json<Vec<User>>> {
    auth.require(Role::Admin)?;
    Ok(Json(app.store.users().await?))
}

#[derive(Deserialize, ToSchema)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub role: Role,
    pub display_name: Option<String>,
    /// Require a password change at first sign-in (default true).
    pub must_change_password: Option<bool>,
}

/// Creates a local user (admin).
#[utoipa::path(post, path = "/users", tag = "admin", request_body = NewUser,
    responses((status = 200, body = User), (status = 409, body = crate::http::error::ErrorBody)))]
pub async fn create_user(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<NewUser>,
) -> ApiResult<Json<User>> {
    auth.require(Role::Admin)?;
    let name = req.username.trim().to_string();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
    {
        return Err(ApiError::bad_request(
            "User names may contain letters, digits, `.`, `-`, `_` and `@`.",
        ));
    }
    password::check_policy(&req.password, &name).map_err(ApiError::bad_request)?;
    let pw = req.password.clone();
    let hash = tokio::task::spawn_blocking(move || password::hash(&pw))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    let user = app
        .store
        .create_user(name.clone(), Some(hash), req.role, None, req.display_name)
        .await?;
    let user = if req.must_change_password.unwrap_or(true) {
        app.store
            .update_user(
                user.id,
                UserUpdate {
                    must_change_password: Some(true),
                    password_hash: user.password_hash.clone(),
                    ..Default::default()
                },
            )
            .await?
    } else {
        user
    };
    app.store
        .audit(
            auth.name(),
            "user.create",
            Some(&name),
            serde_json::json!({"role": req.role}),
        )
        .await?;
    Ok(Json(user))
}

#[derive(Deserialize, ToSchema)]
pub struct UserPatch {
    pub role: Option<Role>,
    pub disabled: Option<bool>,
    /// New temporary password; the user must change it at next sign-in.
    pub password: Option<String>,
    /// Remove two-factor authentication (for a user who lost their device).
    pub reset_totp: Option<bool>,
}

/// Updates a user (admin).
#[utoipa::path(patch, path = "/users/{id}", tag = "admin", params(("id" = i64, Path)),
    request_body = UserPatch, responses((status = 200, body = User)))]
pub async fn update_user(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(id): Path<i64>,
    Json(req): Json<UserPatch>,
) -> ApiResult<Json<User>> {
    auth.require(Role::Admin)?;
    let target = app
        .store
        .user_by_id(id)
        .await?
        .ok_or_else(|| ApiError::not_found("User"))?;
    let demoting = req.role.is_some_and(|r| r != Role::Admin) || req.disabled == Some(true);
    if demoting
        && target.role == Role::Admin
        && !target.disabled
        && app.store.admin_count().await? <= 1
    {
        return Err(ApiError::conflict(
            "At least one active administrator must remain.",
        ));
    }
    if id == auth.user.id && req.disabled == Some(true) {
        return Err(ApiError::conflict("You cannot disable your own account."));
    }
    let password_reset = req.password.is_some();
    let mut update = UserUpdate {
        role: req.role,
        disabled: req.disabled,
        ..Default::default()
    };
    if let Some(pw) = req.password {
        password::check_policy(&pw, &target.username).map_err(ApiError::bad_request)?;
        let hash = tokio::task::spawn_blocking(move || password::hash(&pw))
            .await
            .map_err(ApiError::internal)?
            .map_err(ApiError::internal)?;
        update.password_hash = Some(hash);
        update.must_change_password = Some(true);
    }
    if req.reset_totp == Some(true) {
        update.totp = Some(None);
    }
    let user = app.store.update_user(id, update).await?;
    if password_reset || req.role.is_some() {
        app.store.delete_user_sessions(id, None).await?;
    }
    app.store
        .audit(
            auth.name(),
            "user.update",
            Some(&user.username),
            serde_json::json!({"role": req.role, "disabled": req.disabled, "password_reset": password_reset, "totp_reset": req.reset_totp}),
        )
        .await?;
    Ok(Json(user))
}

/// Deletes a user (admin).
#[utoipa::path(delete, path = "/users/{id}", tag = "admin", params(("id" = i64, Path)),
    responses((status = 204)))]
pub async fn delete_user(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    if id == auth.user.id {
        return Err(ApiError::conflict("You cannot delete your own account."));
    }
    let target = app
        .store
        .user_by_id(id)
        .await?
        .ok_or_else(|| ApiError::not_found("User"))?;
    if target.role == Role::Admin && !target.disabled && app.store.admin_count().await? <= 1 {
        return Err(ApiError::conflict(
            "At least one active administrator must remain.",
        ));
    }
    app.store.delete_user(id).await?;
    app.store
        .audit(
            auth.name(),
            "user.delete",
            Some(&target.username),
            serde_json::json!({}),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct PendingKey {
    /// Hosts that use this endpoint.
    pub hosts: Vec<String>,
    pub endpoint: String,
    pub algorithm: String,
    pub fingerprint: String,
    /// Previously trusted fingerprint, when the key changed.
    pub previous: Option<String>,
    pub seen_at: i64,
}

/// Host keys waiting for a decision (admin).
#[utoipa::path(get, path = "/hostkeys", tag = "admin", responses((status = 200, body = Vec<PendingKey>)))]
pub async fn host_keys(
    State(app): State<Arc<App>>,
    auth: Authed,
) -> ApiResult<Json<Vec<PendingKey>>> {
    auth.require(Role::Admin)?;
    let pending = app.hostkeys.pending();
    let hosts: Vec<(String, String, u16)> = app
        .fleet
        .with(|m| {
            m.values()
                .map(|h| {
                    (
                        h.cfg.name.clone(),
                        h.cfg.address().to_string(),
                        h.cfg.port(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(
        pending
            .into_iter()
            .map(|k| PendingKey {
                hosts: hosts
                    .iter()
                    .filter(|(_, a, p)| endpoint(a, *p) == k.endpoint)
                    .map(|(n, _, _)| n.clone())
                    .collect(),
                endpoint: k.endpoint,
                algorithm: k.algorithm,
                fingerprint: k.fingerprint,
                previous: k.previous,
                seen_at: k.seen_at,
            })
            .collect(),
    ))
}

fn endpoint(address: &str, port: u16) -> String {
    if port == 22 {
        address.to_string()
    } else {
        format!("[{address}]:{port}")
    }
}

#[derive(Deserialize, ToSchema)]
pub struct KeyDecision {
    pub host: String,
    /// The fingerprint the administrator verified; must match the key the
    /// host presents.
    pub fingerprint: Option<String>,
}

/// Trusts the pending key of a host after the admin confirmed its
/// fingerprint (admin).
#[utoipa::path(post, path = "/hostkeys/approve", tag = "admin", request_body = KeyDecision,
    responses((status = 204), (status = 409, body = crate::http::error::ErrorBody)))]
pub async fn approve_host_key(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<KeyDecision>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    let h = app
        .fleet
        .get(&req.host)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let fp = req
        .fingerprint
        .ok_or_else(|| ApiError::bad_request("Confirm the fingerprint you verified."))?;
    let pending = app.hostkeys.pending_for(h.cfg.address(), h.cfg.port());
    app.hostkeys
        .approve(h.cfg.address(), h.cfg.port(), &fp)
        .map_err(ApiError::conflict)?;
    app.store
        .audit(
            auth.name(),
            "hostkey.approve",
            Some(&h.cfg.name),
            serde_json::json!({"fingerprint": fp, "previous": pending.and_then(|p| p.previous)}),
        )
        .await?;
    app.wake(&h.cfg.name);
    let _ = app.events.send(Event::HostKeys);
    Ok(StatusCode::NO_CONTENT)
}

/// Discards a pending key without trusting it (admin).
#[utoipa::path(post, path = "/hostkeys/reject", tag = "admin", request_body = KeyDecision,
    responses((status = 204)))]
pub async fn reject_host_key(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<KeyDecision>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    let h = app
        .fleet
        .get(&req.host)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    app.hostkeys.reject(h.cfg.address(), h.cfg.port());
    app.store
        .audit(
            auth.name(),
            "hostkey.reject",
            Some(&h.cfg.name),
            serde_json::json!({"fingerprint": req.fingerprint}),
        )
        .await?;
    let _ = app.events.send(Event::HostKeys);
    Ok(StatusCode::NO_CONTENT)
}

/// Display tokens (admin).
#[utoipa::path(get, path = "/display-tokens", tag = "admin", responses((status = 200, body = Vec<DisplayToken>)))]
pub async fn display_tokens(
    State(app): State<Arc<App>>,
    auth: Authed,
) -> ApiResult<Json<Vec<DisplayToken>>> {
    auth.require(Role::Admin)?;
    Ok(Json(app.store.display_tokens().await?))
}

#[derive(Deserialize, ToSchema)]
pub struct NewDisplayToken {
    pub name: String,
    /// Groups visible with this token; `*` means all hosts.
    pub groups: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreatedDisplayToken {
    pub token: DisplayToken,
    /// Shown once. Open `/display#token=<secret>` on the wall screen.
    pub secret: String,
}

/// Creates a revocable, read-only token for wall displays (admin).
#[utoipa::path(post, path = "/display-tokens", tag = "admin", request_body = NewDisplayToken,
    responses((status = 200, body = CreatedDisplayToken)))]
pub async fn create_display_token(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<NewDisplayToken>,
) -> ApiResult<Json<CreatedDisplayToken>> {
    auth.require(Role::Admin)?;
    let name = req.name.trim().chars().take(64).collect::<String>();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            "Give the token a name, such as the screen's location.",
        ));
    }
    let groups: Vec<String> = req
        .groups
        .into_iter()
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .take(50)
        .collect();
    if groups.is_empty() {
        return Err(ApiError::bad_request(
            "Choose at least one group, or `*` for all hosts.",
        ));
    }
    let secret = format!("hsd_{}", random_token());
    let token = app
        .store
        .create_display_token(
            name.clone(),
            token_hash(&secret),
            groups.clone(),
            auth.name().to_string(),
        )
        .await?;
    app.store
        .audit(
            auth.name(),
            "display_token.create",
            Some(&name),
            serde_json::json!({"groups": groups, "id": token.id}),
        )
        .await?;
    Ok(Json(CreatedDisplayToken { token, secret }))
}

/// Revokes a display token (admin).
#[utoipa::path(delete, path = "/display-tokens/{id}", tag = "admin", params(("id" = i64, Path)),
    responses((status = 204)))]
pub async fn revoke_display_token(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    app.store.revoke_display_token(id).await?;
    app.store
        .audit(
            auth.name(),
            "display_token.revoke",
            Some(&format!("token:{id}")),
            serde_json::json!({}),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct TestResult {
    pub ok: bool,
    pub error: Option<String>,
}

/// Sends a test notification through a channel (admin).
#[utoipa::path(post, path = "/notify/{channel}/test", tag = "admin", params(("channel" = String, Path)),
    responses((status = 200, body = TestResult)))]
pub async fn test_notification(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(channel): Path<String>,
) -> ApiResult<Json<TestResult>> {
    auth.require(Role::Admin)?;
    let ch = app
        .channels
        .iter()
        .find(|c| c.id == channel)
        .ok_or_else(|| ApiError::not_found("Channel"))?;
    let now = store::now();
    let ev = alerts::notify::Event {
        status: alerts::notify::Status::Test,
        alert: store::alerts::AlertRecord {
            id: 0,
            fingerprint: "test".into(),
            rule_id: "test".into(),
            host: None,
            instance: None,
            severity: "info".into(),
            state: "firing".into(),
            summary: format!("Test notification sent by {}", auth.name()),
            value: None,
            started_at: now,
            resolved_at: None,
            last_notified_at: None,
            ack_by: None,
            ack_at: None,
            ack_note: None,
        },
        link: app.link("/alerts"),
        product: common::PRODUCT_NAME.to_string(),
    };
    let r = app.notifier.send(ch, &ev).await;
    app.store
        .audit(
            auth.name(),
            "notify.test",
            Some(&channel),
            serde_json::json!({"ok": r.is_ok(), "error": r.as_ref().err()}),
        )
        .await?;
    Ok(Json(TestResult {
        ok: r.is_ok(),
        error: r.err(),
    }))
}
