//! HTTP router, security middleware and embedded web UI.

pub mod api;
pub mod error;

use crate::app::App;
use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use error::ApiError;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

#[derive(RustEmbed)]
#[folder = "$OUT_DIR/web"]
struct Assets;

/// Content-Security-Policy for every response: only code and styles served
/// by this binary may run; nothing may be framed or loaded from elsewhere.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
font-src 'self'; connect-src 'self'; manifest-src 'self'; frame-ancestors 'none'; base-uri 'none'; \
form-action 'self'";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "helmsight API",
        description = "JSON API used by the web UI. Authenticate with the session cookie; state-changing requests require the `X-CSRF-Token` header returned by `/api/v1/auth/me`.",
    ),
    tags(
        (name = "meta"), (name = "auth"), (name = "hosts"), (name = "security"),
        (name = "alerts"), (name = "actions"), (name = "admin"), (name = "display")
    )
)]
struct ApiDoc;

/// Builds the API router and returns it with the generated OpenAPI document.
pub fn api_router() -> (Router<Arc<App>>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api/v1", api::routes())
        .split_for_parts()
}

pub fn router(app: Arc<App>) -> Router {
    let (api, mut doc) = api_router();
    doc.info.version = common::VERSION.to_string();
    doc.info.title = format!("{} API", common::PRODUCT_NAME);
    let doc = Arc::new(doc);
    Router::new()
        .merge(api)
        .route(
            "/api/v1/openapi.json",
            axum::routing::get(move || {
                let d = doc.clone();
                async move { axum::Json((*d).clone()) }
            }),
        )
        .route("/metrics", axum::routing::get(crate::prom::metrics))
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .fallback(static_handler)
        .layer(middleware::from_fn_with_state(app.clone(), origin_check))
        .layer(middleware::from_fn_with_state(
            app.clone(),
            security_headers,
        ))
        .layer(RequestBodyLimitLayer::new(256 * 1024))
        .layer(CompressionLayer::new())
        .layer(tower_http::catch_panic::CatchPanicLayer::new())
        .with_state(app)
}

async fn security_headers(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let is_api = req.uri().path().starts_with("/api/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    let set = |h: &mut HeaderMap, k: &'static str, v: &'static str| {
        h.insert(k, HeaderValue::from_static(v));
    };
    set(h, "content-security-policy", CSP);
    set(h, "x-content-type-options", "nosniff");
    set(h, "x-frame-options", "DENY");
    set(h, "referrer-policy", "no-referrer");
    set(h, "cross-origin-opener-policy", "same-origin");
    set(h, "cross-origin-resource-policy", "same-origin");
    set(
        h,
        "permissions-policy",
        "camera=(), microphone=(), geolocation=(), payment=(), usb=(), interest-cohort=()",
    );
    if app.secure_cookies {
        set(h, "strict-transport-security", "max-age=31536000");
    }
    if is_api && !h.contains_key(header::CACHE_CONTROL) {
        set(h, "cache-control", "no-store");
    }
    resp
}

/// Rejects state-changing requests whose `Origin` is not this server. This
/// complements SameSite cookies and the CSRF header.
async fn origin_check(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe
        && let Some(origin) = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
    {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let origin_host = origin
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(origin);
        let public_host = app.cfg.server.public_url.as_deref().and_then(|u| {
            u.split_once("://")
                .map(|(_, rest)| rest.trim_end_matches('/').to_string())
        });
        let ok = origin_host.eq_ignore_ascii_case(host)
            || public_host.is_some_and(|p| p.eq_ignore_ascii_case(origin_host));
        if !ok {
            return ApiError::forbidden("Cross-origin request refused.").into_response();
        }
    }
    next.run(req).await
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") {
        return ApiError::not_found("Endpoint").into_response();
    }
    let rel = path.trim_start_matches('/');
    let (file, is_index) = match Assets::get(rel).filter(|_| !rel.is_empty()) {
        Some(f) => (Some(f), rel == "index.html"),
        None => (Assets::get("index.html"), true),
    };
    let Some(file) = file else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let mime = if is_index {
        "text/html; charset=utf-8".to_string()
    } else {
        mime_guess::from_path(rel)
            .first_or_octet_stream()
            .to_string()
    };
    let cache = if is_index {
        "no-cache"
    } else if rel.starts_with("assets/") {
        // Vite fingerprints files under assets/.
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };
    let mut resp = Response::new(Body::from(file.data.into_owned()));
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    resp
}
