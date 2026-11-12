//! The path from a file to a stored build with its results up to date:
//! preparing the file, holding its details to the rules, storing it, and
//! running its test, its timing, and what follows from them. The site and
//! `rsdb` both go through here.

pub mod derive;
pub mod details;
pub mod evaluate;
pub mod jobs;
pub mod prepare;
pub mod publish;
pub mod read;
pub mod results;
pub mod similar;
pub mod version;
