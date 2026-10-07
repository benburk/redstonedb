use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Background jobs processed asynchronously.
#[derive(Debug)]
pub enum Job {
    /// Send an email (simulated - just logs).
    SendEmail {
        to: String,
        subject: String,
        body: String,
    },
}

/// Runs the job processor, receiving jobs from the channel.
///
/// Spawn this in main.rs:
/// ```ignore
/// let (job_tx, job_rx) = mpsc::unbounded_channel();
/// tokio::spawn(services::job::run(pool.clone(), job_rx));
/// ```
pub async fn run(_pool: SqlitePool, mut rx: mpsc::UnboundedReceiver<Job>) {
    info!("Job processor started");

    while let Some(job) = rx.recv().await {
        match job {
            Job::SendEmail { to, subject, body } => {
                info!(?to, ?subject, ?body, "Sending email (simulated)");
            }
        }
    }

    warn!("Job processor shutting down - channel closed");
}
