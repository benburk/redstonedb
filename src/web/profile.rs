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
    show(&state, user_opt.as_ref(), &account, "").await
}

#[derive(Deserialize)]
pub struct MinecraftName {
    minecraft_name: Option<String>,
}

/// Sets or clears the Minecraft name of the account viewing its own page.
pub async fn set_minecraft_name(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Form(form): Form<MinecraftName>,
) -> Response {
    let Some(user) = user_opt.filter(|user| user.username == username) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let name = form.minecraft_name.unwrap_or_default();
    let name = name.trim();
    let refused = if name.is_empty() || schematic::valid_player_name(name) {
        let name = (!name.is_empty()).then_some(name);
        match User::set_minecraft_name(&state.db, user.id, name).await {
            Ok(Ok(())) => {
                return Redirect::to(&format!("/users/{username}"))
                    .into_response();
            }
            Ok(Err(message)) => message,
            Err(err) => return internal_error(err).into_response(),
        }
    } else {
        format!(
            "{name} is not a Minecraft name: letters, digits, and \
             underscores, up to 16."
        )
    };
    show(&state, Some(&user), &user, &refused).await
}

async fn show(
    state: &AppState,
    viewer: Option<&User>,
    account: &User,
    message: &str,
) -> Response {
    let uploads: Vec<_> = match schematic::all(&state.db).await {
        Ok(all) => all
            .into_iter()
            .filter(|s| {
                s.uploader.as_deref() == Some(account.username.as_str())
            })
            .collect(),
        Err(err) => return internal_error(err).into_response(),
    };
    pages::profile(viewer, account, &uploads, message).into_response()
}
