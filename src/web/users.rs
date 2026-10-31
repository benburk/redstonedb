//! The admin's list of accounts, where roles are set.

use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::AppError,
    extractors::{fixi::Fixi, session::Admin},
    models::user::{Role, User},
};

use super::{pages, profile};

pub async fn list(
    State(state): State<AppState>,
    Admin(admin): Admin,
) -> Result<Response, AppError> {
    show(&state, &admin, "").await
}

/// The answer to a change of account `username`: for fixi, its row as it
/// is now, with `refused` announced if the change was refused; for a plain
/// form, the page, saying why a refused change was.
async fn changed(
    state: &AppState,
    admin: &User,
    fixi: bool,
    username: &str,
    refused: &str,
) -> Result<Response, AppError> {
    if !fixi {
        return if refused.is_empty() {
            Ok(Redirect::to("/users").into_response())
        } else {
            show(state, admin, refused).await
        };
    }
    let user = User::get_by_username(&state.db, username)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((
        [("FX-Notice", super::notice(refused))],
        pages::user_row(admin, &user),
    )
        .into_response())
}

/// The users page, with `message` saying why a change was refused.
async fn show(
    state: &AppState,
    admin: &User,
    message: &str,
) -> Result<Response, AppError> {
    let users = User::all(&state.db).await?;
    Ok(pages::users(admin, &users, message).into_response())
}

/// The account an admin is changing, by its username.
async fn target(state: &AppState, username: &str) -> Result<User, AppError> {
    User::get_by_username(&state.db, username)
        .await?
        .ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct MinecraftName {
    minecraft_name: String,
    /// `yes` to confirm the name; anything else, or nothing, as an unticked
    /// box sends, leaves it unconfirmed.
    #[serde(default)]
    verified: String,
}

/// Gives an account a Minecraft name an admin types, as its player spells
/// it, confirmed or not as the admin ticks.
pub async fn name_minecraft(
    Path(username): Path<String>,
    State(state): State<AppState>,
    Admin(admin): Admin,
    Fixi(fixi): Fixi,
    Form(form): Form<MinecraftName>,
) -> Result<Response, AppError> {
    let target = target(&state, &username).await?;
    let saved = match profile::player_named(form.minecraft_name.trim()).await {
        Ok(player) => profile::saved(&state, target.id, Some(&player)).await?,
        Err(message) => Err(message),
    };
    let refused = match saved {
        Ok(()) => {
            let verified = form.verified == "yes";
            User::set_minecraft_verified(&state.db, target.id, verified)
                .await?;
            String::new()
        }
        Err(message) => message,
    };
    changed(&state, &admin, fixi, &username, &refused).await
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
    Admin(admin): Admin,
    Fixi(fixi): Fixi,
    Form(form): Form<SetRole>,
) -> Result<Response, AppError> {
    let role = Role::parse(&form.role).ok_or_else(|| {
        AppError::BadRequest(format!("{} is not a role.", form.role))
    })?;
    let target = target(&state, &username).await?;
    let refused = if target.id == admin.id && role != Role::Admin {
        "You cannot demote yourself."
    } else {
        User::set_role(&state.db, target.id, role).await?;
        ""
    };
    changed(&state, &admin, fixi, &username, refused).await
}
