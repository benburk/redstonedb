//! The viewer's own Schemati folder, reached with the Schemati session
//! they give: its files to download, publish, rename, and delete, files to
//! add, and builds sent from their pages.

use std::{
    future::Future,
    time::{Duration, Instant},
};

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
    let folder = own_folder(&state.db, user, PATIENT).await?;
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
    let folder = own_folder(&state.db, user, PATIENT).await?;
    let said = if refused.is_empty() { done } else { refused };
    Ok((
        [("FX-Notice", super::notice(said))],
        pages::schemati_folder(&connection(folder.as_ref()), done, refused),
    )
        .into_response())
}

/// The longest anything asked of Schemati is waited on: longer than the
/// client's own 20 s, so that is what usually says it took too long.
const PATIENT: Duration = Duration::from_secs(30);

/// The longest a form that offers the viewer's Schemati files beside
/// others waits on them.
const QUICK: Duration = Duration::from_secs(5);

/// An answer from Schemati slower than this is logged.
const SLOW: Duration = Duration::from_secs(3);

/// `user`'s Schemati folder as Schemati has it now, or why not, waiting
/// on it at most `limit`; `None` when they have given no session.
async fn own_folder(
    db: &SqlitePool,
    user: &User,
    limit: Duration,
) -> Result<Option<Result<Folder, Refusal>>, sqlx::Error> {
    Ok(match Connected::of(db, user).await? {
        Some(connected) => Some(connected.folder(limit).await),
        None => None,
    })
}

/// `own_folder`, for a form that offers its files beside others: waiting
/// on Schemati only so long, then saying it is slow.
pub(super) async fn own_folder_quickly(
    db: &SqlitePool,
    user: &User,
) -> Result<Option<Result<Folder, Refusal>>, sqlx::Error> {
    own_folder(db, user, QUICK).await
}

/// An answer from Schemati, and the player it greets, if it names one.
trait Answer {
    fn player(&self) -> Option<&str> {
        None
    }
}

impl Answer for Folder {
    fn player(&self) -> Option<&str> {
        Some(self.player.as_str()).filter(|player| !player.is_empty())
    }
}

impl Answer for () {}

impl Answer for Vec<u8> {}

