//! What the standard categories are named, ranked, and searched by, as
//! their rows in the table give it: their measures, sections, designs,
//! flags, and aliases; and a category as a build gives it, standard or
//! off the list.

use super::Kind;
use crate::models::{
    schematic::{Build, Logic},
    tag,
};

/// A name's words for people to read, each capitalized, such as
/// `Carry Lookahead` for `carry_lookahead`.
pub fn words(name: &str) -> String {
    name.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

named_values! {
    /// A number a category's builds are named and ranked by, as width is.
    /// Its label is its heading.
    Measure {
        /// Bits, as in `8-bit`.
        Width = "width", "Width";
        /// A memory's capacity, as in `32-byte`.
        Bytes = "bytes", "Capacity";
        /// A serial build's ticks from one bit to the next, as in `2-tick`.
        Throughput = "throughput", "Throughput";
        /// A threshold's k, the fewest inputs on that turn it on, as in `At
        /// Least 3`.
        AtLeast = "at_least", "At least";
        /// A monostable's pulse, in ticks, as in `3-tick`.
        Pulse = "pulse", "Pulse";
    }
}

impl Measure {
    /// The build's value of it.
    pub fn of(self, s: &Build) -> Option<i64> {
        match self {
            Measure::Width => s.specs.width,
            Measure::Bytes => s.specs.bytes,
            Measure::Throughput => s.specs.throughput,
            Measure::AtLeast => s.specs.at_least,
            Measure::Pulse => s.specs.pulse,
        }
    }

    /// `n` of it as a name leads with it, such as `8-bit`, `32-byte`,
    /// `2-tick`, or `At Least 3`; a pulse as a throughput, as in `3-tick
    /// Monostable`.
    pub fn lead(self, n: i64) -> String {
        match self {
            Measure::Width => format!("{n}-bit"),
            Measure::Bytes => format!("{n}-byte"),
            Measure::Throughput | Measure::Pulse => format!("{n}-tick"),
            Measure::AtLeast => format!("At Least {n}"),
        }
    }

    /// `n` of it in words, such as `8 bits` or `at least 3 on`.
    pub fn words(self, n: i64) -> String {
        let count = |one: &str, many: &str| {
            format!("{n} {}", if n == 1 { one } else { many })
        };
        match self {
            Measure::Width => count("bit", "bits"),
            Measure::Bytes => count("byte", "bytes"),
            Measure::Throughput | Measure::Pulse => count("tick", "ticks"),
            Measure::AtLeast => format!("at least {n} on"),
        }
    }

    /// The search term that finds `n` of it, such as `8b`.
    pub fn term(self, n: i64) -> String {
        match self {
            Measure::Width => format!("{n}b"),
            Measure::Bytes => format!("{n}-byte"),
            Measure::Throughput | Measure::Pulse => format!("{n}t"),
            Measure::AtLeast => format!("at least {n}"),
        }
    }
}

named_values! {
    /// The sections of standard categories, in order, each with its
    /// heading, as the home page and the menu of categories group them.
    Section {
        Arithmetic = "arithmetic", "Arithmetic";
        Memory = "memory", "Memory";
        Logic = "logic", "Logic";
        Gates = "gates", "Gates";
        Conversion = "conversion", "Conversion";
        Display = "display", "Display";
        Wiring = "wiring", "Wiring";
    }
}

/// An entry in the list of categories a build may have: one function, so
/// any of its builds could stand in for another. A standard one's is its
/// kind's row; one off the list has its name alone.
#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    /// As people read it, such as `RAM` or `BCD to Binary`.
    pub label: String,
    /// The label for many builds, such as `Adders` or `RAM`.
    pub plural: String,
    /// Whether each build gives its own name, as bussing does.
    pub named: bool,
    /// The section it sits in; `None` for Other, with the
    /// categories off the list.
    pub section: Option<Section>,
    /// Its builds' usual logic; `None` for none.
    pub logic: Option<Logic>,
    /// The numbers its builds are named and ranked by, in order, such as
    /// width then capacity for a memory, or throughput for a serial adder.
    pub measures: Vec<Measure>,
    /// Other words it is searched by, such as `inc`.
    pub aliases: Vec<String>,
}

