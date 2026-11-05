//! The moderation log, for mods and admins.

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    models::{
        activity::{self, Filter},
        schematic,
        user::User,
    },
    web::{
        app_state::AppState, error::AppError, extractors::session::Moderator,
    },
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

/// The log, newest first, filtered to one kind of event when its button
/// is chosen.
pub async fn show(
    State(state): State<AppState>,
    Moderator(viewer): Moderator,
    Query(Shown { show, before }): Query<Shown>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let filter = Filter::named(&show);
    let page = activity::page(
        db,
        filter,
        before.as_deref().filter(|b| !b.is_empty()),
        PAGE,
    )
    .await?;
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
    let builds = schematic::get_many(db, &ids)
        .await?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    let log = pages::ActivityLog {
        viewer: &viewer,
        filter,
        page: &page,
        users: &users,
        builds: &builds,
    };
    Ok(pages::activity(&log).into_response())
}
