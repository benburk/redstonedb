//! Finding the stored builds most like a file, without uploading it: the
//! file is read as it is, with no conventions checked and nothing kept.

use axum::{
    extract::{Multipart, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::internal_error,
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
    let mut builds = Vec::new();
    for part in found.split(',').take(LISTED) {
        let Some((id, permille)) = part.rsplit_once('-') else {
            continue;
        };
        let plain = id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit());
        let Ok(permille) = permille.parse::<u32>() else {
            continue;
        };
        if !plain || permille > 1000 {
            continue;
        }
        if let Some(s) = schematic::get(&state.db, id).await? {
            builds.push((s, permille));
        }
    }
    Ok(builds)
}

/// The search form, and the results its address carries, if it does.
pub async fn form(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Shown { file, found }): Query<Shown>,
) -> Response {
    let builds = match &found {
        Some(found) => match shown(&state, found).await {
            Ok(builds) => Some(builds),
            Err(err) => return internal_error(err).into_response(),
        },
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
) -> Response {
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
    let ranked = match similarity::ranked(&state.db, &blocks, LISTED).await {
        Ok(ranked) => ranked,
        Err(err) => return internal_error(&*err).into_response(),
    };
    // The results go in the address, so Back and reloading keep them: a
    // plain form goes there, and fixi's page takes it as its address.
    let at = shown_at(&name, &ranked);
    if !fixi {
        return Redirect::to(&at).into_response();
    }
    let mut builds = Vec::new();
    for (id, permille) in ranked {
        match schematic::get(&state.db, &id).await {
            Ok(Some(s)) => builds.push((s, permille)),
            Ok(None) => {}
            Err(err) => return internal_error(err).into_response(),
        }
    }
    let found = pages::Found {
        file: &name,
        builds: &builds,
    };
    (
        [("FX-Replace-Url", at)],
        pages::similar_results(Some(&found), ""),
    )
        .into_response()
}

/// A search's answer: through fixi, only the results, which is all that
/// changes; for a plain form, the page.
async fn answer(
    state: &AppState,
    user: Option<&User>,
    fixi: bool,
    found: Option<&pages::Found<'_>>,
    message: &str,
) -> Response {
    if fixi {
        pages::similar_results(found, message).into_response()
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
) -> Response {
    let folder = match user {
        Some(user) => {
            match super::schemati::own_folder_quickly(&state.db, user).await {
                Ok(folder) => Some(folder),
                Err(err) => return internal_error(err).into_response(),
            }
        }
        None => None,
    };
    let connection = folder
        .as_ref()
        .map(|folder| super::schemati::connection(folder.as_ref()));
    pages::similar_search(user, connection.as_ref(), found, message)
        .into_response()
}
