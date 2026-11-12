//! The viewer's own Schemati folder, reached with the Schemati session
//! they give: its files to choose one from to publish, and builds sent to
//! it from their pages. These are the routes: Schemati is reached through
//! `external::schemati`.

use std::{
    future::Future,
    time::{Duration, Instant},
};

use axum::{
    Form,
    extract::{Path, Query, RawForm, State},
    response::{IntoResponse, Redirect, Response},
};
use maud::Markup;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    account::User,
    external::schemati::{self, Folder, Refusal},
    store::builds,
    web::{
        app_state::AppState,
        error::AppError,
        extractors::{fixi::Fixi, session::SignedIn},
    },
};

use super::{
    pages::{self, Connection, Offered},
    schematics,
    settings::PaletteFields,
};

/// The longest anything asked of Schemati is waited on: longer than the
/// client's own 20 s, so that is what usually says it took too long.
const PATIENT: Duration = Duration::from_secs(30);

/// An answer from Schemati slower than this is logged.
const SLOW: Duration = Duration::from_secs(3);

/// `user`'s Schemati folder as Schemati has it now, or why not; `None`
/// when they have given no session. `list` asks Schemati for the folder a
/// session opens, which a test answers in its place.
async fn own_folder_by(
    db: &SqlitePool,
    user: &User,
    list: impl AsyncFnOnce(&str) -> Result<Folder, Refusal>,
) -> Result<Option<Result<Folder, Refusal>>, sqlx::Error> {
    Ok(match Connected::of(db, user).await? {
        Some(connected) => {
            let call = list(&connected.session);
            Some(connected.ask("list", call).await)
        }
        None => None,
    })
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
            "Schemati took too long to answer; try again shortly.".to_owned(),
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

#[derive(Deserialize)]
pub struct Picking {
    /// Where the files are offered.
    #[serde(rename = "for")]
    offered: Offered,
}

/// The viewer's Schemati files, for the slot a page that offers them left
/// for them, asked for once the page is in, so the page need not wait on
/// Schemati: the files to choose from, or why there are none, with a
/// button to ask again when Schemati could not be asked. Without fixi
/// there is no slot, so it goes to the upload page, which has one.
pub async fn picker(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Fixi(fixi): Fixi,
    Query(Picking { offered }): Query<Picking>,
) -> Result<Response, AppError> {
    if !fixi {
        return Ok(Redirect::to("/upload").into_response());
    }
    let files = own_files(&state.db, &user, offered, schemati::folder).await?;
    Ok(files.into_response())
}

/// What `picker` answers for `user`, `list` asking Schemati for the folder
/// their session opens.
async fn own_files(
    db: &SqlitePool,
    user: &User,
    offered: Offered,
    list: impl AsyncFnOnce(&str) -> Result<Folder, Refusal>,
) -> Result<Markup, sqlx::Error> {
    let folder = own_folder_by(db, user, list).await?;
    Ok(pages::own_file_field(&connection(folder.as_ref()), offered))
}

/// An account and the Schemati session it gave, through which its folder
/// is asked for and added to.
struct Connected {
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

    /// Asks Schemati `call`, made with their session; see `ask`.
    async fn ask<T: Answer>(
        &self,
        what: &str,
        call: impl Future<Output = Result<T, Refusal>>,
    ) -> Result<T, Refusal> {
        ask(&self.db, &self.user, &self.session, what, PATIENT, call).await
    }

    /// Their folder.
    async fn folder(&self) -> Result<Folder, Refusal> {
        self.ask("list", schemati::folder(&self.session)).await
    }

    /// The file `name` in their folder.
    async fn file(&self, name: &str) -> Result<Vec<u8>, Refusal> {
        self.ask("download", schemati::file(&self.session, name))
            .await
    }

    /// Saves `data` as `name` in their folder, and says so, and whether it
    /// took the place of a file of that name.
    async fn save(&self, name: &str, data: Vec<u8>) -> Result<String, Refusal> {
        let replaced = self.folder().await?.has(name);
        let call = schemati::put(&self.session, name, data);
        self.ask("send", call).await?;
        Ok(if replaced {
            format!(
                "Saved {name} in place of the file of that name; in game, //load {name}."
            )
        } else {
            format!("Saved {name}; in game, //load {name}.")
        })
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

/// Keeps the session given, once Schemati shows it opens a folder,
/// waiting on it as long as anything asked of it.
pub async fn connect(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Form(given): Form<Given>,
) -> Result<Response, AppError> {
    let refused = match schemati::session(&given.session) {
        Ok(session) => {
            let call = schemati::folder(&session);
            let checked =
                ask(&state.db, &user, &session, "check", PATIENT, call).await;
            match keep_checked(&state.db, &user, &session, checked).await? {
                Ok(()) => {
                    return Ok(Redirect::to("/settings").into_response());
                }
                Err(refused) => refused,
            }
        }
        Err(message) => message,
    };
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

/// Sends a build's file, as the download dialog's choices make it, to the
/// viewer's folder, under the name it downloads with. Its button, through
/// fixi, gets only the sentence saying how it went, to show in the dialog;
/// the form sent without it goes back to the build's page. The form is
/// read twice, for the download's choices and for its custom colours'
/// fields.
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
                crate::store::users::palette(&state.db, user.id).await?;
            let palette = asked.palette(fields, theme)?;
            let (filename, data) = schematics::export(
                &state.db,
                &id,
                &asked,
                true,
                palette.as_ref(),
            )
            .await?;
            let name = schemati::file_name(&filename);
            let saved = connected.save(&name, data).await.map_err(sentence);
            if saved.is_ok() {
                builds::log_download(
                    &state.db,
                    &id,
                    user.id,
                    None,
                    builds::DownloadKind::Schemati,
                )
                .await?;
            }
            saved
        }
        None => Err("Connect your Schemati first.".to_owned()),
    };
    Ok(if fixi {
        pages::schemati_sent(&sent).into_response()
    } else {
        Redirect::to(&format!("/schematics/{id}")).into_response()
    })
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::{account::SchematiLink, external::discord::DiscordProfile};

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

    /// The slot a page leaves for the viewer's Schemati files gets them to
    /// choose from, each named as the forms that take one read it, and
    /// belonging to the details form when they stand outside it; or a
    /// note saying how to connect, or that the session expired; or, when
    /// Schemati could not be asked, why, with a button to ask again.
    #[tokio::test]
    async fn the_files_load_into_their_slot() {
        let (db, user) = signed_in().await;
        let files = |answer: Result<Folder, Refusal>, offered| {
            let (db, user) = (&db, &user);
            async move {
                own_files(db, user, offered, async |_: &str| answer)
                    .await
                    .unwrap()
                    .into_string()
            }
        };
        let unasked = files(Err(Refusal::LoggedOut), Offered::Upload).await;
        assert!(unasked.contains("Connect your Schemati"), "{unasked}");

        User::connect_schemati(&db, user.id, "aaaa", None)
            .await
            .unwrap();
        let listed = || Folder {
            player: "embizone".to_owned(),
            files: vec![crate::external::schemati::File {
                name: "adder.schem".to_owned(),
                modified: None,
            }],
        };
        let picked = files(Ok(listed()), Offered::Details).await;
        assert!(
            picked.contains(
                r#"name="schemati_file" value="adder.schem" form="details-form" checked"#
            ),
            "{picked}"
        );
        let picked = files(Ok(listed()), Offered::Upload).await;
        assert!(!picked.contains("form="), "{picked}");

        let why = "Schemati took too long to answer; try again shortly.";
        let failed =
            files(Err(Refusal::Said(why.to_owned())), Offered::Similar).await;
        assert!(failed.contains(why), "{failed}");
        assert!(failed.contains(r#"id="schemati-files""#), "{failed}");
        assert!(
            failed.contains(r#"fx-action="/schemati/picker?for=similar""#),
            "{failed}"
        );
        assert!(failed.contains(">Retry</button>"), "{failed}");

        let gone = files(Err(Refusal::LoggedOut), Offered::Upload).await;
        assert!(gone.contains("session expired"), "{gone}");
        assert!(link(&db, &user).await.unwrap().expired);
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
