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
    thumbnail,
};

use super::{
    details,
    pages::{self, Fields, UploadPreview},
};

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
    let mut standardize = false;
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("file") => file = field.bytes().await.ok(),
            Some("standardize") => standardize = true,
            _ => {}
        }
    }
    let Some(file) = file.filter(|bytes| !bytes.is_empty()) else {
        return pages::upload(&user, "Choose a .schem file to upload.")
            .into_response();
    };
    let prepared = match import::prepare(file.to_vec(), standardize, true) {
        Ok(prepared) => prepared,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            return pages::upload(&user, &message).into_response();
        }
    };
    let interface = interface::read(&prepared.blocks);
    let (width, orientation) = import::guess(&interface);
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let fields = Fields {
        function: "",
        width: &width,
        orientation: orientation.unwrap_or("horz"),
        encoding: "bin",
        latency: "",
        synchronized: None,
        author: "",
        built_on: "",
        description: "",
    };
    show_preview(&state, &user, &prepared, &fields, &[], false, "").await
}

/// What the details form sends when publishing.
struct Submission {
    /// The prepared file, base64.
    data: String,
    tags: Vec<String>,
    unconventional: bool,
    form: details::Form,
}

impl Submission {
    /// Read by hand, since the tag checkboxes repeat one name.
    fn read(body: &[u8]) -> Self {
        let pairs: Vec<(String, String)> = form_urlencoded::parse(body)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        Submission {
            data: get("data"),
            tags: pairs
                .iter()
                .filter(|(k, _)| k == "tag")
                .map(|(_, v)| v.clone())
                .collect(),
            unconventional: !get("unconventional").is_empty(),
            form: details::Form {
                function: get("function"),
                width: get("width"),
                orientation: get("orientation"),
                encoding: get("encoding"),
                latency: get("latency"),
                synchronized: get("synchronized"),
                author: get("author"),
                built_on: get("built_on"),
                description: get("description"),
            },
        }
    }

    /// The form's values, to show again after a refusal.
    fn fields(&self) -> Fields<'_> {
        let form = &self.form;
        Fields {
            function: &form.function,
            width: &form.width,
            orientation: &form.orientation,
            encoding: &form.encoding,
            latency: &form.latency,
            synchronized: match form.synchronized.as_str() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            },
            author: &form.author,
            built_on: &form.built_on,
            description: &form.description,
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
    let prepared = match import::prepare(data, false, !sent.unconventional) {
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
        &sent.fields(),
        &sent.tags,
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
        return Err(format!(
            "This build breaks {} building conventions, listed above. Fix \
             them in the file, try standardizing it, or mark it as following \
             its own layout.",
            prepared.violations.len()
        ));
    }
    let parsed = sent.form.parse()?;
    let size = &prepared.size;
    let new = New {
        function: &parsed.function,
        width: parsed.width,
        orientation: &parsed.orientation,
        encoding: &parsed.encoding,
        latency: parsed.latency,
        synchronized: parsed.synchronized,
        size: (size.x.into(), size.y.into(), size.z.into()),
        author: parsed.author.as_deref(),
        built_on: parsed.built_on.as_deref(),
        imported_by: None,
        conventional: !sent.unconventional,
        description: parsed.description.as_deref(),
        uploaded_by: Some(user.id),
        sha256: &prepared.sha256,
        data: &prepared.data,
    };
    import::store(&state.db, &new, &sent.tags, &[])
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
    let thumbnail = thumbnail::render(&prepared.blocks)
        .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)))
        .ok();
    pages::upload_preview(
        user,
        &UploadPreview {
            data: &STANDARD.encode(&prepared.data),
            thumbnail: thumbnail.as_deref(),
            analysis: &pages::Analysis {
                interface: interface::read(&prepared.blocks),
                violations: prepared.violations.clone(),
            },
            size: [prepared.size.x, prepared.size.y, prepared.size.z],
            duplicate: duplicate.as_deref(),
            fields,
            tags: &tags,
            chosen_tags,
            unconventional,
            message,
        },
    )
    .into_response()
}
