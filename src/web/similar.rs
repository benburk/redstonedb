//! Finding the stored builds most like a file, without uploading it: the
//! file is read as it is, with no conventions checked and nothing kept.

use axum::{
    extract::{Multipart, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::AppError,
    extractors::fixi::Fixi,
    models::{schematic, similarity, user::User},
    schem::Blocks,
};

use super::{pages, upload};

/// How many of the most alike builds a search lists.
const LISTED: usize = 20;

/// What a search found, as its address carries it, so going back to it,
/// reloading it, or sharing it shows the same without comparing again:
/// the file's name, and each build as its id and how alike, in tenths of a
/// percent, such as `0a1b…-912`, comma-separated, the most alike first.
#[derive(Deserialize)]
pub struct Shown {
    file: Option<String>,
    found: Option<String>,
}

/// The address that shows `ranked`, found for the file called `file`.
fn shown_at(file: &str, ranked: &[(String, u32)]) -> String {
    let found: Vec<String> = ranked
        .iter()
        .map(|(id, permille)| format!("{id}-{permille}"))
        .collect();
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("file", file)
        .append_pair("found", &found.join(","))
        .finish();
    format!("/similar?{query}")
}

/// The builds an address's `found` names, with how alike; any part that
/// names no build, or is not an id and a share, is left out.
async fn shown(
    state: &AppState,
    found: &str,
) -> Result<Vec<(schematic::Schematic, u32)>, sqlx::Error> {
    let ranked: Vec<(String, u32)> = found
        .split(',')
        .take(LISTED)
        .filter_map(|part| {
            let (id, permille) = part.rsplit_once('-')?;
            let plain =
                id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit());
            let permille = permille.parse::<u32>().ok()?;
            (plain && permille <= 1000).then(|| (id.to_owned(), permille))
        })
        .collect();
    builds(&state.db, &ranked).await
}

/// The builds `ranked` names, each with how alike it is, in its order;
/// any that is gone is left out.
pub(super) async fn builds(
    db: &sqlx::SqlitePool,
    ranked: &[(String, u32)],
) -> Result<Vec<(schematic::Schematic, u32)>, sqlx::Error> {
    let ids: Vec<&str> = ranked.iter().map(|(id, _)| id.as_str()).collect();
    let builds = schematic::get_many(db, &ids).await?;
    Ok(builds
        .into_iter()
        .filter_map(|s| {
            let (_, permille) = ranked.iter().find(|(id, _)| *id == s.id)?;
            Some((s, *permille))
        })
        .collect())
}

/// The search form, and the results its address carries, if it does.
pub async fn form(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Shown { file, found }): Query<Shown>,
) -> Result<Response, AppError> {
    let builds = match &found {
        Some(found) => Some(shown(&state, found).await?),
        None => None,
    };
    let file = file.unwrap_or_else(|| "the file".to_owned());
    let found = builds.as_ref().map(|builds| pages::Found {
        file: &file,
        builds,
    });
    page(&state, user_opt.as_ref(), found.as_ref(), "").await
}

/// The builds most like the file chosen: a file, a Schemati link, or a
/// file from the viewer's own Schemati folder.
pub async fn search(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let user = user_opt.as_ref();
    let mut posted = match upload::posted(&mut multipart).await {
        Ok(posted) => posted,
        Err(message) => {
            return answer(&state, user, fixi, None, &message).await;
        }
    };
    if let Some(viewer) = user
        && let Err(message) = posted.fetch_own(&state.db, viewer).await
    {
        return answer(&state, user, fixi, None, &message).await;
    }
    let name = posted.chosen_name();
    let sent = match posted.chosen().await {
        Ok(sent) => sent,
        Err(message) => {
            return answer(&state, user, fixi, None, &message).await;
        }
    };
    let blocks = match Blocks::read(&sent.file) {
        Ok(blocks) => blocks,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return answer(&state, user, fixi, None, &message).await;
        }
    };
    let ranked = similarity::ranked(&state.db, &blocks, LISTED).await?;
    // The results go in the address, so Back and reloading keep them: a
    // plain form goes there, and fixi's page takes it as its address.
    let at = shown_at(&name, &ranked);
    if !fixi {
        return Ok(Redirect::to(&at).into_response());
    }
    let builds = builds(&state.db, &ranked).await?;
    let found = pages::Found {
        file: &name,
        builds: &builds,
    };
    Ok((
        [("FX-Replace-Url", at)],
        pages::similar_results(Some(&found), ""),
    )
        .into_response())
}

/// A search's answer: through fixi, only the results, which is all that
/// changes; for a plain form, the page.
async fn answer(
    state: &AppState,
    user: Option<&User>,
    fixi: bool,
    found: Option<&pages::Found<'_>>,
    message: &str,
) -> Result<Response, AppError> {
    if fixi {
        Ok(pages::similar_results(found, message).into_response())
    } else {
        page(state, user, found, message).await
    }
}

/// The search page, with the viewer's Schemati files to choose from when
/// they are logged in.
async fn page(
    state: &AppState,
    user: Option<&User>,
    found: Option<&pages::Found<'_>>,
    message: &str,
) -> Result<Response, AppError> {
    let folder = match user {
        Some(user) => {
            Some(super::schemati::own_folder_quickly(&state.db, user).await?)
        }
        None => None,
    };
    let connection = folder
        .as_ref()
        .map(|folder| super::schemati::connection(folder.as_ref()));
    Ok(
        pages::similar_search(user, connection.as_ref(), found, message)
            .into_response(),
    )
}
