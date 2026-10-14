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
    error::internal_error,
    import::{self, Prepared},
    interface, lint,
    models::{
        schematic::{self, Measure, New, Schematic, StackDirection, Tag},
        similarity, simulation,
        user::User,
    },
    mojang,
    schem::Blocks,
    schemati,
    simulate::{self, Guess},
    stack,
    standardize::{self, Gray},
};

use super::{
    details,
    pages::{self, Choices, DetailsPage, Fields, File},
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

/// Reads a file form: its `file`, or else the file behind its `schemati`
/// link, and its `standardize`, `whiten_gray`, and `keep_paste` ticks. Says what went
/// wrong when there is no file to read.
pub(super) async fn received(
    multipart: &mut Multipart,
) -> Result<Sent, String> {
    let mut file = None;
    let mut link = String::new();
    let mut standardize = false;
    let mut whiten_gray = false;
    let mut keep_paste = false;
    // A body over the server's limit, 2 MB, breaks off partway.
    let mut broken = false;
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => match field.name() {
                Some("file") => match field.bytes().await {
                    Ok(bytes) => file = Some(bytes),
                    Err(_) => broken = true,
                },
                Some("standardize") => standardize = true,
                Some("whiten_gray") => whiten_gray = true,
                Some("keep_paste") => keep_paste = true,
                Some("schemati") => match field.text().await {
                    Ok(text) => link = text,
                    Err(_) => broken = true,
                },
                _ => {}
            },
            Ok(None) => break,
            Err(_) => {
                broken = true;
                break;
            }
        }
    }
    if broken {
        return Err("That file did not arrive whole. Files up to 2 MB are \
                    accepted; a build's .schem is usually a few kilobytes."
            .to_owned());
    }
    // A file chosen wins over a link pasted.
    let file = match file.filter(|bytes| !bytes.is_empty()) {
        Some(file) => file,
        None if !link.trim().is_empty() => from_schemati(&link).await?,
        None => {
            return Err(
                "Choose a .schem file, or paste a link from //download."
                    .to_owned(),
            );
        }
    };
    let gray = if whiten_gray {
        Gray::Whiten
    } else {
        Gray::Keep
    };
    Ok(Sent {
        file,
        standardize: standardize.then_some(gray),
        keep_paste,
    })
}

pub async fn form(user_opt: Option<User>) -> Response {
    match user_opt {
        Some(user) => pages::upload(&user, "").into_response(),
        None => Redirect::to("/sessions/new").into_response(),
    }
}

/// Reads the chosen file, standardizes it if asked, and shows what the
/// site makes of it with a details form to fill in.
pub async fn preview(
    State(state): State<AppState>,
    user_opt: Option<User>,
    mut multipart: Multipart,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    let Sent {
        file,
        standardize,
        keep_paste,
    } = match received(&mut multipart).await {
        Ok(sent) => sent,
        Err(message) => return pages::upload(&user, &message).into_response(),
    };
    let prepared = match import::prepare(&file, standardize, true, keep_paste) {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return pages::upload(&user, &message).into_response();
        }
    };
    let interface = interface::read(&prepared.blocks);
    let (width, orientation) = import::guess(&interface);
    // What it seems to be, from a quick run of the tests it fits, to start
    // the category and design with.
    let data = prepared.data.clone();
    let guess = tokio::task::spawn_blocking(move || {
        simulate::guess(&Blocks::read(&data).ok()?)
    })
    .await
    .ok()
    .flatten();
    // The carry lines the signs show start ticked.
    let flags = import::sign_flags(&interface);
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let orientation = orientation.unwrap_or("horz");
    let size = &prepared.size;
    let stack_rows = [details::StackRow::suggested(
        orientation,
        (size.x.into(), size.y.into(), size.z.into()),
    )];
    let fields = Fields {
        category: guess.as_ref().map_or("", |guess| guess.category),
        width: &width,
        orientation,
        encoding: "bin",
        latency: "",
        // Most builds are; the uploader says when one is not.
        synchronized: Some(true),
        builders: &[],
        contributors: &[],
        built_on: "",
        description: "",
        given_name: "",
        logic: "",
        throughput: "",
        bytes: "",
        at_least: "",
        design: "other",
        flags: &flags,
        stackable: false,
        stack_rows: &stack_rows,
    };
    let settings = Settings {
        unconventional: false,
        keep_paste,
    };
    let said = Said {
        message: "",
        guess: guess.as_ref(),
    };
    show_preview(&state, &user, &prepared, &fields, &[], settings, said).await
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

