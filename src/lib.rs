//! A database of Minecraft redstone schematics, with a web browser and a CLI.

/// The name shown to visitors. Distinct from the package name, which is
/// lowercase and never displayed.
pub const APP_NAME: &str = "RedstoneDB";

/// Where the live site is, for addresses that must be whole, as in a link
/// preview's picture.
pub const SITE_URL: &str = "https://redstonedb.com";

/// Declares an enum of a few words, as a column may hold: each value is
/// stored, searched, and given on the command line as its name, such
/// as `horz`, and read by people as its label, such as `Horizontal`. It
/// parses from either, in any capitals.
macro_rules! named_values {
    (
        $(#[$meta:meta])*
        $enum:ident {
            $($(#[$doc:meta])* $variant:ident = $name:tt, $label:tt;)+
        }
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            sqlx::Type, clap::ValueEnum,
        )]
        pub enum $enum {
            $($(#[$doc])* #[sqlx(rename = $name)] #[value(name = $name)] $variant,)+
        }

        impl $enum {
            /// Every value, in order.
            pub const ALL: &'static [$enum] = &[$($enum::$variant),+];

            /// As stored, searched, and given to `rsdb`.
            pub fn name(self) -> &'static str {
                match self {
                    $($enum::$variant => $name,)+
                }
            }

            /// As people read it.
            pub fn label(self) -> &'static str {
                match self {
                    $($enum::$variant => $label,)+
                }
            }
        }

        impl std::str::FromStr for $enum {
            type Err = String;

            /// Its name or its label, in any capitals.
            fn from_str(text: &str) -> Result<Self, String> {
                let text = text.trim();
                $enum::ALL
                    .iter()
                    .copied()
                    .find(|value| {
                        value.name().eq_ignore_ascii_case(text)
                            || value.label().eq_ignore_ascii_case(text)
                    })
                    .ok_or_else(|| {
                        let labels: Vec<&str> =
                            $enum::ALL.iter().map(|value| value.label()).collect();
                        format!(
                            "{} must be one of {}.",
                            stringify!($enum),
                            labels.join(", ")
                        )
                    })
            }
        }
    };
}

pub mod account;
pub mod build;
pub mod category;
pub mod db;
pub mod details;
pub mod diff;
pub mod external;
pub mod import;
pub mod ingest;
pub mod interface;
pub mod leaderboard;
pub mod models;
pub mod players;
pub mod recognize;
pub mod schem;
pub mod search;
pub mod similarity;
pub mod simulate;
pub mod stack;
pub mod store;
pub mod thumbnail;
pub mod version;
pub mod web;
pub mod works_from;
