//! Uploading a build: choose a file, preview what the site reads from it,
//! fill in its details, and publish. The preview carries the file as its
//! builder made it in a hidden field, so nothing is stored until
//! publishing, which checks the file again.
//!
//! A build the site recognizes, passing its category's test, is kept
//! standardized, as every such build lies and is coloured the same way;
//! any other is kept with its own blocks, under a category of its own
//! named for what it seems meant to be, until a new version that passes
//! moves it up.

use axum::{
    body::Bytes,
    extract::{Multipart, State},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    app_state::AppState,
    details,
    error::AppError,
    extractors::{client_ip::ClientIp, session::SignedIn},
    import::{self, Kept, Upload},
    interface,
    models::{
        catalogue,
        schematic::{self, Schematic},
        similarity, simulation,
        user::User,
        version,
    },
    probe,
    schem::Blocks,
    schemati,
    simulate::{self, Guess},
    standardize,
};

use super::{
    form::{self, Form},
    pages::{self, File},
};

/// The file behind a Schemati download link, or why it could not be had.
async fn from_schemati(link: &str) -> Result<Bytes, String> {
    let key = schemati::download_key(link)?;
    Ok(Bytes::from(schemati::fetch(&key).await?))
}

/// A file sent with the upload form, or a new version's: chosen, or
/// fetched from a Schemati link.
pub(super) struct Sent {
    pub file: Bytes,
}

/// A multipart form as sent: its text fields, in order, and the file
/// chosen in it, if one was.
pub(super) struct Posted {
    pub pairs: Vec<(String, String)>,
    file: Option<Bytes>,
    /// The chosen file's name, as the browser sent it.
    file_name: Option<String>,
}

/// Reads a multipart form, or says it broke off partway, as a body over
/// the server's limit, 2 MB, does.
pub(super) async fn posted(
    multipart: &mut Multipart,
) -> Result<Posted, String> {
    let mut posted = Posted {
        pairs: Vec::new(),
        file: None,
        file_name: None,
    };
    let broken = loop {
        match multipart.next_field().await {
            Ok(Some(field)) => match field.name().map(str::to_owned) {
                Some(name) if name == "file" => {
                    let file_name = field.file_name().map(str::to_owned);
                    match field.bytes().await {
                        Ok(bytes) => {
                            posted.file = Some(bytes).filter(|b| !b.is_empty());
                            if posted.file.is_some() {
                                posted.file_name = file_name;
                            }
                        }
                        Err(_) => break true,
                    }
                }
                Some(name) => match field.text().await {
                    Ok(text) => posted.pairs.push((name, text)),
                    Err(_) => break true,
                },
                None => {}
            },
            Ok(None) => break false,
            Err(_) => break true,
        }
    };
    if broken {
        return Err("That file did not arrive whole. Files up to 2 MB are \
                    accepted; a build's .schem is usually a few kilobytes."
            .to_owned());
    }
    Ok(posted)
}

impl Posted {
    /// The first value sent under `key`, or empty.
    pub fn get(&self, key: &str) -> &str {
        form::first(&self.pairs, key)
    }

    /// The file chosen in the form: its `file`, or else the file behind
    /// its `schemati` link. Says what went wrong when there is no file.
    pub async fn chosen(&self) -> Result<Sent, String> {
        // A file chosen wins over a link pasted.
        let link = self.get("schemati").trim();
        let file = match &self.file {
            Some(file) => file.clone(),
            None if !link.is_empty() => from_schemati(link).await?,
            None => {
                return Err(
                    "Choose a .schem file, or paste a link from //download."
                        .to_owned(),
                );
            }
        };
        Ok(Sent { file })
    }

    /// Takes as the form's file the one its `schemati_file` names in
    /// `user`'s own Schemati folder, as the Schemati page's Publish button
    /// and the file choosers' Schemati list send, unless a file was chosen
    /// or a link pasted, which win over it.
    pub async fn fetch_own(
        &mut self,
        db: &sqlx::SqlitePool,
        user: &User,
    ) -> Result<(), String> {
        let own = self.get("schemati_file").to_owned();
        if self.file.is_none()
            && self.get("schemati").trim().is_empty()
            && !own.is_empty()
        {
            let data = super::schemati::own_file(db, user, &own).await?;
            self.file = Some(Bytes::from(data));
        }
        Ok(())
    }

    /// What to call the file chosen: its name, the Schemati file named,
    /// or the link pasted, in the order `chosen` takes them.
    pub fn chosen_name(&self) -> String {
        let link = self.get("schemati").trim();
        let own = self.get("schemati_file");
        match &self.file_name {
            Some(name) if self.file.is_some() => name.clone(),
            _ if !link.is_empty() => link.to_owned(),
            _ if !own.is_empty() => own.to_owned(),
            _ => "the file".to_owned(),
        }
    }

    /// Whether the form was sent to check a newly chosen file rather than
    /// to publish or save.
    pub fn choosing(&self) -> bool {
        self.get("action") == "file"
    }
}

