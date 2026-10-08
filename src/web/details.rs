//! Reading a build's details from a form, for editing and uploading.

use super::pages::Fields;
use crate::models::schematic::{self, Details};

/// A details form as sent, every field as text; empty means unknown.
#[derive(Default)]
pub struct Form {
    /// A standard function, or `other` with its name in `function_name`.
    pub function: String,
    pub function_name: String,
    pub width: String,
    pub orientation: String,
    pub encoding: String,
    pub latency: String,
    /// Whether the toggle is on; `None` while an unknown build's toggle is
    /// untouched, so saving keeps it unknown.
    pub synchronized: Option<bool>,
    /// Empty when the "don't know" box beside it is ticked.
    pub built_on: String,
    pub description: String,
    pub design: String,
    pub carry_in: bool,
    pub carry_out: bool,
    /// In order, from the builder pills and anything typed beside them.
    pub builders: Vec<String>,
    /// Likewise, from the contributor pills.
    pub contributors: Vec<String>,
    /// From the tag pills and anything typed beside them.
    pub tags: Vec<String>,
}

/// A details form checked and converted.
pub struct Parsed {
    pub function: String,
    pub width: Option<i64>,
    pub orientation: String,
    pub encoding: String,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub built_on: Option<String>,
    pub description: Option<String>,
    pub design: String,
    pub carry_in: Option<bool>,
    pub carry_out: Option<bool>,
    pub builders: Vec<String>,
    pub contributors: Vec<String>,
}

impl Form {
    /// Reads a urlencoded body by hand, since the pill fields repeat one
    /// name per value. `builder_entry`, `contributor_entry`, and `tag_entry`
    /// are their text
    /// boxes, sent as typed when scripting is off.
    pub fn read(pairs: &[(String, String)]) -> Self {
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let all = |key: &str, entry: &str| -> Vec<String> {
            pairs
                .iter()
                .filter(|(k, v)| {
                    k == key || (k == entry && !v.trim().is_empty())
                })
                .map(|(_, v)| v.trim().to_owned())
                .collect()
        };
        Form {
            function: get("function"),
            function_name: get("function_name"),
            width: get("width"),
            orientation: get("orientation"),
            encoding: get("encoding"),
            latency: get("latency"),
            synchronized: get("synchronized_unknown")
                .is_empty()
                .then(|| !get("synchronized").is_empty()),
            built_on: if get("built_on_unknown").is_empty() {
                get("built_on")
            } else {
                String::new()
            },
            description: get("description"),
            design: get("design"),
            carry_in: !get("carry_in").is_empty(),
            carry_out: !get("carry_out").is_empty(),
            builders: all("builder", "builder_entry"),
            contributors: all("contributor", "contributor_entry"),
            tags: all("tag", "tag_entry")
                .iter()
                .map(|name| schematic::tag_name(name))
                .collect(),
        }
    }

    /// The sent values, to show again after a refusal. A function named
    /// under other shows as that name.
    pub fn fields(&self) -> Fields<'_> {
        Fields {
            function: if self.function == "other" {
                &self.function_name
            } else {
                &self.function
            },
            width: &self.width,
            orientation: &self.orientation,
            encoding: &self.encoding,
            latency: &self.latency,
            synchronized: self.synchronized,
            builders: &self.builders,
            contributors: &self.contributors,
            built_on: &self.built_on,
            description: &self.description,
            design: &self.design,
            carry_in: self.carry_in,
            carry_out: self.carry_out,
        }
    }

    /// The checked fields, or a sentence saying which field is wrong.
    pub fn parse(&self) -> Result<Parsed, String> {
        let text = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        };
        let number = |value: &str, name: &str| -> Result<Option<i64>, String> {
            text(value)
                .map(|v| {
                    v.parse()
                        .map_err(|_| format!("{name} must be a whole number."))
                })
                .transpose()
        };
        let function = if self.function == "other" {
            schematic::tag_name(&self.function_name)
        } else {
            self.function.trim().to_lowercase()
        };
        if function.is_empty() {
            return Err("Choose a function, or name it under other.".to_owned());
        }
        if !matches!(self.orientation.as_str(), "horz" | "vert" | "diag") {
            return Err(
                "Orientation must be Horizontal, Vertical, or Diagonal."
                    .to_owned(),
            );
        }
        if !matches!(self.encoding.as_str(), "bin" | "hex") {
            return Err("Encoding must be binary or hex.".to_owned());
        }
        if let Some(name) = self
            .builders
            .iter()
            .chain(&self.contributors)
            .find(|n| !schematic::valid_player_name(n))
        {
            return Err(format!(
                "{name} is not a Minecraft name: letters, digits, and \
                 underscores, up to 16."
            ));
        }
        if let Some(name) =
            self.tags.iter().find(|n| !schematic::valid_tag_name(n))
        {
            return Err(format!(
                "{} cannot be a tag: use letters, digits, and spaces.",
                name.replace('_', " ")
            ));
        }
        let built_on = text(&self.built_on);
        if built_on.as_deref().is_some_and(|d| {
            d.len() != 10
                || d.chars().enumerate().any(|(i, c)| {
                    if i == 4 || i == 7 {
                        c != '-'
                    } else {
                        !c.is_ascii_digit()
                    }
                })
        }) {
            return Err(
                "Built on must be a date such as 2024-02-02.".to_owned()
            );
        }
        Ok(Parsed {
            function,
            width: number(&self.width, "Width")?.filter(|w| *w > 0),
            orientation: self.orientation.clone(),
            encoding: self.encoding.clone(),
            latency: number(&self.latency, "Latency")?,
            synchronized: self.synchronized,
            built_on,
            description: text(&self.description),
            design: text(&self.design).unwrap_or_else(|| "other".to_owned()),
            carry_in: Some(self.carry_in),
            carry_out: Some(self.carry_out),
            builders: self.builders.clone(),
            contributors: self.contributors.clone(),
        })
    }
}

impl Parsed {
    pub fn as_details(&self) -> Details<'_> {
        Details {
            function: &self.function,
            width: self.width,
            orientation: &self.orientation,
            encoding: &self.encoding,
            latency: self.latency,
            synchronized: self.synchronized,
            built_on: self.built_on.as_deref(),
            description: self.description.as_deref(),
            design: Some(&self.design),
            carry_in: self.carry_in,
            carry_out: self.carry_out,
        }
    }
}

/// The pairs of a urlencoded body.
pub fn pairs(body: &[u8]) -> Vec<(String, String)> {
    form_urlencoded::parse(body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}
