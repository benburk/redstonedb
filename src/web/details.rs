//! Reading a build's details from a form, for editing and uploading.

use super::pages::Fields;
use crate::models::schematic::{self, Category, Details, StackDirection};

/// The ways the form can stack a build, as `StackDirection::name` gives
/// them, with their labels.
pub const STACK_DIRECTIONS: [(&str, &str); 4] = [
    ("west", "West"),
    ("up", "Up"),
    ("north", "North"),
    ("west-up", "West and up"),
];

/// One stacking direction as the form shows it, every field as text.
#[derive(Clone, Default)]
pub struct StackRow {
    /// One of [`STACK_DIRECTIONS`], such as `west`.
    pub direction: String,
    /// Blocks from each copy to the next.
    pub step: String,
    /// The most copies that work; empty for no limit.
    pub max: String,
}

impl StackRow {
    pub fn of(direction: &StackDirection) -> Self {
        StackRow {
            direction: direction.name(),
            step: direction
                .west
                .max(direction.up)
                .max(direction.north)
                .to_string(),
            max: direction
                .max_copies
                .map(|n| n.to_string())
                .unwrap_or_default(),
        }
    }

    /// A likely first direction for a build whose bits run along
    /// `orientation`, with core `size`: copies placed its size apart the
    /// way the bits run. A diagonal build's step is left to the person.
    pub fn suggested(orientation: &str, size: (i64, i64, i64)) -> Self {
        let (direction, step) = match orientation {
            "vert" => ("up", size.1.to_string()),
            "diag" => ("west-up", String::new()),
            _ => ("west", size.0.to_string()),
        };
        StackRow {
            direction: direction.to_owned(),
            step,
            max: String::new(),
        }
    }

    /// The direction this row describes; `None` when its step is empty.
    fn parse(&self) -> Result<Option<StackDirection>, String> {
        let step = self.step.trim();
        if step.is_empty() {
            return Ok(None);
        }
        let step: i64 = step
            .parse()
            .ok()
            .filter(|n| *n > 0)
            .ok_or("A stacking step must be a whole number of blocks.")?;
        let (west, up, north) = match self.direction.as_str() {
            "west" => (step, 0, 0),
            "up" => (0, step, 0),
            "north" => (0, 0, step),
            "west-up" => (step, step, 0),
            _ => return Err("Choose a stacking direction.".to_owned()),
        };
        let max = self.max.trim();
        let max_copies = if max.is_empty() {
            None
        } else {
            Some(max.parse().ok().filter(|n| *n >= 2).ok_or(
                "The most copies must be a whole number, 2 or more; leave \
                 it empty for no limit.",
            )?)
        };
        Ok(Some(StackDirection {
            west,
            up,
            north,
            max_copies,
        }))
    }
}

/// A details form as sent, every field as text; empty means unknown.
#[derive(Default)]
pub struct Form {
    /// The category as typed: a standard one by its name or label, in any
    /// capitals, or else a name of the builder's own.
    pub category: String,
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
    /// The build's own name, for a category that takes one.
    pub given_name: String,
    /// `comb` or `seq`; empty follows the category.
    pub logic: String,
    /// The fewest ticks from one input to the next.
    pub throughput: String,
    /// How many bytes a memory holds.
    pub bytes: String,
    /// A threshold's k.
    pub at_least: String,
    pub design: String,
    /// The ticked flags of every category's group, each with its category,
    /// such as `("adder", "carry_in")`; parsing keeps the chosen one's.
    pub flags: Vec<(String, String)>,
    /// The ticked flags' names, to show the form again.
    pub flag_names: Vec<String>,
    /// Whether the stacking toggle is on; the rows count only when it is.
    pub stackable: bool,
    pub stack_rows: Vec<StackRow>,
    /// In order, from the builder pills and anything typed beside them.
    pub builders: Vec<String>,
    /// Likewise, from the contributor pills.
    pub contributors: Vec<String>,
    /// From the tag pills and anything typed beside them.
    pub tags: Vec<String>,
}

/// A details form checked and converted.
pub struct Parsed {
    pub category: String,
    pub width: Option<i64>,
    pub orientation: String,
    pub encoding: Option<String>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub built_on: Option<String>,
    pub description: Option<String>,
    pub given_name: Option<String>,
    /// `None` follows the category.
    pub logic: Option<String>,
    pub throughput: Option<i64>,
    pub bytes: Option<i64>,
    pub at_least: Option<i64>,
    pub design: String,
    /// Ticked flags; any not of the category are dropped when stored.
    pub flags: Vec<String>,
    pub builders: Vec<String>,
    pub contributors: Vec<String>,
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
}

