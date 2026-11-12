//! A build's details as people give them, and as they stand once held
//! to the site's rules, ready to store.

use crate::{
    build::{Build, Credits, Reason, Specs},
    category::Category,
    stack::StackDirection,
};

/// A build's details, typed. As given they may break a rule, such as naming
/// the category by its label, giving a capacity to a category not ranked
/// by one, or crediting a name no player has; [`check`] holds them to every
/// rule.
#[derive(Debug, Clone, Default)]
pub struct Details {
    /// A standard category by its name or label, in any capitals, or a name
    /// of the author's own.
    pub category: String,
    /// Its specs as given; its design one of the category's, or `None`
    /// for `other`.
    pub specs: Specs,
    /// A date such as `2024-02-02`.
    pub built_on: Option<String>,
    pub description: Option<String>,
    /// Flags it has; any not of its category are dropped.
    pub flags: Vec<String>,
    /// The names credited, each respelled as its player spells it.
    pub credits: Credits,
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
    /// Tag names, which may be written with spaces and capitals.
    pub tags: Vec<String>,
    /// Why it is not recognized as its category, one the site recognizes
    /// builds of, so it goes up unverified with the details given; see
    /// [`unrecognized`].
    pub unverified: Option<Reason>,
}

impl Details {
    /// Build `s`'s details as stored, to change some and check them again.
    pub fn of(s: &Build) -> Self {
        Details {
            category: s.category.clone(),
            specs: s.specs.clone(),
            built_on: s.built_on.clone(),
            description: s.description.clone(),
            flags: s.flags.iter().map(|flag| flag.name.to_owned()).collect(),
            credits: s.credits.clone(),
            stacking: s.stacking.clone(),
            tags: s
                .tags
                .iter()
                .filter(|tag| !tag.is_derived())
                .map(|tag| tag.name.clone())
                .collect(),
            unverified: s.unverified.clone(),
        }
    }
}

/// Details that keep every rule, ready to store.
pub struct Ready {
    /// The category, as the list has it, or one of the author's own.
    pub category: Category,
    /// The category by its name, the fields it has no use for dropped,
    /// flags not of it dropped, the names respelled as their players spell
    /// them, and tags in the form [`tag::name`] gives.
    pub details: Details,
    /// Whether the simulation decides how it stacks, so the stacking given
    /// counts for nothing and whatever the simulation found stays.
    pub stacking_inferred: bool,
    /// Whether the simulation works out its every measured detail, as for
    /// a conventional adder, so an edit leaves them as it set them.
    pub evaluated: bool,
}
