//! Sign-in, sign-out, first-admin setup, password change, TOTP and OIDC.

use crate::app::App;
use crate::auth::session::{self, Authed, client_ip};
use crate::auth::{ct_eq, password, token_hash, totp};
use crate::http::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::Role;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use store::users::{User, UserUpdate};
use utoipa::ToSchema;

/// Request metadata used for sessions and rate limiting.
pub struct ClientInfo {
    pub ip: String,
    pub user_agent: Option<String>,
}

impl FromRequestParts<Arc<App>> for ClientInfo {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, ApiError> {
        Ok(ClientInfo {
            ip: client_ip(app, parts),
            user_agent: parts
                .headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
        })
    }
}

#[derive(Serialize, ToSchema)]
pub struct SessionInfo {
    pub user: User,
    /// Send this value in the `X-CSRF-Token` header of state-changing requests.
    pub csrf: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    /// Six-digit code, required when two-factor authentication is enabled.
    pub totp: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetupRequest {
    /// One-time setup token printed in the server log.
    pub token: String,
    pub username: String,
    pub password: String,
}

fn valid_username(u: &str) -> bool {
    !u.is_empty()
        && u.len() <= 64
        && u.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
}

async fn start_session(app: &App, user: &User, client: &ClientInfo) -> ApiResult<Response> {
    let (token, csrf) = session::create(
        app,
        user.id,
        Some(client.ip.clone()),
        client.user_agent.clone(),
    )
    .await?;
    let user = app
        .store
        .update_user(
            user.id,
            UserUpdate {
                touch_login: true,
                ..Default::default()
            },
        )
        .await?;
    let ttl = i64::try_from(app.cfg.auth.session_ttl.as_secs()).unwrap_or(43_200);
    let mut resp = Json(SessionInfo { user, csrf }).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        session::session_cookie(app, &token, ttl),
    );
    Ok(resp)
}

async fn hash_blocking(pw: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || password::hash(&pw))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

async fn verify_blocking(stored: Option<String>, pw: String) -> bool {
    tokio::task::spawn_blocking(move || password::verify(stored.as_deref(), &pw))
        .await
        .unwrap_or(false)
}

/// Creates the first administrator using the one-time setup token.
#[utoipa::path(post, path = "/auth/setup", tag = "auth", request_body = SetupRequest,
    responses((status = 200, body = SessionInfo), (status = 403, body = crate::http::error::ErrorBody)))]
pub async fn setup(
    State(app): State<Arc<App>>,
    client: ClientInfo,
    Json(req): Json<SetupRequest>,
) -> ApiResult<Response> {
    if let Some(wait) = app.limiter.check(&client.ip, "setup") {
        return Err(rate_limited(wait.as_secs()));
    }
    let expected = app.setup_token.lock().ok().and_then(|t| t.clone());
    let Some(expected) = expected else {
        return Err(ApiError::forbidden(
            "Setup is already complete. Sign in instead.",
        ));
    };
    if !ct_eq(&token_hash(req.token.trim()), &expected) {
        app.limiter.failure(&client.ip, "setup");
        return Err(ApiError::forbidden(
            "Invalid setup token. Use the token printed in the server log at startup.",
        ));
    }
    if !valid_username(&req.username) {
        return Err(ApiError::bad_request(
            "User names may contain letters, digits, `.`, `-`, `_` and `@`.",
        ));
    }
    password::check_policy(&req.password, &req.username).map_err(ApiError::bad_request)?;
    let hash = hash_blocking(req.password).await?;
    let user = app
        .store
        .create_user(req.username.clone(), Some(hash), Role::Admin, None, None)
        .await?;
    if let Ok(mut t) = app.setup_token.lock() {
        *t = None;
    }
    app.store
        .audit(
            &user.username,
            "user.setup",
            Some(&user.username),
            serde_json::json!({"ip": client.ip}),
        )
        .await?;
    tracing::info!(user = %user.username, "first administrator created");
    start_session(&app, &user, &client).await
}

fn rate_limited(secs: u64) -> ApiError {
    let mut e = ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        format!(
            "Too many failed attempts. Try again in {} minutes.",
            secs.div_ceil(60).max(1)
        ),
    );
    e.retry_after = Some(secs.max(1));
    e
}

