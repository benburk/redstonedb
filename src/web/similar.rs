//! Finding the stored builds most like a file, without uploading it: the
//! file is read as it is, with no conventions checked and nothing kept.

use axum::{
    extract::{Multipart, State},
    response::{IntoResponse, Response},
};

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

/// The search form.
pub async fn form(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    page(&state, user_opt.as_ref(), None, "").await
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
    answer(&state, user, fixi, Some(&found), "").await
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