/// Reads a form for choosing a file, as `Posted::chosen` does, or a file
/// from `user`'s own Schemati folder; see `Posted::fetch_own`.
async fn received(
    state: &AppState,
    user: &User,
    multipart: &mut Multipart,
) -> Result<Sent, String> {
    let mut posted = posted(multipart).await?;
    posted.fetch_own(&state.db, user).await?;
    posted.chosen().await
}

pub async fn form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    upload_page(&state, &user, "").await
}

/// The upload page for `user`, with their Schemati folder's files to
/// choose from if they gave a session, and `message` saying why the last
/// try was refused.
async fn upload_page(
    state: &AppState,
    user: &User,
    message: &str,
) -> Result<Response, AppError> {
    let folder = super::schemati::own_folder_quickly(&state.db, user).await?;
    let connection = super::schemati::connection(folder.as_ref());
    Ok(pages::upload(user, message, &connection).into_response())
}

/// Reads the chosen file and shows what the site makes of it, with a
/// details form to fill in.
pub async fn preview(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let Sent { file } = match received(&state, &user, &mut multipart).await {
        Ok(sent) => sent,
        Err(message) => return upload_page(&state, &user, &message).await,
    };
    let upload = match Upload::read(&recognized(&file).await) {
        Ok(upload) => upload,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return upload_page(&state, &user, &message).await;
        }
    };
    // What it seems to be starts the category and design.
    let guess = guessed(&upload.read_as().data).await;
    let form = Form::suggested(
        guess.as_ref(),
        &interface::read(&upload.read_as().blocks),
        &upload.read_as().size,
    );
    let said = Said {
        guess: guess.as_ref(),
        ..Said::default()
    };
    show_preview(&state, &user, &upload, &form, said).await
}

/// The stored build most like an upload's `blocks`, if any is similar.
async fn closest(
    db: &sqlx::SqlitePool,
    blocks: &Blocks,
    size: (i64, i64, i64),
) -> anyhow::Result<Option<(Schematic, u32)>> {
    let Some((id, permille)) = similarity::closest(db, blocks, size).await?
    else {
        return Ok(None);
    };
    Ok(schematic::get(db, &id).await?.map(|s| (s, permille)))
}

/// Publishes the previewed file with the details filled in, or, when the
/// form asks, swaps its hoppers or takes a newly chosen file in its place
/// and shows it again with the details as sent.
pub async fn publish(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    ClientIp(ip): ClientIp,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut posted = match posted(&mut multipart).await {
        Ok(posted) => posted,
        Err(message) => return upload_page(&state, &user, &message).await,
    };
    let form = Form::read(&posted.pairs);
    // The preview carries the prepared file, and how it was prepared.
    let Ok(data) = STANDARD.decode(posted.get("data")) else {
        return upload_page(
            &state,
            &user,
            "The upload expired. Choose the file again.",
        )
        .await;
    };
    // A swap changes the file and shows it again, keeping the details.
    let swapping = posted.get("swap") == "hopper";
    let data = if swapping {
        match standardize::barrels(&data, &["hopper"]) {
            Ok(swapped) => swapped,
            Err(err) => {
                let message = format!("Could not swap them: {err:#}.");
                return upload_page(&state, &user, &message).await;
            }
        }
    } else {
        data
    };
    let upload = match Upload::read(&data) {
        Ok(upload) => upload,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return upload_page(&state, &user, &message).await;
        }
    };
    if posted.choosing() {
        return chosen_instead(&state, &user, &mut posted, upload, form).await;
    }
    if swapping {
        return show_preview(&state, &user, &upload, &form, Said::default())
            .await;
    }
    let published =
        publish_checked(&state, &user, ip.as_deref(), &upload, &form).await?;
    let refused = match published {
        Ok(id) => {
            tracing::info!(id, by = user.username, ip, "upload");
            simulation::refresh_soon(&state.db, &id);
            similarity::refresh_soon(&state.db);
            return Ok(
                Redirect::to(&format!("/schematics/{id}")).into_response()
            );
        }
        Err(message) => message,
    };
    let said = Said {
        message: &refused,
        ..Said::default()
    };
    show_preview(&state, &user, &upload, &form, said).await
}

/// The preview of the file newly chosen on the preview page, in place of
/// `current`, with the details sent in `form` kept and only the empty ones
/// filled from the new file; or `current` again, with the details as sent
/// and why the new one could not be read.
async fn chosen_instead(
    state: &AppState,
    user: &User,
    posted: &mut Posted,
    current: Upload,
    form: Form,
) -> Result<Response, AppError> {
    let chosen = match posted.fetch_own(&state.db, user).await {
        Ok(()) => posted.chosen().await,
        Err(message) => Err(message),
    };
    let read = match chosen {
        Ok(chosen) => Upload::read(&recognized(&chosen.file).await)
            .map_err(|err| format!("Could not read that file: {err:#}.")),
        Err(message) => Err(message),
    };
    let upload = match read {
        Ok(prepared) => prepared,
        Err(message) => {
            let said = Said {
                choosing: Some(&message),
                ..Said::default()
            };
            return show_preview(state, user, &current, &form, said).await;
        }
    };
    let guess = guessed(&upload.read_as().data).await;
    let suggested = Form::suggested(
        guess.as_ref(),
        &interface::read(&upload.read_as().blocks),
        &upload.read_as().size,
    );
    let form = form.keeping_sent(suggested);
    let said = Said {
        guess: guess.as_ref(),
        ..Said::default()
    };
    show_preview(state, user, &upload, &form, said).await
}

