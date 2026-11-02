//! Signing in without Discord, to try the site on a local server: a route
//! only a debug build has, as `just serve` runs, and never the release
//! build the live site runs.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::{
    models::user::{DiscordProfile, Role, User},
    util::current_time_micros,
    web::{app_state::AppState, error::AppError},
};

use super::session::create_session;

/// What `/dev/login` asks for, such as `?role=user&next=/upload`.
#[derive(Deserialize)]
pub struct Login {
    /// `user`, `mod`, or `admin`; admin when none is given.
    role: Option<String>,
    /// A path on the site to go to once signed in; the home page when none
    /// is given.
    next: Option<String>,
}

/// Signs in as the account `debug`, made the first time, given the role
/// asked for, and goes on to `next`.
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(login): Query<Login>,
) -> Result<Response, AppError> {
    let role = match login.role.as_deref() {
        None => Role::Admin,
        Some(text) => Role::parse(text).ok_or(AppError::NotFound)?,
    };
    let profile = DiscordProfile {
        id: "debug".to_owned(),
        username: "debug".to_owned(),
        display_name: Some("Debug".to_owned()),
        avatar: None,
    };
    let user = User::sign_in(&state.db, &profile).await?;
    User::set_role(&state.db, user.id, role).await?;
    let cookie =
        create_session(&state.db, user.id, current_time_micros()).await?;
    // Only a path on this site, so the link cannot lead off it.
    let next = login
        .next
        .filter(|next| next.starts_with('/') && !next.starts_with("//"))
        .unwrap_or_else(|| "/".to_owned());
    Ok((jar.add(cookie), Redirect::to(&next)).into_response())
}
