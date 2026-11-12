//! A build's new version, chosen on its edit page and saved with its
//! details there; its earlier versions' files; restoring one; and the diff
//! between two. Whether a file may be a new version is `crate::version`'s
//! to say, and keeping it `models::version`'s.

use axum::{
    extract::{Path, Query, State},
    http::Method,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    diff::{self, Counts},
    import,
    models::{
        schematic, similarity, simulation,
        user::User,
        version::{self, Replacing, Version},
    },
    schem::standardize,
    web::{
        app_state::AppState,
        error::AppError,
        extractors::{
            client_ip::ClientIp,
            session::{Moderator, SignedIn},
        },
    },
};

use super::{
    pages::Reply,
    schematics::{EditView, editable, show_edit},
};

/// The edit page, its form for choosing a new version open.
pub async fn form(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = editable(&state.db, &user, &id).await?;
    let view = EditView {
        reply: Reply::Choosing(None),
        ..EditView::default()
    };
    show_edit(&state.db, &user, &s, view).await
}

/// An earlier version's file, under the build's filename with its number,
/// to an account alone.
pub async fn download(
    Path((id, number)): Path<(String, i64)>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = schematic::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let data = version::data(&state.db, &id, number)
        .await?
        .ok_or(AppError::NotFound)?;
    schematic::log_download(
        &state.db,
        &id,
        user.id,
        Some(number),
        schematic::DownloadKind::Version,
    )
    .await?;
    let filename = s.filename().replace(".schem", &format!("_v{number}.schem"));
    Ok(super::attachment(&filename, data))
}

/// Restores build `id`'s version `number`, by its uploader or a mod: a new
/// version copying its file and the roles kept with it, which is tested
/// again, the history kept whole.
pub async fn restore(
    Path((id, number)): Path<(String, i64)>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    ClientIp(ip): ClientIp,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = schematic::get(db, &id).await?.ok_or(AppError::NotFound)?;
    if !user.may_restore(&s) {
        return Err(AppError::Forbidden);
    }
    let current = version::list(db, &id).await?.first().map(|v| v.version);
    if current == Some(number) {
        return Err(AppError::BadRequest(
            "That version is the current one.".to_owned(),
        ));
    }
    let file = version::file(db, &id, number)
        .await?
        .ok_or(AppError::NotFound)?;
    let prepared = import::prepare_for(&file, import::Purpose::of(&s, false))
        .map_err(|err| AppError::BadRequest(format!("{err:#}")))?;
    let note = format!("Restored version {number}");
    let how = Replacing::NextVersion {
        added_by: Some(user.id),
        added_from: ip.as_deref(),
        note: Some(&note),
        restores: Some(number),
    };
    let restored = version::replace(db, &id, &prepared, how).await?;
    if let Some(colours) = version::colours_of(db, &id, number).await? {
        version::set_colours(db, &id, restored, &colours).await?;
    }
    tracing::info!(id, by = user.username, ip, number, "restored a version");
    simulation::complete_soon(db, &id);
    similarity::refresh_soon(db);
    Ok(Redirect::to(&format!("/schematics/{id}")).into_response())
}

/// What merging a build into another names.
#[derive(Deserialize)]
pub struct Merging {
    /// The build to merge into, by its link or id.
    into: String,
    #[serde(default)]
    note: String,
}

/// Merges build `id` into the one `merging` names, by a mod: its current
/// file goes to that build as a new version, if it may, and it is deleted,
/// as a fix uploaded as a build of its own is folded into the one it fixes.
/// Goes to the build it went into.
pub async fn merge(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Moderator(by): Moderator,
    axum::Form(merging): axum::Form<Merging>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let into = merging
        .into
        .split(|c: char| !c.is_ascii_hexdigit())
        .find(|part| part.len() == 32)
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            AppError::BadRequest(
                "Give the build to merge into by its link or id.".to_owned(),
            )
        })?;
    let from = schematic::get(db, &id).await?.ok_or(AppError::NotFound)?;
    let Some(into) = schematic::get(db, &into).await? else {
        return Err(AppError::BadRequest(
            "There is no build to merge into at that link.".to_owned(),
        ));
    };
    let note = match merging.note.trim() {
        "" => format!("Merged from {}, {}", from.name(), &from.id[..8]),
        note => note.to_owned(),
    };
    match crate::version::merge(db, &from, &into, Some(by.id), &note).await? {
        Ok(number) => {
            tracing::info!(
                from = id,
                into = into.id,
                by = by.username,
                number,
                "merged a build"
            );
            simulation::complete_soon(db, &into.id);
            similarity::refresh_soon(db);
            Ok(Redirect::to(&format!("/schematics/{}", into.id))
                .into_response())
        }
        Err(refusals) => Err(AppError::BadRequest(refusals.join(" "))),
    }
}

