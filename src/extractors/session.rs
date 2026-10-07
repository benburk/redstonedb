use axum::{
    extract::{FromRequestParts as _, OptionalFromRequestParts},
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use sqlx::Error;
use tracing::{Span, debug, warn};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    error::internal_error,
    models::{session::Session, user::User},
    util::current_time_micros,
};

impl OptionalFromRequestParts<AppState> for User {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        let maybe_session_id = extract_session_id(parts, state).await?;
        let Some(session_id) = maybe_session_id else {
            return Ok(None);
        };

        let maybe_session = load_session(&state.db, session_id).await?;
        let Some(session) = maybe_session else {
            return Ok(None);
        };

        if is_session_expired(&session, session_id, &state.db).await {
            return Ok(None);
        }

        load_user(&state.db, session.user_id).await
    }
}

async fn extract_session_id(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<Uuid>, (StatusCode, String)> {
    let Ok(jar) = CookieJar::from_request_parts(parts, state).await;

    let Some(raw) = jar.get("session_id").map(Cookie::value) else {
        return Ok(None);
    };

    let Ok(session_id) = Uuid::parse_str(raw) else {
        warn!("Invalid session_id cookie");
        return Ok(None);
    };

    debug!(%session_id, "Session cookie found");
    Span::current().record("session_id", session_id.to_string());
    Ok(Some(session_id))
}

async fn load_session(
    db: &sqlx::SqlitePool,
    session_id: Uuid,
) -> Result<Option<Session>, (StatusCode, String)> {
    match Session::get_by_id(db, session_id).await {
        Ok(session) => Ok(session),
        Err(err) => Err(internal_error(err)),
    }
}

async fn is_session_expired(
    session: &Session,
    session_id: Uuid,
    db: &sqlx::SqlitePool,
) -> bool {
    let current_time = current_time_micros();
    if session.expires_at <= current_time {
        warn!("Session {session_id} has expired");
        if let Err(err) = Session::delete_by_id(db, session_id).await {
            warn!("Failed to delete expired session: {err}");
        }
        return true;
    }
    false
}

async fn load_user(
    db: &sqlx::SqlitePool,
    user_id: Uuid,
) -> Result<Option<User>, (StatusCode, String)> {
    match User::get_by_id(db, user_id).await {
        Ok(user) => {
            Span::current().record("user_id", user_id.to_string());
            Ok(Some(user))
        }
        Err(Error::RowNotFound) => Ok(None),
        Err(err) => Err(internal_error(err)),
    }
}
