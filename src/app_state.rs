use sqlx::SqlitePool;

use crate::discord;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// `None` when the Discord settings are missing, which turns login off.
    pub discord: Option<discord::Config>,
    pub http: reqwest::Client,
}