/// Fills in what each of `versions`, newest first, changed from the one
/// before, where it was not yet counted, and keeps it.
pub async fn count_changes(
    db: &SqlitePool,
    id: &str,
    versions: &mut [Version],
) -> Result<(), AppError> {
    for i in 0..versions.len().saturating_sub(1) {
        if versions[i].changes.is_some() {
            continue;
        }
        let (newer, older) = (versions[i].version, versions[i + 1].version);
        let counts = changes(db, id, older, newer).await?;
        version::set_changes(db, id, newer, counts).await?;
        versions[i].changes = Some(counts);
    }
    Ok(())
}

/// What build `id`'s version `newer` changed from version `older`.
async fn changes(
    db: &SqlitePool,
    id: &str,
    older: i64,
    newer: i64,
) -> Result<Counts, AppError> {
    let (old, new) = files(db, id, older, newer).await?;
    let diff = diff::diff(&old, &new).map_err(AppError::from)?;
    Ok(diff.counts)
}

/// The files of build `id`'s versions `older` and `newer`; a 404 if
/// either is missing.
async fn files(
    db: &SqlitePool,
    id: &str,
    older: i64,
    newer: i64,
) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let old = version::data(db, id, older)
        .await?
        .ok_or(AppError::NotFound)?;
    let new = version::data(db, id, newer)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((old, new))
}

/// Which two versions a diff compares: `from`, the older, and `to`, by
/// default the version after `from` and the one before it.
#[derive(Deserialize)]
pub struct Between {
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// Set when the 3D viewer fetches a side, which is no download.
    pub preview: Option<String>,
}

impl Between {
    /// The two versions, older first, of a build whose versions are
    /// `versions`, newest first; by default the current one and the one
    /// before. `None` when they are not two of its versions.
    fn pick(&self, versions: &[Version]) -> Option<(i64, i64)> {
        let numbers: Vec<i64> = versions.iter().map(|v| v.version).collect();
        let current = *numbers.first()?;
        let to = self.to.unwrap_or_else(|| {
            self.from.map_or(current, |from| (from + 1).min(current))
        });
        let from = self.from.unwrap_or(to - 1);
        let (from, to) = (from.min(to), from.max(to));
        (from != to && numbers.contains(&from) && numbers.contains(&to))
            .then_some((from, to))
    }
}

/// The page comparing two versions of build `id`: the older with what the
/// newer takes out, the newer with what it adds, swapped in one viewer.
/// Anyone who may see the build may see it.
pub async fn diff_page(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
    Query(between): Query<Between>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = schematic::get(db, &id).await?.ok_or(AppError::NotFound)?;
    let versions = version::list(db, &id).await?;
    let (from, to) = between.pick(&versions).ok_or(AppError::NotFound)?;
    let counts = changes(db, &id, from, to).await?;
    Ok(super::pages::diff_page(
        viewer.as_ref(),
        &s,
        &versions,
        (from, to),
        counts,
    )
    .into_response())
}

/// One side of a diff, `before` or `after`, as a file: for the viewer to
/// anyone who may see the build, and to download to an account alone,
/// logged as that side's version.
pub async fn diff_file(
    Path((id, side)): Path<(String, String)>,
    State(state): State<AppState>,
    viewer: Option<User>,
    method: Method,
    Query(between): Query<Between>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let preview = between.preview.is_some();
    if !preview && viewer.is_none() {
        return Err(AppError::SignedOut);
    }
    let s = schematic::get(db, &id).await?.ok_or(AppError::NotFound)?;
    let versions = version::list(db, &id).await?;
    let (from, to) = between.pick(&versions).ok_or(AppError::NotFound)?;
    let (old, new) = files(db, &id, from, to).await?;
    let diff = diff::diff(&old, &new).map_err(AppError::from)?;
    let (data, number) = match side.as_str() {
        "before" => (diff.before, from),
        "after" => (diff.after, to),
        _ => return Err(AppError::NotFound),
    };
    if preview {
        let data = standardize::for_viewer(&data).unwrap_or(data);
        return Ok(super::attachment("diff.schem", data));
    }
    if let (Some(user), true) = (&viewer, method == Method::GET) {
        schematic::log_download(
            db,
            &id,
            user.id,
            Some(number),
            schematic::DownloadKind::Diff,
        )
        .await?;
    }
    let stem = s.filename().trim_end_matches(".schem").to_owned();
    let filename = format!("{stem}_v{from}_to_v{to}_{side}.schem");
    Ok(super::attachment(&filename, data))
}
