//! A database of Minecraft redstone schematics, with a web browser and a CLI.

/// The name shown to visitors. Distinct from the package name, which is
/// kebab-case and never displayed.
pub const APP_NAME: &str = "Redstone Database";

pub mod app_state;
pub mod db;
pub mod error;
pub mod extractors;
pub mod interface;
pub mod lint;
pub mod models;
pub mod password;
pub mod schem;
pub mod services;
pub mod thumbnail;
pub mod util;
pub mod web;