/// What the uploader chose about the file itself, carried from the
/// preview to publishing.
#[derive(Clone, Copy)]
struct Settings {
    /// The build does not follow the conventions, so is not linted.
    unconventional: bool,
    /// The file keeps where its uploader's `//paste` put it.
    keep_paste: bool,
}

/// What the details form sends when publishing.
struct Submission {
    /// The prepared file, base64.
    data: String,
    /// `hopper` when the person asked to swap hoppers for barrels rather
    /// than publish.
    swap: String,
    settings: Settings,
    form: details::Form,
}

impl Submission {
    fn read(body: &[u8]) -> Self {
        let pairs = details::pairs(body);
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        Submission {
            data: get("data"),
            swap: get("swap"),
            settings: Settings {
                unconventional: !get("unconventional").is_empty(),
                keep_paste: !get("keep_paste").is_empty(),
            },
            form: details::Form::read(&pairs),
        }
    }
}

/// Publishes the previewed file with the details filled in.
pub async fn publish(
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: Bytes,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    let sent = Submission::read(&body);
    let Ok(data) = STANDARD.decode(&sent.data) else {
        return pages::upload(
            &user,
            "The upload expired. Choose the file again.",
        )
        .into_response();
    };
    let settings = sent.settings;
    // A swap changes the file and shows it again, keeping the details.
    let swapping = sent.swap == "hopper";
    let data = if swapping {
        match standardize::barrels(&data, &[sent.swap.as_str()]) {
            Ok(swapped) => swapped,
            Err(err) => {
                let message = format!("Could not swap them: {err:#}.");
                return pages::upload(&user, &message).into_response();
            }
        }
    } else {
        data
    };
    let prepared = match import::prepare(
        &data,
        None,
        !settings.unconventional,
        settings.keep_paste,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return pages::upload(&user, &message).into_response();
        }
    };
    if swapping {
        return show_preview(
            &state,
            &user,
            &prepared,
            &sent.form.fields(),
            &sent.form.tags,
            settings,
            Said::default(),
        )
        .await;
    }
    let refused = match publish_checked(&state, &user, &prepared, &sent).await {
        Ok(id) => {
            simulation::refresh_soon(&state.db, &id);
            similarity::refresh_soon(&state.db);
            return Redirect::to(&format!("/schematics/{id}")).into_response();
        }
        Err(message) => message,
    };
    show_preview(
        &state,
        &user,
        &prepared,
        &sent.form.fields(),
        &sent.form.tags,
        settings,
        Said {
            message: &refused,
            guess: None,
        },
    )
    .await
}

