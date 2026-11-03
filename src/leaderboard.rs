//! Leaderboards: a category's builds ranked against the others that match
//! them in its measures, such as width, then state, orientation, and
//! design, since only those compete: a stateful build never meets a
//! stateless one.

use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use crate::{
    category::{Flag, Measure, NO_DESIGN},
    models::{
        schematic::{Logic, Orientation, Schematic},
        tag::{Kind, Tag},
    },
};

/// How many builds each ranking shows.
pub const SHOWN: usize = 10;

/// A build's logic as a board narrows by it. A build of unknown logic
/// competes only with others of unknown logic, listed after the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Known(Logic),
    Unknown,
}

impl State {
    fn of(s: &Schematic) -> Self {
        s.logic.map_or(State::Unknown, State::Known)
    }

    /// As a leaderboard's address writes it, such as `stateless`.
    pub fn name(self) -> &'static str {
        match self {
            State::Known(logic) => logic.name(),
            State::Unknown => "unknown",
        }
    }

    /// As people read it, such as `Stateless`.
    pub fn label(self) -> &'static str {
        match self {
            State::Known(logic) => logic.label(),
            State::Unknown => "Unknown",
        }
    }
}

impl FromStr for State {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if text.trim().eq_ignore_ascii_case("unknown") {
            Ok(State::Unknown)
        } else {
            text.parse().map(State::Known)
        }
    }
}

/// What a leaderboard's address asks for, such as
/// `?width=8&orientation=horz&design=carry_cancel&ticks=3&tags=synced`, each
/// part optional; `None` takes the default. A measure goes under its own
/// name, such as `throughput` for a serial category, `at_least` for a
/// threshold, and `pulse` for a monostable.
#[derive(Debug, Default, Clone)]
pub struct Wanted {
    /// A value of each measure asked for, such as a width of 8.
    pub measures: Vec<(Measure, i64)>,
    pub logic: Option<State>,
    /// `None` for every orientation, where they compete.
    pub orientation: Option<Orientation>,
    /// A design's name, or `other` for builds of none; `None` for every
    /// design, where they compete.
    pub design: Option<String>,
    /// A latency; `None` for every latency.
    pub ticks: Option<i64>,
    /// Tags each build must have, by name, as [`ranked_tags`] gives them.
    pub tags: Vec<String>,
}

impl Wanted {
    /// The query string `raw`, without its `?`. A value that names nothing,
    /// such as an empty one, is as good as none.
    pub fn parse(raw: &str) -> Self {
        let mut given: Vec<(String, String)> =
            form_urlencoded::parse(raw.as_bytes())
                .into_owned()
                .collect();
        // The last of a repeated key counts.
        given.reverse();
        let value = |key: &str| {
            given
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, value)| value.as_str())
        };
        Wanted {
            measures: Measure::ALL
                .iter()
                .filter_map(|&m| Some((m, parsed(value(m.name()))?)))
                .collect(),
            logic: parsed(value("logic")),
            orientation: parsed(value("orientation")),
            design: value("design")
                .filter(|d| !d.is_empty())
                .map(str::to_owned),
            ticks: parsed(value("ticks")),
            tags: value("tags")
                .into_iter()
                .flat_map(|tags| tags.split(','))
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Its address under `path`, the board's page, as [`Wanted::parse`]
    /// reads it.
    pub fn href(&self, path: &str) -> String {
        let mut params = form_urlencoded::Serializer::new(String::new());
        for (measure, n) in &self.measures {
            params.append_pair(measure.name(), &n.to_string());
        }
        let ticks = self.ticks.map(|t| t.to_string());
        let tags = (!self.tags.is_empty()).then(|| self.tags.join(","));
        let named = [
            ("logic", self.logic.map(State::name)),
            ("orientation", self.orientation.map(Orientation::name)),
            ("design", self.design.as_deref()),
            ("ticks", ticks.as_deref()),
            ("tags", tags.as_deref()),
        ];
        for (name, value) in named {
            if let Some(value) = value {
                params.append_pair(name, value);
            }
        }
        let params = params.finish();
        if params.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{params}")
        }
    }

    fn measure(&self, measure: Measure) -> Option<i64> {
        self.measures
            .iter()
            .find(|(m, _)| *m == measure)
            .map(|(_, n)| *n)
    }
}

/// A value an address gives; one naming nothing is as good as none.
fn parsed<T: FromStr>(value: Option<&str>) -> Option<T> {
    value?.parse().ok()
}

/// One value a board can be narrowed to, with how many builds have it.
#[derive(Debug, PartialEq, Eq)]
pub struct Choice<V> {
    pub value: V,
    pub label: String,
    pub count: usize,
}

/// One measure a board is narrowed by: each value its builds have, and
/// the one chosen.
#[derive(Debug)]
pub struct Split {
    pub measure: Measure,
    pub choices: Vec<Choice<i64>>,
    pub chosen: Option<i64>,
}

