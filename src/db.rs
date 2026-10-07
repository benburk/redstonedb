use std::str::FromStr as _;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};

/// Opens `DATABASE_URL` with the project's pragmas and applies any pending
/// migrations, so the server and the CLI always see the current schema.
pub async fn connect_to_database() -> SqlitePool {
    let url = dotenvy::var("DATABASE_URL").expect("DATABASE_URL not set");
    let options = SqliteConnectOptions::from_str(&url)
        .expect("DATABASE_URL is not a SQLite URL")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .pragma("cache_size", "-64000");
    let pool = SqlitePool::connect_with(options)
        .await
        .expect("Failed to connect to database");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to apply migrations");
    pool
}
