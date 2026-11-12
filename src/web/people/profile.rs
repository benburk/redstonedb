use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    account::User,
    build::valid_player_name,
    external::mojang,
    store::builds,
    web::{
        app_state::AppState,
        error::AppError,
        href,
        people::person::{self, Person, Shown},
    },
};

/// An account's page. One whose Minecraft name is confirmed is that
/// player, so its address goes to the player's page, which shows the
/// account too; any other is shown here, with the builds it uploaded and
/// those crediting the name it gave, if it gave one.
pub async fn profile(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Shown { show }): Query<Shown>,
) -> Result<Response, AppError> {
    let account = User::get_by_username(&state.db, &username)
        .await?
        .ok_or(AppError::NotFound)?;
    let page = href::account(&account);
    if page != href::user(&account.username) {
        let page = person::showing(&page, show.as_deref());
        return Ok(Redirect::temporary(&page).into_response());
    }
    let all = builds::visible(&state.db, user_opt.as_ref()).await?;
    let who = Person::Account(&account);
    person::page(&state, user_opt.as_ref(), &who, show.as_deref(), all).await
}

/// The player `name` names, spelled as they spell it and with their UUID,
/// or a sentence saying it names none. When Mojang cannot be asked, the
/// name as typed, with no UUID.
pub(crate) async fn player_named(
    name: &str,
) -> Result<mojang::Profile, String> {
    if !valid_player_name(name) {
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
pub(crate) async fn saved(
    state: &AppState,
    id: uuid::Uuid,
    player: Option<&mojang::Profile>,
) -> Result<Result<(), String>, AppError> {
    let name = player.map(|p| p.name.as_str());
    let uuid = player.map(|p| p.uuid.as_str()).filter(|u| !u.is_empty());
    Ok(User::set_minecraft_name(&state.db, id, name, uuid).await?)
}
