//! The one error type handlers answer with.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use tracing::error;

/// Why a request got no ordinary answer. Handlers return it with `?`; a
/// refusal a person can fix, such as a bad form field, is no error here but
/// a message on the page they sent it from.
#[derive(Debug)]
pub enum AppError {
    /// Something broke on the server. The details go to the log, never to
    /// the visitor.
    Internal(anyhow::Error),
    /// No such build, version, account, or page.
    NotFound,
    /// Signed in, but not allowed to do this.
    Forbidden,
    /// Not signed in, for something that needs an account: off to the
    /// login page.
    SignedOut,
    /// A request no page of the site sends, with a sentence saying what is
    /// wrong with it.
    BadRequest(String),
}

/// Which page a 404 or 403 answer still needs. The page shows the header
/// for whoever is viewing it, which only the request can say, so
/// `web::error_pages` fills it in.
#[derive(Clone, Copy)]
pub enum ErrorPage {
    NotFound,
    Forbidden,
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(err: E) -> Self {
        AppError::Internal(err.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let page = |status: StatusCode, page: ErrorPage| {
            let mut response = status.into_response();
            response.extensions_mut().insert(page);
            response
        };
        match self {
            AppError::Internal(err) => {
                error!("internal server error: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Something went wrong on the server.",
                )
                    .into_response()
            }
            AppError::NotFound => {
                page(StatusCode::NOT_FOUND, ErrorPage::NotFound)
            }
            AppError::Forbidden => {
                page(StatusCode::FORBIDDEN, ErrorPage::Forbidden)
            }
            AppError::SignedOut => {
                Redirect::to("/sessions/new").into_response()
            }
            AppError::BadRequest(why) => {
                (StatusCode::BAD_REQUEST, why).into_response()
            }
        }
    }
}
