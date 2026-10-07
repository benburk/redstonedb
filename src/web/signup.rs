use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::{
    Form,
    response::{IntoResponse, Redirect},
};
use axum_extra::TypedHeader;
use axum_extra::extract::CookieJar;
use axum_extra::headers::UserAgent;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::error::internal_error;
use crate::models::invite_code::{InviteCode, normalize};
use crate::models::user::User;
use crate::password;
use crate::util::current_time_micros;

use super::{components, pages};

const INVALID_INVITE_MESSAGE: &str =
    "Invite code is invalid or has already been used";

pub async fn get(user: Option<User>) -> impl IntoResponse {
    let Some(_) = user else {
        return pages::signup_page("", "", "").into_response();
    };
    Redirect::to("/").into_response()
}

#[derive(Deserialize, Debug)]
pub struct InvitePayload {
    invite_code: String,
}

pub async fn check_invite(
    user: Option<User>,
    State(state): State<AppState>,
    Form(form): Form<InvitePayload>,
) -> impl IntoResponse {
    if user.is_some() {
        return Redirect::to("/").into_response();
    }

    let invite_code = normalize(&form.invite_code);
    match InviteCode::get_available(&state.db, &invite_code).await {
        Ok(Some(invite)) => {
            pages::signup_page(&form.invite_code, &invite.issuer_username, "")
                .into_response()
        }
        Ok(None) => {
            pages::signup_page(&form.invite_code, "", INVALID_INVITE_MESSAGE)
                .into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

#[derive(Deserialize, Debug)]
pub struct FormPayload {
    invite_code: String,
    username: String,
    password: String,
}

pub async fn post(
    jar: CookieJar,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Form(form): Form<FormPayload>,
) -> impl IntoResponse {
    let invite_code = normalize(&form.invite_code);
    let invite = match InviteCode::get_available(&state.db, &invite_code).await
    {
        Ok(Some(invite)) => Some(invite),
        Ok(None) => None,
        Err(err) => return internal_error(err).into_response(),
    };
    let username_message = password::validate_username(&form.username);
    let password_message = password::validate_password(&form.password);

    if invite.is_none()
        || !username_message.is_empty()
        || !password_message.is_empty()
    {
        let invite_message = if invite.is_none() {
            INVALID_INVITE_MESSAGE
        } else {
            ""
        };
        return components::signup_form(
            &form.invite_code,
            invite.as_ref().map_or("", |invite| &invite.issuer_username),
            invite_message,
            &form.username,
            &username_message,
            &password_message,
        )
        .into_response();
    }

    let created_at = current_time_micros();
    let user = User {
        id: Uuid::new_v4(),
        username: form.username.clone(),
        password_hash: password::generate_hash(&form.password),
        email: None,
        created_at,
    };

    let mut transaction = match state.db.begin().await {
        Ok(transaction) => transaction,
        Err(err) => return internal_error(err).into_response(),
    };
    match User::insert(&mut *transaction, &user).await {
        Ok(()) => {}
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            return components::signup_form(
                &form.invite_code,
                &invite.expect("invite was checked").issuer_username,
                "",
                &form.username,
                "Username already taken",
                "",
            )
            .into_response();
        }
        Err(err) => return internal_error(err).into_response(),
    }

    match InviteCode::redeem(
        &mut *transaction,
        &invite_code,
        user.id,
        created_at,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            return components::signup_form(
                &form.invite_code,
                "",
                INVALID_INVITE_MESSAGE,
                &form.username,
                "",
                "",
            )
            .into_response();
        }
        Err(err) => return internal_error(err).into_response(),
    }
    if let Err(err) = transaction.commit().await {
        return internal_error(err).into_response();
    }

    match super::session::create_session(
        &state.db,
        user.id,
        created_at,
        addr.to_string(),
        user_agent,
    )
    .await
    {
        Ok(cookie) => (jar.add(cookie), Redirect::to("/")).into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}
