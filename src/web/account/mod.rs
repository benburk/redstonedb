//! The account signed in: logging in and out, and its settings.

pub(crate) mod auth;
#[cfg(debug_assertions)]
pub(crate) mod dev;
pub(crate) mod session;
pub(crate) mod settings;
pub(crate) mod settings_view;

use axum::{
    Router,
    routing::{get, post},
};

use crate::web::app_state::AppState;

/// Logging in and out, and the settings.
pub(crate) fn routes() -> Router<AppState> {
    let routes = Router::new()
        .route("/sessions/new", get(auth::login))
        .route("/auth/discord", get(auth::start))
        .route("/auth/discord/callback", get(auth::callback))
        .route("/logout", post(session::delete))
        .route("/settings", get(settings::show))
        .route("/settings/minecraft", post(settings::set_minecraft_name))
        .route(
            "/settings/palette",
            get(settings::palette_editor).post(settings::set_palette),
        )
        .route("/palette/custom", get(settings::custom_colours));
    // Signing in without Discord, for a local server only.
    #[cfg(debug_assertions)]
    let routes = routes.route("/dev/login", get(dev::login));
    routes
}