/// Signs in with a local account.
#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginRequest,
    responses((status = 200, body = SessionInfo), (status = 401, body = crate::http::error::ErrorBody),
              (status = 429, body = crate::http::error::ErrorBody)))]
pub async fn login(
    State(app): State<Arc<App>>,
    client: ClientInfo,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Response> {
    if app.cfg.auth.disable_local_login {
        return Err(ApiError::forbidden(
            "Local sign-in is disabled. Use single sign-on.",
        ));
    }
    let username = req.username.trim().to_string();
    if username.len() > 64 || req.password.len() > password::MAX_LENGTH * 4 {
        return Err(ApiError::bad_request("Invalid credentials format."));
    }
    if let Some(wait) = app.limiter.check(&client.ip, &username) {
        return Err(rate_limited(wait.as_secs()));
    }
    let user = app.store.user_by_name(username.clone()).await?;
    let ok = verify_blocking(
        user.as_ref().and_then(|u| u.password_hash.clone()),
        req.password,
    )
    .await;
    let fail = |app: &App| {
        app.limiter.failure(&client.ip, &username);
        tracing::info!(user = %username, ip = %client.ip, "failed sign-in");
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Incorrect user name or password.",
        )
    };
    let Some(user) = user.filter(|_| ok) else {
        return Err(fail(&app));
    };
    if user.disabled {
        return Err(fail(&app));
    }
    if user.totp_enabled {
        let Some(code) = req.totp.as_deref().filter(|c| !c.trim().is_empty()) else {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "totp_required",
                "Enter the six-digit code from your authenticator app.",
            ));
        };
        let secret = user
            .totp_secret
            .as_deref()
            .and_then(|s| app.key.open(s, &format!("totp:{}", user.id)).ok());
        let now = u64::try_from(store::now()).unwrap_or(0);
        let valid = secret.is_some_and(|s| app.totp_guard.verify(user.id, &s, code, now));
        if !valid {
            app.limiter.failure(&client.ip, &username);
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_totp",
                "The code is incorrect or was already used. Wait for the next code and try again.",
            ));
        }
    }
    app.limiter.success(&username);
    tracing::info!(user = %user.username, ip = %client.ip, "signed in");
    start_session(&app, &user, &client).await
}

/// Ends the current session.
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204)))]
pub async fn logout(State(app): State<Arc<App>>, auth: Authed) -> ApiResult<Response> {
    app.store.delete_session(auth.session_hash).await?;
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, session::clear_cookie(&app));
    Ok(resp)
}

/// Returns the signed-in user and the CSRF token.
#[utoipa::path(get, path = "/auth/me", tag = "auth",
    responses((status = 200, body = SessionInfo), (status = 401, body = crate::http::error::ErrorBody)))]
pub async fn me(auth: Authed) -> Json<SessionInfo> {
    Json(SessionInfo {
        user: auth.user,
        csrf: auth.csrf,
    })
}

#[derive(Deserialize, ToSchema)]
pub struct PasswordChange {
    pub current: String,
    pub new: String,
}

/// Changes the password of the signed-in user and ends other sessions.
#[utoipa::path(post, path = "/auth/password", tag = "auth", request_body = PasswordChange,
    responses((status = 204), (status = 400, body = crate::http::error::ErrorBody)))]
