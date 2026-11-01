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

/// `ranked` as an address carries it, such as `0a1b…-912,…`.
fn found_of(ranked: &[(String, u32)]) -> String {
    ranked
        .iter()
        .map(|(id, permille)| format!("{id}-{permille}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The home page's address showing `found`, found for the file called
/// `file`.
fn shown_at(file: &str, found: &str) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("file", file)
        .append_pair("found", found)
        .finish();
    format!("/?{query}")
}

/// The builds an address's `found` names, with how alike; any part that
/// names no build `viewer` may see, or is not an id and a share, is left
/// out.
async fn shown(
    state: &AppState,
    viewer: Option<&User>,
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
    builds(&state.db, &ranked, viewer).await
}

/// The builds `ranked` names, each with how alike it is, in its order;
/// any that is gone, or `viewer` may not see, is left out.
pub(super) async fn builds(
    db: &sqlx::SqlitePool,
    ranked: &[(String, u32)],
    viewer: Option<&User>,
) -> Result<Vec<(schematic::Schematic, u32)>, sqlx::Error> {
    let ids: Vec<&str> = ranked.iter().map(|(id, _)| id.as_str()).collect();
    let builds = schematic::get_many(db, &ids).await?;
    Ok(builds
        .into_iter()
        .filter(|s| s.visible_to(viewer))
        .filter_map(|s| {
            let (_, permille) = ranked.iter().find(|(id, _)| *id == s.id)?;
            Some((s, *permille))
        })
        .collect())
}

/// Through fixi, the dialog to choose a file in, with the viewer's own
/// Schemati files; otherwise the home page, which the dialog opens from,
/// showing what an older address's `found` names, if it does.
pub async fn form(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Query(Shown { file, found }): Query<Shown>,
) -> Result<Response, AppError> {
    if !fixi {
        let home = match &found {
            Some(found) => shown_at(file.as_deref().unwrap_or_default(), found),
            None => "/".to_owned(),
        };
        return Ok(Redirect::to(&home).into_response());
    }
    let folder = match &user_opt {
        Some(user) => {
            Some(super::schemati::own_folder_quickly(&state.db, user).await?)
        }
        None => None,
    };
    let connection = folder
        .as_ref()
        .map(|folder| super::schemati::connection(folder.as_ref()));
    Ok(pages::similar_dialog(connection.as_ref()).into_response())
}

/// The home page showing the builds `found` names, as the address of a
/// search's results carries them, found for the file called `file`.
pub(super) async fn shown_page(
    state: &AppState,
    viewer: Option<&User>,
    file: &str,
    found: &str,
) -> Result<Response, AppError> {
    let builds = shown(state, viewer, found).await?;
    let file = if file.is_empty() { "the file" } else { file };
    let found = pages::Found {
        file,
        builds: &builds,
    };
    Ok(pages::similar_page(viewer, Some(&found), "").into_response())
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
        Err(message) => return Ok(refused(user, fixi, &message)),
    };
    if let Some(viewer) = user
        && let Err(message) = posted.fetch_own(&state.db, viewer).await
    {
        return Ok(refused(user, fixi, &message));
    }
    let name = posted.chosen_name();
    let sent = match posted.chosen().await {
        Ok(sent) => sent,
        Err(message) => return Ok(refused(user, fixi, &message)),
    };
    let blocks = match Blocks::read(&sent.file) {
        Ok(blocks) => blocks,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return Ok(refused(user, fixi, &message));
        }
    };
    let ranked = similarity::ranked(&state.db, &blocks, LISTED).await?;
    // The results go in the address, so Back and reloading keep them: a
    // plain form goes there, and fixi's page takes it as its address.
    let at = shown_at(&name, &found_of(&ranked));
    if !fixi {
        return Ok(Redirect::to(&at).into_response());
    }
    let builds = builds(&state.db, &ranked, user).await?;
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

/// Why a search could not run: through fixi, in place of what is below the
/// home page's search; for a plain form, on the home page.
fn refused(user: Option<&User>, fixi: bool, message: &str) -> Response {
    if fixi {
        pages::similar_results(None, message).into_response()
    } else {
        pages::similar_page(user, None, message).into_response()
    }
}
