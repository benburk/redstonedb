//! Uploading a build: choose a file, preview what the site reads from it,
//! fill in its details, and publish. The preview carries the prepared file
//! in a hidden field, so nothing is stored until publishing, which checks
//! the file again.

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
    extractors::session::SignedIn,
    import::{self, Analysis, Prepared},
    interface,
    models::{
        catalogue,
        schematic::{self, Schematic},
        similarity, simulation,
        user::User,
        version,
    },
    schem::Blocks,
    schemati,
    simulate::{self, Guess},
    standardize::{self, Gray},
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
    /// Standardizing, and what to do with gray blocks, if asked.
    pub standardize: Option<Gray>,
    pub keep_paste: bool,
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
    /// its `schemati` link, with its `standardize`, `whiten_gray`, and
    /// `keep_paste` ticks. Says what went wrong when there is no file.
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
        let gray = if self.get("whiten_gray").is_empty() {
            Gray::Keep
        } else {
            Gray::Whiten
        };
        Ok(Sent {
            file,
            standardize: (!self.get("standardize").is_empty()).then_some(gray),
            keep_paste: !self.get("keep_paste").is_empty(),
        })
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

/// Reads the chosen file, standardizes it if asked, and shows what the
/// site makes of it with a details form to fill in.
pub async fn preview(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let Sent {
        file,
        standardize,
        keep_paste,
    } = match received(&state, &user, &mut multipart).await {
        Ok(sent) => sent,
        Err(message) => return upload_page(&state, &user, &message).await,
    };
    let prepared = match import::prepare(&file, standardize, true, keep_paste) {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return upload_page(&state, &user, &message).await;
        }
    };
    // What it seems to be starts the category and design.
    let guess = guessed(&prepared.data).await;
    let form = Form::suggested(
        guess.as_ref(),
        &interface::read(&prepared.blocks),
        &prepared.size,
    );
    let said = Said {
        guess: guess.as_ref(),
        ..Said::default()
    };
    show_preview(&state, &user, &prepared, &form, said).await
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
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut posted = match posted(&mut multipart).await {
        Ok(posted) => posted,
        Err(message) => return upload_page(&state, &user, &message).await,
    };
    let mut form = Form::read(&posted.pairs);
    // The preview carries the prepared file, and how it was prepared.
    let Ok(data) = STANDARD.decode(posted.get("data")) else {
        return upload_page(
            &state,
            &user,
            "The upload expired. Choose the file again.",
        )
        .await;
    };
    let conventional = posted.get("unconventional").is_empty();
    let keep_paste = !posted.get("file_keeps_paste").is_empty();
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
    let prepared = match import::prepare(&data, None, conventional, keep_paste)
    {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return upload_page(&state, &user, &message).await;
        }
    };
    if posted.choosing() {
        return chosen_instead(&state, &user, &mut posted, prepared, &mut form)
            .await;
    }
    if swapping {
        return show_preview(&state, &user, &prepared, &form, Said::default())
            .await;
    }
    let refused = match publish_checked(&state, &user, &prepared, &form).await?
    {
        Ok(id) => {
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
    show_preview(&state, &user, &prepared, &form, said).await
}

/// The preview of the file newly chosen on the preview page, in place of
/// `current`, with the details as sent in `form`; or `current` again, with
/// why the new one could not be read.
async fn chosen_instead(
    state: &AppState,
    user: &User,
    posted: &mut Posted,
    current: Prepared,
    form: &mut Form,
) -> Result<Response, AppError> {
    let chosen = match posted.fetch_own(&state.db, user).await {
        Ok(()) => posted.chosen().await,
        Err(message) => Err(message),
    };
    let read = chosen.and_then(|chosen| {
        import::prepare(
            &chosen.file,
            chosen.standardize,
            current.conventional,
            chosen.keep_paste,
        )
        .map_err(|err| format!("Could not read that file: {err:#}."))
    });
    let prepared = match read {
        Ok(prepared) => prepared,
        Err(message) => {
            let said = Said {
                choosing: Some(&message),
                ..Said::default()
            };
            return show_preview(state, user, &current, form, said).await;
        }
    };
    // What it seems to be starts the category, if none is chosen yet.
    let guess = guessed(&prepared.data).await;
    if form.category.trim().is_empty()
        && let Some(guess) = &guess
    {
        guess.category.clone_into(&mut form.category);
    }
    let said = Said {
        guess: guess.as_ref(),
        ..Said::default()
    };
    show_preview(state, user, &prepared, form, said).await
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

/// Stores the upload and returns its id, or says why it was refused.
async fn publish_checked(
    state: &AppState,
    user: &User,
    prepared: &Prepared,
    form: &Form,
) -> Result<Result<String, String>, AppError> {
    let db = &state.db;
    let categories = catalogue::categories(db).await?;
    let given = match form.parse(&categories) {
        Ok(given) => given,
        Err(message) => return Ok(Err(message)),
    };
    let build = details::Build::of(prepared);
    let ready = match details::check(db, given, build, user.is_admin()).await? {
        Ok(ready) => ready,
        Err(message) => return Ok(Err(message)),
    };
    let analysis = Analysis::of(
        &prepared.blocks,
        &ready.category.name,
        prepared.conventional,
    );
    match analysis.violations.len() {
        0 => {}
        1 => {
            return Ok(Err(
                "It breaks a building convention, listed below.".to_owned()
            ));
        }
        n => {
            return Ok(Err(format!(
                "It breaks {n} building conventions, listed below."
            )));
        }
    }
    Ok(import::store(db, prepared, &ready, Some(user.id), None).await?)
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

/// The preview page for a prepared file, with the details form as `form`
/// has it.
async fn show_preview(
    state: &AppState,
    user: &User,
    prepared: &Prepared,
    form: &Form,
    said: Said<'_>,
) -> Result<Response, AppError> {
    let duplicate = version::holder(&state.db, &prepared.sha256)
        .await?
        .map(|(id, _)| id);
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
        keep_paste: prepared.keeps_paste,
        unconventional: !prepared.conventional,
        swapped: &prepared.swapped,
        guess: said.guess,
        hoppers: standardize::swappable(&prepared.blocks).hoppers,
    };
    let shown = form::Shown {
        file,
        blocks: Some(&prepared.blocks),
        message: said.message,
        choosing: said.choosing,
    };
    form::page(&state.db, user, form, shown).await
}
