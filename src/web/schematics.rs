use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState, error::internal_error, models::schematic,
    models::user::User,
};

use super::pages;

pub async fn list(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let username = user_opt.map(|user| user.username).unwrap_or_default();
    match schematic::all(&state.db).await {
        Ok(schematics) => {
            pages::schematics(&username, &schematics).into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

pub async fn detail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let username = user_opt.map(|user| user.username).unwrap_or_default();
    match schematic::get(&state.db, &id).await {
        Ok(Some(s)) => pages::schematic(&username, &s).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

/// The stored `.schem` file, named by the naming scheme. The viewer fetches
/// the same URL.
pub async fn download(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let found = match schematic::get(&state.db, &id).await {
        Ok(found) => found,
        Err(err) => return internal_error(err).into_response(),
    };
    let Some(s) = found else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match schematic::data(&state.db, &id).await {
        Ok(Some(data)) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", s.filename()),
                ),
            ],
            data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}
