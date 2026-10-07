//! Web routes.
use axum::{
    Router,
    http::header,
    response::IntoResponse,
    routing::{delete, get, post},
};

use crate::app_state::AppState;

mod about;
pub mod components;
mod login;
pub mod pages;
mod profile;
mod schematics;
mod session;
mod settings;
mod signup;

use about::about;
use profile::profile;

async fn get_pico_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../static/pico.min.css"),
    )
}

async fn get_pico_colors() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../static/pico.colors.min.css"),
    )
}

async fn get_fixi() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript")],
        include_str!("../../static/fixi.js"),
    )
}

async fn get_fixi_config() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript")],
        include_str!("../../static/fixi-config.js"),
    )
}

/// Static assets (CSS, JS) - no request logging
pub fn static_router() -> Router<AppState> {
    Router::new()
        .route("/pico.min.css", get(get_pico_css))
        .route("/pico.colors.min.css", get(get_pico_colors))
        .route("/fixi.js", get(get_fixi))
        .route("/fixi-config.js", get(get_fixi_config))
}

/// Dynamic routes - with request logging
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(schematics::list))
        .route("/schematics/{id}", get(schematics::detail))
        .route("/schematics/{id}/download", get(schematics::download))
        .route("/about", get(about))
        .route("/users/{username}", get(profile))
        .route("/sessions/new", get(login::get))
        .route("/sessions", post(session::post).delete(session::delete))
        .route("/sessions/{session_id}", delete(session::delete_by_id))
        .route("/users/new", get(signup::get))
        .route("/invite-codes/check", post(signup::check_invite))
        .route("/users", post(signup::post))
        .route("/settings", get(settings::get).patch(settings::patch))
}
