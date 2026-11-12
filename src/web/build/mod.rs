//! One build: its page, its file, its simulation and timing, and its
//! versions. A download takes an account, and is counted and logged,
//! unless it is the 3D viewer's `?preview=yes` fetch, which anyone may
//! make, or a HEAD request.

pub(crate) mod actions_view;
pub(crate) mod checks;
pub(crate) mod checks_test_view;
pub(crate) mod checks_view;
pub(crate) mod detail;
pub(crate) mod detail_view;
pub(crate) mod diff_view;
pub(crate) mod download;
pub(crate) mod info_view;
pub(crate) mod interface_view;
pub(crate) mod simulation_view;
pub(crate) mod sources;
pub(crate) mod stack_guide_view;
pub(crate) mod trace;
pub(crate) mod trace_view;
pub(crate) mod versions;
pub(crate) mod versions_view;

use axum::{
    Router,
    routing::{get, post},
};

use crate::web::app_state::AppState;

/// A build's page, and what it offers: its files, its versions, and
/// its simulation.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/schematics/{id}", get(detail::detail))
        .route("/schematics/{id}/thumbnail.webp", get(detail::thumbnail))
        .route("/schematics/{id}/thumbnail.png", get(detail::thumbnail))
        .route("/schematics/{id}/favorite", post(detail::favorite))
        .route("/schematics/{id}/download", get(download::download))
        .route("/schematics/{id}/merge", post(versions::merge))
        .route(
            "/schematics/{id}/versions/{version}/download",
            get(versions::download),
        )
        .route(
            "/schematics/{id}/versions/{version}/restore",
            post(versions::restore),
        )
        .route("/schematics/{id}/diff", get(versions::diff_page))
        .route("/schematics/{id}/diff/{side}", get(versions::diff_file))
        .route("/schematics/{id}/trace/glitch", get(trace::glitch_fragment))
        .route("/schematics/{id}/simulation", get(detail::poll))
        .route("/schematics/{id}/delete", post(detail::delete))
}
