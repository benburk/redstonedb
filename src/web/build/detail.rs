//! A build's page, or the part of it fixi redraws, with its picture, the
//! builds like it, why it is not recognized, favouriting it, and deleting
//! it.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    account::User,
    build::Build,
    ingest::{
        jobs,
        prepare::{self, Purpose},
        read::Analysis,
        results, similar,
    },
    interface::problem::{Problem, Rule},
    schem::{self, Blocks},
    stack,
    store::{builds, versions},
    thumbnail,
    web::{
        app_state::AppState,
        back, browse,
        build::{
            actions_view,
            detail_view::{self, BuildPage, ColoursShown, FileRead},
            download::{self, Colours},
            info_view,
        },
        error::AppError,
        extractors::{fixi::Fixi, session::SignedIn},
        href,
        ui::cards,
    },
};

/// An isometric picture of the build. Cards link to it with the file's hash
/// in the query, so a changed file gets a new URL and this one can be
/// cached for good. Each file is drawn once, off the async workers, as
/// drawing one takes tens of milliseconds.
pub async fn thumbnail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
) -> Result<Response, AppError> {
    // A build is drawn as the 3D viewer shows it: in the viewer's palette,
    // if they chose one, and kept apart from others' pictures of it, else
    // in the site's.
    let chosen = match &viewer {
        Some(user) => crate::store::users::palette(&state.db, user.id).await?,
        None => None,
    };
    let painted = chosen.is_some();
    let data = download::shown(&state.db, &id, chosen.as_ref()).await?;
    // Kept by what is drawn, so a build whose needs are kept later, its
    // file the same, is drawn again in its theme's building blocks.
    let sha256 = schem::sha256(&data);
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
) -> anyhow::Result<Vec<(Build, u32)>> {
    let similar = similar::similar(db, id).await?;
    Ok(browse::similar::builds(db, &similar, viewer).await?)
}

/// Which part of a build's page fixi asks for, to redraw it.
#[derive(Deserialize)]
pub struct Asked {
    panel: Option<String>,
    /// The colours the page shows the build in.
    colours: Option<Colours>,
    /// The listing the page was come to from, as its colour switch carries
    /// it, the page it links to then being the referrer.
    from: Option<String>,
}

