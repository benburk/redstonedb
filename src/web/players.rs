//! A page for each Minecraft player the site credits, whether or not they
//! have an account.

use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    app_state::AppState,
    error::AppError,
    leaderboard,
    models::{
        schematic::{self, Schematic},
        user::User,
    },
    mojang,
};

use super::pages;

/// How long a player looked up is kept before Mojang is asked again.
const KEPT: Duration = Duration::from_hours(6);

/// The page of the player `name`, whatever its capitals: what they built,
/// helped with, are named in with `@name`, and hold records for, and their
/// account if they have signed up. The address takes the spelling the
/// site credits, or else the first a description writes.
pub async fn show(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    if !schematic::valid_player_name(&name) {
        return Err(AppError::NotFound);
    }
    let mut all = schematic::visible(&state.db, user_opt.as_ref()).await?;
    let users = User::all(&state.db).await?;
    let same = |other: &String| other.eq_ignore_ascii_case(&name);
    let account = users.iter().find(|u| {
        u.minecraft_verified && u.minecraft_name.as_ref().is_some_and(same)
    });
    let credited = all.iter().find_map(|s| {
        s.builders
            .iter()
            .chain(&s.contributors)
            .chain(&s.imported_by)
            .find(|n| same(n))
            .cloned()
    });
    let named = |s: &Schematic| {
        s.description
            .as_deref()
            .map(pages::mentions)
            .unwrap_or_default()
            .into_iter()
            .find(|n| same(n))
    };
    let mentioned_as = all.iter().find_map(named);
    let Some(spelling) = account
        .and_then(|u| u.minecraft_name.clone())
        .or(credited)
        .or(mentioned_as)
    else {
        return Err(AppError::NotFound);
    };
    if spelling != name {
        return Ok(Redirect::permanent(&format!("/players/{spelling}"))
            .into_response());
    }
    let records = leaderboard::most_records(&all)
        .into_iter()
        .find(|s| same(&s.name))
        .map_or(0, |s| s.count);
    // Newest first, as on the home page.
    all.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    let pick = |names: fn(&Schematic) -> &Vec<String>| -> Vec<Schematic> {
        all.iter()
            .filter(|s| names(s).iter().any(same))
            .cloned()
            .collect()
    };
    let uuid = match account.and_then(|u| u.minecraft_uuid.clone()) {
        Some(uuid) => Some(uuid),
        None => player(&state, &spelling).await.map(|p| p.uuid),
    };
    Ok(pages::player(
        user_opt.as_ref(),
        &pages::PlayerPage {
            name: &spelling,
            uuid: uuid.as_deref(),
            account,
            built: &pick(|s| &s.builders),
            contributed: &pick(|s| &s.contributors),
            // Those that credit them already list them there.
            mentioned: &all
                .iter()
                .filter(|s| {
                    named(s).is_some()
                        && !s.builders.iter().chain(&s.contributors).any(same)
                })
                .cloned()
                .collect::<Vec<_>>(),
            records,
            palette: crate::palette::key_for(&state.db, user_opt.as_ref())
                .await?
                .as_deref(),
        },
    )
    .into_response())
}

/// The head of the player `name`, for a badge to show beside their name:
/// their UUID found as their page finds it, then their head as
/// `/heads/<uuid>` serves it; a 404 for a name no player has.
pub async fn head(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    if !schematic::valid_player_name(&name) {
        return Err(AppError::NotFound);
    }
    let found = player(&state, &name).await.ok_or(AppError::NotFound)?;
    super::heads::show(Path(found.uuid), State(state)).await
}

/// The player Mojang knows as `name`, asked at most every few hours; `None`
/// for a name no player has, or when Mojang cannot be reached.
async fn player(state: &AppState, name: &str) -> Option<mojang::Profile> {
    let key = name.to_lowercase();
    let kept = state
        .players
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .filter(|(asked, _)| asked.elapsed() < KEPT)
        .map(|(_, found)| found.clone());
    if let Some(found) = kept {
        return found;
    }
    // Not remembered when Mojang cannot be reached, so the next view asks.
    let found = mojang::profile(name).await.ok()?;
    state
        .players
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, (Instant::now(), found.clone()));
    found
}
