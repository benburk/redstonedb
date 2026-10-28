use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::request::Parts,
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use tracing::{Span, warn};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    error::AppError,
    models::{session::Session, user::User},
    util::current_time_micros,
};

impl OptionalFromRequestParts<AppState> for User {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        let Some(session_id) = extract_session_id(parts, state).await else {
            return Ok(None);
        };
        let Some(session) = Session::get_by_id(&state.db, session_id).await?
        else {
            return Ok(None);
        };
        if is_session_expired(&session, session_id, &state.db).await {
            return Ok(None);
        }
        let user = User::get_by_id(&state.db, session.user_id).await?;
        Span::current().record("user_id", session.user_id.to_string());
        Ok(user)
    }
}

/// The signed-in account, for a page that needs one; anyone else is sent
/// to the login page.
pub struct SignedIn(pub User);

impl FromRequestParts<AppState> for SignedIn {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = <User as OptionalFromRequestParts<_>>::from_request_parts(
            parts, state,
        )
        .await?;
        user.map(SignedIn).ok_or(AppError::SignedOut)
    }
}

/// The signed-in account, when it is a mod's or an admin's. Anyone else
/// signed in is refused; anyone signed out is sent to the login page.
pub struct Moderator(pub User);

impl FromRequestParts<AppState> for Moderator {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let SignedIn(user) = SignedIn::from_request_parts(parts, state).await?;
        if user.can_delete() {
            Ok(Moderator(user))
        } else {
            Err(AppError::Forbidden)
        }
    }
}

/// The signed-in account, when it is an admin's. Anyone else signed in is
/// refused; anyone signed out is sent to the login page.
pub struct Admin(pub User);

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let SignedIn(user) = SignedIn::from_request_parts(parts, state).await?;
        if user.is_admin() {
            Ok(Admin(user))
        } else {
            Err(AppError::Forbidden)
        }
    }
}

async fn extract_session_id(
    parts: &mut Parts,
    state: &AppState,
) -> Option<Uuid> {
    let Ok(jar) = CookieJar::from_request_parts(parts, state).await;
    let raw = jar.get("session_id").map(Cookie::value)?;
    let Ok(session_id) = Uuid::parse_str(raw) else {
        warn!("Invalid session_id cookie");
        return None;
    };
    // The id stays out of the logs: it works like a password.
    Some(session_id)
}

async fn is_session_expired(
    session: &Session,
    session_id: Uuid,
    db: &sqlx::SqlitePool,
) -> bool {
    let current_time = current_time_micros();
    if session.expires_at <= current_time {
        if let Err(err) = Session::delete_by_id(db, session_id).await {
            warn!("Failed to delete expired session: {err}");
        }
        return true;
    }
    false
}
