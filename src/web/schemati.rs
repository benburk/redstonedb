//! The viewer's own Schemati folder, reached with the Schemati session
//! they give: its files to download, publish, rename, and delete, files to
//! add, and builds sent from their pages.

use std::time::Duration;

use axum::{
    Form,
    extract::{FromRequestParts, Multipart, Path, RawForm, State},
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    external::schemati::{self, Folder, Refusal},
    models::user::User,
    web::{
        app_state::AppState,
        error::AppError,
        extractors::{fixi::Fixi, session::SignedIn},
    },
};

use super::{
    pages::{self, Connection},
    schematics,
    settings::PaletteFields,
};

/// The Schemati page.
pub async fn show(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    page(&state, &user, "", "").await
}

/// The Schemati page for `user`, its folder as Schemati has it now. `done`
/// says what was just done; `refused`, why it could not be.
async fn page(
    state: &AppState,
    user: &User,
    done: &str,
    refused: &str,
) -> Result<Response, AppError> {
    let folder = own_folder(&state.db, user).await?;
    Ok(
        pages::schemati(user, &connection(folder.as_ref()), done, refused)
            .into_response(),
    )
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
) -> Result<Response, AppError> {
    if !fixi {
        return Ok(Redirect::to("/schemati").into_response());
    }
    let folder = own_folder(&state.db, user).await?;
    let said = if refused.is_empty() { done } else { refused };
    Ok((
        [("FX-Notice", super::notice(said))],
        pages::schemati_folder(&connection(folder.as_ref()), done, refused),
    )
        .into_response())
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

/// The signed-in account and the Schemati session it gave, for a change
/// to its folder. Anyone signed out goes to the login page, and anyone who
/// gave no session to the Schemati page, to give one.
pub struct Connected {
    user: User,
    session: String,
}

impl FromRequestParts<AppState> for Connected {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let SignedIn(user) = SignedIn::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        match User::schemati_session(&state.db, user.id).await {
            Ok(Some(session)) => Ok(Connected { user, session }),
            Ok(None) => Err(Redirect::to("/schemati").into_response()),
            Err(err) => Err(AppError::from(err).into_response()),
        }
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
    next: Option<String>,
}

/// Keeps the session given, once Schemati shows it opens a folder.
pub async fn connect(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Form(given): Form<Given>,
) -> Result<Response, AppError> {
    let next = given.next.as_deref();
    let welcoming = next.is_some_and(|n| n.starts_with("/welcome"));
    let refused = match schemati::session(&given.session) {
        Ok(session) => match schemati::folder(&session).await {
            Ok(_) => {
                User::set_schemati_session(&state.db, user.id, Some(&session))
                    .await?;
                let to = super::settings::onward(next, "/settings");
                return Ok(Redirect::to(&to).into_response());
            }
            Err(Refusal::LoggedOut) => "Schemati does not know that session. \
                                        Log in there again and copy the \
                                        newest one."
                .to_owned(),
            Err(Refusal::Said(why)) => why,
        },
        Err(message) => message,
    };
    if welcoming {
        return super::settings::welcome_page(&state, &user, 2, &refused).await;
    }
    refused_session(&state, &user, &refused).await
}

/// The settings page, where a session is given, saying why `user`'s was
/// refused.
async fn refused_session(
    state: &AppState,
    user: &User,
    refused: &str,
) -> Result<Response, AppError> {
    let said = super::settings::Said {
        schemati: refused,
        ..Default::default()
    };
    super::settings::page(state, user, &said).await
}

/// Forgets the viewer's session.
pub async fn disconnect(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    User::set_schemati_session(&state.db, user.id, None).await?;
    Ok(Redirect::to("/settings").into_response())
}

/// The file `name` in `user`'s folder, or a sentence saying why it could
/// not be had, for publishing it.
pub(super) async fn own_file(
    db: &SqlitePool,
    user: &User,
    name: &str,
) -> Result<Vec<u8>, String> {
    // A choice among others in a form, so a failure is said beside them.
    let session = User::schemati_session(db, user.id)
        .await
        .map_err(|err| {
            tracing::error!("could not load a Schemati session: {err}");
            "Could not load your Schemati session. Try again.".to_owned()
        })?
        .ok_or_else(|| "Connect your Schemati in Settings first.".to_owned())?;
    schemati::file(&session, name).await.map_err(sentence)
}

/// A file in the viewer's folder, downloaded.
pub async fn download(
    Path(name): Path<String>,
    State(state): State<AppState>,
    Connected { user, session }: Connected,
) -> Result<Response, AppError> {
    if !schemati::valid_name(&name) {
        return Err(AppError::NotFound);
    }
    match schemati::file(&session, &name).await {
        // A valid name needs no quoting.
        Ok(data) => Ok(super::attachment(&name, data)),
        Err(refusal) => page(&state, &user, "", &sentence(refusal)).await,
    }
}

/// Deletes a file from the viewer's folder.
pub async fn delete(
    Path(name): Path<String>,
    State(state): State<AppState>,
    Connected { user, session }: Connected,
    Fixi(fixi): Fixi,
) -> Result<Response, AppError> {
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
    Connected { user, session }: Connected,
    Fixi(fixi): Fixi,
    Form(form): Form<Rename>,
) -> Result<Response, AppError> {
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
        Ok(folder) if folder.has(&new_name) => Err(Refusal::Said(format!(
            "{new_name} is already in your folder. Delete or rename it first."
        ))),
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
    Connected { user, session }: Connected,
    Fixi(fixi): Fixi,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
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
    let replaced = schemati::folder(session).await?.has(name);
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
/// the form sent without it goes on to the Schemati page. The form is read
/// twice, for the download's choices and for its custom colours' fields.
pub async fn send(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Fixi(fixi): Fixi,
    RawForm(form): RawForm,
) -> Result<Response, AppError> {
    let unread = |err: serde_urlencoded::de::Error| {
        AppError::BadRequest(format!("The form could not be read: {err}."))
    };
    let asked: schematics::Download =
        serde_urlencoded::from_bytes(&form).map_err(unread)?;
    let fields: PaletteFields =
        serde_urlencoded::from_bytes(&form).map_err(unread)?;
    let session = User::schemati_session(&state.db, user.id).await?;
    let sent = match session {
        Some(session) => {
            let theme =
                crate::recognize::palette::of(&state.db, user.id).await?;
            let palette = asked.palette(fields, theme)?;
            let (filename, data) = schematics::export(
                &state.db,
                &id,
                Some(&user),
                &asked,
                true,
                palette.as_ref(),
            )
            .await?;
            let name = schemati::file_name(&filename);
            put(&session, &name, data).await.map_err(sentence)
        }
        None => Err("Connect your Schemati first.".to_owned()),
    };
    Ok(if fixi {
        pages::schemati_sent(&sent).into_response()
    } else {
        Redirect::to("/schemati").into_response()
    })
}
