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
    interface,
    models::{
        schematic::{self, New, Tag},
        user::User,
    },
    mojang, schemati, stack,
};

use super::{
    details,
    pages::{self, Choices, Fields, UploadPreview},
};

/// The file behind a Schemati download link, or why it could not be had.
async fn from_schemati(link: &str) -> Result<Bytes, String> {
    let key = schemati::download_key(link)?;
    Ok(Bytes::from(schemati::fetch(&key).await?))
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
    let mut file = None;
    let mut link = String::new();
    let mut standardize = false;
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
        let message = "That file did not arrive whole. Files up to 2 MB are \
                       accepted; a build's .schem is usually a few kilobytes.";
        return pages::upload(&user, message).into_response();
    }
    // A file chosen wins over a link pasted.
    let file = match file.filter(|bytes| !bytes.is_empty()) {
        Some(file) => file,
        None if !link.trim().is_empty() => match from_schemati(&link).await {
            Ok(file) => file,
            Err(message) => {
                return pages::upload(&user, &message).into_response();
            }
        },
        None => {
            let message =
                "Choose a .schem file, or paste a link from //download.";
            return pages::upload(&user, message).into_response();
        }
    };
    let prepared = match import::prepare(&file, standardize, true) {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return pages::upload(&user, &message).into_response();
        }
    };
    let interface = interface::read(&prepared.blocks);
    let (width, orientation) = import::guess(&interface);
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
        category: "",
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
        period: "",
        design: "other",
        flags: &flags,
        stackable: false,
        stack_rows: &stack_rows,
    };
    show_preview(&state, &user, &prepared, &fields, &[], false, "").await
}

/// What the details form sends when publishing.
struct Submission {
    /// The prepared file, base64.
    data: String,
    unconventional: bool,
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
            unconventional: !get("unconventional").is_empty(),
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
    let prepared = match import::prepare(&data, false, !sent.unconventional) {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return pages::upload(&user, &message).into_response();
        }
    };
    let refused = match publish_checked(&state, &user, &prepared, &sent).await {
        Ok(id) => {
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
        sent.unconventional,
        &refused,
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
    let parsed = sent.form.parse()?;
    // Checked here too, so the refusal reads as a sentence of its own.
    let periodic = match schematic::check_category(
        &state.db,
        &parsed.category,
        &parsed.design,
    )
    .await
    {
        // Only a serial category's build keeps a period.
        Ok(Ok(category)) => category.periodic,
        Ok(Err(message)) => return Err(message),
        Err(err) => {
            return Err(format!("Could not check the category: {err}."));
        }
    };
    stack::check(&prepared.data, &parsed.stacking)?;
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
        conventional: !sent.unconventional,
        design: Some(&parsed.design),
        description: parsed.description.as_deref(),
        given_name: parsed.given_name.as_deref(),
        period: parsed.period.filter(|_| periodic),
        uploaded_by: Some(user.id),
        sha256: &prepared.sha256,
        data: &prepared.data,
    };
    // Credited as their players spell them.
    let builders = mojang::respell(&parsed.builders).await;
    let contributors = mojang::respell(&parsed.contributors).await;
    let extras = import::Extras {
        builders: &builders,
        contributors: &contributors,
        tags: &sent.form.tags,
        // Anyone uploading may name a new tag; it starts under other.
        new_tags: true,
        stacking: &parsed.stacking,
        flags: &parsed.flags,
    };
    import::store(&state.db, &new, &extras)
        .await
        .map_err(|err| format!("Could not publish it: {err:#}."))
}

/// The preview page for a prepared file.
async fn show_preview(
    state: &AppState,
    user: &User,
    prepared: &Prepared,
    fields: &Fields<'_>,
    chosen_tags: &[String],
    unconventional: bool,
    message: &str,
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
    pages::upload_preview(
        user,
        &UploadPreview {
            data: &STANDARD.encode(&prepared.data),
            analysis: &pages::Analysis {
                interface: interface::read(&prepared.blocks),
                violations: prepared.violations.clone(),
            },
            size: [prepared.size.x, prepared.size.y, prepared.size.z],
            duplicate: duplicate.as_deref(),
            fields,
            choices: &choices,
            tags: &tags,
            chosen_tags,
            unconventional,
            message,
        },
    )
    .into_response()
}
