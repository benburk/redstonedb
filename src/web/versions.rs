//! A build's new version, chosen on its edit page and saved with its
//! details there, and its earlier versions' files.

use axum::{
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState,
    error::internal_error,
    models::{schematic, user::User, version},
    version::check,
};

use super::{
    schematics::{NewFile, editor, show_edit},
    upload::{self, Sent},
};

/// The edit page, its form for choosing a new version open.
pub async fn form(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    match editor(&state.db, user_opt, &id).await {
        Ok(user) => {
            show_edit(&state.db, &user, &id, None, "", None, true).await
        }
        Err(response) => response,
    }
}

/// Reads the chosen file and shows it on the edit page as the build's next
/// version, checked against it, to save with the details.
pub async fn preview(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    mut multipart: Multipart,
) -> Response {
    let db = &state.db;
    let user = match editor(db, user_opt, &id).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let Sent {
        file, standardize, ..
    } = match upload::received(&mut multipart).await {
        Ok(sent) => sent,
        Err(message) => {
            return show_edit(db, &user, &id, None, &message, None, true).await;
        }
    };
    let s = match schematic::get(db, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    match check(db, &s, &file, standardize).await {
        Ok(checked) => {
            let new_file = NewFile {
                checked: &checked,
                note: "",
            };
            show_edit(db, &user, &id, None, "", Some(new_file), false).await
        }
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            show_edit(db, &user, &id, None, &message, None, true).await
        }
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
