//! Screening builds: narrowing a list by what its builds are, a field at a
//! time, such as their category, width, or who built them. The values
//! picked of one field are alternatives, a build matching any of them, and
//! the fields narrow together, so `width=8,16` with `orientation=vert` is
//! the vertical builds of 8 or 16 bits.
//!
//! A field of numbers, as ticks are, takes ranges too: `3` alone, `..3` at
//! most, `8..` at least, and `100..499` between, both ends counted in.

use std::collections::BTreeMap;

use crate::build::{Build, tag};

/// A pick matching the builds with no value of a field of numbers, or of
/// unknown logic or timing.
pub const UNKNOWN: &str = "unknown";

/// A pick taking every build, for a field that narrows when nothing is
/// picked of it, as the status does.
pub const ALL: &str = "all";

/// The greatest count of ticks with a choice of its own; the slower share
/// one.
const TICKS_EACH: i64 = 8;

/// Where the choices of volume part, each from one bound up to the next.
const VOLUMES: [i64; 5] = [100, 250, 500, 1000, 5000];

/// Something of a build to screen by, in the order a page lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Category,
    Status,
    Width,
    State,
    Orientation,
    Encoding,
    Design,
    Ticks,
    By,
    Inputs,
    Buses,
    Stacks,
    Sync,
    Throughput,
    Volume,
}

impl Field {
    pub const EVERY: [Field; 15] = [
        Field::Category,
        Field::Status,
        Field::Width,
        Field::State,
        Field::Orientation,
        Field::Encoding,
        Field::Design,
        Field::Ticks,
        Field::By,
        Field::Inputs,
        Field::Buses,
        Field::Stacks,
        Field::Sync,
        Field::Throughput,
        Field::Volume,
    ];

    /// Whether it is one of the fields a page leads with, the rest
    /// folding away under them until one is picked of.
    pub fn leads(self) -> bool {
        matches!(
            self,
            Field::Category
                | Field::Width
                | Field::Orientation
                | Field::Encoding
        )
    }

