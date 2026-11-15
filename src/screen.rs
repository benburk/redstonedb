//! Screening builds: narrowing a list by what its builds are, a field at a
//! time, such as their category, width, or who built them. The values
//! picked of one field are alternatives, a build matching any of them, and
//! the fields narrow together, so `width=8,16` with `orientation=vert` is
//! the vertical builds of 8 or 16 bits.
//!
//! A field of numbers, as ticks are, takes ranges too: `3` alone, `..3` at
//! most, `8..` at least, and `100..499` between, both ends counted in.

use std::collections::{BTreeMap, HashMap};

use crate::{
    build::{Build, tag},
    category::{FlagGroup, Kind},
};

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
    Orientation,
    Encoding,
    Design,
    /// A category's flags, a group to a field: the lines its builds
    /// have, what they work out besides, and how they behave.
    Lines,
    Operations,
    Features,
    State,
    Ticks,
    By,
    Inputs,
    Buses,
    Stacks,
    Sync,
    Throughput,
    Volume,
    /// What a build keeps to, as using no torches.
    Limits,
}

impl Field {
    pub const EVERY: [Field; 19] = [
        Field::Category,
        Field::Status,
        Field::Width,
        Field::Orientation,
        Field::Encoding,
        Field::Design,
        Field::Lines,
        Field::Operations,
        Field::Features,
        Field::State,
        Field::Ticks,
        Field::By,
        Field::Inputs,
        Field::Buses,
        Field::Stacks,
        Field::Sync,
        Field::Throughput,
        Field::Volume,
        Field::Limits,
    ];

    /// Whether it is one of the fields a page leads with: what decides
    /// whether a build fits where it is wanted, and what its category
    /// alone records. The rest fold away under them until one is picked
    /// of, as how fast or small a build is orders the builds rather than
    /// narrowing them.
    pub fn leads(self) -> bool {
        matches!(
            self,
            Field::Category
                | Field::Width
                | Field::Orientation
                | Field::Encoding
                | Field::Design
                | Field::Lines
                | Field::Operations
                | Field::Features
        )
    }

    /// The group of flags it screens by, for a field of a category's
    /// flags.
    fn flags(self) -> Option<FlagGroup> {
        match self {
            Field::Lines => Some(FlagGroup::Lines),
            Field::Operations => Some(FlagGroup::Operations),
            Field::Features => Some(FlagGroup::Features),
            _ => None,
        }
    }

    /// Whether a build must match every pick of it, not any: its choices
    /// are facts a build may have several of, as a carry in and a carry
    /// out, where another field's are alternatives.
    pub fn every(self) -> bool {
        self.flags().is_some() || self == Field::Limits
    }

    /// Whether it means something only within one category, whose list
    /// gives its choices: a design or a flag, as a carry cancel is an
    /// adder's.
    fn of_a_category(self) -> bool {
        self.flags().is_some() || self == Field::Design
    }

