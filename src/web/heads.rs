//! Players' heads, drawn from their Minecraft skins, and the players
//! themselves, looked up by name. Both are kept in the database: a page
//! is served what is kept at once, refreshed from Mojang in the background
//! once it is a few days old, and only what was never asked about waits on
//! Mojang. After starting, every player the site credits is fetched ahead,
//! so the first visitor waits on none.

use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    external::{head, mojang},
    store::players,
    web::{app_state::AppState, error::AppError},
};

/// The head of the player with `uuid`, as an 8×8 PNG for a page to scale
/// up pixelated; a 404 when there is none to show.
pub async fn show(
    Path(uuid): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let plain = uuid.len() == 32
        && uuid
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !plain {
        return Err(AppError::NotFound);
    }
    let png = match players::head(&state.db, &uuid).await? {
        Some((png, fresh)) => {
            if !fresh {
                let (db, uuid) = (state.db.clone(), uuid.clone());
                tokio::spawn(async move { fetch_head(&db, &uuid).await });
            }
            png
        }
        None => fetch_head(&state.db, &uuid).await,
    };
    let png = png.ok_or(AppError::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            // Skins change rarely; a week's delay is fine.
            (header::CACHE_CONTROL, "public, max-age=604800"),
        ],
        png,
    )
        .into_response())
}

/// Fetches the head of the player with `uuid` from Mojang and keeps it, a
/// player with no skin as none; `None` when there is none to show, or
/// Mojang could not be asked, which keeps nothing, so it is asked again.
async fn fetch_head(db: &SqlitePool, uuid: &str) -> Option<Vec<u8>> {
    match head::fetch(uuid).await {
        Ok(png) => {
            if let Err(err) = players::keep_head(db, uuid, png.as_deref()).await
            {
                tracing::warn!("keeping the head of {uuid}: {err}");
            }
            png
        }
        Err(err) => {
            tracing::warn!("no head for {uuid}: {err}");
            None
        }
    }
}

/// The player Mojang knows as `name`, whatever its capitals, as kept, and
/// refreshed in the background once old; asked of Mojang only when never
/// asked before. `None` for a name no player has, or when Mojang cannot be
/// reached.
pub async fn player(db: &SqlitePool, name: &str) -> Option<mojang::Profile> {
    match players::known(db, name).await {
        Ok(Some((found, fresh))) => {
            if !fresh {
                let (db, name) = (db.clone(), name.to_owned());
                tokio::spawn(async move { look_up(&db, &[name]).await });
            }
            found
        }
        Ok(None) => look_up(db, &[name.to_owned()]).await.pop().flatten(),
        Err(err) => {
            tracing::warn!("reading player {name}: {err}");
            None
        }
    }
}

/// Asks Mojang about each of `names` and keeps what it says, a name no
/// player has as none: each name's player, in order. Nothing is kept when
/// Mojang cannot be reached.
async fn look_up(
    db: &SqlitePool,
    names: &[String],
) -> Vec<Option<mojang::Profile>> {
    let found = match mojang::profiles(names).await {
        Ok(found) => found,
        Err(err) => {
            tracing::warn!("looking up players: {err}");
            return vec![None; names.len()];
        }
    };
    let mut players = Vec::new();
    for name in names {
        let profile = found
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .cloned();
        if let Err(err) = players::remember(db, name, profile.as_ref()).await {
            tracing::warn!("keeping player {name}: {err}");
        }
        players.push(profile);
    }
    players
}

/// The pause between heads fetched ahead, as Mojang answers only so many
/// asks a minute.
const PACE: Duration = Duration::from_millis(1500);

/// In the background, looks up every player the site shows a head for and
/// never asked about, those credited, importing, signed up, or named in a
/// description, then fetches each head never fetched, a little at a time.
pub fn fetch_ahead(db: &SqlitePool) {
    let db = db.clone();
    tokio::spawn(async move {
        if let Err(err) = fetch_all(&db).await {
            tracing::warn!("fetching heads ahead: {err:#}");
        }
    });
}

async fn fetch_all(db: &SqlitePool) -> anyhow::Result<()> {
    let descriptions = crate::store::builds::descriptions(db).await?;
    let mentioned: Vec<String> = descriptions
        .iter()
        .flat_map(|text| super::pages::mentions(text))
        .collect();
    let names = players::unasked(db, &mentioned).await?;
    for batch in names.chunks(10) {
        look_up(db, batch).await;
        tokio::time::sleep(PACE).await;
    }
    let uuids = players::headless(db).await?;
    tracing::info!(
        players = names.len(),
        heads = uuids.len(),
        "fetching heads ahead"
    );
    for uuid in uuids {
        fetch_head(db, &uuid).await;
        tokio::time::sleep(PACE).await;
    }
    Ok(())
}
