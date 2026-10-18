use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState,
    error::AppError,
    models::{schematic, user::User},
    mojang,
};

use super::pages;

/// An account's page: who they are and what they uploaded.
pub async fn profile(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let account = User::get_by_username(&state.db, &username)
        .await?
        .ok_or(AppError::NotFound)?;
    show(&state, user_opt.as_ref(), &account).await
}

/// The player `name` names, spelled as they spell it and with their UUID,
/// or a sentence saying it names none. When Mojang cannot be asked, the
/// name as typed, with no UUID.
pub(super) async fn player_named(
    name: &str,
) -> Result<mojang::Profile, String> {
    if !schematic::valid_player_name(name) {
        return Err(format!(
            "{name} is not a Minecraft name: letters, digits, and \
             underscores, up to 16."
        ));
    }
    match mojang::profile(name).await {
        Ok(Some(found)) => Ok(found),
        Ok(None) => Err(format!(
            "No Minecraft player is named {name}. Check the spelling."
        )),
        Err(_) => Ok(mojang::Profile {
            uuid: String::new(),
            name: name.to_owned(),
        }),
    }
}

/// Stores `player` as account `id`'s Minecraft name, or clears it for
/// `None`. The inner `Err` says the name is another account's.
pub(super) async fn saved(
    state: &AppState,
    id: uuid::Uuid,
    player: Option<&mojang::Profile>,
) -> Result<Result<(), String>, AppError> {
    let name = player.map(|p| p.name.as_str());
    let uuid = player.map(|p| p.uuid.as_str()).filter(|u| !u.is_empty());
    Ok(User::set_minecraft_name(&state.db, id, name, uuid).await?)
}

async fn show(
    state: &AppState,
    viewer: Option<&User>,
    account: &User,
) -> Result<Response, AppError> {
    let mut all = schematic::all(&state.db).await?;
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
    Ok(pages::profile(viewer, account, &builds).into_response())
}
