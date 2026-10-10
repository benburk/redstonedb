//! A new version of a build: choose a file, preview how it compares with
//! the build, and publish it in the build's place. The preview carries the
//! prepared file in a hidden field, as uploading does, and publishing
//! checks it again.

use axum::{
    body::Bytes,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    app_state::AppState,
    error::internal_error,
    models::{
        schematic::{self, Schematic},
        simulation,
        user::User,
        version,
    },
    version::{Checked, check},
};

use super::{
    details,
    pages::{self, VersionPreview},
    schematics::editor,
    upload::{self, Sent},
};

/// The page for choosing the new version's file.
pub async fn form(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    match editing(&state, user_opt, &id).await {
        Ok((user, s)) => pages::new_version(&user, &s, "").into_response(),
        Err(response) => response,
    }
}

/// Reads the chosen file and shows how it compares with the build.
pub async fn preview(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    mut multipart: Multipart,
) -> Response {
    let (user, s) = match editing(&state, user_opt, &id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Sent {
        file, standardize, ..
    } = match upload::received(&mut multipart).await {
        Ok(sent) => sent,
        Err(message) => {
            return pages::new_version(&user, &s, &message).into_response();
        }
    };
    match check(&state.db, &s, &file, standardize).await {
        Ok(checked) => show(&state, &user, &s, &checked, "").await,
        Err(err) => {
            let message = format!("Could not read that file: {err:#}.");
            pages::new_version(&user, &s, &message).into_response()
        }
    }
}

/// Publishes the previewed file as the build's next version, unless it
/// fails a check now.
pub async fn publish(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: Bytes,
) -> Response {
    let (user, s) = match editing(&state, user_opt, &id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let pairs = details::pairs(&body);
    let field = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim().to_owned())
            .unwrap_or_default()
    };
    let expired = || {
        pages::new_version(
            &user,
            &s,
            "The preview expired. Choose the file again.",
        )
        .into_response()
    };
    let Ok(data) = STANDARD.decode(field("data")) else {
        return expired();
    };
    let Ok(checked) = check(&state.db, &s, &data, false).await else {
        return expired();
    };
    let note: String = field("note").chars().take(500).collect();
    if !checked.refusals.is_empty() {
        return show(&state, &user, &s, &checked, &note).await;
    }
    let size = &checked.prepared.size;
    let new = version::New {
        data: &checked.prepared.data,
        sha256: &checked.prepared.sha256,
        size: (size.x.into(), size.y.into(), size.z.into()),
        added_by: Some(user.id),
        note: Some(note.as_str()).filter(|note| !note.is_empty()),
    };
    match version::publish(&state.db, &id, &new).await {
        Ok(number) => {
            tracing::info!(
                id,
                version = number,
                by = user.username,
                "new version"
            );
            // The new file is simulated afresh, and its width and timing
            // read from it again.
            simulation::refresh_soon(&state.db, &id).await;
            Redirect::to(&format!("/schematics/{id}")).into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

/// An earlier version's file, under the build's filename with its number.
pub async fn download(
    Path((id, number)): Path<(String, i64)>,
    State(state): State<AppState>,
) -> Response {
    let s = match schematic::get(&state.db, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    match version::data(&state.db, &id, number).await {
        Ok(Some(data)) => {
            let filename =
                s.filename().replace(".schem", &format!("_v{number}.schem"));
            (
                [
                    (
                        header::CONTENT_TYPE,
                        "application/octet-stream".to_owned(),
                    ),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                data,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

/// The account editing build `id`, and the build.
async fn editing(
    state: &AppState,
    user_opt: Option<User>,
    id: &str,
) -> Result<(User, Schematic), Response> {
    let user = editor(&state.db, user_opt, id).await?;
    match schematic::get(&state.db, id).await {
        Ok(Some(s)) => Ok((user, s)),
        Ok(None) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(internal_error(err).into_response()),
    }
}

/// The preview of `checked` as the build's next version.
async fn show(
    state: &AppState,
    user: &User,
    s: &Schematic,
    checked: &Checked,
    note: &str,
) -> Response {
    let loaded = async {
        anyhow::Ok((
            version::list(&state.db, &s.id).await?,
            simulation::current(&state.db, s).await?,
        ))
    };
    let (versions, current) = match loaded.await {
        Ok(loaded) => loaded,
        Err(err) => return internal_error(&*err).into_response(),
    };
    let number = versions.first().map_or(1, |v| v.version) + 1;
    pages::version_preview(
        user,
        &VersionPreview {
            s,
            data: &STANDARD.encode(&checked.prepared.data),
            comparison: checked.comparison,
            refusals: &checked.refusals,
            version: number,
            current: current.as_ref(),
            note,
        },
    )
    .into_response()
}
