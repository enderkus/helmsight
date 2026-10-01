//! HTTP-level tests of authentication, authorization and security headers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use server::{App, BuildOptions};
use std::sync::Arc;
use tower::ServiceExt;

struct Harness {
    app: Arc<App>,
    router: Router,
    _dir: tempfile::TempDir,
}

fn harness() -> Harness {
    server::init_crypto();
    let dir = tempfile::tempdir().unwrap();
    let config = common::Config::default();
    let app = App::build(BuildOptions {
        config,
        config_path: None,
        data_dir: dir.path().join("data"),
        local: false,
    })
    .unwrap();
    let router = server::http::router(app.clone());
    Harness {
        app,
        router,
        _dir: dir,
    }
}

struct Resp {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
    text: String,
}

async fn call(r: &Router, req: Request<Body>) -> Resp {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    Resp {
        status,
        headers,
        body: serde_json::from_str(&text).unwrap_or(Value::Null),
        text,
    }
}

fn post(path: &str, body: Value, cookie: Option<&str>, csrf: Option<&str>) -> Request<Body> {
    let mut b = Request::post(path).header(header::CONTENT_TYPE, "application/json");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    if let Some(t) = csrf {
        b = b.header("x-csrf-token", t);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn get(path: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::get(path);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::empty()).unwrap()
}

fn cookie_of(r: &Resp) -> String {
    let set = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    set.split(';').next().unwrap().to_string()
}

async fn login(h: &Harness, user: &str, pw: &str) -> (String, String) {
    let r = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": user, "password": pw}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    (cookie_of(&r), r.body["csrf"].as_str().unwrap().to_string())
}

async fn add_user(h: &Harness, name: &str, role: common::Role) {
    let hash = server::auth::password::hash("a long enough passphrase").unwrap();
    h.app
        .store
        .create_user(name.into(), Some(hash), role, None, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn setup_creates_first_admin_once() {
    let h = harness();
    let token = "setup-token-value";
    *h.app.setup_token.lock().unwrap() = Some(server::auth::token_hash(token));
    let meta = call(&h.router, get("/api/v1/meta", None)).await;
    assert_eq!(meta.body["setup_required"], true);
    assert_eq!(meta.body["product"], common::PRODUCT_NAME);

    let bad = call(
        &h.router,
        post(
            "/api/v1/auth/setup",
            json!({"token": "nope", "username": "root", "password": "a long enough passphrase"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(bad.status, StatusCode::FORBIDDEN);
    let weak = call(
        &h.router,
        post(
            "/api/v1/auth/setup",
            json!({"token": token, "username": "root", "password": "short"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    let ok = call(
        &h.router,
        post(
            "/api/v1/auth/setup",
            json!({"token": token, "username": "root", "password": "a long enough passphrase"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);
    assert_eq!(ok.body["user"]["role"], "admin");
    let set_cookie = ok
        .headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    let again = call(
        &h.router,
        post(
            "/api/v1/auth/setup",
            json!({"token": token, "username": "x", "password": "a long enough passphrase"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(again.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn setup_is_closed_once_any_user_exists() {
    let h = harness();
    *h.app.setup_token.lock().unwrap() = Some(server::auth::token_hash("t"));
    // For example created by single sign-on.
    add_user(&h, "sso-user", common::Role::Operator).await;
    let meta = call(&h.router, get("/api/v1/meta", None)).await;
    assert_eq!(meta.body["setup_required"], false);
    let body = json!({"token": "t", "username": "x", "password": "a long enough passphrase"});
    let r = call(&h.router, post("/api/v1/auth/setup", body, None, None)).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_session_and_csrf() {
    let h = harness();
    add_user(&h, "alice", common::Role::Admin).await;
    let wrong = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": "alice", "password": "wrong"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body["error"], "invalid_credentials");
    let unknown = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": "nobody", "password": "wrong"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(
        unknown.body["message"], wrong.body["message"],
        "no user enumeration"
    );

    let (cookie, csrf) = login(&h, "alice", "a long enough passphrase").await;
    let me = call(&h.router, get("/api/v1/auth/me", Some(&cookie))).await;
    assert_eq!(me.body["user"]["username"], "alice");
    assert!(
        me.body["user"].get("password_hash").is_none(),
        "hash must never be serialized"
    );

    let body = json!({"duration": "1h", "reason": "maintenance"});
    let no_csrf = call(
        &h.router,
        post("/api/v1/silences", body.clone(), Some(&cookie), None),
    )
    .await;
    assert_eq!(no_csrf.status, StatusCode::FORBIDDEN);
    let bad_csrf = call(
        &h.router,
        post("/api/v1/silences", body.clone(), Some(&cookie), Some("x")),
    )
    .await;
    assert_eq!(bad_csrf.status, StatusCode::FORBIDDEN);
    let ok = call(
        &h.router,
        post("/api/v1/silences", body, Some(&cookie), Some(&csrf)),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);

    let out = call(
        &h.router,
        post("/api/v1/auth/logout", json!({}), Some(&cookie), Some(&csrf)),
    )
    .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = call(&h.router, get("/api/v1/hosts", Some(&cookie))).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn roles_are_enforced() {
    let h = harness();
    add_user(&h, "val", common::Role::Viewer).await;
    let (cookie, csrf) = login(&h, "val", "a long enough passphrase").await;
    assert_eq!(
        call(&h.router, get("/api/v1/hosts", Some(&cookie)))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        call(&h.router, get("/api/v1/users", Some(&cookie)))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&h.router, get("/api/v1/audit", Some(&cookie)))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let s = call(
        &h.router,
        post(
            "/api/v1/silences",
            json!({"duration": "1h", "reason": "x"}),
            Some(&cookie),
            Some(&csrf),
        ),
    )
    .await;
    assert_eq!(s.status, StatusCode::FORBIDDEN);
    let u = call(
        &h.router,
        post(
            "/api/v1/users",
            json!({"username": "eve", "password": "a long enough passphrase", "role": "admin"}),
            Some(&cookie),
            Some(&csrf),
        ),
    )
    .await;
    assert_eq!(u.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_is_rate_limited() {
    let h = harness();
    add_user(&h, "bob", common::Role::Viewer).await;
    let mut last = StatusCode::OK;
    for _ in 0..10 {
        last = call(
            &h.router,
            post(
                "/api/v1/auth/login",
                json!({"username": "bob", "password": "nope"}),
                None,
                None,
            ),
        )
        .await
        .status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    // Even the right password is refused while limited.
    let r = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": "bob", "password": "a long enough passphrase"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers.get(header::RETRY_AFTER).is_some());
}

#[tokio::test]
async fn cross_origin_posts_are_refused() {
    let h = harness();
    let req = Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::HOST, "monitor.example.com")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from(
            json!({"username": "a", "password": "b"}).to_string(),
        ))
        .unwrap();
    let r = call(&h.router, req).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(r.text.contains("Cross-origin"));
}

#[tokio::test]
async fn security_headers_and_static_fallback() {
    let h = harness();
    let r = call(&h.router, get("/hosts/web-1", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let csp = r
        .headers
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp.contains("script-src 'self'") && csp.contains("frame-ancestors 'none'"));
    assert!(!csp.contains("unsafe-inline"));
    assert_eq!(r.headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(r.headers.get("x-content-type-options").unwrap(), "nosniff");

    let api = call(&h.router, get("/api/v1/does-not-exist", None)).await;
    assert_eq!(api.status, StatusCode::NOT_FOUND);
    assert_eq!(api.body["error"], "not_found");

    let unauth = call(&h.router, get("/api/v1/hosts", None)).await;
    assert_eq!(unauth.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        unauth.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
}

#[tokio::test]
async fn openapi_document_lists_endpoints() {
    let h = harness();
    let r = call(&h.router, get("/api/v1/openapi.json", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    let paths = r.body["paths"].as_object().unwrap();
    for p in [
        "/api/v1/hosts",
        "/api/v1/hosts/{name}/metrics",
        "/api/v1/auth/login",
        "/api/v1/actions/{id}/run",
        "/api/v1/compare",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
    assert!(r.body["components"]["schemas"]["HostSummary"].is_object());
}

#[tokio::test]
async fn display_tokens_grant_read_only_scoped_access() {
    let h = harness();
    add_user(&h, "admin", common::Role::Admin).await;
    let (cookie, csrf) = login(&h, "admin", "a long enough passphrase").await;
    let created = call(
        &h.router,
        post(
            "/api/v1/display-tokens",
            json!({"name": "NOC", "groups": ["web"]}),
            Some(&cookie),
            Some(&csrf),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text);
    let secret = created.body["secret"].as_str().unwrap().to_string();
    let id = created.body["token"]["id"].as_i64().unwrap();

    let with_token = |t: &str| {
        Request::get("/api/v1/display/state")
            .header(header::AUTHORIZATION, format!("Bearer {t}"))
            .body(Body::empty())
            .unwrap()
    };
    let r = call(&h.router, with_token(&secret)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["viewer"], "NOC");
    // The token does not open the rest of the API.
    let other = Request::get("/api/v1/hosts")
        .header(header::AUTHORIZATION, format!("Bearer {secret}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        call(&h.router, other).await.status,
        StatusCode::UNAUTHORIZED
    );

    let del = Request::delete(format!("/api/v1/display-tokens/{id}"))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&h.router, del).await.status, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&h.router, with_token(&secret)).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn new_users_must_change_their_password() {
    let h = harness();
    add_user(&h, "admin", common::Role::Admin).await;
    let (cookie, csrf) = login(&h, "admin", "a long enough passphrase").await;
    let r = call(
        &h.router,
        post(
            "/api/v1/users",
            json!({"username": "carol", "password": "temporary passphrase 1", "role": "operator"}),
            Some(&cookie),
            Some(&csrf),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(r.body["must_change_password"], true);

    let (c2, t2) = login(&h, "carol", "temporary passphrase 1").await;
    let blocked = call(&h.router, get("/api/v1/hosts", Some(&c2))).await;
    assert_eq!(blocked.status, StatusCode::FORBIDDEN);
    assert_eq!(blocked.body["error"], "password_change_required");
    let changed = call(
        &h.router,
        post(
            "/api/v1/auth/password",
            json!({"current": "temporary passphrase 1", "new": "a brand new passphrase"}),
            Some(&c2),
            Some(&t2),
        ),
    )
    .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{}", changed.text);
    assert_eq!(
        call(&h.router, get("/api/v1/hosts", Some(&c2)))
            .await
            .status,
        StatusCode::OK
    );

    // The audit log recorded the admin's action.
    let audit = call(&h.router, get("/api/v1/audit?action=user.", Some(&cookie))).await;
    assert!(
        audit
            .body
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "user.create" && e["target"] == "carol")
    );
}

#[tokio::test]
async fn totp_enrolment_and_login() {
    let h = harness();
    add_user(&h, "dave", common::Role::Operator).await;
    let (cookie, csrf) = login(&h, "dave", "a long enough passphrase").await;
    let setup = call(
        &h.router,
        post(
            "/api/v1/auth/totp/setup",
            json!({}),
            Some(&cookie),
            Some(&csrf),
        ),
    )
    .await;
    assert_eq!(setup.status, StatusCode::OK, "{}", setup.text);
    assert!(
        setup.body["qr"]
            .as_str()
            .unwrap()
            .starts_with("data:image/svg+xml;base64,")
    );
    // Recover the raw secret from the stored (encrypted) value.
    let user = h
        .app
        .store
        .user_by_name("dave".into())
        .await
        .unwrap()
        .unwrap();
    let secret = h
        .app
        .key
        .open(
            user.totp_secret.as_deref().unwrap(),
            &format!("totp:{}", user.id),
        )
        .unwrap();
    let now = u64::try_from(store::now()).unwrap();
    let code = server::auth::totp::code_at(&secret, now);
    let en = call(
        &h.router,
        post(
            "/api/v1/auth/totp/enable",
            json!({"code": code}),
            Some(&cookie),
            Some(&csrf),
        ),
    )
    .await;
    assert_eq!(en.status, StatusCode::NO_CONTENT, "{}", en.text);

    let no_code = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": "dave", "password": "a long enough passphrase"}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(no_code.body["error"], "totp_required");
    let reused = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": "dave", "password": "a long enough passphrase", "totp": code}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(
        reused.body["error"], "invalid_totp",
        "a code must not be accepted twice"
    );
    let next = server::auth::totp::code_at(&secret, now + 30);
    let ok = call(
        &h.router,
        post(
            "/api/v1/auth/login",
            json!({"username": "dave", "password": "a long enough passphrase", "totp": next}),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);
}
