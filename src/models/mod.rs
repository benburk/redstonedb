/// Declares an enum for a column that holds one of a few words: each value
/// is stored, searched, and given on the command line as its name, such
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

pub mod catalogue;
pub mod schematic;
pub mod session;
pub mod similarity;
pub mod simulation;
pub mod tag;
pub mod user;
pub mod version;
