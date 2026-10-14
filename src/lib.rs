//! A database of Minecraft redstone schematics, with a web browser and a CLI.

/// The name shown to visitors. Distinct from the package name, which is
/// kebab-case and never displayed.
pub const APP_NAME: &str = "Redstone Database";

pub mod app_state;
pub mod db;
pub mod discord;
pub mod error;
pub mod extractors;
pub mod head;
pub mod import;
pub mod interface;
pub mod leaderboard;
pub mod lint;
pub mod models;
pub mod mojang;
pub mod schem;
pub mod schemati;
pub mod search;
pub mod similarity;
pub mod simulate;
pub mod stack;
pub mod standardize;
pub mod thumbnail;
pub mod util;
pub mod version;
pub mod web;
pub mod works_from;
