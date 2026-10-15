//! A page for each Minecraft player the site credits, whether or not they
//! have an account.

use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    app_state::AppState,
    error::internal_error,
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
/// helped with, and hold records for, and their account if they have
/// signed up. The address takes the spelling the site credits.
pub async fn show(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    if !schematic::valid_player_name(&name) {
        return super::not_found(user_opt.as_ref());
    }
    let loaded = async {
        Ok::<_, sqlx::Error>((
            schematic::all(&state.db).await?,
            User::all(&state.db).await?,
        ))
    };
    let (mut all, users) = match loaded.await {
        Ok(loaded) => loaded,
        Err(err) => return internal_error(err).into_response(),
    };
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
    let Some(spelling) =
        account.and_then(|u| u.minecraft_name.clone()).or(credited)
    else {
        return super::not_found(user_opt.as_ref());
    };
    if spelling != name {
        return Redirect::permanent(&format!("/players/{spelling}"))
            .into_response();
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
    pages::player(
        user_opt.as_ref(),
        &pages::PlayerPage {
            name: &spelling,
            uuid: uuid.as_deref(),
            account,
            built: &pick(|s| &s.builders),
            contributed: &pick(|s| &s.contributors),
            records,
        },
    )
    .into_response()
}

/// The head of the player `name`, for a badge to show beside their name:
/// their UUID found as their page finds it, then their head as
/// `/heads/<uuid>` serves it; a 404 for a name no player has.
pub async fn head(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if !schematic::valid_player_name(&name) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match player(&state, &name).await {
        Some(found) => super::heads::show(Path(found.uuid), State(state)).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
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
