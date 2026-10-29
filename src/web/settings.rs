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
    /// A preset's key, to take its palette whole.
    preset: Option<String>,
    next: Option<String>,
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
    let chosen = match form.preset.as_deref() {
        Some(key) => palette::presets()
            .into_iter()
            .find(|(k, ..)| *k == key)
            .map_or(chosen, |(.., preset)| preset),
        None => chosen,
    };
    let keep = form.reset.is_none() && chosen.valid() && chosen != site;
    palette::set(&state.db, user.id, keep.then_some(&chosen)).await?;
    // A preset or the site's own shows itself on the form; saving goes on.
    let next = if form.preset.is_some() || form.reset.is_some() {
        form.next.as_deref().filter(|n| *n != "/").map_or_else(
            || "/welcome?step=3".to_owned(),
            |n| onward(Some(n), "/settings#palette-heading"),
        )
    } else {
        onward(form.next.as_deref(), "/settings#palette-heading")
    };
    Ok(Redirect::to(&next).into_response())
}

#[derive(Deserialize)]
pub struct MinecraftName {
    minecraft_name: Option<String>,
    next: Option<String>,
}

/// Where a form goes once saved: `next`, if it is one of the site's
/// settings or welcome pages or its home, else `fallback`, so a form can't
/// be made to send anyone elsewhere.
pub(super) fn onward(next: Option<&str>, fallback: &str) -> String {
    match next {
        Some(next)
            if next == "/"
                || next.starts_with("/settings")
                || next.starts_with("/welcome?") =>
        {
            next.to_owned()
        }
        _ => fallback.to_owned(),
    }
}

#[derive(Deserialize)]
pub struct WelcomeStep {
    step: Option<u8>,
}

/// A welcome step, as a new account sees at its first login.
pub async fn welcome(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    axum::extract::Query(at): axum::extract::Query<WelcomeStep>,
) -> Result<Response, AppError> {
    welcome_page(&state, &user, at.step.unwrap_or(1), "").await
}

/// Welcome step `step` for `user`, saying `message`, why their last try
/// at it was refused.
pub(super) async fn welcome_page(
    state: &AppState,
    user: &User,
    step: u8,
    message: &str,
) -> Result<Response, AppError> {
    let chosen = palette::of(&state.db, user.id).await?.unwrap_or_default();
    Ok(
        pages::welcome(user, step.clamp(1, 3), message, &chosen)
            .into_response(),
    )
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
    let next = form.next.as_deref();
    match saved {
        Ok(()) => Ok(Redirect::to(&onward(next, "/settings")).into_response()),
        Err(refused) if next.is_some_and(|n| n.starts_with("/welcome")) => {
            welcome_page(&state, &user, 1, &refused).await
        }
        Err(refused) => {
            let said = Said {
                minecraft: &refused,
                ..Said::default()
            };
            page(&state, &user, &said).await
        }
    }
}
