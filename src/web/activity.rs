//! The moderation log, for admins, where an edit's description
//! or credits can be put back, and a deleted build restored.

use std::collections::HashMap;

use axum::{
    Form,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    account::User,
    build::{Change, Restorable, revertible},
    store::{
        activity::{self, Filter},
        builds,
    },
    web::{app_state::AppState, error::AppError, extractors::session::Admin},
};

use super::pages;

/// How many events a page of the log shows.
const PAGE: i64 = 100;

/// Which events to show, by the filter's name, and from when.
#[derive(Deserialize)]
pub struct Shown {
    #[serde(default)]
    show: String,
    /// The newest time to show, as a page's Older link gives it.
    before: Option<String>,
}

impl Shown {
    /// The time the page starts from, if it names one.
    fn before(&self) -> Option<&str> {
        self.before.as_deref().filter(|b| !b.is_empty())
    }
}

/// The log, newest first, filtered to one kind of event when its button
/// is chosen.
pub async fn show(
    State(state): State<AppState>,
    Admin(viewer): Admin,
    Query(shown): Query<Shown>,
) -> Result<Response, AppError> {
    render(&state, &viewer, &shown, "").await
}

/// What a revert sends: the detail to put back, and the log page it was
/// sent from, to return to.
#[derive(Deserialize)]
pub struct Revert {
    detail: String,
    #[serde(flatten)]
    shown: Shown,
}

/// Puts back a detail as it was before edit `id`, then returns to the log
/// page it was sent from; a refusal shows that page saying why.
pub async fn revert(
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Admin(viewer): Admin,
    Form(form): Form<Revert>,
) -> Result<Response, AppError> {
    match put_back(&state.db, id, &form.detail, &viewer).await? {
        Ok(build) => {
            tracing::info!(
                build,
                edit = id,
                detail = form.detail,
                by = viewer.username,
                "reverted an edit"
            );
            let filter = Filter::named(&form.shown.show);
            let back = pages::activity_href(filter, form.shown.before());
            Ok(Redirect::to(&back).into_response())
        }
        Err(message) => render(&state, &viewer, &form.shown, message).await,
    }
}

/// Restores deleted build `id`, then returns to the log page it was sent
/// from.
pub async fn restore(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Admin(viewer): Admin,
    Form(shown): Form<Shown>,
) -> Result<Response, AppError> {
    if !builds::undelete(&state.db, &id, viewer.id).await? {
        let message = "That build is not deleted.";
        return render(&state, &viewer, &shown, message).await;
    }
    tracing::info!(id, by = viewer.username, "restored a build");
    let back = pages::activity_href(Filter::named(&shown.show), shown.before());
    Ok(Redirect::to(&back).into_response())
}

/// The log page `shown` names, saying `message` if it isn't empty.
async fn render(
    state: &AppState,
    viewer: &User,
    shown: &Shown,
    message: &str,
) -> Result<Response, AppError> {
    let db = &state.db;
    let filter = Filter::named(&shown.show);
    let page = activity::page(db, filter, shown.before(), PAGE).await?;
    let mut users: HashMap<_, _> = User::all(db)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect();
    // Accounts listed among no others, as RedstoneDB is.
    for id in page.events.iter().filter_map(|e| e.user) {
        if !users.contains_key(&id)
            && let Some(user) = User::get_by_id(db, id).await?
        {
            users.insert(id, user);
        }
    }
    let mut ids: Vec<&str> = page
        .events
        .iter()
        .filter_map(|e| e.schematic.as_deref())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let builds = builds::get_many_with_deleted(db, &ids)
        .await?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    let log = pages::ActivityLog {
        viewer,
        filter,
        before: shown.before(),
        message,
        page: &page,
        users: &users,
        builds: &builds,
    };
    Ok(pages::activity(&log).into_response())
}

/// Puts back `detail` as it was before edit `edit`, by `by`, keeping that
/// as an edit of its own; the build's id, or a sentence saying why it
/// cannot be put back.
async fn put_back(
    db: &SqlitePool,
    edit: i64,
    detail: &str,
    by: &User,
) -> Result<Result<String, &'static str>, sqlx::Error> {
    let Some((build_id, changes)) = activity::edit(db, edit).await? else {
        return Ok(Err("That edit is not in the log."));
    };
    let Some(change) = changes.into_iter().find(|c| c.detail == detail) else {
        return Ok(Err("That edit did not change that."));
    };
    let Some(restorable) = Restorable::named(detail) else {
        return Ok(Err(
            "Only a description, authors, or contributors can be put back.",
        ));
    };
    let Some(s) = builds::get(db, &build_id).await? else {
        return Ok(Err("That build is gone."));
    };
    if !revertible(&s, &change) {
        return Ok(Err(
            "It has changed since that edit, so it was left as it is; edit the build to change it.",
        ));
    }
    builds::restore(db, &s.id, restorable, change.from.as_deref()).await?;
    let undone = Change {
        detail: change.detail,
        from: change.to,
        to: change.from,
    };
    activity::log_edit(db, &s.id, by.id, &[undone]).await?;
    Ok(Ok(s.id))
}
