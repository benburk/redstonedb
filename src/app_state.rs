use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::services::Job;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub job_tx: mpsc::UnboundedSender<Job>,
}