    /// As an address names it, such as `width`.
    pub fn name(self) -> &'static str {
        match self {
            Field::Category => "category",
            Field::Status => "status",
            Field::Width => "width",
            Field::State => "state",
            Field::Orientation => "orientation",
            Field::Encoding => "encoding",
            Field::Inputs => "inputs",
            Field::Buses => "buses",
            Field::Design => "design",
            Field::Stacks => "stacks",
            Field::Sync => "sync",
            Field::Ticks => "latency",
            Field::Throughput => "throughput",
            Field::Volume => "volume",
            Field::By => "by",
        }
    }

    /// As people read it, such as `Width`.
    pub fn label(self) -> &'static str {
        match self {
            Field::Category => "Category",
            Field::Status => "Status",
            Field::Width => "Width",
            Field::State => "State",
            Field::Orientation => "Orientation",
            Field::Encoding => "Encoding",
            Field::Inputs => "Inputs",
            Field::Buses => "Buses",
            Field::Design => "Design",
            Field::Stacks => "Stacks",
            Field::Sync => "Timing",
            Field::Ticks => "Ticks",
            Field::Throughput => "Throughput",
            Field::Volume => "Volume",
            Field::By => "Built by",
        }
    }

    /// The field an address names `name`.
    pub fn named(name: &str) -> Option<Field> {
        Field::EVERY.into_iter().find(|field| field.name() == name)
    }

    /// What is picked of it when an address picks nothing: only the
    /// working builds list until the rest are asked for.
    fn default(self) -> &'static [&'static str] {
        match self {
            Field::Status => &["working"],
            _ => &[],
        }
    }

    /// Whether it says where its builds are looked for, a category and
    /// their standing, rather than narrowing them by what they are: the
    /// choices of every other field are those its builds have.
    fn scopes(self) -> bool {
        matches!(self, Field::Category | Field::Status)
    }

    /// Whether its choices go most builds first, as people do, rather
    /// than in their own order.
    pub fn by_count(self) -> bool {
        matches!(self, Field::Category | Field::Design | Field::By)
    }

    /// Whether it is a field of numbers, which takes ranges.
    fn numeric(self) -> bool {
        matches!(
            self,
            Field::Width | Field::Ticks | Field::Throughput | Field::Volume
        )
    }

    /// Build `s`'s number of it, for a field of numbers; `None` where the
    /// build's is unknown.
    fn number(self, s: &Build) -> Option<i64> {
        match self {
            Field::Width => s.specs.width,
            Field::Ticks => s.specs.latency,
            Field::Throughput => s.specs.throughput,
            Field::Volume => Some(s.volume()),
            _ => None,
        }
    }

    /// The choice build `s`'s number `n` of it falls under, as an address
    /// writes it and as people read it.
    fn bucket(self, n: i64) -> (String, String) {
        match self {
            Field::Ticks | Field::Throughput if n > TICKS_EACH => {
                let from = TICKS_EACH + 1;
                (format!("{from}.."), format!("{from}+"))
            }
            Field::Volume => {
                let above = VOLUMES.iter().rev().find(|&&bound| n >= bound);
                let below = VOLUMES.iter().find(|&&bound| n < bound);
                match (above, below) {
                    (None, Some(to)) => {
                        (format!("..{}", to - 1), format!("Under {to}"))
                    }
                    (Some(from), Some(to)) => (
                        format!("{from}..{}", to - 1),
                        format!("{from} to {}", to - 1),
                    ),
                    (Some(from), None) => {
                        (format!("{from}.."), format!("{from}+"))
                    }
                    (None, None) => (n.to_string(), n.to_string()),
                }
            }
            _ => (n.to_string(), n.to_string()),
        }
    }

    /// Where a choice of numbers sorts among its field's: by where its
    /// range starts, the unknown last.
    fn rank(key: &str) -> i64 {
        match Range::parse(key) {
            Some(range) => range.from.unwrap_or(i64::MIN),
            None => i64::MAX,
        }
    }

    /// The choices build `s` falls under, each as an address writes it
    /// and as people read it: one for most fields, and each person
    /// credited or pitch its copies stack at.
    fn of(self, s: &Build) -> Vec<(String, String)> {
        let one = |key: &str, label: &str| vec![(key.to_owned(), label.into())];
        if self.numeric() {
            return match self.number(s) {
                Some(n) => vec![self.bucket(n)],
                None => one(UNKNOWN, "Unknown"),
            };
        }
        match self {
            Field::Category => match s.kind() {
                Some(_) => {
                    let label = crate::category::Category::all()
                        .into_iter()
                        .find(|category| category.name == s.category)
                        .map_or_else(|| s.category.clone(), |c| c.plural);
                    one(&s.category, &label)
                }
                None => one("other", "Other"),
            },
            Field::Status => {
                if s.verified() {
                    one("working", "Working")
                } else if s.untested() {
                    one(tag::UNTESTED, "Untested")
                } else {
                    one(tag::FAILING, "Failing")
                }
            }
            Field::State => match s.logic() {
                Some(logic) => one(logic.name(), logic.label()),
                None => one(UNKNOWN, "Unknown"),
            },
            Field::Orientation => {
                one(s.specs.orientation.name(), s.specs.orientation.label())
            }
            Field::Encoding => match s.specs.encoding {
                Some(encoding) => one(encoding.name(), encoding.label()),
                None => one("none", "None"),
            },
            Field::Inputs => {
                if s.interleaved_inputs() {
                    one("interleaved", "Interleaved")
                } else {
                    one("split", "Split")
                }
            }
            Field::Buses => {
                if s.unaligned {
                    one(tag::UNALIGNED, "Unaligned")
                } else {
                    one("aligned", "Aligned")
                }
            }
            Field::Design => match &s.specs.design {
                Some(design) => {
                    let label = s.listed_design().map_or_else(
                        || design.replace('_', " "),
                        crate::category::Design::label,
                    );
                    one(design, &label)
                }
                None => one("none", "None"),
            },
            Field::Stacks => match pitches(s) {
                _ if s.stacking.is_empty() => one("no", "Not stackable"),
                pitches if pitches.is_empty() => one("other", "Other pitch"),
                pitches => pitches,
            },
            Field::Sync => match s.specs.synchronized {
                Some(true) => one(tag::SYNCED, "Synced"),
                Some(false) => one(tag::UNSYNCED, "Unsynced"),
                None => one(UNKNOWN, "Unknown"),
            },
            Field::By => s
                .credits
                .everyone()
                .map(|name| (name.to_lowercase(), name.clone()))
                .collect(),
            Field::Width | Field::Ticks | Field::Throughput | Field::Volume => {
                Vec::new()
            }
        }
    }

    /// Whether build `s` matches `pick`, a choice of this field as an
    /// address writes it, or any range of a field of numbers.
    fn takes(self, s: &Build, pick: &str) -> bool {
        if pick == ALL {
            return true;
        }
        if !self.numeric() {
            return self.of(s).iter().any(|(key, _)| key == pick);
        }
        match (self.number(s), Range::parse(pick)) {
            (Some(n), Some(range)) => range.holds(n),
            (Some(_), None) => false,
            (None, _) => pick == UNKNOWN,
        }
    }
}

