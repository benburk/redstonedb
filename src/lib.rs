//! A database of Minecraft redstone schematics, with a web browser and a CLI.

/// The name shown to visitors. Distinct from the package name, which is
/// lowercase and never displayed.
pub const APP_NAME: &str = "RedstoneDB";

pub mod category;
pub mod db;
pub mod details;
pub mod external;
pub mod import;
pub mod leaderboard;
pub mod models;
pub mod recognize;
pub mod schem;
pub mod search;
pub mod similarity;
pub mod simulate;
pub mod thumbnail;
pub mod version;
pub mod web;