pub async fn change_password(
    State(app): State<Arc<App>>,
    auth: Authed,
    client: ClientInfo,
    Json(req): Json<PasswordChange>,
) -> ApiResult<StatusCode> {
    if auth.user.oidc_subject.is_some() && auth.user.password_hash.is_none() {
        return Err(ApiError::bad_request(
            "Single sign-on accounts have no local password.",
        ));
    }
    if let Some(wait) = app.limiter.check(&client.ip, auth.name()) {
        return Err(rate_limited(wait.as_secs()));
    }
    if !verify_blocking(auth.user.password_hash.clone(), req.current).await {
        app.limiter.failure(&client.ip, auth.name());
        return Err(ApiError::bad_request("The current password is incorrect."));
    }
    password::check_policy(&req.new, auth.name()).map_err(ApiError::bad_request)?;
    let hash = hash_blocking(req.new).await?;
    app.store
        .update_user(
            auth.user.id,
            UserUpdate {
                password_hash: Some(hash),
                must_change_password: Some(false),
                ..Default::default()
            },
        )
        .await?;
    app.store
        .delete_user_sessions(auth.user.id, Some(auth.session_hash.clone()))
        .await?;
    app.store
        .audit(
            auth.name(),
            "user.password",
            Some(auth.name()),
            serde_json::json!({}),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct TotpSetup {
    /// Base32 secret for manual entry.
    pub secret: String,
    pub otpauth_url: String,
    /// QR code of `otpauth_url` as a `data:image/svg+xml;base64,...` URL.
    pub qr: String,
}

/// Starts TOTP enrolment: generates a secret that becomes active after
/// `/auth/totp/enable` confirms a valid code.
#[utoipa::path(post, path = "/auth/totp/setup", tag = "auth", responses((status = 200, body = TotpSetup)))]
pub async fn totp_setup(State(app): State<Arc<App>>, auth: Authed) -> ApiResult<Json<TotpSetup>> {
    if auth.user.totp_enabled {
        return Err(ApiError::conflict(
            "Two-factor authentication is already enabled.",
        ));
    }
    let secret = common::secrets::random_bytes::<20>();
    let sealed = app.key.seal(&secret, &format!("totp:{}", auth.user.id));
    app.store
        .update_user(
            auth.user.id,
            UserUpdate {
                totp: Some(Some((sealed, false))),
                ..Default::default()
            },
        )
        .await?;
    let url = totp::otpauth_url(&secret, common::PRODUCT_NAME, auth.name());
    let svg = totp::qr_svg(&url).unwrap_or_default();
    use base64::Engine;
    Ok(Json(TotpSetup {
        secret: totp::base32(&secret),
        qr: format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        ),
        otpauth_url: url,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct TotpCode {
    pub code: String,
    /// Current password, required to disable TOTP.
    pub password: Option<String>,
}

/// Confirms enrolment with a valid code and enables TOTP.
#[utoipa::path(post, path = "/auth/totp/enable", tag = "auth", request_body = TotpCode,
    responses((status = 204), (status = 400, body = crate::http::error::ErrorBody)))]
pub async fn totp_enable(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<TotpCode>,
) -> ApiResult<StatusCode> {
    let sealed = auth
        .user
        .totp_secret
        .clone()
        .ok_or_else(|| ApiError::bad_request("Start the setup first."))?;
    let secret = app
        .key
        .open(&sealed, &format!("totp:{}", auth.user.id))
        .map_err(ApiError::internal)?;
    let now = u64::try_from(store::now()).unwrap_or(0);
    if !app.totp_guard.verify(auth.user.id, &secret, &req.code, now) {
        return Err(ApiError::bad_request(
            "The code is incorrect. Check the time on your device and try again.",
        ));
    }
    app.store
        .update_user(
            auth.user.id,
            UserUpdate {
                totp: Some(Some((sealed, true))),
                ..Default::default()
            },
        )
        .await?;
    app.store
        .audit(
            auth.name(),
            "user.totp.enable",
            Some(auth.name()),
            serde_json::json!({}),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Disables TOTP after confirming the password and a current code.
#[utoipa::path(post, path = "/auth/totp/disable", tag = "auth", request_body = TotpCode,
    responses((status = 204), (status = 400, body = crate::http::error::ErrorBody)))]
pub async fn totp_disable(
    State(app): State<Arc<App>>,
    auth: Authed,
    Json(req): Json<TotpCode>,
) -> ApiResult<StatusCode> {
    if app.cfg.auth.require_totp_for_admins && auth.user.role == Role::Admin {
        return Err(ApiError::forbidden(
            "Two-factor authentication is required for administrators.",
        ));
    }
    let ok = verify_blocking(
        auth.user.password_hash.clone(),
        req.password.unwrap_or_default(),
    )
    .await;
    let secret = auth
        .user
        .totp_secret
        .as_deref()
        .and_then(|s| app.key.open(s, &format!("totp:{}", auth.user.id)).ok());
    let now = u64::try_from(store::now()).unwrap_or(0);
    let code_ok = secret.is_some_and(|s| app.totp_guard.verify(auth.user.id, &s, &req.code, now));
    if !ok || !code_ok {
        return Err(ApiError::bad_request("Password or code is incorrect."));
    }
    app.store
        .update_user(
            auth.user.id,
            UserUpdate {
                totp: Some(None),
                ..Default::default()
            },
        )
        .await?;
    app.store
        .audit(
            auth.name(),
            "user.totp.disable",
            Some(auth.name()),
            serde_json::json!({}),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Redirects to the identity provider.
#[utoipa::path(get, path = "/auth/oidc/start", tag = "auth", responses((status = 303)))]
pub async fn oidc_start(State(app): State<Arc<App>>) -> ApiResult<Response> {
    let oidc = app
        .oidc
        .as_ref()
        .ok_or_else(|| ApiError::not_found("Single sign-on"))?;
    let url = oidc.start().await.map_err(|e| {
        tracing::error!(error = %e, "OIDC start failed");
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "oidc_unavailable",
            "The identity provider is unavailable. Try again later.",
        )
    })?;
    Ok(Redirect::to(&url).into_response())
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

fn html_redirect(target: &str) -> Response {
    // A same-origin navigation so the SameSite=Strict cookie is sent.
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0;url={target}\"><title>Signing in</title></head><body></body></html>"
    ))
    .into_response()
}

/// Completes single sign-on and starts a session.
#[utoipa::path(get, path = "/auth/oidc/callback", tag = "auth", params(CallbackQuery), responses((status = 200)))]
pub async fn oidc_callback(
    State(app): State<Arc<App>>,
    client: ClientInfo,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let fail = |msg: &str| {
        tracing::warn!(error = %msg, ip = %client.ip, "single sign-on failed");
        let encoded =
            percent_encoding::utf8_percent_encode(msg, percent_encoding::NON_ALPHANUMERIC);
        html_redirect(&format!("/login?error={encoded}"))
    };
    let Some(oidc) = app.oidc.as_ref() else {
        return fail("Single sign-on is not configured.");
    };
    if let Some(e) = q.error {
        return fail(&format!(
            "The identity provider returned an error: {}",
            e.chars().take(100).collect::<String>()
        ));
    }
    let (Some(code), Some(state)) = (q.code, q.state) else {
        return fail("Missing code or state.");
    };
    let identity = match oidc.callback(&code, &state).await {
        Ok(i) => i,
        Err(e) => return fail(&e),
    };
    let user = match app.store.user_by_oidc(identity.subject.clone()).await {
        Ok(Some(u)) => {
            if u.disabled {
                return fail("Your account is disabled.");
            }
            if u.role != identity.role {
                app.store
                    .update_user(
                        u.id,
                        UserUpdate {
                            role: Some(identity.role),
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap_or(u)
            } else {
                u
            }
        }
        Ok(None) => {
            let mut name = identity.username.clone();
            let mut created = None;
            for attempt in 0..5 {
                if attempt > 0 {
                    name = format!("{}-sso{}", identity.username, attempt);
                }
                match app
                    .store
                    .create_user(
                        name.clone(),
                        None,
                        identity.role,
                        Some(identity.subject.clone()),
                        identity.display_name.clone(),
                    )
                    .await
                {
                    Ok(u) => {
                        created = Some(u);
                        break;
                    }
                    Err(store::StoreError::Conflict(_)) => continue,
                    Err(e) => {
                        tracing::error!(error = %e, "creating SSO user failed");
                        return fail("Internal error.");
                    }
                }
            }
            let Some(u) = created else {
                return fail("Could not create an account for this identity.");
            };
            let _ = app
                .store
                .audit(
                    &u.username,
                    "user.create.sso",
                    Some(&u.username),
                    serde_json::json!({"role": u.role}),
                )
                .await;
            u
        }
        Err(_) => return fail("Internal error."),
    };
    match start_session(&app, &user, &client).await {
        Ok(json_resp) => {
            let cookie = json_resp.headers().get(header::SET_COOKIE).cloned();
            let mut resp = html_redirect("/");
            if let Some(c) = cookie {
                resp.headers_mut().insert(header::SET_COOKIE, c);
            }
            resp
        }
        Err(_) => fail("Could not start a session."),
    }
}
