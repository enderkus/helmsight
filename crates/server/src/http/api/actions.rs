//! Pre-approved actions and the audit log.

use crate::app::App;
use crate::auth::session::Authed;
use crate::http::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{Path, Query, State};
use common::Role;
use common::selector::Selector;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use store::audit::{AuditEntry, ChainStatus};
use utoipa::{IntoParams, ToSchema};

const OUTPUT_LIMIT: usize = 64 * 1024;
const AUDIT_OUTPUT: usize = 4 * 1024;

#[derive(Serialize, ToSchema)]
pub struct ActionView {
    pub id: String,
    pub label: String,
    pub description: String,
    /// The exact command that will run.
    pub command: String,
    pub role: Role,
    pub timeout_secs: u64,
    /// Hosts where the action may run.
    pub hosts: Vec<String>,
}

fn target_hosts(app: &App, sel: &Selector) -> Vec<String> {
    app.fleet
        .with(|m| {
            m.values()
                .filter(|h| !h.cfg.disabled && sel.matches(&h.cfg))
                .map(|h| h.cfg.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Actions the signed-in user may run. Empty when none are configured.
#[utoipa::path(get, path = "/actions", tag = "actions", responses((status = 200, body = Vec<ActionView>)))]
pub async fn list(State(app): State<Arc<App>>, auth: Authed) -> Json<Vec<ActionView>> {
    if app.local_mode {
        return Json(Vec::new());
    }
    Json(
        app.cfg
            .actions
            .iter()
            .filter(|a| auth.user.role.allows(a.role))
            .map(|a| ActionView {
                id: a.id.clone(),
                label: a.label.clone(),
                description: a.description.clone(),
                command: a.command.clone(),
                role: a.role,
                timeout_secs: a.timeout.as_secs(),
                hosts: target_hosts(&app, &a.targets),
            })
            .collect(),
    )
}

#[derive(Deserialize, ToSchema)]
pub struct RunRequest {
    pub host: String,
}

#[derive(Serialize, ToSchema)]
pub struct RunResult {
    pub action: String,
    pub host: String,
    pub command: String,
    pub exit_status: Option<u32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub duration_ms: u64,
    pub audit_id: i64,
}

struct Running<'a> {
    app: &'a App,
    key: String,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut set) = self.app.running_actions.lock() {
            set.remove(&self.key);
        }
    }
}

fn clip(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or("").to_string()
}

/// Runs an action on one host. Only the configured command runs; the
/// request selects the action and the host, nothing else.
#[utoipa::path(post, path = "/actions/{id}/run", tag = "actions", params(("id" = String, Path)),
    request_body = RunRequest,
    responses((status = 200, body = RunResult), (status = 403, body = crate::http::error::ErrorBody),
              (status = 409, body = crate::http::error::ErrorBody)))]
pub async fn run(
    State(app): State<Arc<App>>,
    auth: Authed,
    Path(id): Path<String>,
    Json(req): Json<RunRequest>,
) -> ApiResult<Json<RunResult>> {
    if app.local_mode {
        return Err(ApiError::not_found("Action"));
    }
    let action = app
        .cfg
        .actions
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| ApiError::not_found("Action"))?;
    auth.require(action.role)?;
    let host = app
        .fleet
        .get(&req.host)
        .ok_or_else(|| ApiError::not_found("Host"))?;
    if host.cfg.disabled || !action.targets.matches(&host.cfg) {
        return Err(ApiError::forbidden(format!(
            "Action `{}` is not permitted on host `{}`.",
            action.id, host.cfg.name
        )));
    }
    let exec = app
        .executors
        .get(&host.cfg.name)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Host"))?;
    let key = format!("{}@{}", action.id, host.cfg.name);
    {
        let mut set = app.running_actions.lock().map_err(ApiError::internal)?;
        if !set.insert(key.clone()) {
            return Err(ApiError::conflict(
                "This action is already running on this host.",
            ));
        }
    }
    let _running = Running { app: &app, key };
    app.store
        .audit(
            auth.name(),
            "action.start",
            Some(&host.cfg.name),
            serde_json::json!({"action": action.id, "command": action.command}),
        )
        .await?;
    tracing::info!(user = %auth.name(), action = %action.id, host = %host.cfg.name, "running action");
    let result = exec
        .exec(&action.command, None, OUTPUT_LIMIT, action.timeout.0)
        .await;
    let (exit, stdout, stderr, truncated, duration_ms, error) = match &result {
        Ok(out) => (
            out.exit_status,
            clip(&out.stdout, OUTPUT_LIMIT),
            clip(&out.stderr, OUTPUT_LIMIT),
            out.truncated,
            u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
            None,
        ),
        Err(e) => (
            None,
            String::new(),
            String::new(),
            false,
            0,
            Some(e.to_string()),
        ),
    };
    let audit_id = app
        .store
        .audit(
            auth.name(),
            "action.run",
            Some(&host.cfg.name),
            serde_json::json!({
                "action": action.id,
                "command": action.command,
                "exit_status": exit,
                "error": error,
                "duration_ms": duration_ms,
                "stdout": clip(stdout.as_bytes(), AUDIT_OUTPUT),
                "stderr": clip(stderr.as_bytes(), AUDIT_OUTPUT),
                "truncated": truncated || stdout.len() > AUDIT_OUTPUT || stderr.len() > AUDIT_OUTPUT,
            }),
        )
        .await?;
    if let Some(e) = error {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            "action_failed",
            format!("The action could not run: {e}"),
        ));
    }
    Ok(Json(RunResult {
        action: action.id.clone(),
        host: host.cfg.name.clone(),
        command: action.command.clone(),
        exit_status: exit,
        stdout,
        stderr,
        truncated,
        duration_ms,
        audit_id,
    }))
}

#[derive(Deserialize, IntoParams)]
pub struct AuditQuery {
    /// Return entries with an id lower than this (paging).
    pub before: Option<i64>,
    /// Filter by action prefix, e.g. `action.` or `user.`.
    pub action: Option<String>,
    pub limit: Option<u32>,
}

/// Audit log entries, newest first (admin).
#[utoipa::path(get, path = "/audit", tag = "actions", params(AuditQuery),
    responses((status = 200, body = Vec<AuditEntry>)))]
pub async fn audit(
    State(app): State<Arc<App>>,
    auth: Authed,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<AuditEntry>>> {
    auth.require(Role::Admin)?;
    let action = q.action.filter(|a| !a.is_empty() && a.len() <= 64);
    Ok(Json(
        app.store
            .audit_entries(q.before, action, q.limit.unwrap_or(100).min(1000))
            .await?,
    ))
}

/// Verifies the audit log hash chain (admin).
#[utoipa::path(get, path = "/audit/verify", tag = "actions", responses((status = 200, body = ChainStatus)))]
pub async fn audit_verify(
    State(app): State<Arc<App>>,
    auth: Authed,
) -> ApiResult<Json<ChainStatus>> {
    auth.require(Role::Admin)?;
    Ok(Json(app.store.verify_audit_chain().await?))
}
