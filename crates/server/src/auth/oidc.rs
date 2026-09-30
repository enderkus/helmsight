//! OpenID Connect single sign-on (authorization code flow with PKCE).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::Role;
use common::config::OidcConfig;
use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, HttpRequest,
    HttpResponse, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const PENDING_TTL: Duration = Duration::from_secs(600);
const METADATA_TTL: Duration = Duration::from_secs(3600);
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HttpError(String);

/// HTTP client for the OIDC library: no redirects, bounded responses.
pub struct Http(reqwest::Client);

impl<'c> openidconnect::AsyncHttpClient<'c> for Http {
    type Error = HttpError;
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'c>>;

    fn call(&'c self, request: HttpRequest) -> Self::Future {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let resp = self
                .0
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body)
                .send()
                .await
                .map_err(|e| HttpError(e.without_url().to_string()))?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let bytes = resp.bytes().await.map_err(|e| HttpError(e.to_string()))?;
            if bytes.len() > MAX_RESPONSE {
                return Err(HttpError("identity provider response too large".into()));
            }
            let mut builder = openidconnect::http::Response::builder().status(status);
            if let Some(h) = builder.headers_mut() {
                *h = headers;
            }
            builder
                .body(bytes.to_vec())
                .map_err(|e| HttpError(e.to_string()))
        })
    }
}

struct Pending {
    nonce: Nonce,
    verifier: PkceCodeVerifier,
    created: Instant,
}

/// Identity established by a successful callback.
#[derive(Debug, Clone)]
pub struct OidcIdentity {
    /// `issuer|sub`, unique per provider.
    pub subject: String,
    pub username: String,
    pub display_name: Option<String>,
    pub role: Role,
}

pub struct Oidc {
    cfg: OidcConfig,
    redirect: String,
    secret: Option<Zeroizing<String>>,
    http: Http,
    metadata: tokio::sync::Mutex<Option<(Instant, CoreProviderMetadata)>>,
    pending: Mutex<HashMap<String, Pending>>,
}

impl std::fmt::Debug for Oidc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oidc")
            .field("issuer", &self.cfg.issuer)
            .finish_non_exhaustive()
    }
}

