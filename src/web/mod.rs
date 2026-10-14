//! Web routes.
use std::sync::LazyLock;

use axum::{
    Router,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};

use crate::{app_state::AppState, models::user::User};

mod about;
mod auth;
pub mod components;
mod details;
mod heads;
mod leaderboards;
pub mod pages;
mod players;
mod profile;
mod schematics;
mod session;
mod simulation;
mod upload;
mod users;
mod versions;

use about::about;
use profile::profile;

/// Built from `styles/app.css` by `just css`.
const CSS: &str = include_str!("../../static/app.css");
const FORM_JS: &str = include_str!("../../static/form.js");
const SIMULATION_JS: &str = include_str!("../../static/simulation.js");

/// An asset's address with its contents' hash, so browsers may keep it for
/// good: a changed file gets a new address.
fn versioned(path: &str, contents: &str) -> String {
    let hash = crate::import::sha256(contents.as_bytes());
    format!("{path}?v={}", &hash[..12])
}

static CSS_URL: LazyLock<String> = LazyLock::new(|| versioned("/app.css", CSS));
static FORM_JS_URL: LazyLock<String> =
    LazyLock::new(|| versioned("/form.js", FORM_JS));
static SIMULATION_JS_URL: LazyLock<String> =
    LazyLock::new(|| versioned("/simulation.js", SIMULATION_JS));

/// Where pages link the stylesheet.
pub fn css_url() -> &'static str {
    &CSS_URL
}

/// Where the details forms load their script.
pub fn form_js_url() -> &'static str {
    &FORM_JS_URL
}

/// Where a build's page loads the script that waits for its simulation.
pub fn simulation_js_url() -> &'static str {
    &SIMULATION_JS_URL
}

/// Versioned addresses never change, so they are cached for a year.
const FOREVER: &str = "public, max-age=31536000, immutable";

async fn get_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CACHE_CONTROL, FOREVER),
        ],
        CSS,
    )
}

async fn get_simulation_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, FOREVER),
        ],
        SIMULATION_JS,
    )
}

async fn get_form_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, FOREVER),
        ],
        FORM_JS,
    )
}

/// Static assets (CSS, JS) - no request logging
pub fn static_router() -> Router<AppState> {
    Router::new()
        .route("/app.css", get(get_css))
        .route("/form.js", get(get_form_js))
        .route("/simulation.js", get(get_simulation_js))
}

/// A 404 with the not-found page, for an address naming no build, person,
/// or page.
pub fn not_found(viewer: Option<&User>) -> Response {
    (StatusCode::NOT_FOUND, pages::not_found(viewer)).into_response()
}

async fn fallback(user_opt: Option<User>) -> Response {
    not_found(user_opt.as_ref())
}

/// Dynamic routes - with request logging
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(schematics::list))
        .route("/schematics/{id}", get(schematics::detail))
        .route("/leaderboards", get(leaderboards::people))
        .route("/leaderboards/{category}", get(leaderboards::show))
        .route("/schematics/{id}/download", get(schematics::download))
        .route("/schematics/{id}/thumbnail.png", get(schematics::thumbnail))
        .route(
            "/schematics/{id}/edit",
            get(schematics::edit).post(schematics::save),
        )
        .route("/schematics/{id}/delete", post(schematics::delete))
        .route("/schematics/{id}/versions/new", get(versions::form))
        .route(
            "/schematics/{id}/versions/{version}/download",
            get(versions::download),
        )
        .route("/upload", get(upload::form).post(upload::publish))
        .route("/upload/preview", post(upload::preview))
        .route("/upload/simulation", post(simulation::upload))
        .route("/upload/simulation/timing", post(simulation::upload_timing))
        .route(
            "/schematics/{id}/simulation",
            get(simulation::poll).post(simulation::stored),
        )
        .route(
            "/schematics/{id}/simulation/timing",
            post(simulation::stored_timing),
        )
        .route("/about", get(about))
        .route("/about/categories", get(about::categories))
        .route("/heads/{uuid}", get(heads::show))
        .route("/players/{name}", get(players::show))
        .route("/users", get(users::list))
        .route("/users/{username}", get(profile))
        .route("/users/{username}/role", post(users::set_role))
        .route(
            "/users/{username}/minecraft/verify",
            post(users::verify_minecraft),
        )
        .route(
            "/users/{username}/minecraft/name",
            post(users::name_minecraft),
        )
        .route(
            "/users/{username}/minecraft",
            post(profile::set_minecraft_name),
        )
        .route("/sessions/new", get(auth::login))
        .route("/logout", post(session::delete))
        .route("/auth/discord", get(auth::start))
        .route("/auth/discord/callback", get(auth::callback))
        .fallback(fallback)
}
