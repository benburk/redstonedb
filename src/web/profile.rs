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
    mojang,
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
        Ok(None) => return super::not_found(user_opt.as_ref()),
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
    let refused = if name.is_empty() {
        saved(&state, &user, None).await
    } else if !schematic::valid_player_name(name) {
        Err(format!(
            "{name} is not a Minecraft name: letters, digits, and \
             underscores, up to 16."
        ))
    } else {
        // Stored as its player spells it, with their UUID; when Mojang
        // cannot be asked, as typed.
        match mojang::profile(name).await {
            Ok(Some(found)) => saved(&state, &user, Some(&found)).await,
            Ok(None) => Err(format!(
                "No Minecraft player is named {name}. Check the spelling."
            )),
            Err(_) => {
                let typed = mojang::Profile {
                    uuid: String::new(),
                    name: name.to_owned(),
                };
                saved(&state, &user, Some(&typed)).await
            }
        }
    };
    let refused = match refused {
        Ok(Ok(())) => {
            return Redirect::to(&format!("/users/{username}")).into_response();
        }
        Ok(Err(message)) | Err(message) => message,
    };
    show(&state, Some(&user), &user, &refused).await
}

/// Stores `player` as the account's Minecraft name, or clears it for
/// `None`. The inner `Err` says the name is another account's; the outer
/// one is any other failure, already logged.
async fn saved(
    state: &AppState,
    user: &User,
    player: Option<&mojang::Profile>,
) -> Result<Result<(), String>, String> {
    let name = player.map(|p| p.name.as_str());
    let uuid = player.map(|p| p.uuid.as_str()).filter(|u| !u.is_empty());
    User::set_minecraft_name(&state.db, user.id, name, uuid)
        .await
        .map_err(|err| internal_error(err).1.to_owned())
}

async fn show(
    state: &AppState,
    viewer: Option<&User>,
    account: &User,
    message: &str,
) -> Response {
    let mut all = match schematic::all(&state.db).await {
        Ok(all) => all,
        Err(err) => return internal_error(err).into_response(),
    };
    // Newest first, as on the home page.
    all.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    // Credits are in-game names, so they match the profile's Minecraft
    // name, whatever its case.
    let credited = |names: &[String]| {
        account.minecraft_name.as_deref().is_some_and(|own| {
            names.iter().any(|name| name.eq_ignore_ascii_case(own))
        })
    };
    let pick = |keep: &dyn Fn(&schematic::Schematic) -> bool| -> Vec<_> {
        all.iter().filter(|s| keep(s)).cloned().collect()
    };
    let builds = pages::ProfileBuilds {
        built: pick(&|s| credited(&s.builders)),
        contributed: pick(&|s| credited(&s.contributors)),
        uploaded: pick(&|s| {
            s.uploader.as_deref() == Some(account.username.as_str())
        }),
    };
    pages::profile(viewer, account, &builds, message).into_response()
}
