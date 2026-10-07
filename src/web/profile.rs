use std::time::{Duration, SystemTime};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
};

use crate::app_state::AppState;
use crate::models::{session::Session, user::User};

use super::{components::SessionDisplay, pages};

pub async fn profile(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> impl IntoResponse {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };

    if user.username != username {
        return Redirect::to(&format!("/users/{}", user.username))
            .into_response();
    }

    let sessions = match Session::get_by_user_id(&state.db, user.id).await {
        Ok(sessions) => sessions
            .into_iter()
            .map(|session| SessionDisplay {
                id: session.id.to_string(),
                ip_address: session.ip_address,
                user_agent: truncate_user_agent(&session.user_agent),
                created_at: format_timestamp(session.created_at),
                expires_at: format_timestamp(session.expires_at),
                is_current: false,
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    pages::profile(&user.username, &sessions).into_response()
}

fn truncate_user_agent(user_agent: &str) -> String {
    if user_agent.len() > 50 {
        let mut truncated = user_agent.chars().take(47).collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        user_agent.to_owned()
    }
}

#[allow(
    clippy::allow_attributes,
    reason = "Need to allow specific arithmetic operations for time calculations"
)]
fn format_timestamp(timestamp_micros: i64) -> String {
    if timestamp_micros <= 0 {
        return "Invalid timestamp".to_owned();
    }

    let Ok(timestamp_secs) =
        u64::try_from(timestamp_micros.saturating_div(1_000_000))
    else {
        return "Invalid timestamp".to_owned();
    };

    let Some(dt) =
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(timestamp_secs))
    else {
        return "Invalid timestamp".to_owned();
    };

    let Ok(elapsed) = dt.elapsed() else {
        return format!("Future: {timestamp_secs}");
    };

    let total_secs = elapsed.as_secs();

    #[allow(
        clippy::integer_division,
        reason = "We want integer division for time calculations"
    )]
    #[allow(
        clippy::integer_division_remainder_used,
        reason = "Standard time calculation pattern"
    )]
    {
        let days = total_secs / 86400;
        let hours = (total_secs % 86400) / 3600;
        let minutes = (total_secs % 3600) / 60;

        if days > 0 {
            return format!("{days} days ago");
        }
        if hours > 0 {
            return format!("{hours} hours ago");
        }
        if minutes > 0 {
            return format!("{minutes} minutes ago");
        }
    }

    "Just now".to_owned()
}
