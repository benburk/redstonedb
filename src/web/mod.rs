//! Web routes.
use std::{fmt::Write as _, sync::LazyLock};

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
mod schemati;
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
const SIDEBAR_JS: &str = include_str!("../../static/sidebar.js");
const FIXI_JS: &str = include_str!("../../static/fixi.js");
const APP_JS: &str = include_str!("../../static/app.js");

/// An asset's address with its contents' hash, so browsers may keep it for
/// good: a changed file gets a new address.
fn versioned(path: &str, contents: &str) -> String {
    let hash = crate::import::sha256(contents.as_bytes());
    format!("{path}?v={}", &hash[..12])
}

static CSS_URL: LazyLock<String> = LazyLock::new(|| versioned("/app.css", CSS));
static FORM_JS_URL: LazyLock<String> =
    LazyLock::new(|| versioned("/form.js", FORM_JS));
static SIDEBAR_JS_URL: LazyLock<String> =
    LazyLock::new(|| versioned("/sidebar.js", SIDEBAR_JS));
static FIXI_JS_URL: LazyLock<String> =
    LazyLock::new(|| versioned("/fixi.js", FIXI_JS));
static APP_JS_URL: LazyLock<String> =
    LazyLock::new(|| versioned("/app.js", APP_JS));

/// Where pages link the stylesheet.
pub fn css_url() -> &'static str {
    &CSS_URL
}

/// Where the details forms load their script.
pub fn form_js_url() -> &'static str {
    &FORM_JS_URL
}

/// Where a page with the category sidebar loads the script that fits it to
/// the window.
pub fn sidebar_js_url() -> &'static str {
    &SIDEBAR_JS_URL
}

/// Where every page loads fixi.
pub fn fixi_js_url() -> &'static str {
    &FIXI_JS_URL
}

/// Where every page loads the script that sets fixi's policies; see
/// `static/app.js`.
pub fn app_js_url() -> &'static str {
    &APP_JS_URL
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

/// A script, cached for good, since its address names its version.
fn script(contents: &'static str) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, FOREVER),
        ],
        contents,
    )
}

async fn get_form_js() -> impl IntoResponse {
    script(FORM_JS)
}

async fn get_sidebar_js() -> impl IntoResponse {
    script(SIDEBAR_JS)
}

async fn get_fixi_js() -> impl IntoResponse {
    script(FIXI_JS)
}

async fn get_app_js() -> impl IntoResponse {
    script(APP_JS)
}

/// Static assets (CSS, JS) - no request logging
pub fn static_router() -> Router<AppState> {
    Router::new()
        .route("/app.css", get(get_css))
        .route("/form.js", get(get_form_js))
        .route("/sidebar.js", get(get_sidebar_js))
        .route("/fixi.js", get(get_fixi_js))
        .route("/app.js", get(get_app_js))
}

/// `text` for an `FX-Notice` header, which `static/app.js` shows: percent-
/// encoded, since a header value is only ASCII.
fn notice(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b" .,;:!?'()-_/".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
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
        .route("/schematics/{id}/schemati", post(schemati::send))
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
        .route("/schemati", get(schemati::show))
        .route("/schemati/session", post(schemati::connect))
        .route("/schemati/session/delete", post(schemati::disconnect))
        .route("/schemati/files", post(schemati::add))
        .route("/schemati/files/{name}", get(schemati::download))
        .route("/schemati/files/{name}/rename", post(schemati::rename))
        .route("/schemati/files/{name}/delete", post(schemati::delete))
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
