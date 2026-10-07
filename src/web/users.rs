//! The admin's list of accounts, where roles are set.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::internal_error,
    models::user::{Role, User},
};

use super::pages;

pub async fn list(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let Some(admin) = user_opt.filter(User::is_admin) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    match User::all(&state.db).await {
        Ok(users) => pages::users(&admin, &users).into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetRole {
    role: String,
}

/// Sets an account's role, then returns to the list. An admin cannot
/// demote themselves, so the site always keeps one.
pub async fn set_role(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Form(form): Form<SetRole>,
) -> Response {
    let Some(admin) = user_opt.filter(User::is_admin) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let Some(role) = Role::parse(&form.role) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let target = match User::get_by_username(&state.db, &username).await {
        Ok(Some(target)) => target,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    if target.id == admin.id && role != Role::Admin {
        return (StatusCode::BAD_REQUEST, "You cannot demote yourself.")
            .into_response();
    }
    match User::set_role(&state.db, target.id, role).await {
        Ok(()) => Redirect::to("/users").into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}