/// What the check by function makes of the file `data`, off the async
/// workers, as probing takes a second or two.
async fn recognition(data: &[u8]) -> anyhow::Result<probe::Recognition> {
    let data = data.to_vec();
    tokio::task::spawn_blocking(move || probe::recognize(&data)).await?
}

/// The file `data` with its inputs and outputs coloured in, when the check
/// by function recognizes it, so the rest of the site reads them; else as
/// it came.
async fn recognized(data: &[u8]) -> Vec<u8> {
    recognition(data)
        .await
        .map_or_else(|_| data.to_vec(), |r| r.file)
}

/// What a file seems to be, from a quick run of the tests it fits.
async fn guessed(data: &[u8]) -> Option<Guess> {
    let data = data.to_vec();
    tokio::task::spawn_blocking(move || {
        simulate::guess(&Blocks::read(&data).ok()?)
    })
    .await
    .ok()
    .flatten()
}

/// Stores the upload, from IP address `ip`, and returns its id, or says
/// why it was refused. A
/// build not recognized as the category it is given goes up with its own
/// blocks under a category of its own, named for what it seems meant to
/// be, as `details::unrecognized` names it.
async fn publish_checked(
    state: &AppState,
    user: &User,
    ip: Option<&str>,
    upload: &Upload,
    form: &Form,
) -> Result<Result<String, String>, AppError> {
    let db = &state.db;
    let categories = catalogue::categories(db).await?;
    let mut given = match form.parse(&categories) {
        Ok(given) => given,
        Err(message) => return Ok(Err(message)),
    };
    let kept = match import::kept(&categories, &given.category, upload).await? {
        Ok(kept) => kept,
        Err(message) => return Ok(Err(message)),
    };
    let prepared = match &kept {
        Kept::Standard { file, category } => {
            given.category.clone_from(category);
            file
        }
        Kept::Original(off) => {
            if let Some(off) = off {
                given.category.clone_from(&off.name);
            }
            &upload.original
        }
    };
    let build = details::Build::of(prepared);
    let ready = match details::check(db, given, build, user.is_admin()).await? {
        Ok(ready) => ready,
        Err(message) => return Ok(Err(message)),
    };
    Ok(
        import::store(db, prepared, &ready, Some(user.id), ip, None, None)
            .await?,
    )
}

/// What the preview says beside the form: why publishing was refused,
/// what the file seems to be, and why a file chosen in its place could not
/// be read.
#[derive(Default)]
struct Said<'a> {
    message: &'a str,
    guess: Option<&'a Guess>,
    choosing: Option<&'a str>,
}

/// The preview page for an upload, with the details form as `form` has it.
async fn show_preview(
    state: &AppState,
    user: &User,
    upload: &Upload,
    form: &Form,
    said: Said<'_>,
) -> Result<Response, AppError> {
    let prepared = &upload.original;
    // Either file may be the one kept.
    let mut duplicate = version::holder(&state.db, &prepared.sha256)
        .await?
        .map(|(id, _)| id);
    if let (None, Ok(standard)) = (&duplicate, &upload.standard) {
        duplicate = version::holder(&state.db, &standard.sha256)
            .await?
            .map(|(id, _)| id);
    }
    // Its roles, if recognized, are coloured in already; this says so.
    let recognition = recognition(&prepared.data).await?;
    let size = &prepared.size;
    let similar = closest(
        &state.db,
        &prepared.blocks,
        (size.x.into(), size.y.into(), size.z.into()),
    )
    .await?;
    let file = File::Upload {
        data: &STANDARD.encode(&prepared.data),
        size: [size.x, size.y, size.z],
        duplicate: duplicate.as_deref(),
        similar: similar.as_ref().map(|(s, permille)| (s, *permille)),
        unconventional: !upload.read_as().conventional,
        unstandard: upload.standard.as_ref().err().map(String::as_str),
        swapped: &prepared.swapped,
        guess: said.guess,
        hoppers: standardize::swappable(&prepared.blocks).hoppers,
        recognized: recognition.found.as_deref(),
        problems: &recognition.problems,
    };
    let shown = form::Shown {
        file,
        blocks: Some(&upload.read_as().blocks),
        message: said.message,
        choosing: said.choosing,
    };
    form::page(&state.db, user, form, shown).await
}