/// Asks Schemati `call`, made with `user`'s `session`, waiting at most
/// `limit`: the one way the site asks with a player's session, so every
/// failure and slow answer is logged as `what` was asked, and what the
/// answer shows of the session is kept, that Schemati no longer knows it
/// or that it still works and whose folder it opens, for Settings to say
/// without asking again.
async fn ask<T: Answer>(
    db: &SqlitePool,
    user: &User,
    session: &str,
    what: &str,
    limit: Duration,
    call: impl Future<Output = Result<T, Refusal>>,
) -> Result<T, Refusal> {
    let started = Instant::now();
    let answer = tokio::time::timeout(limit, call).await.unwrap_or_else(|_| {
        Err(Refusal::Said(
            "Schemati is slow to answer; try again shortly.".to_owned(),
        ))
    });
    let took = started.elapsed();
    let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
    let by = &user.username;
    let kept = match &answer {
        Ok(answer) => {
            if took > SLOW {
                tracing::info!(by, asked = what, ms, "slow Schemati answer");
            }
            User::schemati_answered(db, user.id, session, answer.player()).await
        }
        Err(Refusal::LoggedOut) => {
            tracing::info!(by, asked = what, ms, "Schemati session expired");
            User::schemati_forgot(db, user.id, session).await
        }
        Err(Refusal::Said(why)) => {
            tracing::warn!(by, asked = what, ms, why, "Schemati failed");
            Ok(())
        }
    };
    if let Err(err) = kept {
        tracing::error!(by, "keeping how a Schemati session stands: {err}");
    }
    answer
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

/// The signed-in account and the Schemati session it gave, through which
/// its folder is asked for and changed. As an extractor, for a change to
/// its folder, anyone signed out goes to the login page, and anyone who
/// gave no session to the Schemati page, to give one.
pub struct Connected {
    db: SqlitePool,
    user: User,
    session: String,
}

impl Connected {
    /// `user` and their session, if they gave one.
    async fn of(
        db: &SqlitePool,
        user: &User,
    ) -> Result<Option<Connected>, sqlx::Error> {
        let session = User::schemati_session(db, user.id).await?;
        Ok(session.map(|session| Connected {
            db: db.clone(),
            user: user.clone(),
            session,
        }))
    }

    /// Their folder, waiting at most `limit`.
    async fn folder(&self, limit: Duration) -> Result<Folder, Refusal> {
        let call = schemati::folder(&self.session);
        ask(&self.db, &self.user, &self.session, "list", limit, call).await
    }

    /// The file `name` in their folder.
    async fn file(&self, name: &str) -> Result<Vec<u8>, Refusal> {
        let call = schemati::file(&self.session, name);
        ask(
            &self.db,
            &self.user,
            &self.session,
            "download",
            PATIENT,
            call,
        )
        .await
    }

    /// Saves `data` as `name` in their folder, and says so, and whether it
    /// took the place of a file of that name.
    async fn save(&self, name: &str, data: Vec<u8>) -> Result<String, Refusal> {
        let replaced = self.folder(PATIENT).await?.has(name);
        let call = schemati::put(&self.session, name, data);
        ask(&self.db, &self.user, &self.session, "send", PATIENT, call).await?;
        Ok(if replaced {
            format!(
                "Saved {name} in place of the file of that name; in game, //load {name}."
            )
        } else {
            format!("Saved {name}; in game, //load {name}.")
        })
    }

    /// Deletes the file `name` from their folder.
    async fn delete(&self, name: &str) -> Result<(), Refusal> {
        let call = schemati::delete(&self.session, name);
        ask(&self.db, &self.user, &self.session, "delete", PATIENT, call).await
    }

    /// Renames the file `name` in their folder to `new_name`.
    async fn rename(&self, name: &str, new_name: &str) -> Result<(), Refusal> {
        let call = schemati::rename(&self.session, name, new_name);
        ask(&self.db, &self.user, &self.session, "rename", PATIENT, call).await
    }
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
        match Connected::of(&state.db, &user).await {
            Ok(Some(connected)) => Ok(connected),
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

/// Keeps the session given, once Schemati shows it opens a folder,
/// waiting on it as long as anything asked of it.
pub async fn connect(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Form(given): Form<Given>,
) -> Result<Response, AppError> {
    let next = given.next.as_deref();
    let welcoming = next.is_some_and(|n| n.starts_with("/welcome"));
    let refused = match schemati::session(&given.session) {
        Ok(session) => {
            let call = schemati::folder(&session);
            let checked =
                ask(&state.db, &user, &session, "check", PATIENT, call).await;
            match keep_checked(&state.db, &user, &session, checked).await? {
                Ok(()) => {
                    let to = super::settings::onward(next, "/settings");
                    return Ok(Redirect::to(&to).into_response());
                }
                Err(refused) => refused,
            }
        }
        Err(message) => message,
    };
    if welcoming {
        return super::settings::welcome_page(&state, &user, 2, &refused).await;
    }
    refused_session(&state, &user, &refused).await
}

/// Keeps `session` as `user`'s, with the player whose folder it opened,
/// if `checked`, Schemati's answer with it, shows it works; else says why
/// not. A session Schemati could not be asked about is not kept, as it
/// may not work.
async fn keep_checked(
    db: &SqlitePool,
    user: &User,
    session: &str,
    checked: Result<Folder, Refusal>,
) -> Result<Result<(), String>, sqlx::Error> {
    Ok(match checked {
        Ok(folder) => {
            let player = folder.player();
            User::connect_schemati(db, user.id, session, player).await?;
            Ok(())
        }
        Err(Refusal::LoggedOut) => Err("Schemati does not know that session. \
                                        Log in there again and copy the \
                                        newest one."
            .to_owned()),
        Err(Refusal::Said(why)) => Err(format!(
            "Not saved, as Schemati could not check the session. {why}"
        )),
    })
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

/// Forgets the viewer's session, and all that was known of it.
pub async fn disconnect(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    User::disconnect_schemati(&state.db, user.id).await?;
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
    let connected = Connected::of(db, user)
        .await
        .map_err(|err| {
            tracing::error!("could not load a Schemati session: {err}");
            "Could not load your Schemati session. Try again.".to_owned()
        })?
        .ok_or_else(|| "Connect your Schemati in Settings first.".to_owned())?;
    connected.file(name).await.map_err(sentence)
}

/// A file in the viewer's folder, downloaded.
pub async fn download(
    Path(name): Path<String>,
    State(state): State<AppState>,
    connected: Connected,
) -> Result<Response, AppError> {
    if !schemati::valid_name(&name) {
        return Err(AppError::NotFound);
    }
    match connected.file(&name).await {
        // A valid name needs no quoting.
        Ok(data) => Ok(super::attachment(&name, data)),
        Err(refusal) => {
            page(&state, &connected.user, "", &sentence(refusal)).await
        }
    }
}

/// Deletes a file from the viewer's folder.
pub async fn delete(
    Path(name): Path<String>,
    State(state): State<AppState>,
    connected: Connected,
    Fixi(fixi): Fixi,
) -> Result<Response, AppError> {
    let user = &connected.user;
    match connected.delete(&name).await {
        Ok(()) => {
            answer(&state, user, fixi, &format!("Deleted {name}."), "").await
        }
        Err(refusal) => {
            answer(&state, user, fixi, "", &sentence(refusal)).await
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
    connected: Connected,
    Fixi(fixi): Fixi,
    Form(form): Form<Rename>,
) -> Result<Response, AppError> {
    let user = &connected.user;
    let extension = name.rsplit_once('.').map_or("schem", |(_, e)| e);
    let typed = form.new_name.trim();
    let new_name = if typed.ends_with(&format!(".{extension}")) {
        typed.to_owned()
    } else {
        format!("{typed}.{extension}")
    };
    if new_name == name {
        return answer(&state, user, fixi, "", "").await;
    }
    let renamed = match connected.folder(PATIENT).await {
        // Schemati would put it in place of the other.
        Ok(folder) if folder.has(&new_name) => Err(Refusal::Said(format!(
            "{new_name} is already in your folder. Delete or rename it first."
        ))),
        Ok(_) => connected.rename(&name, &new_name).await,
        Err(refusal) => Err(refusal),
    };
    match renamed {
        Ok(()) => {
            answer(
                &state,
                user,
                fixi,
                &format!("Renamed {name} to {new_name}."),
                "",
            )
            .await
        }
        Err(refusal) => {
            answer(&state, user, fixi, "", &sentence(refusal)).await
        }
    }
}

/// Adds the files chosen to the viewer's folder, each named as Schemati
/// takes it, in place of any file of that name.
pub async fn add(
    State(state): State<AppState>,
    connected: Connected,
    Fixi(fixi): Fixi,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let user = &connected.user;
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
                        return answer(&state, user, fixi, "", TOO_LARGE).await;
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => return answer(&state, user, fixi, "", TOO_LARGE).await,
        }
    }
    if files.is_empty() {
        return answer(&state, user, fixi, "", "Choose a file to add.").await;
    }
    // Schemati keeps whatever it is sent, but a schematic is gzipped NBT.
    if let Some((name, _)) = files
        .iter()
        .find(|(_, data)| !data.starts_with(&[0x1f, 0x8b]))
    {
        let refused = format!("{name} is not a schematic file.");
        return answer(&state, user, fixi, "", &refused).await;
    }
    let mut saved = Vec::new();
    for (name, data) in files {
        match connected.save(&name, data).await {
            Ok(said) => saved.push(said),
            Err(refusal) => {
                let refused =
                    format!("Could not add {name}. {}", sentence(refusal));
                return answer(&state, user, fixi, &saved.join(" "), &refused)
                    .await;
            }
        }
    }
    answer(&state, user, fixi, &saved.join(" "), "").await
}

const TOO_LARGE: &str =
    "Those files did not arrive whole. Up to 2 MB is accepted at once.";

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
    let sent = match Connected::of(&state.db, &user).await? {
        Some(connected) => {
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
            connected.save(&name, data).await.map_err(sentence)
        }
        None => Err("Connect your Schemati first.".to_owned()),
    };
    Ok(if fixi {
        pages::schemati_sent(&sent).into_response()
    } else {
        Redirect::to("/schemati").into_response()
    })
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::models::user::{DiscordProfile, SchematiLink};

    async fn signed_in() -> (SqlitePool, User) {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&db).await.unwrap();
        let profile = DiscordProfile {
            id: "1".to_owned(),
            username: "steve".to_owned(),
            display_name: None,
            avatar: None,
        };
        let user = User::sign_in(&db, &profile).await.unwrap();
        (db, user)
    }

    async fn link(db: &SqlitePool, user: &User) -> Option<SchematiLink> {
        User::schemati_link(db, user.id).await.unwrap()
    }

    fn folder(player: &str) -> Folder {
        Folder {
            player: player.to_owned(),
            files: Vec::new(),
        }
    }

    /// Whatever is asked with the session, Schemati saying it no longer
    /// knows it is kept, and answering again, that it works.
    #[tokio::test]
    async fn answers_with_the_session_keep_how_it_stands() {
        let (db, user) = signed_in().await;
        User::connect_schemati(&db, user.id, "aaaa", None)
            .await
            .unwrap();
        let sent = ask(&db, &user, "aaaa", "send", PATIENT, async {
            Err::<(), _>(Refusal::LoggedOut)
        })
        .await;
        assert_eq!(sent, Err(Refusal::LoggedOut));
        assert!(link(&db, &user).await.unwrap().expired);

        let listed = ask(&db, &user, "aaaa", "list", PATIENT, async {
            Ok(folder("embizone"))
        })
        .await;
        assert!(listed.is_ok());
        let kept = link(&db, &user).await.unwrap();
        assert!(!kept.expired);
        assert!(kept.checked.is_some());
        assert_eq!(kept.player.as_deref(), Some("embizone"));

        // Failing otherwise says nothing of the session.
        let failed = ask(&db, &user, "aaaa", "download", PATIENT, async {
            Err::<Vec<u8>, _>(Refusal::Said("Could not reach".to_owned()))
        })
        .await;
        assert!(failed.is_err());
        assert_eq!(link(&db, &user).await, Some(kept));
    }

    /// Waiting too long is a refusal, not a session gone.
    #[tokio::test]
    async fn a_slow_answer_is_given_up_on() {
        let (db, user) = signed_in().await;
        User::connect_schemati(&db, user.id, "aaaa", None)
            .await
            .unwrap();
        let limit = Duration::from_millis(10);
        let asked = ask(&db, &user, "aaaa", "list", limit, async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(folder("embizone"))
        })
        .await;
        assert!(matches!(asked, Err(Refusal::Said(_))), "{asked:?}");
        assert!(!link(&db, &user).await.unwrap().expired);
    }

    /// A session given is kept with its player once Schemati shows it
    /// works, and not at all when it could not be checked.
    #[tokio::test]
    async fn a_session_is_saved_once_checked() {
        let (db, user) = signed_in().await;
        let unreachable = Err(Refusal::Said("Could not reach".to_owned()));
        let refused = keep_checked(&db, &user, "aaaa", unreachable)
            .await
            .unwrap()
            .unwrap_err();
        assert!(refused.starts_with("Not saved"), "{refused}");
        assert_eq!(link(&db, &user).await, None);

        let unknown = Err(Refusal::LoggedOut);
        let refused = keep_checked(&db, &user, "aaaa", unknown)
            .await
            .unwrap()
            .unwrap_err();
        assert!(refused.contains("does not know"), "{refused}");
        assert_eq!(link(&db, &user).await, None);

        keep_checked(&db, &user, "aaaa", Ok(folder("embizone")))
            .await
            .unwrap()
            .unwrap();
        let kept = link(&db, &user).await.unwrap();
        assert_eq!(kept.player.as_deref(), Some("embizone"));
        assert!(kept.checked.is_some());
        let session = User::schemati_session(&db, user.id).await.unwrap();
        assert_eq!(session.as_deref(), Some("aaaa"));
    }
}
