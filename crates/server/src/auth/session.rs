//! Session cookies, the authenticated-user extractor and CSRF checks.

use crate::app::App;
use crate::auth::{ct_eq, random_token, token_hash};
use crate::http::error::ApiError;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, header};
use common::Role;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use store::users::User;

pub const CSRF_HEADER: &str = "x-csrf-token";

pub fn cookie_name(app: &App) -> String {
    if app.secure_cookies {
        format!("__Host-{}_session", common::PRODUCT_NAME)
    } else {
        format!("{}_session", common::PRODUCT_NAME)
    }
}

/// `Set-Cookie` value for a new session.
pub fn session_cookie(app: &App, token: &str, max_age: i64) -> HeaderValue {
    let secure = if app.secure_cookies { "; Secure" } else { "" };
    let v = format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}",
        cookie_name(app)
    );
    HeaderValue::from_str(&v).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// `Set-Cookie` value that removes the session cookie.
pub fn clear_cookie(app: &App) -> HeaderValue {
    let secure = if app.secure_cookies { "; Secure" } else { "" };
    let v = format!(
        "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}",
        cookie_name(app)
    );
    HeaderValue::from_str(&v).unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// Client address, honouring `X-Forwarded-For` only from trusted proxies.
pub fn client_ip(app: &App, parts: &Parts) -> String {
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let trusted = peer.is_some_and(|p| {
        app.cfg
            .server
            .trusted_proxies
            .iter()
            .filter_map(|t| t.parse::<IpAddr>().ok())
            .any(|t| t == p)
    });
    if trusted
        && let Some(xff) = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
    {
        // The last address that is not itself a trusted proxy.
        let trusted: Vec<IpAddr> = app
            .cfg
            .server
            .trusted_proxies
            .iter()
            .filter_map(|t| t.parse().ok())
            .collect();
        if let Some(ip) = xff
            .split(',')
            .map(str::trim)
            .rev()
            .filter_map(|s| s.parse::<IpAddr>().ok())
            .find(|ip| !trusted.contains(ip))
        {
            return ip.to_string();
        }
    }
    peer.map(|p| p.to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Creates a session and returns `(token, csrf)`.
pub async fn create(
    app: &App,
    user_id: i64,
    ip: Option<String>,
    user_agent: Option<String>,
) -> Result<(String, String), ApiError> {
    let token = random_token();
    let csrf = random_token();
    let ttl = i64::try_from(app.cfg.auth.session_ttl.as_secs()).unwrap_or(43_200);
    app.store
        .create_session(
            token_hash(&token),
            user_id,
            csrf.clone(),
            store::now() + ttl,
            ip,
            user_agent.map(|u| u.chars().take(256).collect()),
        )
        .await?;
    Ok((token, csrf))
}

/// The signed-in user of a request.
#[derive(Debug, Clone)]
pub struct Authed {
    pub user: User,
    pub session_hash: String,
    pub csrf: String,
}

impl Authed {
    pub fn require(&self, role: Role) -> Result<(), ApiError> {
        if self.user.role.allows(role) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "This requires the {role} role; you are signed in as {}.",
                self.user.role
            )))
        }
    }

    pub fn name(&self) -> &str {
        &self.user.username
    }
}

/// Endpoints reachable while a password change is pending.
const PASSWORD_CHANGE_ALLOWED: &[&str] = &[
    "/api/v1/auth/me",
    "/api/v1/auth/password",
    "/api/v1/auth/logout",
    "/api/v1/meta",
];

/// Endpoints reachable while TOTP enrolment is required.
const TOTP_ENROLMENT_ALLOWED: &[&str] = &[
    "/api/v1/auth/me",
    "/api/v1/auth/totp/setup",
    "/api/v1/auth/totp/enable",
    "/api/v1/auth/logout",
    "/api/v1/meta",
];

impl FromRequestParts<Arc<App>> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        app: &Arc<App>,
    ) -> Result<Self, Self::Rejection> {
        let token =
            read_cookie(&parts.headers, &cookie_name(app)).ok_or_else(ApiError::unauthorized)?;
        if token.len() > 128 {
            return Err(ApiError::unauthorized());
        }
        let idle = i64::try_from(app.cfg.auth.idle_timeout.as_secs()).unwrap_or(7200);
        let session = app
            .store
            .session(token_hash(&token), idle)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        let safe = matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
        if !safe {
            let sent = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !ct_eq(sent, &session.csrf) {
                return Err(ApiError::forbidden(
                    "Missing or invalid CSRF token. Reload the page and try again.",
                ));
            }
        }
        // Nested routers see a stripped URI; compare the full request path.
        let full = parts
            .extensions
            .get::<axum::extract::OriginalUri>()
            .map(|u| u.0.path().to_string())
            .unwrap_or_else(|| parts.uri.path().to_string());
        let path = full.as_str();
        let needs_totp = app.cfg.auth.require_totp_for_admins
            && session.user.role == Role::Admin
            && !session.user.totp_enabled
            && session.user.password_hash.is_some();
        if needs_totp && !TOTP_ENROLMENT_ALLOWED.contains(&path) {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "totp_enrollment_required",
                "Administrators must enable two-factor authentication to continue.",
            ));
        }
        if session.user.must_change_password && !PASSWORD_CHANGE_ALLOWED.contains(&path) {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "password_change_required",
                "Change your password to continue.",
            ));
        }
        Ok(Authed {
            user: session.user,
            session_hash: session.id_hash,
            csrf: session.csrf,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cookies() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; helmsight_session=tok; b=2"),
        );
        assert_eq!(read_cookie(&h, "helmsight_session").as_deref(), Some("tok"));
        assert_eq!(read_cookie(&h, "missing"), None);
    }
}
