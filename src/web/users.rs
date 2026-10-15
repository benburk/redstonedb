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
    extractors::fixi::Fixi,
    models::user::{Role, User},
};

use super::{pages, profile};

pub async fn list(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let Some(admin) = user_opt.filter(User::is_admin) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    show(&state, &admin, "").await
}

/// The users page, with `message` saying why a change was refused.
/// The answer to a change of account `username`: for fixi, its row as it
/// is now, with `refused` announced if the change was refused; for a plain
/// form, the page, saying why a refused change was.
async fn changed(
    state: &AppState,
    admin: &User,
    fixi: bool,
    username: &str,
    refused: &str,
) -> Response {
    if !fixi {
        return if refused.is_empty() {
            Redirect::to("/users").into_response()
        } else {
            show(state, admin, refused).await
        };
    }
    match User::get_by_username(&state.db, username).await {
        Ok(Some(user)) => (
            [("FX-Notice", super::notice(refused))],
            pages::user_row(admin, &user),
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

async fn show(state: &AppState, admin: &User, message: &str) -> Response {
    match User::all(&state.db).await {
        Ok(users) => pages::users(admin, &users, message).into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

#[derive(Deserialize)]
pub struct MinecraftName {
    minecraft_name: String,
}

/// Gives an account a Minecraft name an admin types, as its player spells
/// it. An admin typing it confirms it.
pub async fn name_minecraft(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Form(form): Form<MinecraftName>,
) -> Response {
    let Some(admin) = user_opt.filter(User::is_admin) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let target = match User::get_by_username(&state.db, &username).await {
        Ok(Some(target)) => target,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    let refused = match profile::player_named(form.minecraft_name.trim()).await
    {
        Ok(player) => {
            match profile::saved(&state, target.id, Some(&player)).await {
                Ok(Ok(())) => {
                    return match User::set_minecraft_verified(
                        &state.db, target.id, true,
                    )
                    .await
                    {
                        Ok(()) => {
                            changed(&state, &admin, fixi, &username, "").await
                        }
                        Err(err) => internal_error(err).into_response(),
                    };
                }
                Ok(Err(message)) | Err(message) => message,
            }
        }
        Err(message) => message,
    };
    changed(&state, &admin, fixi, &username, &refused).await
}

#[derive(Deserialize)]
pub struct Verify {
    /// `yes` to confirm the account's Minecraft name, anything else to
    /// withdraw the confirmation.
    verified: String,
}

/// Confirms, or withdraws the confirmation of, an account's Minecraft name,
/// then returns to the list. A confirmed name lets its account edit the
/// builds crediting it as builder.
pub async fn verify_minecraft(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Form(form): Form<Verify>,
) -> Response {
    let Some(admin) = user_opt.filter(User::is_admin) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let target = match User::get_by_username(&state.db, &username).await {
        Ok(Some(target)) => target,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    let verified = form.verified == "yes" && target.minecraft_name.is_some();
    match User::set_minecraft_verified(&state.db, target.id, verified).await {
        Ok(()) => changed(&state, &admin, fixi, &username, "").await,
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
    Fixi(fixi): Fixi,
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
        return changed(
            &state,
            &admin,
            fixi,
            &username,
            "You cannot demote yourself.",
        )
        .await;
    }
    match User::set_role(&state.db, target.id, role).await {
        Ok(()) => changed(&state, &admin, fixi, &username, "").await,
        Err(err) => internal_error(err).into_response(),
    }
}
