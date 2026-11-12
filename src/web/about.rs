//! The About pages: what the site is, how it recognizes builds, and the
//! work it is built on.

use axum::response::IntoResponse;

use crate::account::User;

use super::pages;

pub async fn about(user_opt: Option<User>) -> impl IntoResponse {
    pages::about(user_opt.as_ref())
}

/// How builds are recognized: each category's input and output contract,
/// and how stacking is found.
pub async fn recognized(user_opt: Option<User>) -> impl IntoResponse {
    pages::recognized(user_opt.as_ref())
}

/// The open-source work the site is built on, and its licenses.
pub async fn credits(user_opt: Option<User>) -> impl IntoResponse {
    pages::credits(user_opt.as_ref())
}
