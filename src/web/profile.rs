use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState,
    error::internal_error,
    models::{schematic, user::User},
};

use super::pages;

/// An account's page: who they are and what they uploaded.
pub async fn profile(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let account = match User::get_by_username(&state.db, &username).await {
        Ok(Some(account)) => account,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    let uploads: Vec<_> = match schematic::all(&state.db).await {
        Ok(all) => all
            .into_iter()
            .filter(|s| {
                s.uploader.as_deref() == Some(account.username.as_str())
            })
            .collect(),
        Err(err) => return internal_error(err).into_response(),
    };
    pages::profile(user_opt.as_ref(), &account, &uploads).into_response()
}
