//! Public metadata used by the UI before sign-in.

use crate::app::App;
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct Features {
    /// Actions are configured (the actions UI is shown).
    pub actions: bool,
    /// Label of the single sign-on button, when OIDC is configured.
    pub oidc: Option<String>,
    pub local_login: bool,
    pub local_mode: bool,
    pub prometheus: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Meta {
    pub product: String,
    pub version: String,
    pub features: Features,
    /// No user exists yet; the first admin must be created with the setup token.
    pub setup_required: bool,
    /// Collection interval in seconds.
    pub interval: u64,
}

#[utoipa::path(get, path = "/meta", tag = "meta", responses((status = 200, body = Meta)))]
pub async fn meta(State(app): State<Arc<App>>) -> Json<Meta> {
    let setup_required = app.setup_token.lock().map(|t| t.is_some()).unwrap_or(false);
    Json(Meta {
        product: common::PRODUCT_NAME.to_string(),
        version: common::VERSION.to_string(),
        features: Features {
            actions: !app.cfg.actions.is_empty() && !app.local_mode,
            oidc: app.oidc.as_ref().map(|o| o.label().to_string()),
            local_login: !app.cfg.auth.disable_local_login,
            local_mode: app.local_mode,
            prometheus: app.prometheus_token.is_some(),
        },
        setup_required,
        interval: app.cfg.collect.interval.as_secs(),
    })
}