/// One board: the choices that narrowed it, the ones chosen, and the
/// builds left.
#[derive(Debug)]
pub struct Board<'a> {
    /// Its category's measures, in order.
    pub splits: Vec<Split>,
    /// Stateless and stateful; empty when the builds are all one.
    pub logics: Vec<Choice<State>>,
    pub logic: Option<State>,
    pub orientations: Vec<Choice<Orientation>>,
    /// `None` for every orientation together.
    pub orientation: Option<Orientation>,
    /// Empty when the category has a single design.
    pub designs: Vec<Choice<String>>,
    /// `None` for every design together.
    pub design: Option<String>,
    /// Whether "every design" is a choice.
    pub all_designs: bool,
    /// The latencies of the builds left by the choices before.
    pub latencies: Vec<Choice<i64>>,
    /// `None` for every latency.
    pub ticks: Option<i64>,
    /// The tags builds left by the choices before have, but for their
    /// designs and orientations, chosen above.
    pub tag_choices: Vec<Choice<String>>,
    /// The tags each build has.
    pub tags: Vec<String>,
    pub builds: Vec<&'a Schematic>,
}

/// The board of `category`, whose measures are `measures`, that `wanted`
/// asks for. Each measure and state not asked for falls to the most common
/// value among the builds the earlier choices leave, so a board starts
/// where most builds are; every orientation, latency, and design compete
/// unless asked for, but for a category's one design. A build missing a
/// measure, such as one of unknown width, competes on no board, nor does
/// one not verified as its category.
pub fn board<'a>(
    category: &[&'a Schematic],
    wanted: &Wanted,
    measures: &[Measure],
) -> Board<'a> {
    let mut ranked: Vec<&Schematic> =
        category.iter().copied().filter(|s| s.verified()).collect();
    let mut splits = Vec::new();
    for &measure in measures {
        ranked.retain(|s| measure.of(s).is_some());
        let (choices, chosen, left) =
            narrow(ranked, wanted.measure(measure), |s| {
                let n = measure.of(s).unwrap_or_default();
                (n, measure.lead(n))
            });
        ranked = left;
        splits.push(Split {
            measure,
            choices,
            chosen,
        });
    }

    // Stateless and stateful builds never compete: a stateful build's
    // latency is no stateless one's.
    let (logics, logic, mut ranked) = narrow(ranked, wanted.logic, |s| {
        let state = State::of(s);
        (state, state.label().to_owned())
    });
    let orientations = choices(&ranked, |s| {
        (s.orientation, s.orientation.label().to_owned())
    });
    let orientation = wanted.orientation;
    ranked.retain(|s| orientation.is_none_or(|o| s.orientation == o));

    // By the short name people say, such as CCA, where there is one.
    let designs = choices(&ranked, |s| {
        let label = s
            .design_abbreviation
            .clone()
            .or_else(|| s.design_label())
            .unwrap_or_else(|| "Other".to_owned());
        (s.design.as_deref().unwrap_or(NO_DESIGN).to_owned(), label)
    });
    let all_designs = designs.len() > 1;
    let design = wanted.design.clone().or_else(|| {
        (!all_designs)
            .then(|| most_common(&designs).cloned())
            .flatten()
    });
    ranked.retain(|s| {
        design
            .as_deref()
            .is_none_or(|d| s.design.as_deref().unwrap_or(NO_DESIGN) == d)
    });
    let latencies = choices(&ranked, |s| {
        let ticks = s.latency.unwrap_or(i64::MAX);
        let label = s.latency.map_or("Unknown".to_owned(), |t| t.to_string());
        (ticks, label)
    });
    let ticks = wanted.ticks;
    ranked.retain(|s| ticks.is_none_or(|t| s.latency == Some(t)));
    let mut tag_choices: Vec<Choice<String>> = Vec::new();
    for s in &ranked {
        for tag in ranked_tags(s) {
            if matches!(tag.kind, Kind::Design | Kind::Orientation) {
                continue;
            }
            match tag_choices.iter_mut().find(|c| c.value == tag.name) {
                Some(choice) => choice.count += 1,
                None => tag_choices.push(Choice {
                    label: tag.label(),
                    value: tag.name,
                    count: 1,
                }),
            }
        }
    }
    tag_choices.sort_by(|a, b| a.label.cmp(&b.label));
    let tags = wanted.tags.clone();
    let builds = ranked
        .into_iter()
        .filter(|s| {
            let own = ranked_tags(s);
            tags.iter().all(|t| own.iter().any(|tag| &tag.name == t))
        })
        .collect();
    Board {
        splits,
        logics: if logics.len() > 1 { logics } else { Vec::new() },
        logic,
        orientations,
        orientation,
        designs: if designs.len() > 1 {
            designs
        } else {
            Vec::new()
        },
        design,
        all_designs,
        latencies,
        ticks,
        tag_choices,
        tags,
        builds,
    }
}