impl Oidc {
    pub fn new(
        cfg: OidcConfig,
        public_url: &str,
        secret: Option<Zeroizing<String>>,
    ) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .use_preconfigured_tls(alerts::notify::client_tls_config()?)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            redirect: format!(
                "{}/api/v1/auth/oidc/callback",
                public_url.trim_end_matches('/')
            ),
            cfg,
            secret,
            http: Http(http),
            metadata: tokio::sync::Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
        })
    }

    pub fn label(&self) -> &str {
        &self.cfg.label
    }

    async fn metadata(&self) -> Result<CoreProviderMetadata, String> {
        let mut guard = self.metadata.lock().await;
        if let Some((at, m)) = guard.as_ref()
            && at.elapsed() < METADATA_TTL
        {
            return Ok(m.clone());
        }
        let issuer = IssuerUrl::new(self.cfg.issuer.clone()).map_err(|e| e.to_string())?;
        let m = CoreProviderMetadata::discover_async(issuer, &self.http)
            .await
            .map_err(|e| format!("OIDC discovery at {} failed: {e}", self.cfg.issuer))?;
        *guard = Some((Instant::now(), m.clone()));
        Ok(m)
    }

    async fn client(
        &self,
    ) -> Result<
        CoreClient<
            openidconnect::EndpointSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointMaybeSet,
            openidconnect::EndpointMaybeSet,
        >,
        String,
    > {
        let m = self.metadata().await?;
        let redirect = RedirectUrl::new(self.redirect.clone()).map_err(|e| e.to_string())?;
        Ok(CoreClient::from_provider_metadata(
            m,
            ClientId::new(self.cfg.client_id.clone()),
            self.secret
                .as_ref()
                .map(|s| ClientSecret::new(s.to_string())),
        )
        .set_redirect_uri(redirect))
    }

    /// Returns the URL to send the browser to.
    pub async fn start(&self) -> Result<String, String> {
        let client = self.client().await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut req = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(challenge);
        for s in self.cfg.scopes.iter().filter(|s| *s != "openid") {
            req = req.add_scope(Scope::new(s.clone()));
        }
        let (url, state, nonce) = req.url();
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "internal lock error".to_string())?;
        pending.retain(|_, p| p.created.elapsed() < PENDING_TTL);
        if pending.len() > 1000 {
            return Err("too many sign-in attempts in progress; try again shortly".into());
        }
        pending.insert(
            state.secret().clone(),
            Pending {
                nonce,
                verifier,
                created: Instant::now(),
            },
        );
        Ok(url.to_string())
    }

    /// Completes the flow. Returns the verified identity and its role.
    pub async fn callback(&self, code: &str, state: &str) -> Result<OidcIdentity, String> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| "internal lock error".to_string())?
            .remove(state)
            .filter(|p| p.created.elapsed() < PENDING_TTL)
            .ok_or("Sign-in request expired or unknown. Start the sign-in again.")?;
        let client = self.client().await?;
        let token = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .map_err(|e| e.to_string())?
            .set_pkce_verifier(pending.verifier)
            .request_async(&self.http)
            .await
            .map_err(|e| format!("token exchange failed: {e}"))?;
        use openidconnect::TokenResponse;
        let id_token = token
            .id_token()
            .ok_or("identity provider returned no ID token")?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &pending.nonce)
            .map_err(|e| format!("ID token verification failed: {e}"))?;
        let subject = format!("{}|{}", self.cfg.issuer, claims.subject().as_str());
        let username = claims
            .preferred_username()
            .map(|u| u.as_str().to_string())
            .or_else(|| claims.email().map(|e| e.as_str().to_string()))
            .unwrap_or_else(|| claims.subject().as_str().to_string());
        let display_name = claims
            .name()
            .and_then(|n| n.get(None))
            .map(|n| n.as_str().to_string());
        // The signature was verified above; read the role claim from the
        // same token's payload.
        let raw = id_token.to_string();
        let payload = raw
            .split('.')
            .nth(1)
            .and_then(|p| URL_SAFE_NO_PAD.decode(p).ok())
            .and_then(|p| serde_json::from_slice::<serde_json::Value>(&p).ok())
            .unwrap_or(serde_json::Value::Null);
        let values = claim_values(&payload, &self.cfg.role_claim);
        let role = map_role(&values, &self.cfg).ok_or_else(|| {
            format!(
                "Your account has no role mapped for this application (claim `{}`). Ask an administrator for access.",
                self.cfg.role_claim
            )
        })?;
        Ok(OidcIdentity {
            subject,
            username: sanitize_username(&username),
            display_name,
            role,
        })
    }
}

/// Reads a claim by dotted path; strings and arrays of strings are supported.
pub fn claim_values(payload: &serde_json::Value, path: &str) -> Vec<String> {
    let mut v = payload;
    for part in path.split('.') {
        match v.get(part) {
            Some(next) => v = next,
            None => return Vec::new(),
        }
    }
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn map_role(values: &[String], cfg: &OidcConfig) -> Option<Role> {
    values
        .iter()
        .filter_map(|v| cfg.role_map.get(v).copied())
        .max()
        .or(cfg.default_role)
}

fn sanitize_username(s: &str) -> String {
    let u: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
        .take(64)
        .collect();
    if u.is_empty() { "sso-user".into() } else { u }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OidcConfig {
        OidcConfig {
            issuer: "https://id.example.com".into(),
            client_id: "x".into(),
            client_secret: None,
            scopes: vec!["openid".into()],
            role_claim: "realm_access.roles".into(),
            role_map: [
                ("ops".to_string(), Role::Operator),
                ("admins".to_string(), Role::Admin),
            ]
            .into_iter()
            .collect(),
            default_role: None,
            label: "SSO".into(),
        }
    }

    #[test]
    fn role_mapping() {
        let payload = serde_json::json!({"realm_access": {"roles": ["staff", "ops", "admins"]}, "groups": "ops"});
        let v = claim_values(&payload, "realm_access.roles");
        assert_eq!(map_role(&v, &cfg()), Some(Role::Admin));
        assert_eq!(claim_values(&payload, "groups"), ["ops"]);
        assert_eq!(map_role(&["staff".into()], &cfg()), None);
        let mut c = cfg();
        c.default_role = Some(Role::Viewer);
        assert_eq!(map_role(&[], &c), Some(Role::Viewer));
    }

    #[test]
    fn usernames_are_sanitized() {
        assert_eq!(sanitize_username("alice@example.com"), "alice@example.com");
        assert_eq!(sanitize_username("<script>"), "script");
        assert_eq!(sanitize_username("   "), "sso-user");
    }
}
