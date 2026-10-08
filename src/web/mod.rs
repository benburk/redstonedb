//! Web routes.
use axum::{
    Router,
    http::header,
    response::IntoResponse,
    routing::{get, post},
};

use crate::app_state::AppState;

mod about;
mod auth;
pub mod components;
mod details;
pub mod pages;
mod profile;
mod schematics;
mod session;
mod upload;
mod users;

use about::about;
use profile::profile;

/// Built from `styles/app.css` by `just css`.
async fn get_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../static/app.css"),
    )
}

async fn get_form_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript")],
        include_str!("../../static/form.js"),
    )
}

/// Static assets (CSS, JS) - no request logging
pub fn static_router() -> Router<AppState> {
    Router::new()
        .route("/app.css", get(get_css))
        .route("/form.js", get(get_form_js))
}

/// Dynamic routes - with request logging
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(schematics::list))
        .route("/schematics/{id}", get(schematics::detail))
        .route("/schematics/{id}/download", get(schematics::download))
        .route("/schematics/{id}/thumbnail.png", get(schematics::thumbnail))
        .route(
            "/schematics/{id}/edit",
            get(schematics::edit).post(schematics::save),
        )
        .route("/schematics/{id}/delete", post(schematics::delete))
        .route("/upload", get(upload::form).post(upload::publish))
        .route("/upload/preview", post(upload::preview))
        .route("/about", get(about))
        .route("/users", get(users::list))
        .route("/users/{username}", get(profile))
        .route("/users/{username}/role", post(users::set_role))
        .route(
            "/users/{username}/minecraft",
            post(profile::set_minecraft_name),
        )
        .route("/sessions/new", get(auth::login))
        .route("/logout", post(session::delete))
        .route("/auth/discord", get(auth::start))
        .route("/auth/discord/callback", get(auth::callback))
}
