//! Versioned JSON API (`/api/v1`).

pub mod actions;
pub mod admin;
pub mod alerts;
pub mod auth;
pub mod display;
pub mod events;
pub mod hosts;
pub mod meta;
pub mod security;

use crate::app::App;
use std::sync::Arc;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn routes() -> OpenApiRouter<Arc<App>> {
    OpenApiRouter::new()
        .routes(routes!(meta::meta))
        .routes(routes!(auth::setup))
        .routes(routes!(auth::login))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::me))
        .routes(routes!(auth::change_password))
        .routes(routes!(auth::totp_setup))
        .routes(routes!(auth::totp_enable))
        .routes(routes!(auth::totp_disable))
        .routes(routes!(auth::oidc_start))
        .routes(routes!(auth::oidc_callback))
        .routes(routes!(hosts::list))
        .routes(routes!(hosts::detail))
        .routes(routes!(hosts::metrics))
        .routes(routes!(hosts::changes))
        .routes(routes!(hosts::host_security))
        .routes(routes!(hosts::fleet_changes))
        .routes(routes!(hosts::compare))
        .routes(routes!(hosts::groups))
        .routes(routes!(security::overview))
        .routes(routes!(alerts::list))
        .routes(routes!(alerts::get))
        .routes(routes!(alerts::ack))
        .routes(routes!(alerts::silences, alerts::create_silence))
        .routes(routes!(alerts::expire_silence))
        .routes(routes!(alerts::rules))
        .routes(routes!(actions::list))
        .routes(routes!(actions::run))
        .routes(routes!(actions::audit))
        .routes(routes!(actions::audit_verify))
        .routes(routes!(admin::users, admin::create_user))
        .routes(routes!(admin::update_user, admin::delete_user))
        .routes(routes!(admin::host_keys))
        .routes(routes!(admin::approve_host_key))
        .routes(routes!(admin::reject_host_key))
        .routes(routes!(admin::display_tokens, admin::create_display_token))
        .routes(routes!(admin::revoke_display_token))
        .routes(routes!(admin::test_notification))
        .routes(routes!(display::state))
        .routes(routes!(events::stream))
}

/// Query helper: `?from=..&to=..` with sane defaults and bounds.
pub fn time_range(from: Option<i64>, to: Option<i64>, default_span: i64) -> (i64, i64) {
    let now = store::now();
    let to = to.unwrap_or(now).min(now + 60);
    let from = from.unwrap_or(to - default_span).min(to - 60);
    (from.max(to - 400 * 86_400), to)
}