impl Form {
    /// Reads a urlencoded body by hand, since the pill fields repeat one
    /// name per value. `builder_entry`, `contributor_entry`, and `tag_entry`
    /// are their text
    /// boxes, sent as typed when scripting is off.
    pub fn read(pairs: &[(String, String)]) -> Self {
        let get = |key: &str| first(pairs, key).to_owned();
        let each = |key: &str| -> Vec<String> {
            pairs
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.trim().to_owned())
                .collect()
        };
        let stack_rows = each("stack_direction")
            .into_iter()
            .zip(each("stack_step"))
            .zip(each("stack_max"))
            .map(|((direction, step), max)| StackRow {
                direction,
                step,
                max,
            })
            .collect();
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
            category: get("category"),
            width: get("width"),
            orientation: get("orientation"),
            encoding: get("encoding"),
            latency: get("latency"),
            synchronized: get("synchronized_unknown")
                .is_empty()
                .then(|| !get("synchronized").is_empty()),
            built_on: if get("built_on_unknown").is_empty() {
                joined_date(
                    &get("built_on_year"),
                    &get("built_on_month"),
                    &get("built_on_day"),
                )
            } else {
                String::new()
            },
            description: get("description"),
            given_name: get("given_name"),
            logic: get("logic"),
            throughput: get("throughput"),
            bytes: get("bytes"),
            at_least: get("at_least"),
            design: get("design"),
            // Each category's flags are sent under its own name; parsing
            // keeps the chosen category's.
            flags: pairs
                .iter()
                .filter_map(|(k, v)| {
                    let category = k.strip_prefix("flag:")?;
                    Some((category.to_owned(), v.trim().to_owned()))
                })
                .collect(),
            flag_names: pairs
                .iter()
                .filter(|(k, _)| k.starts_with("flag:"))
                .map(|(_, v)| v.trim().to_owned())
                .collect(),
            stackable: !get("stackable").is_empty(),
            stack_rows,
            builders: all("builder", "builder_entry"),
            contributors: all("contributor", "contributor_entry"),
            tags: all("tag", "tag_entry")
                .iter()
                .map(|name| schematic::tag_name(name))
                .collect(),
        }
    }

    /// The sent values, to show again after a refusal. A category named
    /// under other shows as that name.
    pub fn fields(&self) -> Fields<'_> {
        Fields {
            category: &self.category,
            width: &self.width,
            orientation: &self.orientation,
            encoding: &self.encoding,
            latency: &self.latency,
            synchronized: self.synchronized,
            builders: &self.builders,
            contributors: &self.contributors,
            built_on: &self.built_on,
            description: &self.description,
            given_name: &self.given_name,
            logic: &self.logic,
            throughput: &self.throughput,
            bytes: &self.bytes,
            at_least: &self.at_least,
            design: &self.design,
            flags: &self.flag_names,
            stackable: self.stackable,
            stack_rows: &self.stack_rows,
        }
    }

    /// The stacking directions, checked: none unless the toggle is on, and
    /// then at least one, each way once.
    fn stacking(&self) -> Result<Vec<StackDirection>, String> {
        if !self.stackable {
            return Ok(Vec::new());
        }
        let mut directions: Vec<StackDirection> = Vec::new();
        for row in &self.stack_rows {
            if let Some(direction) = row.parse()? {
                if directions.iter().any(|d| d.name() == direction.name()) {
                    return Err("Give each stacking direction once.".to_owned());
                }
                directions.push(direction);
            }
        }
        if directions.is_empty() {
            return Err("Give a stacking step, or untick Stackable.".to_owned());
        }
        Ok(directions)
    }

    /// The category's name, and whether it is off the list: a standard one
    /// by its name or label, whatever the capitals, among `categories`, and
    /// any other text a category of the builder's own.
    pub fn category(
        &self,
        categories: &[Category],
    ) -> Result<(String, bool), String> {
        let typed = self.category.trim();
        let standard = categories.iter().find(|c| {
            c.name.eq_ignore_ascii_case(typed)
                || c.label.eq_ignore_ascii_case(typed)
                || c.name == schematic::tag_name(typed)
        });
        let (category, other) = match standard {
            Some(c) => (c.name.clone(), false),
            None => (schematic::tag_name(typed), true),
        };
        if category.is_empty() {
            return Err(
                "Choose a category, or type a name of your own.".to_owned()
            );
        }
        Ok((category, other))
    }

    /// Refuses a tag worked out from the build, or a name no tag can have.
    fn check_tags(&self) -> Result<(), String> {
        if let Some(name) = self
            .tags
            .iter()
            .find(|n| schematic::DERIVED.contains(&n.as_str()))
        {
            return Err(format!(
                "The {} tag is worked out from the build; it cannot be \
                 added by hand.",
                name.replace('_', " ")
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
        Ok(())
    }

    /// The ticks in `category`'s group of flags.
    fn flags_of(&self, category: &str) -> Vec<String> {
        self.flags
            .iter()
            .filter(|(of, _)| of == category)
            .map(|(_, flag)| flag.clone())
            .collect()
    }

    /// The checked fields, or a sentence saying which field is wrong. The
    /// category is matched among the standard `categories`.
    pub fn parse(&self, categories: &[Category]) -> Result<Parsed, String> {
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
        let (category, other) = self.category(categories)?;
        if !matches!(self.orientation.as_str(), "horz" | "vert" | "diag") {
            return Err(
                "Orientation must be Horizontal, Vertical, or Diagonal."
                    .to_owned(),
            );
        }
        // A category off the list, such as a piston door, has no width or
        // encoding.
        let encoding = if other {
            None
        } else if matches!(self.encoding.as_str(), "bin" | "hex") {
            Some(self.encoding.clone())
        } else {
            return Err("Encoding must be Binary or Hex.".to_owned());
        };
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
        self.check_tags()?;
        if self.given_name.trim().chars().count() > 60 {
            return Err("A name is at most 60 characters.".to_owned());
        }
        let logic = text(&self.logic);
        if logic
            .as_deref()
            .is_some_and(|l| !matches!(l, "comb" | "seq"))
        {
            return Err("Logic must be Combinational or Sequential.".to_owned());
        }
        let built_on = text(&self.built_on);
        if built_on.as_deref().is_some_and(|d| !is_date(d)) {
            return Err(
                "Built on must be a date such as 2024-02-02.".to_owned()
            );
        }
        let flags = self.flags_of(&category);
        Ok(Parsed {
            category,
            width: if other {
                None
            } else {
                number(&self.width, "Width")?.filter(|w| *w > 0)
            },
            orientation: self.orientation.clone(),
            encoding,
            latency: number(&self.latency, "Latency")?,
            synchronized: self.synchronized,
            built_on,
            description: text(&self.description),
            given_name: text(&self.given_name),
            logic,
            throughput: if other {
                None
            } else {
                number(&self.throughput, "Throughput")?.filter(|t| *t > 0)
            },
            bytes: number(&self.bytes, "Capacity")?.filter(|b| *b > 0),
            at_least: number(&self.at_least, "At least")?.filter(|k| *k > 0),
            design: text(&self.design).unwrap_or_else(|| "other".to_owned()),
            flags,
            builders: self.builders.clone(),
            contributors: self.contributors.clone(),
            stacking: self.stacking()?,
        })
    }
}

impl Parsed {
    pub fn as_details(&self) -> Details<'_> {
        Details {
            category: &self.category,
            width: self.width,
            orientation: &self.orientation,
            encoding: self.encoding.as_deref(),
            latency: self.latency,
            synchronized: self.synchronized,
            built_on: self.built_on.as_deref(),
            description: self.description.as_deref(),
            given_name: self.given_name.as_deref(),
            logic: self.logic.as_deref(),
            throughput: self.throughput,
            bytes: self.bytes,
            at_least: self.at_least,
            design: Some(&self.design),
        }
    }
}

/// The pairs of a urlencoded body.
pub fn pairs(body: &[u8]) -> Vec<(String, String)> {
    form_urlencoded::parse(body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// The first value a form sent under `key`, or empty.
pub fn first<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, v)| v.as_str())
}

