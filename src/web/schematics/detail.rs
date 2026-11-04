//! A build's page, or the part of it fixi redraws, with its picture, the
//! builds like it, why it is not recognized, and favouriting it.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    import::{self, Analysis},
    models::{
        schematic::{self, Schematic},
        similarity, simulation,
        user::User,
        version,
    },
    recognize::palette,
    schem::{Blocks, stack},
    thumbnail,
    web::{
        app_state::AppState,
        error::AppError,
        extractors::{fixi::Fixi, session::SignedIn},
    },
};

use super::{super::pages, Colours};

/// An isometric picture of the build. Cards link to it with the file's hash
/// in the query, so a changed file gets a new URL and this one can be
/// cached for good. Each file is drawn once, off the async workers, as
/// drawing one takes tens of milliseconds.
pub async fn thumbnail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
) -> Result<Response, AppError> {
    let s = schematic::get_visible(&state.db, &id, viewer.as_ref())
        .await?
        .ok_or(AppError::NotFound)?;
    let data = version::current_file(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    // A build is drawn in the viewer's palette, if they chose one, and kept
    // apart from others' pictures of it, else in the site's: its roles in
    // their colours and, for a recognized build, its building blocks the
    // palette's. A build kept as built is drawn as its author made it.
    let chosen = match &viewer {
        Some(user) if s.conventional => palette::of(&state.db, user.id).await?,
        _ => None,
    };
    let painted = chosen.is_some();
    let palette = chosen.unwrap_or_default();
    let (data, key) = if s.conventional {
        let needs = palette::needs(&state.db, &id).await?;
        let key = format!("{}-{}", import::sha256(&data), palette.key());
        (palette::apply(&data, needs.as_ref(), &palette)?, key)
    } else {
        let data = crate::schem::lines::without(&data)?;
        let key = import::sha256(&data);
        (data, key)
    };
    let sha256 = key;
    let cached = state.thumbnails.lock().unwrap().get(&sha256);
    // From memory, else as drawn before and kept beside the database, else
    // drawn now, fetching the game's files it needs that aren't kept.
    let kept = std::path::Path::new(thumbnail::DRAWN_DIR)
        .join(format!("{}-{sha256}.webp", thumbnail::RENDERER));
    let png = if let Some(png) = cached {
        png
    } else if let Ok(webp) = tokio::fs::read(&kept).await {
        let webp = Bytes::from(webp);
        state
            .thumbnails
            .lock()
            .unwrap()
            .insert(sha256, webp.clone());
        webp
    } else {
        let blocks = Blocks::read(&data)?;
        let renderer = Arc::clone(&state.renderer);
        let webp = tokio::task::spawn_blocking(move || {
            thumbnail::render_with(&blocks, &renderer)
        })
        .await
        .map_err(anyhow::Error::from)??;
        let _ = tokio::fs::create_dir_all(thumbnail::DRAWN_DIR).await;
        let _ = tokio::fs::write(&kept, &webp).await;
        let webp = Bytes::from(webp);
        state
            .thumbnails
            .lock()
            .unwrap()
            .insert(sha256, webp.clone());
        webp
    };
    // The address names the palette, but who may see it is the viewer's.
    let cache = if painted {
        "private, max-age=31536000, immutable"
    } else {
        "public, max-age=31536000, immutable"
    };
    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, cache),
        ],
        png,
    )
        .into_response())
}

/// The builds build `id` is similar to that `viewer` may see, with how
/// alike, the most alike first.
async fn similar_to(
    db: &SqlitePool,
    id: &str,
    viewer: Option<&User>,
) -> anyhow::Result<Vec<(Schematic, u32)>> {
    let similar = similarity::similar(db, id).await?;
    Ok(super::super::similar::builds(db, &similar, viewer).await?)
}

/// Which part of a build's page fixi asks for, to redraw it.
#[derive(Deserialize)]
pub struct Panel {
    panel: Option<String>,
    /// The colours the page shows the build in.
    colours: Option<Colours>,
}

/// What build `s`'s page shows of its file, `data`, with `blocks`: as its
/// category reads it, or for one unverified or off the list while it is
/// not recognized as the category it stands in for, standardized and read
/// as that category, as recognizing it would.
fn read_for_page(s: &Schematic, data: &[u8], blocks: &Blocks) -> Analysis {
    if let Some(meant) = simulation::stands_in(s)
        && let Ok(standard) = import::prepare_for(
            data,
            import::Purpose::Standard {
                keep_paste: s.keeps_paste,
            },
        )
    {
        return Analysis::of(&standard.blocks, meant);
    }
    Analysis::of(blocks, &s.category)
}

