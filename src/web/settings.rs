use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;
use tracing::error;

use crate::app_state::AppState;
use crate::models::user::User;
use crate::password;
use crate::services::Job;

use super::{components, pages};

pub async fn get(user_opt: Option<User>) -> impl IntoResponse {
    match user_opt {
        Some(user) => pages::settings(&user.username, user.email.as_deref())
            .into_response(),
        None => Redirect::to("/sessions/new").into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateSettingsPayload {
    new_username: Option<String>,
    current_password: Option<String>,
    new_password: Option<String>,
    email: Option<String>,
}

pub async fn patch(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Form(form): Form<UpdateSettingsPayload>,
) -> impl IntoResponse {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };

    if let Some(new_username) = form.new_username {
        return handle_username_update(&state, &user, &new_username).await;
    }

    if form.current_password.is_some() || form.new_password.is_some() {
        return handle_password_update(
            &state,
            &user,
            form.current_password.as_deref().unwrap_or(""),
            form.new_password.as_deref().unwrap_or(""),
        )
        .await;
    }

    if let Some(email) = form.email {
        return handle_email_update(&state, &user, &email).await;
    }

    Redirect::to("/settings").into_response()
}

async fn handle_username_update(
    state: &AppState,
    user: &User,
    new_username: &str,
) -> axum::response::Response {
    let username_error = password::validate_username(new_username);
    if !username_error.is_empty() {
        return components::username_form(new_username, &username_error, false)
            .into_response();
    }

    let query_result = sqlx::query!(
        "UPDATE user SET username = ? WHERE id = ?",
        new_username,
        user.id
    )
    .execute(&state.db)
    .await;

    match query_result {
        Ok(_) => components::username_form(
            new_username,
            "Username updated successfully!",
            true,
        )
        .into_response(),
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            components::username_form(
                new_username,
                "Username already taken",
                false,
            )
            .into_response()
        }
        Err(err) => {
            error!("Failed to update username: {}", err);
            components::username_form(
                new_username,
                "Failed to update username",
                false,
            )
            .into_response()
        }
    }
}

async fn handle_password_update(
    state: &AppState,
    user: &User,
    current_password: &str,
    new_password: &str,
) -> axum::response::Response {
    let password_error = password::validate_password(new_password);
    if !password_error.is_empty() {
        return components::password_form("", &password_error, false, false)
            .into_response();
    }

    let valid_login =
        User::check_login(&state.db, &user.username, current_password).await;
    if valid_login.is_none() {
        return components::password_form(
            "Current password is incorrect",
            "",
            false,
            false,
        )
        .into_response();
    }

    let new_password_hash = password::generate_hash(new_password);

    let query_result = sqlx::query!(
        "UPDATE user SET password_hash = ? WHERE id = ?",
        new_password_hash,
        user.id
    )
    .execute(&state.db)
    .await;

    match query_result {
        Ok(_) => components::password_form(
            "",
            "Password updated successfully!",
            false,
            true,
        )
        .into_response(),
        Err(err) => {
            error!("Failed to update password: {}", err);
            components::password_form(
                "",
                "Failed to update password",
                false,
                false,
            )
            .into_response()
        }
    }
}

async fn handle_email_update(
    state: &AppState,
    user: &User,
    email: &str,
) -> axum::response::Response {
    let email = email.trim();
    let email_opt = if email.is_empty() { None } else { Some(email) };

    match User::update_email(&state.db, user.id, email_opt).await {
        Ok(()) => {
            // Send verification email if email was provided
            if let Some(addr) = email_opt
                && state
                    .job_tx
                    .send(Job::SendEmail {
                        to: addr.to_owned(),
                        subject: "Verify your email".to_owned(),
                        body: "Click here to verify your email address."
                            .to_owned(),
                    })
                    .is_err()
            {
                error!("Failed to queue verification email");
            }
            components::email_form(email, "Email updated!", true)
                .into_response()
        }
        Err(err) => {
            error!("Failed to update email: {}", err);
            components::email_form(email, "Failed to update email", false)
                .into_response()
        }
    }
}
