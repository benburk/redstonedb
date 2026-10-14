//! A build's new version, chosen on its edit page and saved with its
//! details there, and its earlier versions' files.

use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState,
    error::internal_error,
    models::{schematic, user::User, version},
};

use super::schematics::{editor, show_edit};

/// The edit page, its form for choosing a new version open.
pub async fn form(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    match editor(&state.db, user_opt, &id).await {
        Ok(user) => {
            show_edit(&state.db, &user, &id, None, "", None, Some("")).await
        }
        Err(response) => response,
    }
}

/// An earlier version's file, under the build's filename with its number.
pub async fn download(
    Path((id, number)): Path<(String, i64)>,
    State(state): State<AppState>,
) -> Response {
    let s = match schematic::get(&state.db, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    match version::data(&state.db, &id, number).await {
        Ok(Some(data)) => {
            let filename =
                s.filename().replace(".schem", &format!("_v{number}.schem"));
            (
                [
                    (
                        header::CONTENT_TYPE,
                        "application/octet-stream".to_owned(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                data,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}