/// Narrows `ranked` to the builds of one `(value, label)` of `key`: the
/// `wanted` value, else the most common. Returns every value's choice, the
/// value chosen, and the builds left.
fn narrow<V: Ord + Clone>(
    ranked: Vec<&Schematic>,
    wanted: Option<V>,
    key: impl Fn(&Schematic) -> (V, String),
) -> (Vec<Choice<V>>, Option<V>, Vec<&Schematic>) {
    let options = choices(&ranked, &key);
    let chosen = wanted.or_else(|| most_common(&options).cloned());
    let ranked = ranked
        .into_iter()
        .filter(|s| chosen.as_ref() == Some(&key(s).0))
        .collect();
    (options, chosen, ranked)
}

/// Each distinct `(value, label)` of `builds`, in value order, with its
/// count.
fn choices<V: Ord>(
    builds: &[&Schematic],
    key: impl Fn(&Schematic) -> (V, String),
) -> Vec<Choice<V>> {
    let mut counts: BTreeMap<V, (String, usize)> = BTreeMap::new();
    for s in builds {
        let (value, label) = key(s);
        counts.entry(value).or_insert((label, 0)).1 += 1;
    }
    counts
        .into_iter()
        .map(|(value, (label, count))| Choice {
            value,
            label,
            count,
        })
        .collect()
}

/// The value with the highest count, the first of a tie.
fn most_common<V>(choices: &[Choice<V>]) -> Option<&V> {
    choices
        .iter()
        .rev()
        .max_by_key(|c| c.count)
        .map(|c| &c.value)
}

/// Copies of each of `builds` stacked into each wider build they make, as
/// they compete at its width; see `Schematic::widened_to`.
pub fn stacked<'a>(
    builds: impl IntoIterator<Item = &'a Schematic>,
) -> Vec<Schematic> {
    builds
        .into_iter()
        .flat_map(|s| s.widened.iter().filter_map(|w| s.widened_to(w)))
        .collect()
}

/// A build with its place: one more than the builds ranked strictly ahead
/// of it, so a tie shares a place.
pub type Placed<'a> = (usize, &'a Schematic);

/// The builds of known latency, quickest first, a tie going to the
/// smaller.
pub fn fastest<'a>(builds: &[&'a Schematic]) -> Vec<Placed<'a>> {
    let mut timed: Vec<&Schematic> = builds
        .iter()
        .copied()
        .filter(|s| s.latency.is_some())
        .collect();
    timed.sort_by_key(|s| (s.latency, s.volume(), s.added_at.clone()));
    placed(&timed, |s| (s.latency, s.volume()))
}

/// Every build, smallest first, a tie going to the quicker and then to one
/// of known latency.
pub fn smallest<'a>(builds: &[&'a Schematic]) -> Vec<Placed<'a>> {
    let mut sorted = builds.to_vec();
    let key = |s: &Schematic| (s.volume(), s.latency.unwrap_or(i64::MAX));
    sorted.sort_by_key(|s| (key(s), s.added_at.clone()));
    placed(&sorted, key)
}

/// The builds nothing beats on both counts: each is smaller than every
/// build at least as quick, so the list runs quickest to smallest.
pub fn tradeoffs<'a>(builds: &[&'a Schematic]) -> Vec<&'a Schematic> {
    let mut best = i64::MAX;
    fastest(builds)
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| {
            let smaller = s.volume() < best;
            if smaller {
                best = s.volume();
            }
            smaller
        })
        .collect()
}

/// `sorted` with each build's place by `key`.
fn placed<'a, K: PartialEq>(
    sorted: &[&'a Schematic],
    key: impl Fn(&Schematic) -> K,
) -> Vec<Placed<'a>> {
    let mut place = 0;
    sorted
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i == 0 || key(sorted[i - 1]) != key(s) {
                place = i + 1;
            }
            (place, *s)
        })
        .collect()
}

/// A person's count in a people ranking, under the spelling first seen.
#[derive(Debug, PartialEq, Eq)]
pub struct Standing {
    pub name: String,
    /// The account behind the name, for counts of what accounts did.
    pub account: Option<String>,
    pub count: usize,
}

/// Counts each person in `people`, whatever the case of their name, most
/// first, then by name.
fn tally(
    people: impl Iterator<Item = (String, Option<String>)>,
) -> Vec<Standing> {
    let mut standings: Vec<Standing> = Vec::new();
    for (name, account) in people {
        match standings
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(&name))
        {
            Some(standing) => standing.count += 1,
            None => standings.push(Standing {
                name,
                account,
                count: 1,
            }),
        }
    }
    standings.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    standings
}

/// Builders by how many builds credit them.
pub fn most_built(builds: &[Schematic]) -> Vec<Standing> {
    tally(builds.iter().flat_map(credited))
}