/// The pitch classes build `s`'s copies stack at where they add bits, such
/// as `2-wide`, each as a choice.
fn pitches(s: &Build) -> Vec<(String, String)> {
    s.every_tag()
        .iter()
        .filter(|tag| {
            tag::is_pitch_stackable(&tag.name)
                && !tag::is_word_stackable(&tag.name)
        })
        .filter_map(|tag| {
            let pitch =
                tag.name.strip_suffix(tag::STACKABLE)?.strip_suffix('_')?;
            Some((pitch.to_owned(), pitch.to_owned()))
        })
        .collect()
}

/// A range of numbers as an address writes it, both ends counted in: `3`,
/// `..3`, `8..`, or `100..499`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

impl Range {
    pub fn parse(text: &str) -> Option<Range> {
        let bound = |text: &str| match text.trim() {
            "" => Some(None),
            number => number.parse::<i64>().ok().map(Some),
        };
        if let Some((from, to)) = text.split_once("..") {
            let range = Range {
                from: bound(from)?,
                to: bound(to)?,
            };
            return (range.from.is_some() || range.to.is_some())
                .then_some(range);
        }
        let n = text.trim().parse::<i64>().ok()?;
        Some(Range {
            from: Some(n),
            to: Some(n),
        })
    }

    fn holds(self, n: i64) -> bool {
        self.from.is_none_or(|from| n >= from)
            && self.to.is_none_or(|to| n <= to)
    }

    /// As people read one no choice stands for, such as `3 to 5`.
    fn label(self) -> String {
        match (self.from, self.to) {
            (Some(from), Some(to)) if from == to => from.to_string(),
            (Some(from), Some(to)) => format!("{from} to {to}"),
            (Some(from), None) => format!("{from}+"),
            (None, Some(to)) => format!("Up to {to}"),
            (None, None) => String::new(),
        }
    }
}

/// What is picked of each field, as an address gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picks(BTreeMap<Field, Vec<String>>);

impl Picks {
    /// Whether nothing is picked, so only the defaults narrow.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether an address picks anything of `field`.
    pub fn has(&self, field: Field) -> bool {
        self.0.contains_key(&field)
    }

    /// The fields with something picked and their picks, as an address
    /// writes them.
    pub fn given(&self) -> impl Iterator<Item = (Field, &[String])> {
        self.0.iter().map(|(&field, picks)| (field, &picks[..]))
    }

    /// The picks an address gives of `field`, as `8,16`, its alternatives
    /// between commas, in place of any it had. Each is lowercase, as a
    /// name is matched whatever its capitals.
    pub fn set(&mut self, field: Field, text: &str) {
        let picks: Vec<String> = text
            .split(',')
            .map(|pick| pick.trim().to_lowercase())
            .filter(|pick| !pick.is_empty())
            .collect();
        if picks.is_empty() {
            self.0.remove(&field);
        } else {
            self.0.insert(field, picks);
        }
    }

    /// What narrows `field`: its picks, or its default when it has none;
    /// nothing when every build is asked for.
    pub fn of(&self, field: Field) -> Vec<&str> {
        match self.0.get(&field) {
            Some(picks) if picks.iter().any(|pick| pick == ALL) => Vec::new(),
            Some(picks) => picks.iter().map(String::as_str).collect(),
            None => field.default().to_vec(),
        }
    }

    /// The picks with `field`'s set to `picks`: left out where they are
    /// its default, and every build asked for where there are none though
    /// its default narrows.
    fn with(&self, field: Field, picks: Vec<String>) -> Picks {
        let mut next = self.clone();
        if picks
            .iter()
            .map(String::as_str)
            .eq(field.default().iter().copied())
        {
            next.0.remove(&field);
        } else if picks.is_empty() {
            next.0.insert(field, vec![ALL.to_owned()]);
        } else {
            next.0.insert(field, picks);
        }
        next
    }

