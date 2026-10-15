//! The viewer's own Schemati folder, reached with the Schemati session
//! they give: its files to download, publish, rename, and delete, files to
//! add, and builds sent from their pages.

use std::{collections::HashMap, time::Duration};

use axum::{
    Form,
    extract::{Multipart, Path, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    app_state::AppState,
    error::internal_error,
    extractors::fixi::Fixi,
    models::user::User,
    schemati::{self, Folder, Refusal},
};

use super::{
    pages::{self, Connection},
    schematics,
};

/// The Schemati page.
pub async fn show(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    page(&state, &user, "", "").await
}

/// The Schemati page for `user`, its folder as Schemati has it now. `done`
/// says what was just done; `refused`, why it could not be.
async fn page(
    state: &AppState,
    user: &User,
    done: &str,
    refused: &str,
) -> Response {
    let folder = match own_folder(&state.db, user).await {
        Ok(folder) => folder,
        Err(err) => return internal_error(err).into_response(),
    };
    pages::schemati(user, &connection(folder.as_ref()), done, refused)
        .into_response()
}

/// The answer to a change made on the Schemati page: for fixi, the folder
/// as it is now, with `done` or `refused` in it and announced; for a plain
/// form, the page again.
async fn answer(
    state: &AppState,
    user: &User,
    fixi: bool,
    done: &str,
    refused: &str,
) -> Response {
    if !fixi {
        return Redirect::to("/schemati").into_response();
    }
    let folder = match own_folder(&state.db, user).await {
        Ok(folder) => folder,
        Err(err) => return internal_error(err).into_response(),
    };
    let said = if refused.is_empty() { done } else { refused };
    (
        [("FX-Notice", super::notice(said))],
        pages::schemati_folder(&connection(folder.as_ref()), done, refused),
    )
        .into_response()
}

/// `user`'s Schemati folder as Schemati has it now, or why not; `None`
/// when they have given no session.
async fn own_folder(
    db: &SqlitePool,
    user: &User,
) -> Result<Option<Result<Folder, Refusal>>, sqlx::Error> {
    let session = User::schemati_session(db, user.id).await?;
    Ok(match session {
        Some(session) => Some(schemati::folder(&session).await),
        None => None,
    })
}

/// `own_folder`, for a form that offers its files beside others: waiting
/// on Schemati only so long, then saying it is slow.
pub(super) async fn own_folder_quickly(
    db: &SqlitePool,
    user: &User,
) -> Result<Option<Result<Folder, Refusal>>, sqlx::Error> {
    let asked =
        tokio::time::timeout(Duration::from_secs(5), own_folder(db, user));
    match asked.await {
        Ok(folder) => folder,
        Err(_) => Ok(Some(Err(Refusal::Said(
            "Schemati is slow to answer; try again shortly.".to_owned(),
        )))),
    }
}

/// How a folder `own_folder` gave stands, for a page.
pub(super) fn connection(
    folder: Option<&Result<Folder, Refusal>>,
) -> Connection<'_> {
    match folder {
        None => Connection::Unset,
        Some(Ok(folder)) => Connection::Open(folder),
        Some(Err(Refusal::LoggedOut)) => Connection::Forgotten,
        Some(Err(Refusal::Said(why))) => Connection::Unreachable(why),
    }
}

/// The viewer and the Schemati session they gave, or the page to send
/// them to: the login page, or the Schemati page to give one.
async fn connected(
    state: &AppState,
    user_opt: Option<User>,
) -> Result<(User, String), Response> {
    let Some(user) = user_opt else {
        return Err(Redirect::to("/sessions/new").into_response());
    };
    match User::schemati_session(&state.db, user.id).await {
        Ok(Some(session)) => Ok((user, session)),
        Ok(None) => Err(Redirect::to("/schemati").into_response()),
        Err(err) => Err(internal_error(err).into_response()),
    }
}

/// What to tell the person about a refusal.
fn sentence(refusal: Refusal) -> String {
    match refusal {
        Refusal::LoggedOut => "Schemati has forgotten your session, as it \
                               does after a day. Give it a new one."
            .to_owned(),
        Refusal::Said(sentence) => sentence,
    }
}

#[derive(Deserialize)]
pub struct Given {
    session: String,
}

/// Keeps the session given, once Schemati shows it opens a folder.
pub async fn connect(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Form(given): Form<Given>,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    let session = match schemati::session(&given.session) {
        Ok(session) => session,
        Err(message) => return page(&state, &user, "", &message).await,
    };
    let refused = match schemati::folder(&session).await {
        Ok(_) => {
            if let Err(err) =
                User::set_schemati_session(&state.db, user.id, Some(&session))
                    .await
            {
                return internal_error(err).into_response();
            }
            return Redirect::to("/schemati").into_response();
        }
        Err(Refusal::LoggedOut) => "Schemati does not know that session. Log \
                                    in there again and copy the newest one."
            .to_owned(),
        Err(Refusal::Said(why)) => why,
    };
    page(&state, &user, "", &refused).await
}

/// Forgets the viewer's session.
pub async fn disconnect(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    match User::set_schemati_session(&state.db, user.id, None).await {
        Ok(()) => Redirect::to("/schemati").into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

/// The file `name` in `user`'s folder, or a sentence saying why it could
/// not be had, for publishing it.
pub(super) async fn own_file(
    db: &SqlitePool,
    user: &User,
    name: &str,
) -> Result<Vec<u8>, String> {
    let session = User::schemati_session(db, user.id)
        .await
        .map_err(|err| internal_error(err).1.to_owned())?
        .ok_or_else(|| "Connect your Schemati on its page first.".to_owned())?;
    schemati::file(&session, name).await.map_err(sentence)
}

/// A file in the viewer's folder, downloaded.
pub async fn download(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let (user, session) = match connected(&state, user_opt).await {
        Ok(connected) => connected,
        Err(response) => return response,
    };
    if !schemati::valid_name(&name) {
        return super::not_found(Some(&user));
    }
    match schemati::file(&session, &name).await {
        // A valid name needs no quoting.
        Ok(data) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{name}\""),
                ),
            ],
            data,
        )
            .into_response(),
        Err(refusal) => page(&state, &user, "", &sentence(refusal)).await,
    }
}

