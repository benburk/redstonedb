//! The About pages: what the site is, how it recognizes builds, and the
//! work it is built on.

pub(crate) mod view;

use axum::{Router, response::IntoResponse, routing::get};

use crate::{account::User, web::app_state::AppState};

/// The About pages.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/about", get(about))
        .route("/about/recognized", get(recognized))
        .route("/about/credits", get(credits))
}

pub async fn about(user_opt: Option<User>) -> impl IntoResponse {
    view::about(user_opt.as_ref())
}

/// How builds are recognized: each category's input and output contract,
/// and how stacking is found.
pub async fn recognized(user_opt: Option<User>) -> impl IntoResponse {
    view::recognized(user_opt.as_ref())
}

/// The open-source work the site is built on, and its licenses.
pub async fn credits(user_opt: Option<User>) -> impl IntoResponse {
    view::credits(user_opt.as_ref())
}
