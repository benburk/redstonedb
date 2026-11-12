//! Sending a build in: uploading one, editing its details, and what the
//! form says of its file while it is filled in.

pub(crate) mod analysis;
pub(crate) mod details_view;
pub(crate) mod edit;
pub(crate) mod form;
pub(crate) mod form_view;
pub(crate) mod upload;
pub(crate) mod upload_view;

use axum::{
    Router,
    routing::{get, post},
};

use crate::web::app_state::AppState;

/// Uploading a build, and editing or deleting one.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/upload", get(upload::form).post(upload::publish))
        .route("/upload/preview", post(upload::preview))
        .route("/upload/{id}", get(upload::analyzed))
        .route("/upload/{id}/progress", get(upload::progress))
        .route("/schematics/{id}/edit", get(edit::edit).post(edit::save))
        .route("/schematics/{id}/delete", post(edit::delete))
        .route(
            "/schematics/{id}/versions/check/{check}",
            get(edit::checked),
        )
        .route(
            "/schematics/{id}/versions/check/{check}/progress",
            get(edit::check_progress),
        )
}
