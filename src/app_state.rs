use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use sqlx::SqlitePool;

use crate::{discord, mojang};

/// Players' heads by UUID, as PNGs, with when each was fetched.
pub type Heads = Arc<Mutex<HashMap<String, (Instant, Vec<u8>)>>>;

/// Minecraft players by name in lowercase, as Mojang knows them, or `None`
/// for a name no player has, with when each was asked.
pub type Players =
    Arc<Mutex<HashMap<String, (Instant, Option<mojang::Profile>)>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// `None` when the Discord settings are missing, which turns login off.
    pub discord: Option<discord::Config>,
    pub http: reqwest::Client,
    /// Fetched heads, so Mojang is asked about each at most every few
    /// hours.
    pub heads: Heads,
    /// Players looked up, so Mojang is asked about each name at most every
    /// few hours.
    pub players: Players,
}
