//! Finding builds: the home page and its listing, and the search for
//! builds like a file.

pub(crate) mod list;
pub(crate) mod list_view;
pub(crate) mod query;
pub(crate) mod similar;
pub(crate) mod similar_view;

use axum::{Router, routing::get};

use crate::web::app_state::AppState;

/// The home page, the listing, and the search for builds like a file.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list::home))
        .route("/builds", get(list::list))
        .route("/similar", get(similar::form).post(similar::search))
}