/// Deletes a file from the viewer's folder.
pub async fn delete(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
) -> Response {
    let (user, session) = match connected(&state, user_opt).await {
        Ok(connected) => connected,
        Err(response) => return response,
    };
    match schemati::delete(&session, &name).await {
        Ok(()) => {
            answer(&state, &user, fixi, &format!("Deleted {name}."), "").await
        }
        Err(refusal) => {
            answer(&state, &user, fixi, "", &sentence(refusal)).await
        }
    }
}

#[derive(Deserialize)]
pub struct Rename {
    new_name: String,
}

/// Renames a file in the viewer's folder, keeping its extension, unless
/// another file has the name.
pub async fn rename(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Form(form): Form<Rename>,
) -> Response {
    let (user, session) = match connected(&state, user_opt).await {
        Ok(connected) => connected,
        Err(response) => return response,
    };
    let extension = name.rsplit_once('.').map_or("schem", |(_, e)| e);
    let typed = form.new_name.trim();
    let new_name = if typed.ends_with(&format!(".{extension}")) {
        typed.to_owned()
    } else {
        format!("{typed}.{extension}")
    };
    if new_name == name {
        return answer(&state, &user, fixi, "", "").await;
    }
    let renamed = match schemati::folder(&session).await {
        // Schemati would put it in place of the other.
        Ok(folder) if folder.files.contains(&new_name) => {
            Err(Refusal::Said(format!(
                "{new_name} is already in your folder. Delete or rename it first."
            )))
        }
        Ok(_) => schemati::rename(&session, &name, &new_name).await,
        Err(refusal) => Err(refusal),
    };
    match renamed {
        Ok(()) => {
            answer(
                &state,
                &user,
                fixi,
                &format!("Renamed {name} to {new_name}."),
                "",
            )
            .await
        }
        Err(refusal) => {
            answer(&state, &user, fixi, "", &sentence(refusal)).await
        }
    }
}

/// Adds the files chosen to the viewer's folder, each named as Schemati
/// takes it, in place of any file of that name.
pub async fn add(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    mut multipart: Multipart,
) -> Response {
    let (user, session) = match connected(&state, user_opt).await {
        Ok(connected) => connected,
        Err(response) => return response,
    };
    let mut files = Vec::new();
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => {
                let name =
                    schemati::file_name(field.file_name().unwrap_or_default());
                match field.bytes().await {
                    Ok(data) if !data.is_empty() => {
                        files.push((name, data.to_vec()));
                    }
                    Ok(_) => {}
                    Err(_) => {
                        return answer(&state, &user, fixi, "", TOO_LARGE)
                            .await;
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => return answer(&state, &user, fixi, "", TOO_LARGE).await,
        }
    }
    if files.is_empty() {
        return answer(&state, &user, fixi, "", "Choose a file to add.").await;
    }
    // Schemati keeps whatever it is sent, but a schematic is gzipped NBT.
    if let Some((name, _)) = files
        .iter()
        .find(|(_, data)| !data.starts_with(&[0x1f, 0x8b]))
    {
        let refused = format!("{name} is not a schematic file.");
        return answer(&state, &user, fixi, "", &refused).await;
    }
    let mut saved = Vec::new();
    for (name, data) in files {
        match put(&session, &name, data).await {
            Ok(said) => saved.push(said),
            Err(refusal) => {
                let refused =
                    format!("Could not add {name}. {}", sentence(refusal));
                return answer(&state, &user, fixi, &saved.join(" "), &refused)
                    .await;
            }
        }
    }
    answer(&state, &user, fixi, &saved.join(" "), "").await
}

const TOO_LARGE: &str =
    "Those files did not arrive whole. Up to 2 MB is accepted at once.";

/// Saves `data` as `name` in `session`'s folder, and says so, and whether
/// it took the place of a file of that name.
async fn put(
    session: &str,
    name: &str,
    data: Vec<u8>,
) -> Result<String, Refusal> {
    let replaced = schemati::folder(session)
        .await?
        .files
        .iter()
        .any(|f| f == name);
    schemati::put(session, name, data).await?;
    Ok(if replaced {
        format!(
            "Saved {name} in place of the file of that name; in game, //load {name}."
        )
    } else {
        format!("Saved {name}; in game, //load {name}.")
    })
}

/// Sends a build's file, as the download dialog's choices make it, to the
/// viewer's folder, under the name it downloads with. Its button, through
/// fixi, gets only the sentence saying how it went, to show in the dialog;
/// the form sent without it goes on to the Schemati page.
pub async fn send(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Form(choices): Form<HashMap<String, String>>,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    let session = match User::schemati_session(&state.db, user.id).await {
        Ok(session) => session,
        Err(err) => return internal_error(err).into_response(),
    };
    let sent = match session {
        Some(session) => {
            let (filename, data) =
                match schematics::export(&state.db, &id, choices, true).await {
                    Ok(exported) => exported,
                    Err(response) => return response,
                };
            let name = schemati::file_name(&filename);
            put(&session, &name, data).await.map_err(sentence)
        }
        None => Err("Connect your Schemati first.".to_owned()),
    };
    if fixi {
        pages::schemati_sent(&sent).into_response()
    } else {
        Redirect::to("/schemati").into_response()
    }
}
