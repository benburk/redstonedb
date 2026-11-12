//! Editing a build's details and tags, with a new version when one is
//! chosen.

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sqlx::SqlitePool;

use crate::{
    account::User,
    build::{self, Build, Prepared},
    category::{self, Category},
    ingest::{
        details, jobs, prepare, publish, results, similar,
        version::{Checked, check as check_version},
    },
    schem::Blocks,
    store::{
        activity, builds,
        versions::{self, Replacing},
    },
    web::{
        app_state::AppState,
        error::AppError,
        extractors::{client_ip::ClientIp, session::SignedIn},
        href,
        submit::{
            analysis,
            details_view::{File, Reply},
            form::{self, Form},
            upload, upload_view,
        },
    },
};

/// The page where a mod, or one of the build's confirmed authors, edits
/// its details and tags.
pub async fn edit(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = editable(&state.db, &user, &id).await?;
    show_edit(&state.db, &user, &s, EditView::default()).await
}

/// Build `id`, when `user` may edit it: a mod, one of its confirmed
/// authors, or its uploader. A 404 for no build, or a deleted one, and a
/// 403 for anyone else.
pub(in crate::web) async fn editable(
    db: &SqlitePool,
    user: &User,
    id: &str,
) -> Result<Build, AppError> {
    let s = builds::get(db, id).await?.ok_or(AppError::NotFound)?;
    if user.may_edit(&s) {
        Ok(s)
    } else {
        Err(AppError::Forbidden)
    }
}

/// Saves an edit of a build's details and tags, by a mod or one of its
/// authors, then shows its page; a refusal shows the edit page again with
/// the values sent.
pub async fn save(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    ClientIp(ip): ClientIp,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = editable(db, &user, &id).await?;
    let mut posted = match upload::posted(&mut multipart).await {
        Ok(posted) => posted,
        Err(message) => {
            let view = EditView {
                reply: Reply::Choosing(Some(&message)),
                ..EditView::default()
            };
            return show_edit(db, &user, &s, view).await;
        }
    };
    let form = Form::read(&posted.pairs);
    let note: String = posted.get("note").trim().chars().take(500).collect();
    // A new version, when the page carried one, is checked again first.
    let checked = match attached(db, &s, posted.get("data")).await {
        Ok(checked) => checked,
        Err(message) => {
            let view = EditView {
                sent: Some(&form),
                reply: Reply::Choosing(Some(&message)),
                ..EditView::default()
            };
            return show_edit(db, &user, &s, view).await;
        }
    };
    // A file chosen is checked in the background, its page showing how far
    // that has got, and then the edit page with it waiting.
    if posted.choosing() {
        let chosen = match posted.fetch_own(db, &user).await {
            Ok(()) => posted.chosen().await,
            Err(message) => Err(message),
        };
        let file = match chosen {
            Ok(chosen) => chosen.file.to_vec(),
            // A file that could not be read leaves the one waiting, if any.
            Err(message) => {
                let view = EditView {
                    sent: Some(&form),
                    new_file: checked.as_ref().map(|checked| NewFile {
                        checked,
                        note: &note,
                    }),
                    reply: Reply::Choosing(Some(&message)),
                };
                return show_edit(db, &user, &s, view).await;
            }
        };
        let (db, build) = (db.clone(), s.clone());
        let check = analysis::start_version(
            &state,
            &user,
            id.clone(),
            posted.pairs,
            async move { chosen_version(&db, &build, &file).await },
        );
        return Ok(Redirect::to(&format!(
            "/schematics/{id}/versions/check/{check}"
        ))
        .into_response());
    }
    let (kept, refused) =
        save_details(db, &user, &s, &form, checked.as_ref()).await?;
    if let Some(message) = refused {
        let view = EditView {
            sent: Some(&form),
            new_file: checked.as_ref().map(|checked| NewFile {
                checked,
                note: &note,
            }),
            reply: Reply::NotSaved(&message),
        };
        return show_edit(db, &user, &s, view).await;
    }
    let new_prepared = kept.as_ref().or(checked.as_ref().map(|c| &c.prepared));
    if form.public != s.entry.public {
        builds::set_public(db, &id, form.public).await?;
    }
    // What the edit changed, before a new version changes what its file
    // sets.
    if let Some(after) = builds::get(db, &id).await? {
        let changed = build::changed(&s, &after);
        activity::log_edit(db, &id, user.id, &changed).await?;
    }
    if let Some(prepared) = new_prepared {
        let how = Replacing::NextVersion {
            added_by: Some(user.id),
            added_from: ip.as_deref(),
            note: Some(note.as_str()).filter(|note| !note.is_empty()),
            restores: None,
        };
        versions::replace(db, &id, prepared, how).await?;
        tracing::info!(id, by = user.username, ip, "new version");
    }
    // A new file, category, or width may call for a test, or another.
    jobs::complete_soon(db, &id);
    similar::refresh_soon(db);
    Ok(Redirect::to(&href::build(&id)).into_response())
}