    /// The picks with `pick` of `field` picked if it was not, and not if
    /// it was.
    #[must_use]
    pub fn toggled(&self, field: Field, pick: &str) -> Picks {
        let mut picks: Vec<String> =
            self.of(field).into_iter().map(str::to_owned).collect();
        match picks.iter().position(|p| p == pick) {
            Some(i) => {
                picks.remove(i);
            }
            None => picks.push(pick.to_owned()),
        }
        self.with(field, picks)
    }

    /// The picks with nothing narrowing `field`.
    #[must_use]
    pub fn any(&self, field: Field) -> Picks {
        self.with(field, Vec::new())
    }

    /// Whether build `s` matches a pick of every field but `except`.
    pub fn take(&self, s: &Build, except: Option<Field>) -> bool {
        Field::EVERY
            .into_iter()
            .filter(|&field| Some(field) != except)
            .all(|field| {
                let picks = self.of(field);
                picks.is_empty()
                    || picks.iter().any(|pick| field.takes(s, pick))
            })
    }

    /// Whether build `s` is where the picks look: of a category and a
    /// standing picked.
    fn scope(&self, s: &Build) -> bool {
        Field::EVERY
            .into_iter()
            .filter(|field| field.scopes())
            .all(|field| {
                let picks = self.of(field);
                picks.is_empty()
                    || picks.iter().any(|pick| field.takes(s, pick))
            })
    }
}

/// One choice of a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// As an address writes it.
    pub key: String,
    pub label: String,
    /// How many builds picking it adds or keeps, the other fields
    /// narrowing as they do.
    pub count: usize,
    /// How many builds have it whatever else is picked, which orders a
    /// row of many, so its choices keep their places as the others narrow.
    pub all: usize,
    pub chosen: bool,
}

/// A field and its choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub field: Field,
    pub choices: Vec<Choice>,
}

