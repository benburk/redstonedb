//! Players' heads, drawn from their Minecraft skins.

use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};

use crate::{app_state::AppState, error::AppError, head};

/// How long a fetched head is kept before Mojang is asked again.
const KEPT: Duration = Duration::from_hours(6);

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
    let kept = {
        let heads = state
            .heads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        heads
            .get(&uuid)
            .filter(|(fetched, _)| fetched.elapsed() < KEPT)
            .map(|(_, png)| png.clone())
    };
    let png = match kept {
        Some(png) => png,
        None => match head::fetch(&uuid).await {
            Ok(png) => {
                state
                    .heads
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(uuid, (Instant::now(), png.clone()));
                png
            }
            Err(err) => {
                tracing::warn!("no head for {uuid}: {err}");
                return Err(AppError::NotFound);
            }
        },
    };
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            // Skins change rarely; a day's delay is fine.
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        png,
    )
        .into_response())
}