/// Stores the upload and returns its id, or says why it was refused.
async fn publish_checked(
    state: &AppState,
    user: &User,
    prepared: &Prepared,
    sent: &Submission,
) -> Result<String, String> {
    if !prepared.violations.is_empty() {
        return Err(match prepared.violations.len() {
            1 => "It breaks a building convention, listed below.".to_owned(),
            n => format!("It breaks {n} building conventions, listed below."),
        });
    }
    let categories = schematic::categories(&state.db)
        .await
        .map_err(|err| format!("Could not load the categories: {err}."))?;
    let parsed = sent.form.parse(&categories)?;
    // Only an adder may leave its lone control and status lines unsigned,
    // which the preview, before the category was known, let pass.
    if !sent.settings.unconventional
        && let Some(violation) =
            lint::check_for(&prepared.blocks, &parsed.category).first()
    {
        return Err(format!(
            "A {} needs every control and status line signed: {}.",
            parsed.category.replace('_', " "),
            violation.message
        ));
    }
    // Only an admin may name a tag not yet on the list.
    match schematic::check_new_tags(&state.db, &sent.form.tags, user.is_admin())
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(message)) => return Err(message),
        Err(err) => return Err(format!("Could not check the tags: {err}.")),
    }
    // Checked here too, so the refusal reads as a sentence of its own.
    let category = match schematic::check_category(
        &state.db,
        &parsed.category,
        &parsed.design,
    )
    .await
    {
        Ok(Ok(category)) => category,
        Ok(Err(message)) => return Err(message),
        Err(err) => {
            return Err(format!("Could not check the category: {err}."));
        }
    };
    // The simulation decides how a recognized build stacks once it is
    // published, so its form's stacking counts for nothing.
    let inferred = !sent.settings.unconventional
        && simulate::INFERS_STACKING.contains(&parsed.category.as_str());
    let stacking: &[StackDirection] =
        if inferred { &[] } else { &parsed.stacking };
    stack::check(&prepared.data, stacking)?;
    let size = &prepared.size;
    let new = New {
        category: &parsed.category,
        width: parsed.width,
        orientation: &parsed.orientation,
        encoding: parsed.encoding.as_deref(),
        latency: parsed.latency,
        synchronized: parsed.synchronized,
        size: (size.x.into(), size.y.into(), size.z.into()),
        built_on: parsed.built_on.as_deref(),
        imported_by: None,
        conventional: !sent.settings.unconventional,
        design: Some(&parsed.design),
        description: parsed.description.as_deref(),
        given_name: parsed.given_name.as_deref(),
        logic: parsed.logic.as_deref(),
        throughput: parsed.throughput,
        // Only a category ranked by a measure keeps it, as a memory does
        // its capacity.
        bytes: parsed.bytes.filter(|_| category.has(Measure::Bytes)),
        at_least: parsed.at_least.filter(|_| category.has(Measure::AtLeast)),
        facts: &prepared.facts,
        keeps_paste: sent.settings.keep_paste,
        uploaded_by: Some(user.id),
        sha256: &prepared.sha256,
        data: &prepared.data,
    };
    // Credited as their players spell them; a name no player has is
    // refused.
    let builders = mojang::respell(&parsed.builders).await?;
    let contributors = mojang::respell(&parsed.contributors).await?;
    let extras = import::Extras {
        builders: &builders,
        contributors: &contributors,
        tags: &sent.form.tags,
        // An admin may name a new tag; it starts under other.
        new_tags: user.is_admin(),
        stacking,
        flags: &parsed.flags,
    };
    import::store(&state.db, &new, &extras)
        .await
        .map_err(|err| format!("Could not publish it: {err:#}."))
}

/// What the preview says beside the form: why publishing was refused, and
/// what the file seems to be.
#[derive(Default)]
struct Said<'a> {
    message: &'a str,
    guess: Option<&'a Guess>,
}

/// The preview page for a prepared file.
async fn show_preview(
    state: &AppState,
    user: &User,
    prepared: &Prepared,
    fields: &Fields<'_>,
    chosen_tags: &[String],
    settings: Settings,
    said: Said<'_>,
) -> Response {
    let tags: Vec<Tag> = match schematic::all_tags(&state.db).await {
        Ok(tags) => tags,
        Err(err) => return internal_error(err).into_response(),
    };
    let duplicate =
        match schematic::id_by_sha256(&state.db, &prepared.sha256).await {
            Ok(found) => found,
            Err(err) => return internal_error(err).into_response(),
        };
    let choices = match Choices::load(&state.db).await {
        Ok(choices) => choices,
        Err(err) => return internal_error(err).into_response(),
    };
    let size = &prepared.size;
    let size = (size.x.into(), size.y.into(), size.z.into());
    let similar = match closest(&state.db, &prepared.blocks, size).await {
        Ok(similar) => similar,
        Err(err) => return internal_error(&*err).into_response(),
    };
    let interface = interface::read(&prepared.blocks);
    let derived = import::derived("adder", &interface);
    let swappable = standardize::swappable(&prepared.blocks);
    pages::details_page(
        user,
        &DetailsPage {
            file: File::Upload {
                data: &STANDARD.encode(&prepared.data),
                size: [prepared.size.x, prepared.size.y, prepared.size.z],
                duplicate: duplicate.as_deref(),
                similar: similar.as_ref().map(|(s, permille)| (s, *permille)),
                keep_paste: settings.keep_paste,
                unconventional: settings.unconventional,
                swapped: &prepared.swapped,
                guess: said.guess,
                hoppers: swappable.hoppers,
            },
            analysis: &pages::Analysis {
                interface,
                violations: prepared.violations.clone(),
            },
            fields,
            chosen_tags,
            choices: &choices,
            tags: &tags,
            may_create_tags: user.is_admin(),
            derived,
            message: said.message,
        },
    )
    .into_response()
}