/// What build `s`'s page shows of its file, `data`, with `blocks`: as its
/// category reads it, or for one not verified as its category,
/// standardized first, as recognizing it would.
fn read_for_page(s: &Build, data: &[u8], blocks: &Blocks) -> Analysis {
    if !s.verified()
        && let Ok(standard) = prepare::prepare_for(
            data,
            Purpose::Standard {
                keep_paste: s.file.keeps_paste,
            },
        )
    {
        return Analysis::of(&standard.blocks, &s.category);
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
    headers: HeaderMap,
    Query(Asked {
        panel,
        colours,
        from,
    }): Query<Asked>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let from = back::came_from(&headers, from.as_deref());
    let s = builds::get(db, &id).await?.ok_or(AppError::NotFound)?;
    let simulation = results::current(db, &s).await?;
    let report = simulation.as_ref();
    let simulating = jobs::completing(&id);
    let panel = panel.as_deref().filter(|_| fixi);
    if panel == Some("banner") {
        return Ok(detail_view::build_banner(&s, simulating).into_response());
    }
    let data = versions::current_file(db, &id).await?.unwrap_or_default();
    let unrecognized = why_not_recognized(&s, &data).await;
    let file = Blocks::read(&data)
        .map(|blocks| FileRead {
            unrecognized,
            analysis: read_for_page(&s, &data, &blocks),
            block_counts: crate::schem::block::block_counts(&blocks),
            strength_inputs: crate::interface::takes_digits(&blocks),
            kept: Vec::new(),
            stacking: s
                .stacking
                .iter()
                .filter_map(|&direction| {
                    stack::review(&data, direction, s.buses()).ok()
                })
                .collect(),
        })
        .map_err(|err| err.to_string());
    let similar = similar_to(db, &id, user_opt.as_ref()).await?;
    if panel == Some("info") {
        return Ok(
            info_view::build_info(&s, &similar, &file, report).into_response()
        );
    }
    let mut versions = versions::list(db, &id).await?;
    super::versions::count_changes(db, &id, &mut versions).await?;
    let favorited = match &user_opt {
        Some(user) => builds::favorited(db, user.id, &id).await?,
        None => false,
    };
    // A theme or an author's colours give the page a choice of colours; a
    // theme only for a build recognized, verified, and passing, whose
    // blocks the site knows the needs of, as any other's blocks are its
    // author's own.
    let themeable = crate::store::versions::needs(db, &id).await?.is_some();
    let theme = themeable
        && match &user_opt {
            Some(user) => {
                crate::store::users::palette(db, user.id).await?.is_some()
            }
            None => false,
        };
    // Its stored blocks, its author's, differ from the site's colours
    // once a theme colours its roles, as for a build that follows the
    // conventions; one kept as built shows only as built.
    let original = crate::ingest::results::carries_roles(&data);
    let colours = ColoursShown {
        chosen: match colours {
            Some(Colours::Original) if original => Colours::Original,
            Some(Colours::Theme) | None if theme => Colours::Theme,
            _ => Colours::Default,
        },
        original,
        theme,
        themeable,
    };
    Ok(detail_view::schematic_page(
        user_opt.as_ref(),
        &BuildPage {
            s: &s,
            similar: &similar,
            file: &file,
            simulation: &simulation,
            simulating,
            versions: &versions,
            favorited,
            colours,
            from: from.as_deref(),
        },
    )
    .into_response())
}

/// For build `s`, not verified as its category, one the site recognizes
/// builds of, what keeps it from being recognized as one by the check by
/// function on its file `data`: one line for each fault. Empty for any
/// other build.
async fn why_not_recognized(s: &Build, data: &[u8]) -> Vec<Problem> {
    // Probing doesn't read hex inputs, so a hex adder's faults are its
    // test's.
    if s.verified()
        || !crate::category::listed(&s.category)
        || s.category == "hex_adder"
    {
        return Vec::new();
    }
    let meant = s.category.clone();
    let data = data.to_vec();
    // Probing simulates, so off the async threads.
    let recognized = tokio::task::spawn_blocking(move || {
        crate::recognize::probe::recognize_as(&data, Some(&meant))
    })
    .await;
    match recognized {
        Ok(Ok(recognition)) => recognition.problems,
        Ok(Err(err)) => vec![Problem::said(Rule::Other, err.to_string())],
        Err(err) => vec![Problem::said(Rule::Other, err.to_string())],
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
    builds::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    builds::set_favorite(&state.db, user.id, &id, on.is_some()).await?;
    let path = href::build(&id);
    if fixi && card.is_some() {
        return Ok(cards::card_star(&id, on.is_some()).into_response());
    }
    if fixi {
        let count = builds::favorite_count(&state.db, &id).await?;
        return Ok(actions_view::favorite_button(&path, on.is_some(), count)
            .into_response());
    }
    Ok(Redirect::to(&path).into_response())
}

/// Build `id`'s simulation, for its page to ask after while storing it
/// simulates it in the background: 204 until that is done, then the page
/// again, its checks drawn from what was stored.
pub async fn poll(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    builds::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    if jobs::completing(&id) {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    let page = format!("/schematics/{id}#checks");
    Ok((StatusCode::NO_CONTENT, [("FX-Redirect", page)]).into_response())
}

/// Where a deleted build's page goes back to.
#[derive(serde::Deserialize)]
pub struct Deleting {
    /// The listing its page was come to from, if any.
    #[serde(default)]
    next: String,
}

/// Deletes a build, by its uploader, or a mod or admin: it is hidden, and
/// the moderation log shows the delete, for an admin to restore. A 404 for
/// no build, or one deleted already, and a 403 for anyone else. It goes
/// back to the listing its page was come to from, else home.
pub async fn delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(by): SignedIn,
    axum::Form(deleting): axum::Form<Deleting>,
) -> Result<Response, AppError> {
    let s = builds::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !by.may_delete(&s) {
        return Err(AppError::Forbidden);
    }
    builds::delete(&state.db, &id, by.id).await?;
    tracing::info!(id, by = by.username, "deleted a build");
    let next = back::listing(&deleting.next).unwrap_or("/");
    Ok(Redirect::to(next).into_response())
}
