use std::error;

use axum::http::StatusCode;
use tracing::warn;

/// A `500 Internal Server Error` for any error. The details go to the log,
/// not to the visitor.
pub fn internal_error<E>(err: E) -> (StatusCode, &'static str)
where
    E: error::Error,
{
    warn!("internal server error: {err}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong on the server.",
    )
}
