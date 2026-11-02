//! Logging in with Discord.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;
use tracing::warn;

use crate::{models::user::User, web::app_state::AppState};

use super::{pages, session::create_session};

/// The cookie holding the `state` a login sent to Discord, checked when
/// Discord sends the browser back so another site cannot finish a login.
const STATE_COOKIE: &str = "discord_state";

pub async fn login(State(state): State<AppState>) -> Response {
    pages::login(state.discord.is_some(), "").into_response()
}

/// Sends the browser to Discord to approve the login.
pub async fn start(State(state): State<AppState>, jar: CookieJar) -> Response {
    let Some(discord) = &state.discord else {
        return Redirect::to("/sessions/new").into_response();
    };
    let login_state = uuid::Uuid::new_v4().simple().to_string();
    // Lax, so the browser sends it on Discord's redirect back to us.
    let cookie = Cookie::build((STATE_COOKIE, login_state.clone()))
        .path("/auth/discord")
        .same_site(SameSite::Lax)
        .secure(!cfg!(debug_assertions))
        .http_only(true)
        .max_age(time::Duration::minutes(10))
        .build();
    (
        jar.add(cookie),
        Redirect::to(&discord.authorize_url(&login_state)),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct Callback {
    code: Option<String>,
    state: Option<String>,
    /// Set when the person declined on Discord.
    error: Option<String>,
}

/// Where Discord sends the browser back: checks the state, asks Discord who
/// logged in, and starts a session for their account.
pub async fn callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(callback): Query<Callback>,
) -> Response {
    let expected = jar.get(STATE_COOKIE).map(|c| c.value().to_owned());
    let jar = jar.remove(Cookie::build(STATE_COOKIE).path("/auth/discord"));
    let refuse = |jar: CookieJar, message: &str| {
        (jar, pages::login(true, message)).into_response()
    };
    let Some(discord) = &state.discord else {
        return Redirect::to("/sessions/new").into_response();
    };
    if callback.error.is_some() {
        return refuse(jar, "The login was cancelled on Discord.");
    }
    let (Some(code), Some(returned)) = (callback.code, callback.state) else {
        return refuse(jar, "Discord sent back an incomplete login.");
    };
    if expected.as_deref() != Some(returned.as_str()) {
        return refuse(jar, "That login link has expired. Try again.");
    }
    let profile = match discord.profile(&state.http, &code).await {
        Ok(profile) => profile,
        Err(err) => {
            warn!("Discord login failed: {err:#}");
            return refuse(
                jar,
                "Discord could not confirm the login. Try again.",
            );
        }
    };
    let user = match User::sign_in(&state.db, &profile).await {
        Ok(user) => user,
        Err(err) => {
            warn!("Could not save the account: {err}");
            return refuse(jar, "Could not save your account. Try again.");
        }
    };
    // A new account sees the welcome steps, once.
    let welcome = sqlx::query_scalar!(
        "UPDATE user SET welcomed = 1 WHERE id = ? AND welcomed = 0
         RETURNING welcomed",
        user.id,
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let to = if welcome.is_some() { "/welcome" } else { "/" };
    match create_session(&state.db, user.id).await {
        Ok(cookie) => (jar.add(cookie), Redirect::to(to)).into_response(),
        Err(err) => {
            warn!("Could not start a session: {err}");
            refuse(jar, "Could not start your session. Try again.")
        }
    }
}
