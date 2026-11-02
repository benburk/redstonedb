//! A page for each Minecraft player, whether or not they have an account
//! or the site credits them yet.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    leaderboard,
    models::{
        schematic::{self, Schematic},
        user::User,
    },
    web::{app_state::AppState, error::AppError},
};

use super::pages;

/// The page of the player `name`, whatever its capitals: what they built,
/// helped with, are named in with `@name`, and hold records for, and their
/// account if they have signed up. The address takes the spelling the
/// site credits, or else the first a description writes, or else Mojang's;
/// a 404 for a name no player has.
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
    let known = account
        .and_then(|u| u.minecraft_name.clone())
        .or(credited)
        .or(mentioned_as);
    // A player the site has no build of yet, as one the Credits page names,
    // still has a page, spelled as Mojang spells them.
    let spelling = match known {
        Some(spelling) => spelling,
        None => {
            super::heads::player(&state.db, &name)
                .await
                .ok_or(AppError::NotFound)?
                .name
        }
    };
    if spelling != name {
        return Ok(Redirect::permanent(&format!("/players/{spelling}"))
            .into_response());
    }
    // Newest first, as on the home page.
    all.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    // Private builds compete with no one.
    let public: Vec<&Schematic> = all.iter().filter(|s| s.public).collect();
    let stacks = leaderboard::stacked(public.iter().copied());
    let builds: Vec<&Schematic> = public.into_iter().chain(&stacks).collect();
    let records = leaderboard::records(&builds);
    let held: Vec<&leaderboard::Record> = records
        .iter()
        .filter(|r| r.holders.iter().any(|s| s.builders.iter().any(same)))
        .collect();
    let pick = |names: fn(&Schematic) -> &Vec<String>| -> Vec<Schematic> {
        all.iter()
            .filter(|s| names(s).iter().any(same))
            .cloned()
            .collect()
    };
    let uuid = match account.and_then(|u| u.minecraft_uuid.clone()) {
        Some(uuid) => Some(uuid),
        None => super::heads::player(&state.db, &spelling)
            .await
            .map(|p| p.uuid),
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
            records: &held,
            viewing: &super::viewing(&state.db, user_opt.as_ref()).await?,
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
    let found = super::heads::player(&state.db, &name)
        .await
        .ok_or(AppError::NotFound)?;
    super::heads::show(Path(found.uuid), State(state)).await
}
