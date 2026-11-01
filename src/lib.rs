//! A database of Minecraft redstone schematics, with a web browser and a CLI.

/// The name shown to visitors. Distinct from the package name, which is
/// kebab-case and never displayed.
pub const APP_NAME: &str = "Redstone Database";

pub mod app_state;
pub mod category;
pub mod colour;
pub mod db;
pub mod details;
pub mod discord;
pub mod dust;
pub mod error;
pub mod evaluate;
pub mod extractors;
pub mod guide;
pub mod head;
pub mod import;
pub mod inspect;
pub mod interface;
pub mod leaderboard;
pub mod models;
pub mod mojang;
pub mod orient;
pub mod palette;
pub mod probe;
pub mod roles;
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