impl Category {
    /// Every standard category, alphabetically by label.
    pub fn all() -> Vec<Category> {
        let mut all: Vec<Category> = Kind::all().map(Category::of).collect();
        all.sort_by_key(|c| c.label.to_lowercase());
        all
    }

    /// The standard category `kind`.
    pub fn of(kind: Kind) -> Category {
        let facts = kind.facts();
        Category {
            name: facts.name.to_owned(),
            label: facts.label.to_owned(),
            plural: kind.plural(),
            named: facts.named,
            section: Some(facts.section),
            logic: facts.logic,
            measures: facts.measures.to_vec(),
            aliases: aliases(facts.name),
        }
    }

    pub fn has(&self, measure: Measure) -> bool {
        self.measures.contains(&measure)
    }

    /// The standard category among `categories` that `typed` names, by its
    /// name or its label, whatever the capitals and spacing.
    pub fn find<'a>(
        categories: &'a [Category],
        typed: &str,
    ) -> Option<&'a Self> {
        let typed = typed.trim();
        categories.iter().find(|c| {
            c.label.eq_ignore_ascii_case(typed) || c.name == tag::name(typed)
        })
    }

    /// The name of the category `typed` names: a standard one's among
    /// `categories`, or else the text as the name of a category of the
    /// author's own, empty when nothing is typed.
    pub fn name_of(categories: &[Category], typed: &str) -> String {
        Self::find(categories, typed)
            .map_or_else(|| tag::name(typed), |c| c.name.clone())
    }

    /// A category off the list, named `name`, which has no measures.
    fn off_the_list(name: &str) -> Self {
        Category {
            name: name.to_owned(),
            label: words(name),
            plural: words(name),
            named: false,
            section: None,
            logic: None,
            measures: Vec::new(),
            aliases: aliases(name),
        }
    }
}

/// A design within a category, such as the carry lookahead adder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Design {
    pub name: &'static str,
    /// The usual short name, such as `CLA`.
    pub abbreviation: Option<&'static str>,
    /// How people read it, such as `LFSR`; `None` for the name's words.
    pub label: Option<&'static str>,
}

impl Design {
    /// Such as `Carry Lookahead (CLA)`.
    pub fn label(&self) -> String {
        let words = self.label.map_or_else(|| words(self.name), str::to_owned);
        match self.abbreviation {
            Some(short) => format!("{words} ({short})"),
            None => words,
        }
    }

    /// The design of the category named `category` named `name`, if it
    /// has one so named.
    pub fn of(category: &str, name: &str) -> Option<&'static Design> {
        Kind::named(category)?
            .facts()
            .designs
            .iter()
            .find(|design| design.name == name)
    }
}

/// A yes-or-no fact a category's builds record, such as an adder's carry
/// in or a shift register being bidirectional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag {
    /// Such as `carry_in`.
    pub name: &'static str,
    /// Such as `Carry in`.
    pub label: &'static str,
}

impl Flag {
    /// The flag of a build with a carry in.
    pub const CARRY_IN: &str = "carry_in";
    /// The flag of a build with a carry out.
    pub const CARRY_OUT: &str = "carry_out";
    /// The flag of a build with a borrow in.
    pub const BORROW_IN: &str = "borrow_in";
    /// The flag of a build with a borrow out.
    pub const BORROW_OUT: &str = "borrow_out";
    /// The flags that only say which lines a build has, which its picture
    /// shows plainly.
    pub const LINES: [&str; 4] = [
        Flag::CARRY_IN,
        Flag::CARRY_OUT,
        Flag::BORROW_IN,
        Flag::BORROW_OUT,
    ];
    /// The flag of an adder or subtractor whose carry in reads inverted,
    /// powered meaning 0, which says only how one line is read.
    pub const INVERTED_CARRY_IN: &str = "inverted_carry_in";
    /// The flag of an inverted adder whose carry out reads inverted, as its
    /// sum does.
    pub const INVERTED_CARRY_OUT: &str = "inverted_carry_out";
}

/// The flags of the category named `category` among `names`, in its
/// order; any other means nothing for its builds, and one off the list has
/// none.
pub fn flags_among<S: AsRef<str>>(
    category: &str,
    names: &[S],
) -> Vec<&'static Flag> {
    Kind::named(category).map_or_else(Vec::new, |kind| {
        kind.facts()
            .flags
            .iter()
            .filter(|flag| names.iter().any(|name| name.as_ref() == flag.name))
            .collect()
    })
}

