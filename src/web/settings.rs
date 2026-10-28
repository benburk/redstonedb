//! The viewer's settings: their Minecraft name and their Schemati session.

use axum::{
    Form,
    extract::State,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::AppError,
    extractors::session::SignedIn,
    models::user::User,
    palette::{self, Palette},
};

use super::{
    pages,
    profile::{player_named, saved},
    schemati::{connection, own_folder_quickly},
};

/// The settings page.
pub async fn show(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    page(&state, &user, &Said::default()).await
}

/// Why a change on the settings page could not be made, beside the
/// section it was made in.
#[derive(Default)]
pub(super) struct Said<'a> {
    pub minecraft: &'a str,
    pub schemati: &'a str,
}

/// The settings page for `user`, their Schemati session as Schemati has it
/// now, with `said`.
pub(super) async fn page(
    state: &AppState,
    user: &User,
    said: &Said<'_>,
) -> Result<Response, AppError> {
    let folder = own_folder_quickly(&state.db, user).await?;
    let chosen = palette::of(&state.db, user.id).await?.unwrap_or_default();
    Ok(pages::settings(
        user,
        &connection(folder.as_ref()),
        said.minecraft,
        said.schemati,
        &chosen,
    )
    .into_response())
}

/// The palette form: every field, or `reset` to go back to the site's.
#[derive(Deserialize)]
pub struct PaletteForm {
    prefer: Option<String>,
    solid: Option<String>,
    transparent: Option<String>,
    a: Option<String>,
    b: Option<String>,
    control: Option<String>,
    output: Option<String>,
    status: Option<String>,
    reset: Option<String>,
}

/// Keeps the viewer's palette, or goes back to the site's: as the site's
/// when it is the same, so nothing is made over for it.
pub async fn set_palette(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Form(form): Form<PaletteForm>,
) -> Result<Response, AppError> {
    let site = Palette::default();
    let chosen = Palette {
        prefer_solid: form.prefer.as_deref() != Some("transparent"),
        solid: form.solid.unwrap_or(site.solid.clone()),
        transparent: form.transparent.unwrap_or(site.transparent.clone()),
        colours: [
            ("a", form.a),
            ("b", form.b),
            ("control", form.control),
            ("output", form.output),
            ("status", form.status),
        ]
        .into_iter()
        .filter_map(|(key, colour)| Some((key.to_owned(), colour?)))
        .collect(),
    };
    let keep = form.reset.is_none() && chosen.valid() && chosen != site;
    palette::set(&state.db, user.id, keep.then_some(&chosen)).await?;
    Ok(Redirect::to("/settings#palette-heading").into_response())
}

#[derive(Deserialize)]
pub struct MinecraftName {
    minecraft_name: Option<String>,
}

/// Sets or clears the viewer's Minecraft name, as Mojang spells it.
pub async fn set_minecraft_name(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Form(form): Form<MinecraftName>,
) -> Result<Response, AppError> {
    let name = form.minecraft_name.unwrap_or_default();
    let name = name.trim();
    let saved = if name.is_empty() {
        saved(&state, user.id, None).await?
    } else {
        match player_named(name).await {
            Ok(player) => saved(&state, user.id, Some(&player)).await?,
            Err(message) => Err(message),
        }
    };
    match saved {
        Ok(()) => Ok(Redirect::to("/settings").into_response()),
        Err(refused) => {
            let said = Said {
                minecraft: &refused,
                ..Said::default()
            };
            page(&state, &user, &said).await
        }
    }
}