/// A build's builders, as people to count.
fn credited(
    s: &Schematic,
) -> impl Iterator<Item = (String, Option<String>)> + '_ {
    s.builders.iter().map(|name| (name.clone(), None))
}

/// People by how many builds they put on the site: an upload counts for
/// its account, under its Minecraft name if it has one, and an import for
/// the in-game name of whoever imported it.
pub fn most_added(builds: &[Schematic]) -> Vec<Standing> {
    tally(builds.iter().filter_map(|s| {
        match (&s.uploader, &s.uploader_name, &s.imported_by) {
            (Some(account), Some(name), _) => {
                Some((name.clone(), Some(account.clone())))
            }
            (_, _, Some(name)) => Some((name.clone(), None)),
            _ => None,
        }
    }))
}

/// Builders by records held: each record a build of theirs holds, see
/// [`records`], a tie giving every holder's builders one.
pub fn most_records(records: &[Record]) -> Vec<Standing> {
    tally(records.iter().flat_map(|record| {
        let mut names: Vec<(String, Option<String>)> = Vec::new();
        for (name, account) in record.holders.iter().flat_map(|s| credited(s)) {
            if !names.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                names.push((name, account));
            }
        }
        names
    }))
}

/// The tags of a build a board may narrow to: those the site finds, such
/// as its design, orientation, `synced`, or `torchless`, but for the
/// flags that only say which lines it has, as a carry in, which never
/// narrow a record's board.
pub fn ranked_tags(s: &Schematic) -> Vec<Tag> {
    s.every_tag()
        .into_iter()
        .filter(|tag| {
            tag.is_found()
                && !tag.is_unverified()
                && !Flag::LINES.contains(&tag.name.as_str())
        })
        .collect()
}

/// How a board ranks its builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    /// Quickest first, a tie going to the smaller.
    Fastest,
    /// Smallest first, a tie going to the quicker.
    Smallest,
}

impl Rank {
    pub fn label(self) -> &'static str {
        match self {
            Rank::Fastest => "Fastest",
            Rank::Smallest => "Smallest",
        }
    }
}

/// One thing a record's board narrows to beyond its category, measures,
/// and state: a latency, or a tag the site finds, by its name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Trait {
    Ticks(i64),
    Tag(String),
}

/// The most traits a record's board narrows to.
const MOST_TRAITS: usize = 3;

/// First place on a board: a standard category's verified builds of one
/// value of each of its measures and one state, narrowed to a latency and
/// tags the site finds, ranked fastest or smallest.
#[derive(Debug)]
pub struct Record<'a> {
    pub category: &'a str,
    /// A value of each of the category's measures, such as a width of 8.
    pub measures: Vec<(Measure, i64)>,
    pub logic: State,
    /// Whether the category has builds of another state at these measures,
    /// so the record's name says which.
    pub logic_named: bool,
    pub ticks: Option<i64>,
    /// The tags its board narrows to, in the order a build lists them.
    pub tags: Vec<Tag>,
    pub rank: Rank,
    /// The builds it ranks.
    pub field: Vec<&'a Schematic>,
    /// The builds in first place, more than one on a tie.
    pub holders: Vec<&'a Schematic>,
    /// How many people built the field, whatever the case of their names.
    pub builders: usize,
}