/// `file`, newly chosen on build `s`'s edit page, checked as its next
/// version, or why it could not be read.
async fn chosen_version(
    db: &SqlitePool,
    s: &Build,
    file: &[u8],
) -> Result<Checked, String> {
    // A listed build that follows the conventions is kept standardized;
    // one unverified or off the list as built, until a version it is
    // recognized by verifies it.
    let listed = s.kind().is_some();
    let standardize = s.file.conventional && listed && s.unverified.is_none();
    check_version(db, s, file, standardize)
        .await
        .map_err(|err| format!("Could not read that file: {err:#}."))
}

/// The value of `key` among a form's `pairs`, as sent; empty when it was
/// not.
fn sent<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, value)| value.as_str())
}

/// Build `id`'s new version check `check`: how far it has got while it
/// runs, then the edit page with the details as sent and the version
/// waiting, or, for a file that can't be one, why, the one waiting before
/// it, if any, waiting still. A check gone, or someone else's, goes back
/// to choosing a file.
pub async fn checked(
    Path((id, check)): Path<(String, String)>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = editable(db, &user, &id).await?;
    let choose = || {
        Ok(Redirect::to(&format!("/schematics/{id}/versions/new"))
            .into_response())
    };
    let Some((_, done)) = check
        .parse()
        .ok()
        .and_then(|check| analysis::get(&state, check, &user))
    else {
        return choose();
    };
    let Some(done) = done else {
        return Ok(
            upload_view::checking_version(&user, &s, &check).into_response()
        );
    };
    let analysis::Done::Version(done) = done else {
        return choose();
    };
    if done.schematic != id {
        return choose();
    }
    let pairs = &done.pairs;
    let form = Form::read(pairs);
    let note: String = sent(pairs, "note").trim().chars().take(500).collect();
    let waiting = match &done.checked {
        Ok(_) => None,
        Err(_) => attached(db, &s, sent(pairs, "data")).await.ok().flatten(),
    };
    let (checked, refused) = match &done.checked {
        Ok(checked) => (Some(checked), None),
        Err(message) => (waiting.as_ref(), Some(message.as_str())),
    };
    let view = EditView {
        sent: Some(&form),
        new_file: checked.map(|checked| NewFile {
            checked,
            note: &note,
        }),
        reply: Reply::Choosing(refused),
    };
    show_edit(db, &user, &s, view).await
}

/// How far build `id`'s version check `check` has got, for its page to
/// redraw as it polls; once it is done, or gone, an `FX-Redirect` to its
/// page.
pub async fn check_progress(
    Path((id, check)): Path<(String, String)>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let running = check
        .parse()
        .ok()
        .and_then(|check| analysis::get(&state, check, &user))
        .is_some_and(|(_, done)| done.is_none());
    if !running {
        let page = format!("/schematics/{id}/versions/check/{check}");
        return Ok(
            (StatusCode::NO_CONTENT, [("FX-Redirect", page)]).into_response()
        );
    }
    Ok(upload_view::version_steps().into_response())
}

/// The new version an edit page carried in `data`, base64, checked against
/// build `s`; none for an empty `data`. The `Err` says the file cannot be
/// read, for the page to ask for it again.
async fn attached(
    db: &SqlitePool,
    s: &Build,
    data: &str,
) -> Result<Option<Checked>, String> {
    if data.is_empty() {
        return Ok(None);
    }
    let checked = match STANDARD.decode(data) {
        Ok(data) => check_version(db, s, &data, false).await,
        Err(_) => Err(anyhow::anyhow!("it did not arrive whole")),
    };
    checked.map(Some).map_err(|err| {
        format!(
            "Could not read the new version: {err:#}. Choose its file again."
        )
    })
}

/// Checks and stores `user`'s edit of build `s`, with the new version
/// `checked` when one waits: the file it is kept as, when that differs
/// from the one checked, and why the edit was refused, if it was.
async fn save_details(
    db: &SqlitePool,
    user: &User,
    s: &Build,
    form: &Form,
    checked: Option<&Checked>,
) -> Result<(Option<Prepared>, Option<String>), AppError> {
    if checked.is_some_and(|c| !c.refusals.is_empty()) {
        let why = "The new version can't replace the build, as the checks say.";
        return Ok((None, Some(why.to_owned())));
    }
    let kept = match checked {
        Some(checked) => {
            version_kept(s, &form.category, &checked.prepared).await?
        }
        None => None,
    };
    let new = kept.as_ref().or(checked.map(|c| &c.prepared));
    let refused = saved(db, user, s, form, new).await?.err();
    Ok((kept, refused))
}

