use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use sqlx::SqlitePool;

use crate::discord;

/// Players' heads by UUID, as PNGs, with when each was fetched.
pub type Heads = Arc<Mutex<HashMap<String, (Instant, Vec<u8>)>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// `None` when the Discord settings are missing, which turns login off.
    pub discord: Option<discord::Config>,
    pub http: reqwest::Client,
    /// Fetched heads, so Mojang is asked about each at most every few
    /// hours.
    pub heads: Heads,
}
