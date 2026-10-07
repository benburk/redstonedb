use std::error;

use axum::http::StatusCode;
use tracing::warn;

/// Utility function for mapping any error into a `500 Internal Server Error`
/// response.
pub fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: error::Error,
{
    warn!("internal server error {err}");
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