    /// Its choices as the category named `category` lists them, each as
    /// an address writes it and as people read it, so a row offers all
    /// the category records, whether or not a build has each yet.
    fn listed(self, category: &str) -> Vec<(String, String)> {
        let Some(facts) = Kind::named(category).map(Kind::facts) else {
            return Vec::new();
        };
        match self.flags() {
            Some(group) => facts
                .flags
                .iter()
                .filter(|flag| flag.group() == Some(group))
                .map(|flag| (flag.name.to_owned(), flag.label.to_owned()))
                .collect(),
            None if self == Field::Design => facts
                .designs
                .iter()
                .map(|design| (design.name.to_owned(), design.label()))
                .collect(),
            None => Vec::new(),
        }
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
            Field::Lines => FlagGroup::Lines.name(),
            Field::Operations => FlagGroup::Operations.name(),
            Field::Features => FlagGroup::Features.name(),
            Field::Limits => "limits",
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
            Field::Lines => FlagGroup::Lines.label(),
            Field::Operations => FlagGroup::Operations.label(),
            Field::Features => FlagGroup::Features.label(),
            Field::Limits => "Limits",
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
    /// credited, pitch its copies stack at, or flag it has.
    fn of(self, s: &Build) -> Vec<(String, String)> {
        let one = |key: &str, label: &str| vec![(key.to_owned(), label.into())];
        if let Some(group) = self.flags() {
            return s
                .flags
                .iter()
                .filter(|flag| flag.group() == Some(group))
                .map(|flag| (flag.name.to_owned(), flag.label.to_owned()))
                .collect();
        }
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
            Field::Limits => s
                .tags
                .iter()
                .filter(|tag| tag.kind == tag::Kind::Restriction)
                .map(|tag| (tag.name.clone(), tag.label()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether build `s` matches `pick`, a choice of this field as an
    /// address writes it, or any range of a field of numbers. A width
    /// takes a build whose copies stack to it too, as a 1-bit adder slice
    /// makes a 16-bit adder.
    fn takes(self, s: &Build, pick: &str) -> bool {
        if pick == ALL {
            return true;
        }
        if !self.numeric() {
            return self.of(s).iter().any(|(key, _)| key == pick);
        }
        let numbers: Vec<i64> = match self {
            Field::Width => crate::search::widths(s)
                .into_iter()
                .map(|(width, _)| width)
                .collect(),
            _ => self.number(s).into_iter().collect(),
        };
        match Range::parse(pick) {
            _ if numbers.is_empty() => pick == UNKNOWN,
            Some(range) => numbers.into_iter().any(|n| range.holds(n)),
            None => false,
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

    /// The one category picked, when it is one alone.
    pub fn category(&self) -> Option<&str> {
        match self.of(Field::Category)[..] {
            [category] => Some(category),
            _ => None,
        }
    }

    /// Whether build `s` matches what is picked of `field`: any pick, or
    /// every one where the field's choices are facts a build has several
    /// of.
    fn matches(&self, s: &Build, field: Field) -> bool {
        let picks = self.of(field);
        if field.every() {
            picks.iter().all(|pick| field.takes(s, pick))
        } else {
            picks.is_empty() || picks.iter().any(|pick| field.takes(s, pick))
        }
    }

    /// Whether build `s` matches the picks of every field but `except`.
    pub fn take(&self, s: &Build, except: Option<Field>) -> bool {
        Field::EVERY
            .into_iter()
            .filter(|&field| Some(field) != except)
            .all(|field| self.matches(s, field))
    }

    /// Whether build `s` is where the picks look: of a category and a
    /// standing picked.
    fn scope(&self, s: &Build) -> bool {
        Field::EVERY
            .into_iter()
            .filter(|field| field.scopes())
            .all(|field| self.matches(s, field))
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
/// builds of the categories and standing picked have, and all their one
/// category lists of a design or flag, so a row keeps its choices as the
/// others narrow, each with how many builds it would leave. A field its
/// builds all agree on has no row, unless something is picked of it, nor
/// has a category's own field outside one category.
pub fn rows(builds: &[&Build], picks: &Picks) -> Vec<Row> {
    Field::EVERY
        .into_iter()
        .filter_map(|field| row(field, builds, picks))
        .collect()
}

/// [`rows`]' row of `field`, when it has one.
fn row(field: Field, builds: &[&Build], picks: &Picks) -> Option<Row> {
    let category = picks.category();
    // The builds the field's choices come from, and those a choice counts
    // among: the builds the other fields take, and this one's picks too
    // where a build must match them all.
    let scoped: Vec<&Build> = builds
        .iter()
        .copied()
        .filter(|s| field.scopes() || picks.scope(s))
        .collect();
    let except = (!field.every()).then_some(field);
    let counted: Vec<&Build> = builds
        .iter()
        .copied()
        .filter(|s| picks.take(s, except))
        .collect();
    let mut found: Vec<(String, String)> =
        category.map_or_else(Vec::new, |category| field.listed(category));
    for s in &scoped {
        for (key, label) in field.of(s) {
            if !found.iter().any(|(known, _)| *known == key) {
                found.push((key, label));
            }
        }
    }
    // A standard width no build here is built at is a choice all the
    // same where copies of one stack to it, as a 2-bit XOR gate's make a
    // 64-bit one: nobody builds the wide gate, and it is what is wanted.
    if field == Field::Width {
        for width in crate::ingest::evaluate::STANDARD_WIDTHS {
            let key = width.to_string();
            if !found.iter().any(|(known, _)| *known == key)
                && scoped.iter().any(|s| field.takes(s, &key))
            {
                found.push((key.clone(), key));
            }
        }
    }
    let picked = picks.of(field);
    let enough = if field.every() {
        !found.is_empty()
    } else {
        found.len() > 1
    };
    // A range no choice stands for, or a value no build here has.
    for pick in &picked {
        if !found.iter().any(|(known, _)| known == pick) {
            let label = Range::parse(pick)
                .map_or_else(|| pick.replace('_', " "), Range::label);
            found.push(((*pick).to_owned(), label));
        }
    }
    let having = |among: &[&Build], key: &str| {
        among.iter().filter(|s| field.takes(s, key)).count()
    };
    let mut choices: Vec<Choice> = found
        .into_iter()
        .map(|(key, label)| Choice {
            count: having(&counted, &key),
            all: having(&scoped, &key),
            chosen: picked.contains(&key.as_str()),
            key,
            label,
        })
        .collect();
    if field.numeric() {
        choices.sort_by_key(|c| Field::rank(&c.key));
    } else if field.by_count() {
        choices.sort_by(|a, b| b.all.cmp(&a.all).then(a.label.cmp(&b.label)));
    } else if !field.every() {
        // By name, the working first and what says a build has none of
        // the field last.
        let last = [UNKNOWN, "none", "no", "other"];
        choices.sort_by_key(|c| {
            (c.key != "working", last.contains(&&*c.key), c.key.clone())
        });
    }
    let apart = field.of_a_category() && category.is_none();
    (picks.has(field) || enough && !apart).then_some(Row { field, choices })
}

/// Where a build comes among the top: its turn within its category, then
/// its category's place among the categories, the one with the most
/// builds first. Listed by it, the best of each category lead, the most
/// popular category's first, then each category's second, and so on, so
/// the top shows what is best of every kind, not one kind's best alone.
pub type Place = (usize, usize);

/// How many of `peers` build `s` beats: those it is faster than, or those
/// it is smaller than, whichever are more, then the other, which parts
/// builds as fast as one another by their size. A build of unknown
/// latency is faster than none, and slower than any whose latency is
/// known.
fn beats(s: &Build, peers: &[&Build]) -> (usize, usize) {
    let slower = |other: &&&Build| match (s.specs.latency, other.specs.latency)
    {
        (Some(mine), Some(theirs)) => mine < theirs,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let fast = peers.iter().filter(slower).count();
    let small = peers.iter().filter(|o| s.volume() < o.volume()).count();
    (fast.max(small), fast.min(small))
}

/// Each build's place among the top, by its id. Within a category, builds
/// take turns by their peers, those that match in its measures, such as
/// width, and in whether they hold state: the best of each set of peers,
/// the largest set's first, then each set's second. Among its peers a
/// build stands by how many it [`beats`], then by its favourites, its
/// downloads, and how new it is. A build that ranks on no board has no
/// place.
pub fn places(builds: &[Build]) -> HashMap<String, Place> {
    type Peers<'a> = (&'a str, Vec<i64>, Option<crate::build::Logic>);
    let mut groups: BTreeMap<Peers, Vec<&Build>> = BTreeMap::new();
    for s in builds.iter().filter(|s| s.ranks()) {
        let measures: Option<Vec<i64>> =
            s.measures().iter().map(|&m| s.specs.of(m)).collect();
        if let Some(measures) = measures.filter(|m| !m.is_empty()) {
            groups
                .entry((s.category.as_str(), measures, s.logic()))
                .or_default()
                .push(s);
        }
    }
    // Each category's sets of peers, each best first.
    let mut categories: BTreeMap<&str, Vec<Vec<&Build>>> = BTreeMap::new();
    for ((category, ..), peers) in &groups {
        let mut standing: Vec<((usize, usize), &Build)> =
            peers.iter().map(|&s| (beats(s, peers), s)).collect();
        standing.sort_by(|(beats_a, a), (beats_b, b)| {
            beats_b
                .cmp(beats_a)
                .then(b.entry.favorites.cmp(&a.entry.favorites))
                .then(b.entry.downloads.cmp(&a.entry.downloads))
                .then(b.entry.added_at.cmp(&a.entry.added_at))
                .then(a.id.cmp(&b.id))
        });
        categories
            .entry(category)
            .or_default()
            .push(standing.into_iter().map(|(_, s)| s).collect());
    }
    let mut by_size: Vec<(&str, Vec<Vec<&Build>>)> =
        categories.into_iter().collect();
    let size = |sets: &[Vec<&Build>]| sets.iter().map(Vec::len).sum::<usize>();
    by_size.sort_by(|(a, x), (b, y)| size(y).cmp(&size(x)).then(a.cmp(b)));
    let mut places: HashMap<String, Place> = HashMap::new();
    for (among, (_, mut sets)) in by_size.into_iter().enumerate() {
        sets.sort_by_key(|set| std::cmp::Reverse(set.len()));
        let mut turns: Vec<(usize, usize, &Build)> = sets
            .iter()
            .enumerate()
            .flat_map(|(set, peers)| {
                peers
                    .iter()
                    .enumerate()
                    .map(move |(turn, &s)| (turn, set, s))
            })
            .collect();
        turns.sort_by_key(|&(turn, set, _)| (turn, set));
        for (turn, (_, _, s)) in turns.into_iter().enumerate() {
            places.insert(s.id.clone(), (turn, among));
        }
    }
    places
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

    /// An adder with the flags named `flags`.
    fn adder(flags: &[&'static str]) -> Build {
        let mut s = build("adder", 8, Some(3));
        s.flags = flags
            .iter()
            .map(|&name| crate::category::Flag { name, label: name })
            .collect();
        s
    }

    /// Within a category, a row offers every flag its list gives of a
    /// group, had or not, and a build must have every one picked.
    #[test]
    fn a_category_offers_its_flags_and_a_build_must_have_each_picked() {
        let builds = [
            adder(&["carry_in", "carry_out"]),
            adder(&["carry_in"]),
            adder(&[]),
        ];
        let builds: Vec<&Build> = builds.iter().collect();
        let within = |io: &str| {
            let mut given = vec![(Field::Category, "adder")];
            if !io.is_empty() {
                given.push((Field::Lines, io));
            }
            picks(&given)
        };
        assert_eq!(
            counts(&rows(&builds, &within("")), Field::Lines),
            [
                ("carry_in".to_owned(), 2, false),
                ("carry_out".to_owned(), 1, false),
                ("inverted_carry_in".to_owned(), 0, false),
                ("inverted_carry_out".to_owned(), 0, false),
            ]
        );
        let both = within("carry_in,carry_out");
        assert_eq!(builds.iter().filter(|s| both.take(s, None)).count(), 1);
        // A choice counts the builds that have it with those picked.
        assert_eq!(
            counts(&rows(&builds, &within("carry_out")), Field::Lines)[0],
            ("carry_in".to_owned(), 1, false)
        );
        // Across categories a flag means nothing, so it has no row.
        assert_eq!(counts(&rows(&builds, &Picks::default()), Field::Lines), []);
    }

    /// The top take turns: the best of each category, the most popular
    /// first, then each category's second; and within a category, the
    /// best of each width, the most common first. Among its peers a build
    /// stands by how many it beats, as faster or as smaller.
    #[test]
    fn the_top_take_turns_by_category_and_width() {
        let sized = |id: &str, category: &str, width, ticks, blocks| {
            let mut s = build(category, width, Some(ticks));
            s.id = id.to_owned();
            s.file.size = (blocks, 1, 1);
            s
        };
        let builds = [
            sized("fast", "adder", 8, 3, 900),
            sized("fast-and-small", "adder", 8, 3, 700),
            sized("small", "adder", 8, 9, 400),
            sized("slow-and-big", "adder", 8, 9, 950),
            sized("wide", "adder", 16, 20, 5000),
            sized("lone", "parity", 8, 2, 50),
        ];
        let places = places(&builds);
        let mut top: Vec<&str> = builds.iter().map(|s| s.id.as_str()).collect();
        top.sort_by_key(|id| places[*id]);
        assert_eq!(
            top,
            [
                // Each category's best, the adders' first: the one that
                // beats the most of its peers, three of them on size.
                "small",
                "lone",
                // Then the adders' other width, and the rest of the first.
                "wide",
                "fast-and-small",
                "fast",
                "slow-and-big",
            ]
        );
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
