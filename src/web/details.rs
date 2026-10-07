//! Reading a build's details from a form, for editing and uploading.

use serde::Deserialize;

use crate::models::schematic::Details;

/// A details form as sent, every field as text; empty means unknown.
#[derive(Deserialize)]
pub struct Form {
    pub function: String,
    #[serde(default)]
    pub width: String,
    pub orientation: String,
    pub encoding: String,
    #[serde(default)]
    pub latency: String,
    #[serde(default)]
    pub synchronized: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub built_on: String,
    #[serde(default)]
    pub description: String,
}

/// A details form checked and converted.
pub struct Parsed {
    pub function: String,
    pub width: Option<i64>,
    pub orientation: String,
    pub encoding: String,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub author: Option<String>,
    pub built_on: Option<String>,
    pub description: Option<String>,
}

impl Form {
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
        let function = self.function.trim().to_lowercase();
        let valid_function = function
            .starts_with(|c: char| c.is_ascii_lowercase())
            && function.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
            });
        if !valid_function {
            return Err(
                "Function must be lowercase words joined by underscores."
                    .to_owned(),
            );
        }
        if !matches!(self.orientation.as_str(), "horz" | "vert" | "diag") {
            return Err(
                "Orientation must be horizontal, vertical, or diagonal."
                    .to_owned(),
            );
        }
        if !matches!(self.encoding.as_str(), "bin" | "hex") {
            return Err(
                "Encoding must be binary or signal strength.".to_owned()
            );
        }
        let author = text(&self.author).map(|a| a.to_lowercase());
        if author.as_deref().is_some_and(|a| {
            !a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }) {
            return Err("Built by must be a Minecraft name: letters, digits, and underscores.".to_owned());
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
            synchronized: match self.synchronized.as_str() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            },
            author,
            built_on,
            description: text(&self.description),
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
            author: self.author.as_deref(),
            built_on: self.built_on.as_deref(),
            description: self.description.as_deref(),
        }
    }
}
