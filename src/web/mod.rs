//! The site: its routes, and a folder for each part of it. In a folder, a
//! file named for a page holds its handlers, and the one ending in `view`
//! its markup; `ui` is the markup pages share.
use std::fmt::Write as _;

use axum::{
    Router,
    extract::{OptionalFromRequestParts, Request, State},
    http::header,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};

use crate::{
    account::User,
    web::{
        app_state::AppState,
        error::{AppError, ErrorPage},
    },
};

mod about;
mod account;
pub mod app_state;
pub mod assets;
mod back;
mod browse;
mod build;
pub mod error;
pub mod extractors;
mod href;
mod leaderboard;
mod moderation;
mod people;
mod schemati;
mod submit;
mod ui;

pub use people::heads::fetch_ahead as fetch_heads_ahead;
pub use submit::upload::publish_suggested;

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

/// A file to download, saved as `filename`, which needs no quoting.
fn attachment(filename: &str, data: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        data,
    )
        .into_response()
}

/// Puts the page into a not-found or forbidden answer, with the header for
/// whoever sent the request; see [`ErrorPage`]. Only such answers look up
/// the viewer, so every other request is spared it.
async fn error_pages(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers().clone();
    let response = next.run(request).await;
    let Some(&page) = response.extensions().get::<ErrorPage>() else {
        return response;
    };
    let (mut parts, ()) = Request::new(()).into_parts();
    parts.headers = headers;
    let viewer = <User as OptionalFromRequestParts<_>>::from_request_parts(
        &mut parts, &state,
    )
    .await
    .ok()
    .flatten();
    let body = match page {
        ErrorPage::NotFound => ui::layout::not_found(viewer.as_ref()),
        ErrorPage::Forbidden => ui::layout::forbidden(viewer.as_ref()),
    };
    (response.status(), body).into_response()
}

async fn fallback() -> AppError {
    AppError::NotFound
}

/// Dynamic routes - with request logging. `state` is for the layer that
/// draws the not-found and forbidden pages.
pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(browse::routes())
        .merge(build::routes())
        .merge(submit::routes())
        .merge(leaderboard::routes())
        .merge(people::routes())
        .merge(account::routes())
        .merge(moderation::routes())
        .merge(schemati::routes())
        .merge(about::routes())
        .fallback(fallback)
        .layer(middleware::from_fn_with_state(state, error_pages))
}