/// The fields to narrow `builds` by, each with its choices: those the
/// builds of the categories and standing picked have, so a row keeps its
/// choices as the others narrow, each with how many builds it would
/// leave. A field its builds all agree on has no row, unless something is
/// picked of it.
pub fn rows(builds: &[&Build], picks: &Picks) -> Vec<Row> {
    Field::EVERY
        .into_iter()
        .filter_map(|field| {
            let mut choices: Vec<Choice> = Vec::new();
            for s in builds {
                if !field.scopes() && !picks.scope(s) {
                    continue;
                }
                let counts = picks.take(s, Some(field));
                for (key, label) in field.of(s) {
                    let found = choices.iter().position(|c| c.key == key);
                    let i = found.unwrap_or_else(|| {
                        choices.push(Choice {
                            key,
                            label,
                            count: 0,
                            all: 0,
                            chosen: false,
                        });
                        choices.len() - 1
                    });
                    choices[i].count += usize::from(counts);
                    choices[i].all += 1;
                }
            }
            let picked = picks.of(field);
            let several = choices.len() > 1;
            for pick in &picked {
                match choices.iter_mut().find(|c| c.key == *pick) {
                    Some(choice) => choice.chosen = true,
                    // A range no choice stands for, or a value no build
                    // here has.
                    None => choices.push(Choice {
                        key: (*pick).to_owned(),
                        label: Range::parse(pick).map_or_else(
                            || pick.replace('_', " "),
                            Range::label,
                        ),
                        count: builds
                            .iter()
                            .filter(|s| {
                                picks.take(s, Some(field))
                                    && field.takes(s, pick)
                            })
                            .count(),
                        all: 0,
                        chosen: true,
                    }),
                }
            }
            if field.numeric() {
                choices.sort_by_key(|c| Field::rank(&c.key));
            } else if field.by_count() {
                choices.sort_by(|a, b| {
                    b.all.cmp(&a.all).then(a.label.cmp(&b.label))
                });
            } else {
                // By name, the working first and what says a build has
                // none of the field last.
                let last = [UNKNOWN, "none", "no", "other"];
                choices.sort_by_key(|c| {
                    (c.key != "working", last.contains(&&*c.key), c.key.clone())
                });
            }
            // A design is its category's, as a carry cancel is an adder's,
            // so it has a row only within one.
            let apart =
                field == Field::Design && picks.of(Field::Category).len() != 1;
            (picks.0.contains_key(&field) || several && !apart)
                .then_some(Row { field, choices })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{interface::Orientation, search::tests::schematic};

    fn build(category: &str, width: i64, ticks: Option<i64>) -> Build {
        let mut s = schematic(category, Orientation::Horizontal, &[]);
        s.specs.width = Some(width);
        s.specs.latency = ticks;
        s
    }

    fn picks(given: &[(Field, &str)]) -> Picks {
        let mut picks = Picks::default();
        for &(field, text) in given {
            picks.set(field, text);
        }
        picks
    }

    fn counts(rows: &[Row], field: Field) -> Vec<(String, usize, bool)> {
        rows.iter()
            .find(|row| row.field == field)
            .map(|row| {
                row.choices
                    .iter()
                    .map(|c| (c.key.clone(), c.count, c.chosen))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The picks of one field are alternatives, and the fields narrow
    /// together.
    #[test]
    fn picks_of_a_field_are_alternatives_and_fields_narrow_together() {
        let builds = [
            build("adder", 8, Some(3)),
            build("adder", 16, Some(3)),
            build("adder", 16, Some(5)),
            build("subtractor", 8, Some(3)),
        ];
        let taken = |given: &[(Field, &str)]| {
            let picks = picks(given);
            builds.iter().filter(|s| picks.take(s, None)).count()
        };
        assert_eq!(taken(&[]), 4);
        assert_eq!(taken(&[(Field::Width, "8,16")]), 4);
        assert_eq!(taken(&[(Field::Width, "16")]), 2);
        assert_eq!(
            taken(&[(Field::Width, "16"), (Field::Category, "adder")]),
            2
        );
        assert_eq!(taken(&[(Field::Width, "16"), (Field::Ticks, "..3")]), 1);
        assert_eq!(
            taken(&[
                (Field::Category, "adder,subtractor"),
                (Field::Ticks, "3")
            ]),
            3
        );
    }

    /// A field of numbers takes a range, either end left open, and
    /// `unknown` the builds with none.
    #[test]
    fn a_field_of_numbers_takes_ranges() {
        let s = build("adder", 8, Some(4));
        let unknown = build("adder", 8, None);
        for (pick, takes) in [
            ("4", true),
            ("..4", true),
            ("4..", true),
            ("2..5", true),
            ("5..", false),
            ("..3", false),
            ("unknown", false),
            ("fast", false),
        ] {
            assert_eq!(Field::Ticks.takes(&s, pick), takes, "{pick}");
        }
        assert!(Field::Ticks.takes(&unknown, "unknown"));
        assert!(!Field::Ticks.takes(&unknown, "..4"));
    }

    /// A choice counts the builds picking it would leave: those the other
    /// fields take, whatever is picked of its own.
    #[test]
    fn a_choice_counts_what_the_other_fields_leave() {
        let builds = [
            build("adder", 8, Some(3)),
            build("adder", 16, Some(3)),
            build("adder", 16, Some(12)),
        ];
        let builds: Vec<&Build> = builds.iter().collect();
        let rows = rows(&builds, &picks(&[(Field::Width, "8")]));
        assert_eq!(
            counts(&rows, Field::Width),
            [("8".to_owned(), 1, true), ("16".to_owned(), 2, false)]
        );
        // The slow share a choice, and one no build picked has stays, at
        // none.
        assert_eq!(
            counts(&rows, Field::Ticks),
            [("3".to_owned(), 1, false), ("9..".to_owned(), 0, false)]
        );
        // All agree on their category, so it has no row.
        assert_eq!(counts(&rows, Field::Category), []);
    }

    /// Only working builds list until the rest are asked for, and picking
    /// the last choice off asks for all.
    #[test]
    fn the_status_narrows_to_working_builds_by_default() {
        let none = Picks::default();
        assert_eq!(none.of(Field::Status), ["working"]);
        let both = none.toggled(Field::Status, "failing");
        assert_eq!(both.of(Field::Status), ["working", "failing"]);
        assert_eq!(both.toggled(Field::Status, "failing"), none);
        let all = none.toggled(Field::Status, "working");
        assert!(all.of(Field::Status).is_empty());
        assert_eq!(all, none.any(Field::Status));
        assert_eq!(
            all.toggled(Field::Status, "failing").of(Field::Status),
            ["failing"]
        );
    }

    /// Volumes fall under choices by size.
    #[test]
    fn volumes_fall_under_choices_by_size() {
        let key = |n| Field::Volume.bucket(n).0;
        assert_eq!(key(99), "..99");
        assert_eq!(key(100), "100..249");
        assert_eq!(key(4999), "1000..4999");
        assert_eq!(key(5000), "5000..");
    }
}
