//! A build's new version, chosen on its edit page and saved with its
//! details there, and its earlier versions' files.

use axum::{
    extract::{Path, State},
    response::Response,
};

use crate::{
    app_state::AppState,
    error::AppError,
    extractors::session::SignedIn,
    models::{schematic, version},
};

use super::schematics::{editable, show_edit};

/// The edit page, its form for choosing a new version open.
pub async fn form(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = editable(&state.db, &user, &id).await?;
    show_edit(&state.db, &user, &s, None, "", None, Some("")).await
}

/// An earlier version's file, under the build's filename with its number.
pub async fn download(
    Path((id, number)): Path<(String, i64)>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let s = schematic::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let data = version::data(&state.db, &id, number)
        .await?
        .ok_or(AppError::NotFound)?;
    let filename = s.filename().replace(".schem", &format!("_v{number}.schem"));
    Ok(super::attachment(&filename, data))
}