/// The date the form's year, month, and day boxes give, as `2024-02-02`,
/// a single-digit month or day taking a leading zero; empty when all are.
/// One left out leaves a date `is_date` refuses.
fn joined_date(year: &str, month: &str, day: &str) -> String {
    let [year, month, day] = [year, month, day].map(str::trim);
    if [year, month, day].iter().all(|part| part.is_empty()) {
        String::new()
    } else {
        format!("{year}-{month:0>2}-{day:0>2}")
    }
}

/// Whether `d` is a real day written as `yyyy-mm-dd`, such as `2024-02-29`.
fn is_date(d: &str) -> bool {
    let shaped = d.len() == 10
        && d.bytes().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    // Being all ASCII, it slices anywhere.
    shaped
        && match (d[..4].parse(), d[5..7].parse::<u8>(), d[8..].parse()) {
            (Ok(year), Ok(month), Ok(day)) => time::Month::try_from(month)
                .and_then(|month| {
                    time::Date::from_calendar_date(year, month, day)
                })
                .is_ok(),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schematic::Measure;

    #[test]
    fn a_date_is_a_real_day_as_yyyy_mm_dd() {
        assert!(is_date("2024-02-29"));
        assert!(!is_date("2023-02-29"));
        assert!(!is_date("2024-13-01"));
        assert!(!is_date("02/29/2024"));
        assert!(!is_date("2024-2-29"));
        assert!(!is_date("2024-02-2é"));
    }

    #[test]
    fn the_date_boxes_join_into_one_date() {
        assert_eq!(joined_date("2024", "2", " 9 "), "2024-02-09");
        assert_eq!(joined_date("", "", ""), "");
        // A part left out leaves no date.
        assert!(!is_date(&joined_date("2024", "", "09")));
    }

    fn form(stackable: bool, rows: &[(&str, &str, &str)]) -> Form {
        Form {
            stackable,
            stack_rows: rows
                .iter()
                .map(|&(direction, step, max)| StackRow {
                    direction: direction.to_owned(),
                    step: step.to_owned(),
                    max: max.to_owned(),
                })
                .collect(),
            ..Form::default()
        }
    }

    #[test]
    fn stacking_counts_only_while_ticked() {
        let off = form(false, &[("west", "2", "")]);
        assert!(off.stacking().unwrap().is_empty());
    }

    #[test]
    fn stacking_reads_each_filled_row() {
        let on = form(
            true,
            &[("west", "2", "7"), ("north", "4", ""), ("west", "", "")],
        );
        let directions = on.stacking().unwrap();
        assert_eq!(directions.len(), 2);
        assert_eq!(
            (directions[0].west, directions[0].max_copies),
            (2, Some(7))
        );
        assert_eq!((directions[1].north, directions[1].max_copies), (4, None));
        let diagonal = form(true, &[("west-up", "1", "")]).stacking().unwrap();
        assert_eq!((diagonal[0].west, diagonal[0].up), (1, 1));
    }

    #[test]
    fn stacking_refuses_what_cannot_stack() {
        for rows in [
            vec![("west", "", "")],
            vec![("west", "0", "")],
            vec![("west", "2", "1")],
            vec![("west", "2", ""), ("west", "3", "")],
            vec![("sideways", "2", "")],
        ] {
            assert!(form(true, &rows).stacking().is_err(), "{rows:?}");
        }
    }

    #[test]
    fn a_category_off_the_list_has_no_width_or_encoding() {
        let sent = Form {
            category: "8-Bit Horizontal Diode".to_owned(),
            width: "8".to_owned(),
            orientation: "horz".to_owned(),
            encoding: "bin".to_owned(),
            ..Form::default()
        };
        let parsed = sent.parse(&[]).unwrap();
        assert_eq!(parsed.category, "8-bit_horizontal_diode");
        assert_eq!((parsed.width, parsed.encoding), (None, None));
    }

    #[test]
    fn a_standard_category_is_typed_by_name_or_label() {
        let hex = Category {
            name: "hex_to_binary".to_owned(),
            label: "Hex to Binary".to_owned(),
            plural: "Hex to Binary".to_owned(),
            named: false,
            section: None,
            logic: None,
            measures: vec![Measure::Width],
            aliases: Vec::new(),
        };
        for typed in [
            "Hex to Binary",
            "hex to binary",
            "hex_to_binary",
            " HEX TO BINARY ",
        ] {
            let sent = Form {
                category: typed.to_owned(),
                width: "4".to_owned(),
                orientation: "horz".to_owned(),
                encoding: "hex".to_owned(),
                ..Form::default()
            };
            let parsed = sent.parse(std::slice::from_ref(&hex)).unwrap();
            assert_eq!(parsed.category, "hex_to_binary", "{typed}");
            assert_eq!(parsed.width, Some(4), "{typed}");
        }
    }

    #[test]
    fn a_stored_direction_shows_as_its_row() {
        let direction: StackDirection = "north=4,max=3".parse().unwrap();
        let row = StackRow::of(&direction);
        assert_eq!(
            (row.direction.as_str(), row.step.as_str(), row.max.as_str()),
            ("north", "4", "3")
        );
    }
}
