//! People: players credited on builds, accounts, and their heads.

pub(crate) mod heads;
pub(crate) mod player_view;
pub(crate) mod players;
pub(crate) mod profile;
pub(crate) mod users;
pub(crate) mod users_view;

use axum::{
    Router,
    routing::{get, post},
};

use crate::web::app_state::AppState;

/// Players' pages and heads, and accounts' pages.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/heads/{uuid}", get(heads::show))
        .route("/players/{name}", get(players::show))
        .route("/players/{name}/head.png", get(players::head))
        .route("/users", get(users::list))
        .route("/users/{username}", get(profile::profile))
        .route("/users/{username}/role", post(users::set_role))
        .route(
            "/users/{username}/minecraft/name",
            post(users::name_minecraft),
        )
}