impl Record<'_> {
    /// Such as `Smallest 3-tick 8-bit synced CCA adder`.
    pub fn name(&self) -> String {
        let (designs, tags): (Vec<&Tag>, Vec<&Tag>) =
            self.tags.iter().partition(|tag| tag.kind == Kind::Design);
        [self.rank.label().to_owned()]
            .into_iter()
            .chain(self.ticks.map(|t| format!("{t}-tick")))
            .chain(self.measures.iter().map(|(m, n)| m.lead(*n)))
            .chain(self.logic_named.then(|| self.logic.name().to_owned()))
            .chain(tags.into_iter().chain(designs).map(Tag::label))
            .chain([in_sentence(&self.holders[0].category_label())])
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A category's name as a sentence carries it, such as `ripple adders`,
/// each word lowercased but one that starts with two capitals, such as
/// `ALUs`.
pub fn in_sentence(name: &str) -> String {
    name.split(' ')
        .map(|word| {
            let mut letters = word.chars().filter(char::is_ascii_alphabetic);
            let acronym =
                letters.next().is_some_and(|c| c.is_ascii_uppercase())
                    && letters.next().is_some_and(|c| c.is_ascii_uppercase());
            if acronym {
                word.to_owned()
            } else {
                word.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every record `builds` hold, the most contested first: by most builders,
/// then most builds. A board is one of a category's measures and state,
/// narrowed to up to [`MOST_TRAITS`] traits: a latency, and tags the site
/// finds, such as `synced`, a design, or an orientation. Its first place is
/// a record when the board ranks two or more builds, and
///
/// - its traits each change who is first, so no board with one fewer has
///   the same first place: a design nearly every fast build has makes no
///   fastest board of its own;
/// - as fastest, speed decides it: when builds tie on the fewest ticks,
///   the record is the smallest at that latency, a board of its own;
/// - as fastest, it narrows to no latency, which would rank by size.
///
/// Boards of the same builds with the same first place are one record.
pub fn records<'a>(builds: &[&'a Schematic]) -> Vec<Record<'a>> {
    type Base<'a> = (&'a str, Vec<i64>, State);
    let mut bases: BTreeMap<Base, Vec<&Schematic>> = BTreeMap::new();
    for &s in builds
        .iter()
        .filter(|s| s.category_label.is_some() && s.verified())
    {
        let Some(values) = s
            .measures
            .iter()
            .map(|m| m.of(s))
            .collect::<Option<Vec<i64>>>()
            .filter(|values| !values.is_empty())
        else {
            continue;
        };
        bases
            .entry((&s.category, values, State::of(s)))
            .or_default()
            .push(s);
    }
    let mut records = Vec::new();
    for ((category, values, logic), base) in &bases {
        let logic_named = bases
            .keys()
            .any(|(c, v, l)| c == category && v == values && l != logic);
        records.extend(records_of(base).into_iter().map(|found| {
            Record {
                category,
                measures: base[0]
                    .measures
                    .iter()
                    .copied()
                    .zip(values.iter().copied())
                    .collect(),
                logic: *logic,
                logic_named,
                ticks: found.ticks,
                tags: found.tags,
                rank: found.rank,
                builders: tally(found.field.iter().flat_map(|s| credited(s)))
                    .len(),
                field: found.field,
                holders: found.holders,
            }
        }));
    }
    records.sort_by(|a, b| {
        (b.builders, b.field.len()).cmp(&(a.builders, a.field.len()))
    });
    records
}

/// A record as one category's builds at one value of each of its measures
/// and one state find it, before it is named by them; see [`Record`].
struct Found<'a> {
    ticks: Option<i64>,
    tags: Vec<Tag>,
    rank: Rank,
    field: Vec<&'a Schematic>,
    holders: Vec<&'a Schematic>,
}

/// Adds to `boards` each set of at most [`MOST_TRAITS`] of `own`, the empty
/// set among them, that includes `chosen` and otherwise draws only on
/// `own[from..]`.
fn boards_of(
    own: &[&Trait],
    from: usize,
    chosen: &mut Vec<Trait>,
    boards: &mut BTreeSet<BTreeSet<Trait>>,
) {
    boards.insert(chosen.iter().cloned().collect());
    if chosen.len() == MOST_TRAITS {
        return;
    }
    for i in from..own.len() {
        chosen.push(own[i].clone());
        boards_of(own, i + 1, chosen, boards);
        chosen.pop();
    }
}

