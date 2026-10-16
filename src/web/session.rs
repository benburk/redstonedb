use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{app_state::AppState, error::AppError, models::session::Session};

/// How long a login lasts.
const LIFETIME: time::Duration = time::Duration::WEEK;

fn session_cookie(session_id: Uuid) -> Cookie<'static> {
    Cookie::build(("session_id", session_id.to_string()))
        .path("/")
        // Lax, not Strict: a login ends with Discord sending the browser
        // back here, and Strict would hold the cookie back on that visit.
        .same_site(SameSite::Lax)
        // Plain http is allowed only in debug builds, for local testing.
        .secure(!cfg!(debug_assertions))
        .http_only(true)
        .max_age(LIFETIME)
        .build()
}

/// Starts a session for an account and returns its cookie. Expired
/// sessions are cleared out on the way.
pub async fn create_session(
    db: &SqlitePool,
    user_id: Uuid,
    now: i64,
) -> Result<Cookie<'static>, sqlx::Error> {
    Session::delete_expired(db, now).await?;
    let lifetime = i64::try_from(LIFETIME.whole_microseconds())
        .expect("a week in microseconds fits in i64");
    let session = Session {
        id: Uuid::new_v4(),
        user_id,
        expires_at: now.saturating_add(lifetime),
    };
    Session::insert(db, &session).await?;
    Ok(session_cookie(session.id))
}

/// Logs out: deletes the session and its cookie.
pub async fn delete(
    jar: CookieJar,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(cookie) = jar.get("session_id")
        && let Ok(id) = Uuid::parse_str(cookie.value())
    {
        Session::delete_by_id(&state.db, id).await?;
    }
    Ok((jar.remove(Cookie::build("session_id")), Redirect::to("/")))
}
