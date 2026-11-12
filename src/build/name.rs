//! How a build is named: its name, its title, and its download's filename.

use super::Build;
use crate::{
    category::{self, Measure},
    models::tag::{self, Tag},
};

/// `name` as a file's name, less `.schem`: an underscore for each run of
/// anything but letters and digits, none at either end, as Schemati takes
/// no other characters. Empty when `name` has no letter or digit.
pub fn file_stem(name: &str) -> String {
    let name = name.strip_suffix(".schem").unwrap_or(name);
    let mut stem = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            stem.push(c);
        } else if !stem.is_empty() && !stem.ends_with('_') {
            stem.push('_');
        }
    }
    stem.trim_end_matches('_').to_owned()
}

impl Build {
    /// The width in words, such as `8 bits` or `1 bit`.
    pub fn width_label(&self) -> Option<String> {
        self.width
            .map(|w| format!("{w} {}", if w == 1 { "bit" } else { "bits" }))
    }

    /// The category as people read it, such as `RAM` or `Line Drawer`.
    pub fn category_label(&self) -> String {
        self.category_label
            .clone()
            .unwrap_or_else(|| category::words(&self.category))
    }

    /// What the build is: its given name, or else its category.
    pub(super) fn what(&self) -> String {
        // A subtractor is an adder whose B is inverted, called by what it
        // does.
        let subtracts = self.kind() == Some(crate::category::Kind::Adder)
            && self.flags.iter().any(|flag| flag.name == "subtract");
        if subtracts {
            return "Subtractor".to_owned();
        }
        self.given_name
            .clone()
            .unwrap_or_else(|| self.category_label())
    }

    /// Its category's measures that lead a name, such as `8-bit` and
    /// `32-byte`, or `2-tick`; an unknown one is left out. A multiplier's
    /// width is its shape, as `8x8=16`, keeping the upper half of its
    /// product, or `8x8=8`, and `8x8+8=16` for a fused multiply-add.
    fn measures(&self) -> impl Iterator<Item = String> {
        let shape = self.product_shape();
        self.measures.iter().filter_map(move |m| match (m, &shape) {
            (Measure::Width, Some(shape)) => Some(shape.clone()),
            _ => m.of(self).map(|n| m.lead(n)),
        })
    }

    /// A multiplier's operands and product, as `8x8=16`; `None` for any
    /// other build, or one of unknown width.
    fn product_shape(&self) -> Option<String> {
        let width = self.width.filter(|_| {
            self.kind() == Some(crate::category::Kind::Multiplier)
        })?;
        let flagged = |name: &str| self.flags.iter().any(|f| f.name == name);
        let product = if flagged("upper_product") {
            2 * width
        } else {
            width
        };
        let addend = if flagged("fma") {
            format!("+{width}")
        } else {
            String::new()
        };
        Some(format!("{width}x{width}{addend}={product}"))
    }

    /// The width, design, and category, such as `8-bit Carry Lookahead
    /// Adder`, for views that show the tags beside it. A build with a given
    /// name, such as `8-bit Bus Crossing`, uses it for the category.
    pub fn name(&self) -> String {
        self.measures()
            .chain(self.ending())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The design and what the build is, as a name ends, such as `Carry
    /// Lookahead Adder`.
    fn ending(&self) -> impl Iterator<Item = String> {
        self.design_label().into_iter().chain([self.what()])
    }

    /// A readable name from the most identifying facts, such as
    /// `8-bit alpha Carry Lookahead Adder`.
    pub fn title(&self) -> String {
        self.measures()
            .chain(self.tags_in(tag::Kind::Restriction).map(Tag::label))
            .chain(self.tags_in(tag::Kind::Other).map(Tag::label))
            .chain(self.ending())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The design in words, such as `Carry Lookahead`; `None` for no
    /// design.
    pub fn design_label(&self) -> Option<String> {
        self.design.as_ref().map(|design| {
            self.design_given_label
                .clone()
                .unwrap_or_else(|| category::words(design))
        })
    }

    /// The download filename: the build's name with an underscore for each
    /// run of anything but letters and digits, such as
    /// `8_bit_Carry_Lookahead_Adder.schem`, as Schemati, the Open Redstone
    /// Engineers server's schematic store, takes no other characters.
    pub fn filename(&self) -> String {
        format!("{}.schem", file_stem(&self.name()))
    }
}