/// The records of `base`, one category's builds of one value of each of
/// its measures and one state; see [`records`].
fn records_of<'a>(base: &[&'a Schematic]) -> Vec<Found<'a>> {
    let tags: Vec<Vec<Tag>> = base.iter().map(|s| ranked_tags(s)).collect();
    let traits: Vec<BTreeSet<Trait>> = base
        .iter()
        .zip(&tags)
        .map(|(s, tags)| {
            tags.iter()
                .map(|tag| Trait::Tag(tag.name.clone()))
                .chain(s.latency.map(Trait::Ticks))
                .collect()
        })
        .collect();
    let mut boards: BTreeSet<BTreeSet<Trait>> = BTreeSet::new();
    for own in &traits {
        let own: Vec<&Trait> = own.iter().collect();
        boards_of(&own, 0, &mut Vec::new(), &mut boards);
    }
    // The builds a board of `wanted` ranks `rank`, by their place in base.
    let field = |wanted: &BTreeSet<Trait>, rank: Rank| -> Vec<usize> {
        (0..base.len())
            .filter(|&i| wanted.is_subset(&traits[i]))
            .filter(|&i| rank == Rank::Smallest || base[i].latency.is_some())
            .collect()
    };
    let key = |i: usize, rank: Rank| {
        let (s, ticks) = (base[i], base[i].latency.unwrap_or(i64::MAX));
        match rank {
            Rank::Fastest => (ticks, s.volume()),
            Rank::Smallest => (s.volume(), ticks),
        }
    };
    let firsts = |field: &[usize], rank: Rank| -> Vec<usize> {
        let best = field.iter().map(|&i| key(i, rank)).min();
        field
            .iter()
            .copied()
            .filter(|&i| Some(key(i, rank)) == best)
            .collect()
    };
    let mut seen: BTreeSet<(Vec<usize>, Vec<usize>)> = BTreeSet::new();
    let mut found = Vec::new();
    for wanted in &boards {
        let ticks = wanted.iter().find_map(|t| match t {
            Trait::Ticks(n) => Some(*n),
            Trait::Tag(_) => None,
        });
        for rank in [Rank::Smallest, Rank::Fastest] {
            if rank == Rank::Fastest && ticks.is_some() {
                continue;
            }
            let ranked = field(wanted, rank);
            if ranked.len() < 2 {
                continue;
            }
            let first = firsts(&ranked, rank);
            if rank == Rank::Fastest {
                let fewest = base[first[0]].latency;
                if ranked
                    .iter()
                    .filter(|&&i| base[i].latency == fewest)
                    .count()
                    > 1
                {
                    continue;
                }
            }
            let changes = wanted.iter().all(|t| {
                let mut fewer = wanted.clone();
                fewer.remove(t);
                firsts(&field(&fewer, rank), rank) != first
            });
            if !changes || !seen.insert((ranked.clone(), first.clone())) {
                continue;
            }
            let named = tags[first[0]]
                .iter()
                .filter(|tag| wanted.contains(&Trait::Tag(tag.name.clone())))
                .cloned()
                .collect();
            found.push(Found {
                ticks,
                tags: named,
                rank,
                field: ranked.iter().map(|&i| base[i]).collect(),
                holders: first.iter().map(|&i| base[i]).collect(),
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schematic::{Encoding, StackDirection, Widened};

    /// A board's address reads back as it was written, with each measure
    /// under its own name.
    #[test]
    fn a_board_address_reads_back_as_written() {
        let href = "/leaderboards/adder?width=8&logic=stateless&orientation=horz&design=carry_cancel&ticks=3&tags=synced%2Ctorchless";
        let raw = href.split_once('?').unwrap().1;
        let wanted = Wanted::parse(raw);
        assert_eq!(wanted.measures, [(Measure::Width, 8)]);
        assert_eq!(wanted.tags, ["synced", "torchless"]);
        assert_eq!(wanted.href("/leaderboards/adder"), href);
        let wanted = Wanted::parse("at_least=3&design=&ticks=fast");
        assert_eq!(wanted.href("/b"), "/b?at_least=3");
    }

    fn build(
        id: &str,
        width: i64,
        design: Option<&str>,
        latency: Option<i64>,
        volume: i64,
    ) -> Schematic {
        Schematic {
            id: id.to_owned(),
            category: "adder".to_owned(),
            category_label: Some("Adder".to_owned()),
            width: Some(width),
            orientation: Orientation::Horizontal,
            encoding: Some(Encoding::Binary),
            latency,
            synchronized: None,
            logic: None,
            own_logic: None,
            throughput: None,
            at_least: None,
            pulse: None,
            measures: vec![Measure::Width],
            aliases: Vec::new(),
            torches: None,
            works_from: None,
            works_from_by: None,
            bytes: None,
            size: (volume, 1, 1),
            stacking: Vec::new(),
            widened: Vec::new(),
            stacked_from: None,
            builders: Vec::new(),
            contributors: Vec::new(),
            built_on: None,
            imported_by: None,
            conventional: true,
            keeps_paste: false,
            design: design.map(str::to_owned),
            design_abbreviation: None,
            design_given_label: None,
            flags: Vec::new(),
            description: None,
            given_name: None,
            uploader: None,
            uploader_name: None,
            sha256: String::new(),
            added_at: id.to_owned(),
            edited_at: None,
            downloads: 0,
            public: true,
            unverified: None,
            tags: Vec::new(),
        }
    }

    fn adders() -> Vec<Schematic> {
        vec![
            build("a", 8, Some("carry_cancel"), Some(3), 684),
            build("b", 8, Some("carry_cancel"), Some(3), 2640),
            build("c", 8, Some("carry_lookahead"), Some(4), 6498),
            build("d", 8, Some("carry_look_everywhere"), Some(4), 3696),
            build("e", 8, Some("carry_lookahead"), Some(8), 3762),
            build("f", 16, Some("carry_cancel"), Some(4), 1960),
            build("g", 8, Some("ripple_carry"), None, 500),
        ]
    }

    fn ids<'a>(placed: &[Placed<'a>]) -> Vec<(usize, &'a str)> {
        placed.iter().map(|(n, s)| (*n, s.id.as_str())).collect()
    }

    #[test]
    fn stateful_and_stateless_builds_never_share_a_board() {
        let mut adders = adders();
        for s in &mut adders {
            s.logic = Some(Logic::Stateless);
        }
        adders[0].logic = Some(Logic::Stateful);
        let stateful_id = adders[0].id.clone();
        let all: Vec<&Schematic> = adders.iter().collect();
        let stateless = board(&all, &Wanted::default(), &[Measure::Width]);
        assert_eq!(stateless.logic, Some(State::Known(Logic::Stateless)));
        assert!(stateless.builds.iter().all(|s| s.id != stateful_id));
        let labels: Vec<&str> =
            stateless.logics.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Stateless", "Stateful"]);
        let wanted = Wanted {
            logic: Some(State::Known(Logic::Stateful)),
            ..Wanted::default()
        };
        let stateful = board(&all, &wanted, &[Measure::Width]);
        let ids: Vec<&str> =
            stateful.builds.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, [stateful_id.as_str()]);
    }

    #[test]
    fn a_board_starts_at_the_most_common_width_with_every_orientation_and_design()
     {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board = board(&all, &Wanted::default(), &[Measure::Width]);
        assert_eq!(board.splits[0].chosen, Some(8));
        assert_eq!(
            board.splits[0]
                .choices
                .iter()
                .map(|c| c.value)
                .collect::<Vec<_>>(),
            [8, 16]
        );
        // Every orientation competes, as every design does.
        assert_eq!(board.orientation, None);
        assert_eq!(board.design, None);
        assert!(board.all_designs);
        assert_eq!(board.builds.len(), 6);
    }

    #[test]
    fn memories_compete_only_at_one_capacity() {
        let ram = |id: &str, bytes: Option<i64>| Schematic {
            bytes,
            ..build(id, 8, None, Some(4), 100)
        };
        let rams = [
            ram("a", Some(32)),
            ram("b", Some(32)),
            ram("c", Some(256)),
            ram("d", None),
        ];
        let all: Vec<&Schematic> = rams.iter().collect();
        let board =
            board(&all, &Wanted::default(), &[Measure::Width, Measure::Bytes]);
        assert_eq!(board.splits[1].chosen, Some(32));
        assert_eq!(
            board.splits[1]
                .choices
                .iter()
                .map(|c| c.value)
                .collect::<Vec<_>>(),
            [32, 256]
        );
        assert_eq!(board.builds.len(), 2);
    }

    #[test]
    fn a_threshold_competes_only_at_one_k() {
        let threshold = |id: &str, at_least: i64| Schematic {
            at_least: Some(at_least),
            ..build(id, 8, None, Some(4), 100)
        };
        let all = [threshold("a", 3), threshold("b", 5), threshold("c", 5)];
        let all: Vec<&Schematic> = all.iter().collect();
        let measures = [Measure::Width, Measure::AtLeast];
        let wanted = Wanted {
            measures: vec![(Measure::AtLeast, 3)],
            ..Wanted::default()
        };
        let board = board(&all, &wanted, &measures);
        assert_eq!(board.splits[0].chosen, Some(8));
        let ids: Vec<&str> =
            board.builds.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a"]);
    }

    #[test]
    fn rankings_share_a_place_on_a_tie_and_skip_unknown_latency() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board = board(&all, &Wanted::default(), &[Measure::Width]);
        assert_eq!(
            ids(&fastest(&board.builds)),
            [(1, "a"), (2, "b"), (3, "d"), (4, "c"), (5, "e")]
        );
        assert_eq!(
            ids(&smallest(&board.builds)),
            [(1, "g"), (2, "a"), (3, "b"), (4, "d"), (5, "e"), (6, "c")]
        );
        let tied = [
            build("x", 8, None, Some(3), 100),
            build("y", 8, None, Some(3), 100),
            build("z", 8, None, Some(2), 100),
        ];
        let tied: Vec<&Schematic> = tied.iter().collect();
        assert_eq!(ids(&fastest(&tied)), [(1, "z"), (2, "x"), (2, "y")]);
    }

    /// The builds of the tests below, each by one builder.
    fn credited_adders() -> Vec<Schematic> {
        let by = |name: &str, s: Schematic| Schematic {
            builders: vec![name.to_owned()],
            ..s
        };
        vec![
            // 8-bit: a is fastest and smallest.
            by("Alex", build("a", 8, Some("carry_cancel"), Some(3), 684)),
            by("bob", build("b", 8, Some("carry_cancel"), Some(4), 900)),
            // Alone on its board, so no record.
            by("bob", build("c", 16, Some("carry_cancel"), Some(4), 1960)),
            by(
                "alex",
                build("d", 8, Some("carry_lookahead"), Some(4), 6498),
            ),
            by("Bob", build("e", 8, Some("carry_lookahead"), Some(8), 3762)),
        ]
    }

    /// Records are first places on boards of two or more builds, each
    /// trait changing who is first: a, first of all, is first of the CCAs
    /// too, so they make no record; of the CLAs, d is fastest and e
    /// smallest; and of the 4-tick builds, b is smallest.
    #[test]
    fn records_are_first_places_each_trait_changes() {
        let adders = credited_adders();
        let builds: Vec<&Schematic> = adders.iter().collect();
        let records = records(&builds);
        let found: Vec<(String, Vec<&str>)> = records
            .iter()
            .map(|r| {
                (r.name(), r.holders.iter().map(|s| s.id.as_str()).collect())
            })
            .collect();
        assert_eq!(
            found,
            [
                ("Smallest 8-bit adder".to_owned(), vec!["a"]),
                ("Smallest 4-tick 8-bit adder".to_owned(), vec!["b"]),
                ("Smallest 8-bit carry lookahead adder".to_owned(), vec!["e"]),
                ("Fastest 8-bit carry lookahead adder".to_owned(), vec!["d"]),
            ]
        );
        let counts: Vec<(String, usize)> = most_records(&records)
            .into_iter()
            .map(|s| (s.name, s.count))
            .collect();
        assert_eq!(counts, [("Alex".to_owned(), 2), ("bob".to_owned(), 2)]);
    }

    /// When builds tie on the fewest ticks, size decides, so the record is
    /// the smallest at that latency, not the fastest.
    #[test]
    fn a_tie_on_ticks_makes_a_record_of_the_smallest_at_them() {
        let adders = [
            build("a", 8, None, Some(3), 900),
            build("b", 8, None, Some(3), 700),
            build("c", 8, None, Some(5), 400),
        ];
        let builds: Vec<&Schematic> = adders.iter().collect();
        let names: Vec<String> =
            records(&builds).iter().map(Record::name).collect();
        assert_eq!(
            names,
            ["Smallest 8-bit adder", "Smallest 3-tick 8-bit adder"]
        );
    }

    /// A build with more traits than a bit mask holds still ranks: only its
    /// sets of a few traits are boards.
    #[test]
    fn a_build_with_many_traits_still_ranks() {
        let tags: Vec<Tag> = (0..40)
            .map(|n| Tag {
                name: format!("trait-{n}"),
                token: None,
                kind: Kind::Feature,
            })
            .collect();
        let adders = [
            Schematic {
                tags: tags.clone(),
                ..build("a", 8, None, Some(3), 900)
            },
            Schematic {
                tags,
                ..build("b", 8, None, Some(3), 700)
            },
        ];
        let builds: Vec<&Schematic> = adders.iter().collect();
        let names: Vec<String> =
            records(&builds).iter().map(Record::name).collect();
        assert_eq!(names, ["Smallest 8-bit adder"]);
    }

    #[test]
    fn people_count_whatever_their_case() {
        let builds = [
            Schematic {
                builders: vec!["embizone".to_owned()],
                imported_by: Some("embizone".to_owned()),
                ..build("a", 8, None, None, 1)
            },
            Schematic {
                builders: vec!["EmbiZone".to_owned(), "alex".to_owned()],
                uploader: Some("lookaside".to_owned()),
                uploader_name: Some("EMBIZONE".to_owned()),
                ..build("b", 8, None, None, 1)
            },
        ];
        let built: Vec<(String, usize)> = most_built(&builds)
            .into_iter()
            .map(|s| (s.name, s.count))
            .collect();
        assert_eq!(built, [("embizone".to_owned(), 2), ("alex".to_owned(), 1)]);
        // The uploading account goes by its Minecraft name, so it and the
        // import count as one person.
        let added = most_added(&builds);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].count, 2);
    }

    /// A 1-bit adder stacking 2 west, measured at 8 bits as 8 copies,
    /// ranks among the 8-bit adders by that timing and the box its copies
    /// take, each adding its step.
    #[test]
    fn copies_of_a_narrower_build_compete_at_the_width_they_make() {
        let slice = Schematic {
            stacking: vec![StackDirection {
                west: 2,
                up: 0,
                max_copies: None,
                faces: None,
                pitch: Some(2),
            }],
            widened: vec![Widened {
                width: 8,
                copies: 8,
                latency: Some(20),
                synchronized: Some(false),
                throughput: Some(19),
            }],
            size: (3, 4, 5),
            ..build("s", 1, Some("ripple_carry"), Some(6), 60)
        };
        let adders =
            [slice, build("a", 8, Some("ripple_carry"), Some(25), 1000)];
        let stacks = stacked(&adders);
        let all: Vec<&Schematic> = adders.iter().chain(&stacks).collect();
        let wanted = Wanted {
            measures: vec![(Measure::Width, 8)],
            ..Wanted::default()
        };
        let board = board(&all, &wanted, &[Measure::Width]);
        let fastest = fastest(&board.builds);
        assert_eq!(ids(&fastest), [(1, "s"), (2, "a")]);
        let stack = fastest[0].1;
        assert_eq!(stack.stacked_from, Some(1));
        assert_eq!(stack.latency, Some(20));
        assert_eq!(stack.size, (3 + 2 * 7, 4, 5));
        assert_eq!(stack.volume(), 2 * 8 * 4 * 5);
    }

    #[test]
    fn tradeoffs_keep_each_build_smaller_than_all_quicker_ones() {
        let builds = [
            build("a", 8, None, Some(3), 900),
            build("b", 8, None, Some(4), 950),
            build("c", 8, None, Some(5), 400),
            build("d", 8, None, Some(9), 380),
        ];
        let builds: Vec<&Schematic> = builds.iter().collect();
        let kept: Vec<&str> =
            tradeoffs(&builds).iter().map(|s| s.id.as_str()).collect();
        assert_eq!(kept, ["a", "c", "d"]);
    }
}
