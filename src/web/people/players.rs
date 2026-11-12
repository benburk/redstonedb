//! A page for each Minecraft player, whether or not they have an account
//! or the site credits them yet.

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    account::User,
    build::{Build, valid_player_name},
    store::builds,
    web::{
        app_state::AppState,
        error::AppError,
        href,
        people::{
            heads,
            person::{self, Person, Shown},
        },
    },
};

/// The page of the player `name`, whatever its capitals: what they built,
/// helped with, are named in with `@name`, and hold records for, with
/// their account's builds if they have signed up. The address takes the spelling the
/// site credits, or else the first a description writes, or else Mojang's;
/// a 404 for a name no player has.
pub async fn show(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Shown { show }): Query<Shown>,
) -> Result<Response, AppError> {
    if !valid_player_name(&name) {
        return Err(AppError::NotFound);
    }
    let all = builds::visible(&state.db, user_opt.as_ref()).await?;
    let users = User::all(&state.db).await?;
    let same = |other: &String| other.eq_ignore_ascii_case(&name);
    let account = users.iter().find(|u| {
        u.minecraft_verified && u.minecraft_name.as_ref().is_some_and(same)
    });
    let credited = all.iter().find_map(|s| {
        s.credits
            .authors
            .iter()
            .chain(&s.credits.contributors)
            .chain(&s.entry.imported_by)
            .find(|n| same(n))
            .cloned()
    });
    let named = |s: &Build| {
        s.description
            .as_deref()
            .map(crate::build::mentions)
            .unwrap_or_default()
            .into_iter()
            .find(|n| same(n))
    };
    let mentioned_as = all.iter().find_map(named);
    let known = account
        .and_then(|u| u.minecraft_name.clone())
        .or(credited)
        .or(mentioned_as);
    // A player the site has no build of yet, as one the Credits page names,
    // still has a page, spelled as Mojang spells them.
    let spelling = match known {
        Some(spelling) => spelling,
        None => {
            heads::player(&state.db, &name)
                .await
                .ok_or(AppError::NotFound)?
                .name
        }
    };
    if spelling != name {
        let page = person::showing(&href::player(&spelling), show.as_deref());
        return Ok(Redirect::permanent(&page).into_response());
    }
    let who = Person::Player {
        name: &spelling,
        account,
    };
    person::page(&state, user_opt.as_ref(), &who, show.as_deref(), all).await
}

/// The head of the player `name`, for a badge to show beside their name:
/// their UUID found as their page finds it, then their head as
/// `/heads/<uuid>` serves it; a 404 for a name no player has.
pub async fn head(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    if !valid_player_name(&name) {
        return Err(AppError::NotFound);
    }
    let found = heads::player(&state.db, &name)
        .await
        .ok_or(AppError::NotFound)?;
    heads::show(Path(found.uuid), State(state)).await
}
