//! The list of categories a build may have, with what each is named, ranked,
//! and searched by: its measures, designs, flags, and aliases. These are
//! rows people add to with `rsdb`; what the code does with a category's
//! builds is `crate::recognize::category`'s.

use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

use super::schematic::{Logic, Schematic};
use super::tag;

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
    pub fn of(self, s: &Schematic) -> Option<i64> {
        match self {
            Measure::Width => s.width,
            Measure::Bytes => s.bytes,
            Measure::Throughput => s.throughput,
            Measure::AtLeast => s.at_least,
            Measure::Pulse => s.pulse,
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
/// any of its builds could stand in for another.
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
    /// builder's own, empty when nothing is typed.
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
            aliases: Vec::new(),
        }
    }
}

/// A design within a category, such as the carry lookahead adder.
#[derive(Debug, Clone)]
pub struct Design {
    pub category: String,
    pub name: String,
    /// The usual short name, such as `CLA`.
    pub abbreviation: Option<String>,
    /// How people read it, such as `XOR`; `None` for the name's words.
    pub label: Option<String>,
}

impl Design {
    /// Such as `Carry Lookahead (CLA)`.
    pub fn label(&self) -> String {
        let words = self.label.clone().unwrap_or_else(|| words(&self.name));
        match &self.abbreviation {
            Some(short) => format!("{words} ({short})"),
            None => words,
        }
    }
}

/// A yes-or-no fact a category's builds record, such as an adder's carry
/// in or a shift register being bidirectional.
#[derive(Debug, Clone)]
pub struct Flag {
    pub category: String,
    /// Such as `carry_in`.
    pub name: String,
    /// Such as `Carry in`.
    pub label: String,
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
}

/// What every category is named and searched by beyond its row: its
/// measures and its aliases. A category off the list may have aliases too.
pub(super) struct Extras {
    measures: HashMap<String, Vec<Measure>>,
    aliases: HashMap<String, Vec<String>>,
}

impl Extras {
    pub(super) async fn load(db: &SqlitePool) -> Result<Self, sqlx::Error> {
        let mut measures: HashMap<String, Vec<Measure>> = HashMap::new();
        for row in sqlx::query!(
            r#"SELECT category, measure as "measure: Measure"
             FROM category_measure ORDER BY category, position"#
        )
        .fetch_all(db)
        .await?
        {
            measures.entry(row.category).or_default().push(row.measure);
        }
        let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
        for row in sqlx::query!(
            "SELECT category, alias FROM category_alias ORDER BY alias"
        )
        .fetch_all(db)
        .await?
        {
            aliases.entry(row.category).or_default().push(row.alias);
        }
        Ok(Extras { measures, aliases })
    }

    pub(super) fn measures(&self, category: &str) -> Vec<Measure> {
        self.measures.get(category).cloned().unwrap_or_default()
    }

    pub(super) fn aliases(&self, category: &str) -> Vec<String> {
        self.aliases.get(category).cloned().unwrap_or_default()
    }
}

/// Every standard category, alphabetically by label.
pub async fn categories(db: &SqlitePool) -> Result<Vec<Category>, sqlx::Error> {
    let extras = Extras::load(db).await?;
    Ok(sqlx::query!(
        r#"SELECT name as "name!", label,
            coalesce(plural, label || 's') as "plural!: String",
            named as "named: bool", section as "section: Section",
            logic as "logic: Logic"
         FROM category ORDER BY label COLLATE NOCASE"#
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| Category {
        measures: extras.measures(&row.name),
        aliases: extras.aliases(&row.name),
        name: row.name,
        label: row.label,
        plural: row.plural,
        named: row.named,
        section: row.section,
        logic: row.logic,
    })
    .collect())
}

/// Every design, grouped by category.
pub async fn designs<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Design>, sqlx::Error> {
    sqlx::query_as!(
        Design,
        "SELECT category, name, abbreviation, label FROM design
         ORDER BY category, name = 'other', name"
    )
    .fetch_all(db)
    .await
}

/// Every category's flags, each category's in its order.
pub async fn flags<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Flag>, sqlx::Error> {
    sqlx::query_as!(
        Flag,
        "SELECT category, name, label FROM flag ORDER BY category, position"
    )
    .fetch_all(db)
    .await
}