/// The file a new version, `prepared`, of build `s` is kept as, when that
/// differs from the one given: standardized and turned to lie the standard
/// way, as an upload's is, for a build unverified as its category whose
/// new version is recognized as one, and kept as it stands for a build
/// recognized already. Only a build whose category, as `category` gives
/// it, is unchanged is verified by its version.
async fn version_kept(
    s: &Build,
    category: &str,
    prepared: &Prepared,
) -> anyhow::Result<Option<Prepared>> {
    let categories = Category::all();
    let Some(listed) = Category::find(&categories, category) else {
        return Ok(None);
    };
    if listed.name == s.category
        && !s.verified()
        && let Some(verified) =
            publish::verified_by(listed, prepared, s.file.keeps_paste).await?
    {
        return Ok(Some(verified));
    }
    let evaluated = category::behaviour(&listed.name).evaluated;
    if evaluated && prepared.conventional && s.unverified.is_none() {
        return Ok(Some(prepare::rewrapped(prepared)?));
    }
    Ok(None)
}

/// Checks and stores `user`'s edit of build `s`, or says why it was
/// refused. A new version's file, `new`, is what the stacking must fit,
/// when there is one.
async fn saved(
    db: &SqlitePool,
    user: &User,
    s: &Build,
    form: &Form,
    new: Option<&Prepared>,
) -> Result<Result<(), String>, AppError> {
    let mut given = match form.parse() {
        Ok(given) => given,
        Err(message) => return Ok(Err(message)),
    };
    let categories = Category::all();
    let same = Category::name_of(&categories, &form.category) == s.category;
    // Its details saved alone, under its category, leave it as verified as
    // it was; a new version, or another category, is recognized afresh.
    if new.is_none() && same {
        given.unverified.clone_from(&s.unverified);
    }
    let stored;
    let build = if let Some(new) = new {
        details::Candidate::of(new)
    } else {
        stored = versions::current_file(db, &s.id).await?.unwrap_or_default();
        // Its file, kept under its category, passed its test already,
        // which a save of its details needn't run again.
        details::Candidate {
            data: &stored,
            conventional: s.file.conventional,
            passes: same
                && results::current(db, s)
                    .await?
                    .is_some_and(|report| report.passed()),
        }
    };
    match details::check(db, given, build, user.can_create_tags()).await? {
        Ok(ready) => Ok(Ok(builds::set_details(db, &s.id, &ready).await?)),
        Err(message) => Ok(Err(message)),
    }
}

/// A new version waiting on the edit page, to save with the details.
pub(in crate::web) struct NewFile<'a> {
    pub checked: &'a Checked,
    pub note: &'a str,
}

/// What a build's edit page shows of what was last sent from it.
#[derive(Default)]
pub(in crate::web) struct EditView<'a> {
    /// The details as sent; `None` shows them as stored.
    pub sent: Option<&'a Form>,
    /// The file a new version would replace the build's with, if one is
    /// waiting.
    pub new_file: Option<NewFile<'a>>,
    pub reply: Reply<'a>,
}

/// The edit page for build `s`, showing `view`.
pub(in crate::web) async fn show_edit(
    db: &SqlitePool,
    user: &User,
    s: &Build,
    view: EditView<'_>,
) -> Result<Response, AppError> {
    let EditView {
        sent,
        new_file,
        reply,
    } = view;
    let versions = versions::list(db, &s.id).await?;
    let simulation = results::current(db, s).await?;
    let stored = Form::of(s);
    // What the page shows is the new file's, when one is waiting.
    let (encoded, stored_blocks);
    let (file, blocks) = if let Some(new) = &new_file {
        encoded = STANDARD.encode(&new.checked.prepared.data);
        let file = File::Version {
            s,
            data: &encoded,
            comparison: new.checked.comparison,
            refusals: &new.checked.refusals,
            warnings: &new.checked.warnings,
            version: versions.first().map_or(1, |v| v.version) + 1,
            note: new.note,
        };
        (file, Some(&new.checked.prepared.blocks))
    } else {
        let data = versions::current_file(db, &s.id).await?.unwrap_or_default();
        stored_blocks = Blocks::read(&data).ok();
        let file = File::Stored {
            s,
            simulation: simulation.as_ref(),
        };
        (file, stored_blocks.as_ref())
    };
    let shown = form::Shown {
        file,
        blocks,
        reply,
    };
    form::page(db, user, sent.unwrap_or(&stored), shown).await
}

/// The edit page, its form for choosing a new version open.
pub async fn new_version(
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
