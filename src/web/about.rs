use axum::response::IntoResponse;

use crate::models::user::User;

use super::pages;

pub async fn about(user_opt: Option<User>) -> impl IntoResponse {
    pages::about(user_opt.as_ref())
}