/// A build's page, or the one part of it fixi asks for, loading only what
/// that part shows.
pub async fn detail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Query(Panel { panel, colours }): Query<Panel>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = schematic::get_visible(db, &id, user_opt.as_ref())
        .await?
        .ok_or(AppError::NotFound)?;
    let simulation = simulation::status(db, &s).await?;
    let report = match &simulation {
        simulation::Status::Done(report)
        | simulation::Status::Pending(Some(report)) => Some(report),
        simulation::Status::Pending(None) | simulation::Status::Untested => {
            None
        }
    };
    let panel = panel.as_deref().filter(|_| fixi);
    if panel == Some("banner") {
        let simulating =
            matches!(simulation, simulation::Status::Pending(None));
        return Ok(pages::build_banner(&s, report, simulating).into_response());
    }
    let data = version::current_file(db, &id).await?.unwrap_or_default();
    let unrecognized = why_not_recognized(&s, &data).await;
    let file = Blocks::read(&data)
        .map(|blocks| pages::FileRead {
            unrecognized,
            analysis: read_for_page(&s, &data, &blocks),
            block_counts: crate::schem::standardize::block_counts(&blocks),
            strength_inputs: crate::recognize::interface::takes_digits(&blocks),
            stacking: s
                .stacking
                .iter()
                .filter_map(|&direction| {
                    stack::review(
                        &data,
                        direction,
                        stack::Buses::of(s.kind(), s.orientation),
                    )
                    .ok()
                })
                .collect(),
        })
        .map_err(|err| err.to_string());
    let similar = similar_to(db, &id, user_opt.as_ref()).await?;
    if panel == Some("info") {
        return Ok(
            pages::build_info(&s, &similar, &file, report).into_response()
        );
    }
    let mut versions = version::list(db, &id).await?;
    crate::web::versions::count_changes(db, &id, &mut versions).await?;
    let favorited = match &user_opt {
        Some(user) => schematic::favorited(db, user.id, &id).await?,
        None => false,
    };
    // A theme or an author's colours give the page a choice of colours; a
    // theme only for a build recognized, verified, and passing, whose
    // blocks the site knows the needs of, as any other's blocks are its
    // author's own.
    let themeable = palette::needs(db, &id).await?.is_some();
    let theme = themeable
        && match &user_opt {
            Some(user) => palette::of(db, user.id).await?.is_some(),
            None => false,
        };
    // Its stored blocks, its author's, differ from the site's colours
    // once a theme colours its roles, as for a build that follows the
    // conventions; one kept as built shows only as built.
    let original = s.conventional;
    let colours = pages::ColoursShown {
        chosen: match colours {
            Some(Colours::Original) if original => Colours::Original,
            Some(Colours::Theme) | None if theme => Colours::Theme,
            _ => Colours::Default,
        },
        original,
        theme,
        themeable,
    };
    Ok(pages::schematic_page(
        user_opt.as_ref(),
        &pages::BuildPage {
            s: &s,
            similar: &similar,
            file: &file,
            simulation: &simulation,
            versions: &versions,
            favorited,
            colours,
        },
    )
    .into_response())
}

/// For build `s`, unverified or filed off the list as an adder or a
/// register, what keeps it from being recognized as one by the check by
/// function on its file `data`: one line for each fault. Empty for any
/// other build.
async fn why_not_recognized(s: &Schematic, data: &[u8]) -> Vec<String> {
    // Probing doesn't read hex inputs, so a hex adder's faults are its
    // test's.
    let Some(meant) = crate::models::simulation::stands_in(s)
        .filter(|meant| crate::category::blessed(meant))
        .filter(|meant| *meant != "hex_adder")
        .map(str::to_owned)
    else {
        return Vec::new();
    };
    let data = data.to_vec();
    // Probing simulates, so off the async threads.
    let recognized = tokio::task::spawn_blocking(move || {
        crate::recognize::probe::recognize_as(&data, Some(&meant))
    })
    .await;
    match recognized {
        Ok(Ok(recognition)) => recognition.problems,
        Ok(Err(err)) => vec![err.to_string()],
        Err(err) => vec![err.to_string()],
    }
}

/// Whether a form favourites a build or takes it back.
#[derive(Deserialize)]
pub struct Favorite {
    on: Option<String>,
    /// Sent by a card's star, which is redrawn rather than the page's.
    card: Option<String>,
}

/// Favourites build `id` for the signed-in user, or takes it back, then
/// shows its page again, or through fixi redraws the star pressed: a
/// card's, or the page's.
pub async fn favorite(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Fixi(fixi): Fixi,
    axum::Form(Favorite { on, card }): axum::Form<Favorite>,
) -> Result<Response, AppError> {
    schematic::get_visible(&state.db, &id, Some(&user))
        .await?
        .ok_or(AppError::NotFound)?;
    schematic::set_favorite(&state.db, user.id, &id, on.is_some()).await?;
    let path = format!("/schematics/{id}");
    if fixi && card.is_some() {
        return Ok(pages::card_star(&id, on.is_some()).into_response());
    }
    if fixi {
        return Ok(pages::favorite_button(&path, on.is_some()).into_response());
    }
    Ok(Redirect::to(&path).into_response())
}