/// The names among `names` that are flags of `category`, in its order; any
/// other means nothing for its builds.
pub async fn keep_flags<'e, E: SqliteExecutor<'e>>(
    db: E,
    category: &str,
    names: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    Ok(flags(db)
        .await?
        .into_iter()
        .filter(|flag| flag.category == category && names.contains(&flag.name))
        .map(|flag| flag.name)
        .collect())
}

/// Checks a build's category and design, and returns the category. A
/// category is a standard one from the list, or any other name made of
/// lowercase words joined by underscores. A design is `other`, or one the
/// category lists among `designs`. The `Err` is a sentence for the person.
pub fn check_category(
    categories: &[Category],
    designs: &[Design],
    category: &str,
    design: &str,
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
    let listed = designs
        .iter()
        .any(|d| d.category == category && d.name == design);
    if design != "other" && !listed {
        return Err(format!(
            "{} is not a design of {}.",
            design.replace('_', " "),
            category.replace('_', " ")
        ));
    }
    Ok(categories
        .iter()
        .find(|f| f.name == category)
        .cloned()
        .unwrap_or_else(|| Category::off_the_list(category)))
}

/// Adds a standard category. A plural that is the label with an `s` is
/// left for the database to make.
pub async fn define_category(
    db: &SqlitePool,
    category: &Category,
) -> Result<(), sqlx::Error> {
    let plural = (category.plural != format!("{}s", category.label))
        .then_some(category.plural.as_str());
    let mut tx = db.begin().await?;
    sqlx::query!(
        "INSERT INTO category (name, label, plural, named, section, logic)
         VALUES (?, ?, ?, ?, ?, ?)",
        category.name,
        category.label,
        plural,
        category.named,
        category.section,
        category.logic
    )
    .execute(&mut *tx)
    .await?;
    for (position, measure) in (1_i64..).zip(&category.measures) {
        sqlx::query!(
            "INSERT INTO category_measure (category, measure, position)
             VALUES (?, ?, ?)",
            category.name,
            measure,
            position
        )
        .execute(&mut *tx)
        .await?;
    }
    for alias in &category.aliases {
        define_alias(&mut *tx, alias, &category.name).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Gives `category`, of the list or off it, another word to be searched
/// by, in the form a search term takes: lowercase, with spaces.
pub async fn define_alias<'e, E: SqliteExecutor<'e>>(
    db: E,
    alias: &str,
    category: &str,
) -> Result<(), sqlx::Error> {
    let alias = alias.trim().to_lowercase().replace('_', " ");
    sqlx::query!(
        "INSERT INTO category_alias (alias, category) VALUES (?, ?)",
        alias,
        category
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn define_design<'e, E: SqliteExecutor<'e>>(
    db: E,
    design: &Design,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO design (category, name, abbreviation, label)
         VALUES (?, ?, ?, ?)",
        design.category,
        design.name,
        design.abbreviation,
        design.label
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Adds a flag to the end of its category's.
pub async fn define_flag<'e, E: SqliteExecutor<'e>>(
    db: E,
    flag: &Flag,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO flag (category, name, label, position)
         VALUES (?, ?, ?, (SELECT coalesce(max(position), 0) + 1 FROM flag
            WHERE category = ?))",
        flag.category,
        flag.name,
        flag.label,
        flag.category
    )
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standard_category_is_typed_by_name_or_label() {
        let hex = Category {
            label: "Hex to Binary".to_owned(),
            measures: vec![Measure::Width],
            ..Category::off_the_list("hex_to_binary")
        };
        let categories = std::slice::from_ref(&hex);
        for typed in [
            "Hex to Binary",
            "hex to binary",
            "hex_to_binary",
            " HEX TO BINARY ",
        ] {
            assert_eq!(Category::name_of(categories, typed), "hex_to_binary");
        }
        assert_eq!(
            Category::name_of(categories, "8-Bit Horizontal Diode"),
            "8-bit_horizontal_diode"
        );
        assert!(Category::find(categories, "piston door").is_none());
    }
}
