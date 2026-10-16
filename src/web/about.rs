use axum::{
    extract::State,
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState, error::AppError, models::schematic, models::user::User,
};

use super::pages;

pub async fn about(user_opt: Option<User>) -> impl IntoResponse {
    pages::about(user_opt.as_ref())
}

/// How builds are sorted: categories, designs, measures, and the rest,
/// with the standard categories as they stand.
pub async fn categories(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let categories = schematic::categories(&state.db).await?;
    Ok(pages::sorting(user_opt.as_ref(), &categories).into_response())
}
