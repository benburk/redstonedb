//! The viewer's settings: their Minecraft name and their Schemati session.

use axum::{
    Form,
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    models::user::User,
    recognize::palette::{self, Palette},
    web::{
        app_state::AppState,
        error::AppError,
        extractors::{fixi::Fixi, session::SignedIn},
    },
};

use super::{
    pages,
    profile::{player_named, saved},
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

/// The settings page for `user`, their Schemati session as Schemati last
/// answered with it, so the page waits on nothing but the database, with
/// `said`.
pub(super) async fn page(
    state: &AppState,
    user: &User,
    said: &Said<'_>,
) -> Result<Response, AppError> {
    let link = User::schemati_link(&state.db, user.id).await?;
    let chosen = palette::of(&state.db, user.id).await?.unwrap_or_default();
    Ok(pages::settings(
        user,
        link.as_ref(),
        said.minecraft,
        said.schemati,
        &chosen,
    )
    .into_response())
}

/// A palette's fields, as the theme editor sends them: from the settings
/// page, where they are kept, or from the download dialog's custom
/// colours, which only that download takes.
#[derive(Deserialize)]
pub(super) struct PaletteFields {
    prefer: Option<String>,
    solid: Option<String>,
    transparent: Option<String>,
    a: Option<String>,
    b: Option<String>,
    control: Option<String>,
    output: Option<String>,
    status: Option<String>,
    hard: Option<String>,
    soft: Option<String>,
    /// A preset's key, to take its palette whole.
    preset: Option<String>,
}

impl PaletteFields {
    /// Whether any field is given, as none is before the custom colours'
    /// editor first comes in.
    pub(super) fn given(&self) -> bool {
        [
            &self.prefer,
            &self.solid,
            &self.transparent,
            &self.a,
            &self.b,
            &self.control,
            &self.output,
            &self.status,
            &self.hard,
            &self.soft,
            &self.preset,
        ]
        .iter()
        .any(|field| field.is_some())
    }

    /// The palette the fields give, the site's blocks for any left out.
    pub(super) fn palette(self) -> Palette {
        if let Some((.., preset)) = self.preset.as_deref().and_then(|key| {
            palette::presets().into_iter().find(|(k, ..)| *k == key)
        }) {
            return preset;
        }
        let site = Palette::default();
        Palette {
            prefer_solid: self.prefer.as_deref() != Some("transparent"),
            solid: self.solid.unwrap_or(site.solid),
            transparent: self.transparent.unwrap_or(site.transparent),
            colours: [
                ("a", self.a),
                ("b", self.b),
                ("control", self.control),
                ("output", self.output),
                ("status", self.status),
            ]
            .into_iter()
            .filter_map(|(key, colour)| Some((key.to_owned(), colour?)))
            .collect(),
            // `none` keeps the solid block for blocks carrying power.
            hard: self.hard.filter(|block| block != palette::NONE),
            soft: self.soft.filter(|block| block != palette::NONE),
        }
    }
}

/// The palette form: every field, or `reset` to go back to the site's.
#[derive(Deserialize)]
pub struct PaletteForm {
    #[serde(flatten)]
    fields: PaletteFields,
    #[serde(flatten)]
    reset: Reset,
    next: Option<String>,
}

/// Whether to go back to the site's palette.
#[derive(Deserialize)]
pub struct Reset {
    reset: Option<String>,
}

/// Keeps the viewer's palette, or goes back to the site's: as the site's
/// when it is the same, so nothing is made over for it. Through fixi, as a
/// preset sends it, the answer is the fields as saved.
pub async fn set_palette(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Fixi(fixi): Fixi,
    Form(form): Form<PaletteForm>,
) -> Result<Response, AppError> {
    let site = Palette::default();
    let preset = form.fields.preset.is_some();
    let reset = form.reset.reset.is_some();
    let chosen = form.fields.palette();
    // Roles that couldn't be told apart are refused, the theme as it was.
    if let Some(why) = chosen.clash().filter(|_| !reset) {
        if !fixi {
            return Ok(Redirect::to(&onward(
                form.next.as_deref(),
                "/settings",
            ))
            .into_response());
        }
        let kept = palette::of(&state.db, user.id).await?.unwrap_or_default();
        return Ok((
            [("FX-Notice", super::notice(&why))],
            pages::palette_fields(&kept, pages::Presets::Saved),
        )
            .into_response());
    }
    let keep = !reset && chosen.valid() && chosen != site;
    palette::set(&state.db, user.id, keep.then_some(&chosen)).await?;
    if fixi {
        let saved = if keep { chosen } else { site };
        return Ok((
            [("FX-Notice", super::notice("Block theme saved"))],
            pages::palette_fields(&saved, pages::Presets::Saved),
        )
            .into_response());
    }
    // A preset or the site's own shows itself on the form; saving goes on.
    let next = if preset || reset {
        form.next.as_deref().filter(|n| *n != "/").map_or_else(
            || "/welcome?step=3".to_owned(),
            |n| onward(Some(n), "/settings"),
        )
    } else {
        onward(form.next.as_deref(), "/settings")
    };
    Ok(Redirect::to(&next).into_response())
}

/// The block theme's editor, as the viewer has it: through fixi, its
/// dialog, and without it, its page.
pub async fn palette_editor(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Fixi(fixi): Fixi,
) -> Result<Response, AppError> {
    let chosen = palette::of(&state.db, user.id).await?.unwrap_or_default();
    Ok(if fixi {
        pages::block_theme(&chosen).into_response()
    } else {
        pages::block_theme_page(&user, &chosen).into_response()
    })
}

/// The download's custom colours' editor, as its fields so far give it,
/// or a preset or the site's own; when none is given, as it first comes
/// in, the viewer's block theme, or the site's.
pub async fn custom_colours(
    State(state): State<AppState>,
    viewer: Option<User>,
    Query(fields): Query<PaletteFields>,
    Query(reset): Query<Reset>,
) -> Result<Response, AppError> {
    let start = if reset.reset.is_some() {
        Palette::default()
    } else if fields.given() {
        Some(fields.palette())
            .filter(Palette::known)
            .unwrap_or_default()
    } else {
        match &viewer {
            Some(user) => palette::of(&state.db, user.id).await?,
            None => None,
        }
        .unwrap_or_default()
    };
    Ok(pages::custom_colours(&start, viewer.is_some()).into_response())
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