/// Words search takes for a category off the list, which has no row to
/// hold them.
const OFF_LIST_ALIASES: &[(&str, &str)] = &[
    ("connect 4", "four_in_a_row_detector"),
    ("connect4", "four_in_a_row_detector"),
];

/// The other words search takes for the category named `category`, of the
/// list or off it, alphabetically.
pub fn aliases(category: &str) -> Vec<String> {
    let aliases = match Kind::named(category) {
        Some(kind) => kind.facts().aliases.to_vec(),
        None => OFF_LIST_ALIASES
            .iter()
            .filter(|(_, of)| *of == category)
            .map(|(alias, _)| *alias)
            .collect(),
    };
    aliases.into_iter().map(str::to_owned).collect()
}

/// The word search and a leaderboard's address name no design by, as a
/// build of none is found and ranked as one of `other`.
pub const NO_DESIGN: &str = "other";

/// Checks a build's category and design, and returns the category. A
/// category is a standard one, or any other name made of lowercase words
/// joined by underscores. A design is none, or one the category lists.
/// The `Err` is a sentence for the person.
pub fn check_category(
    category: &str,
    design: Option<&str>,
) -> Result<Category, String> {
    let valid = |name: &str| {
        name.chars().any(|c| c.is_ascii_alphanumeric())
            && name.chars().all(|c| {
                c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || c == '_'
                    || c == '-'
            })
    };
    if !valid(category) {
        return Err(
            "A category's name is words, numbers, and hyphens, such as line \
             drawer or 8-bit horizontal diode."
                .to_owned(),
        );
    }
    // The menu gathers every category off the list under `other`.
    if category == "other" {
        return Err("Name the category, rather than other.".to_owned());
    }
    // A build of a category the site recognizes builds of goes under it,
    // recognized or not, rather than under a name made from it.
    if let Some(kind) = made_from(category).filter(|kind| kind.blessed()) {
        return Err(format!(
            "Choose {} as the category, rather than a name made from it.",
            kind.label()
        ));
    }
    if let Some(design) = design
        && Design::of(category, design).is_none()
    {
        return Err(format!(
            "{} is not a design of {}.",
            design.replace('_', " "),
            category.replace('_', " ")
        ));
    }
    Ok(Kind::named(category)
        .map_or_else(|| Category::off_the_list(category), Category::of))
}

/// The standard category the name `category` is made from: `adder` for
/// `8-bit_adder` or `unrecognized_adder`.
fn made_from(category: &str) -> Option<Kind> {
    let of = category.strip_prefix("unrecognized_").or_else(|| {
        let (bits, of) = category.split_once("-bit_")?;
        bits.parse::<u32>().ok().map(|_| of)
    })?;
    Kind::named(of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_made_from_a_recognized_category_is_refused() {
        for name in ["8-bit_adder", "unrecognized_register"] {
            assert!(check_category(name, None).is_err(), "{name}");
        }
        // The site recognizes no RAM yet, so one goes under a name made
        // from it.
        for name in ["8-bit_ram", "eight-bit_adder", "8-bit_horizontal_diode"] {
            assert!(check_category(name, None).is_ok(), "{name}");
        }
    }

    #[test]
    fn a_standard_category_is_typed_by_name_or_label() {
        let categories = Category::all();
        for typed in [
            "Hex to Binary",
            "hex to binary",
            "hex_to_binary",
            " HEX TO BINARY ",
        ] {
            assert_eq!(Category::name_of(&categories, typed), "hex_to_binary");
        }
        assert_eq!(
            Category::name_of(&categories, "8-Bit Horizontal Diode"),
            "8-bit_horizontal_diode"
        );
        assert!(Category::find(&categories, "piston door").is_none());
    }

    #[test]
    fn a_category_off_the_list_has_its_aliases_too() {
        assert_eq!(
            aliases("four_in_a_row_detector"),
            ["connect 4", "connect4"]
        );
        assert_eq!(aliases("monostable"), ["mono", "pulse generator"]);
        assert!(aliases("piston_door").is_empty());
    }
}
