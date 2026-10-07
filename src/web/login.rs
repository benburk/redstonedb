use axum::response::{IntoResponse, Redirect};

use crate::models::user::User;

use super::{components, pages};

pub async fn get(user: Option<User>) -> impl IntoResponse {
    let Some(_) = user else {
        return pages::login_page().into_response();
    };
    Redirect::to("/").into_response()
}

pub fn login_form(username: &str, error_message: &str) -> impl IntoResponse {
    components::login_form(username, error_message)
}
