//! A build's new version, chosen on its edit page and saved with its
//! details there, and its earlier versions' files. Whether a file may be one
//! is `crate::version`'s to say, and keeping it `models::version`'s.

use axum::{
    extract::{Path, State},
    response::Response,
};

use crate::{
    models::{schematic, version},
    web::{
        app_state::AppState, error::AppError, extractors::session::SignedIn,
    },
};

use super::{
    pages::Reply,
    schematics::{EditView, editable, show_edit},
};

/// The edit page, its form for choosing a new version open.
pub async fn form(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = editable(&state.db, &user, &id).await?;
    let view = EditView {
        reply: Reply::Choosing(None),
        ..EditView::default()
    };
    show_edit(&state.db, &user, &s, view).await
}

/// An earlier version's file, under the build's filename with its number,
/// to an account alone.
pub async fn download(
    Path((id, number)): Path<(String, i64)>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = schematic::get_visible(&state.db, &id, Some(&user))
        .await?
        .ok_or(AppError::NotFound)?;
    let data = version::data(&state.db, &id, number)
        .await?
        .ok_or(AppError::NotFound)?;
    schematic::log_download(
        &state.db,
        &id,
        user.id,
        Some(number),
        schematic::DownloadKind::Version,
    )
    .await?;
    let filename = s.filename().replace(".schem", &format!("_v{number}.schem"));
    Ok(super::attachment(&filename, data))
}
