//! Leaderboards: a category's builds ranked against the others that match
//! them in its measures, such as width, then state, orientation, and
//! design, since only those compete: a stateful build never meets a
//! stateless one. A category whose bits are cells of their own, as a
//! gate's, a monostable's, or a register's, ranks them as cells instead:
//! never by width, as an 8-bit XOR gate is eight XOR gates, but by how
//! closely their copies stack and how small each cell is.

use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use crate::{
    category::{self, Flag, Logic, Measure, NO_DESIGN},
    interface::Orientation,
    models::{
        schematic::Build,
        tag::{self, Kind, Tag},
    },
    simulate::Group,
    stack::PITCHES,
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
    fn of(s: &Build) -> Self {
        s.logic().map_or(State::Unknown, State::Known)
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

/// Which builds a board takes by how their operands lie. A horizontal
/// build whose A and B alternate along one row, carrying
/// `interleaved_inputs`, is the odd case, competing only where it is asked
/// for, alone or with the rest.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Inputs {
    /// Builds whose operands lie split, as most do.
    #[default]
    Split,
    /// Builds whose operands are interleaved, alone.
    Interleaved,
    /// Both together.
    All,
}

impl Inputs {
    /// As a leaderboard's address writes it, such as `interleaved`.
    pub fn name(self) -> &'static str {
        match self {
            Inputs::Split => "split",
            Inputs::Interleaved => "interleaved",
            Inputs::All => "all",
        }
    }

    /// As people read it, such as `Interleaved`.
    pub fn label(self) -> &'static str {
        match self {
            Inputs::Split => "Split",
            Inputs::Interleaved => "Interleaved",
            Inputs::All => "All",
        }
    }

    /// Whether a board of these inputs takes `s`.
    pub fn takes(self, s: &Build) -> bool {
        match self {
            Inputs::Split => !s.interleaved_inputs(),
            Inputs::Interleaved => s.interleaved_inputs(),
            Inputs::All => true,
        }
    }
}

impl FromStr for Inputs {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        [Inputs::Split, Inputs::Interleaved, Inputs::All]
            .into_iter()
            .find(|inputs| text.trim().eq_ignore_ascii_case(inputs.name()))
            .ok_or_else(|| format!("no inputs named {text}"))
    }
}

/// Whether builds of `group` rank as cells: each bit, or each copy, a cell
/// of its own, as a gate's or a monostable's, or of cells on shared
/// lines, as a register's, rather than data crossing its bits.
pub fn as_cells(group: Group) -> bool {
    group != Group::CrossesBits
}

/// The group of the category named `category`; see [`Group`].
pub fn group_of(category: &str) -> Group {
    category::behaviour(category).group
}

/// The measures a board of a category of `group`, measured by `measures`,
/// splits by: each of them, but width where builds rank as cells, as an
/// 8-bit XOR gate is eight XOR gates, its blocks counted a bit.
pub fn split_by(measures: &[Measure], group: Group) -> Vec<Measure> {
    measures
        .iter()
        .copied()
        .filter(|&m| !(as_cells(group) && m == Measure::Width))
        .collect()
}

/// The value of each measure a board of `s`'s category splits by, in its
/// order; `None` where one is unknown, a cell's width among them, as its
/// blocks are counted a bit, or where its category has no measures.
fn ranked_values(s: &Build) -> Option<Vec<(Measure, i64)>> {
    let group = group_of(&s.category);
    let unknown_width = as_cells(group)
        && s.measures().contains(&Measure::Width)
        && s.specs.width.is_none();
    if s.measures().is_empty() || unknown_width {
        return None;
    }
    split_by(s.measures(), group)
        .into_iter()
        .map(|m| Some((m, s.specs.of(m)?)))
        .collect()
}

/// The pitch class a tag names, the blocks a bit apart copies stack and
/// the way, as `2-wide_stackable` names `(2, "wide")`; `None` for any
/// other tag, as `stackable` or `3-wide_word_stackable`.
pub fn pitch_class(name: &str) -> Option<(i64, &str)> {
    let class = name.strip_suffix(tag::STACKABLE)?.strip_suffix('_')?;
    let (pitch, shape) = class.split_once('-')?;
    let pitch = pitch.parse().ok()?;
    matches!(shape, "wide" | "high").then_some((pitch, shape))
}

/// A build's size as a board compares it: `blocks` over `bits`, so a 2-bit
/// cell of 98 blocks is as small as a 1-bit cell of 49.
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub blocks: i64,
    pub bits: i64,
}

impl Ord for Size {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let cross =
            |a: Size, b: Size| i128::from(a.blocks) * i128::from(b.bits);
        cross(*self, *other).cmp(&cross(*other, *self))
    }
}

impl PartialOrd for Size {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Size {}

/// How a board measures its builds' size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ruler {
    /// A build's blocks as [`Build::volume`] counts them: what each
    /// copy adds for a stackable build, its box for any other, and a
    /// narrower build's stack, the whole stack.
    Whole,
    /// A cell's blocks a bit: its box, but its step along each way its
    /// copies add bits that the board's pitch class names, west or up,
    /// over the bits it holds. A cell on no board of a class counts its
    /// box alone, the air its copies need between them not counted.
    Cell { west: bool, up: bool },
}

impl Ruler {
    /// The ruler of a board of builds of `group`, narrowed to the pitch
    /// classes that `tags` name, by their names.
    pub fn of<'a>(
        group: Group,
        tags: impl IntoIterator<Item = &'a str>,
    ) -> Ruler {
        if !as_cells(group) {
            return Ruler::Whole;
        }
        let (mut west, mut up) = (false, false);
        for (_, shape) in tags.into_iter().filter_map(pitch_class) {
            match shape {
                "wide" => west = true,
                _ => up = true,
            }
        }
        Ruler::Cell { west, up }
    }

    /// Whether it counts copies stacked, as a pitch class's board does.
    pub fn stacked(self) -> bool {
        matches!(self, Ruler::Cell { west, up } if west || up)
    }

    /// The size of `s` as it measures it.
    pub fn size(self, s: &Build) -> Size {
        let Ruler::Cell { west, up } = self else {
            return Size {
                blocks: s.volume(),
                bits: 1,
            };
        };
        let (mut x, mut y, z) = s.file.size;
        for way in s.stacking.iter().filter(|way| way.pitch.is_some()) {
            match (way.west, way.up) {
                (step, 0) if west => x = step,
                (0, step) if up => y = step,
                _ => {}
            }
        }
        Size {
            blocks: x * y * z,
            bits: bits(s),
        }
    }
}

/// The bits a build holds, as its cells count: a memory's bytes' bits,
/// else its width, else one.
fn bits(s: &Build) -> i64 {
    s.specs
        .bytes
        .map(|bytes| bytes * 8)
        .or(s.specs.width)
        .unwrap_or(1)
        .max(1)
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
    /// How the builds' operands lie; `None` for split, or interleaved
    /// where no build is split.
    pub inputs: Option<Inputs>,
    /// Where builds rank as cells, the pitch class their copies stack in,
    /// such as `2-wide`; `None` for any, or none.
    pub stacking: Option<String>,
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
            inputs: parsed(value("inputs")),
            stacking: value("stacking")
                .filter(|class| {
                    pitch_class(&tag::pitch_stackable(class)).is_some()
                })
                .map(str::to_owned),
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
            ("inputs", self.inputs.map(Inputs::name)),
            ("stacking", self.stacking.as_deref()),
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
    /// Its category's group, which says whether its builds rank as cells.
    pub group: Group,
    /// Whether any build ranks, before the choices narrow them.
    pub ranked: bool,
    /// Its category's measures it splits by, in order: width only where
    /// its builds do not rank as cells.
    pub splits: Vec<Split>,
    /// Stateless and stateful; empty when the builds are all one.
    pub logics: Vec<Choice<State>>,
    pub logic: Option<State>,
    /// Split and interleaved; empty when no build left is interleaved.
    pub inputs_choices: Vec<Choice<Inputs>>,
    pub inputs: Inputs,
    /// Where builds rank as cells, each pitch class the builds left stack
    /// in, such as `2-wide`, tightest first; else empty, a class being one
    /// of the tags.
    pub stackings: Vec<Choice<String>>,
    /// `None` for every build, however it stacks.
    pub stacking: Option<String>,
    /// How it measures its builds' size, as its stacking says.
    pub ruler: Ruler,
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
    pub builds: Vec<&'a Build>,
}

/// The board of `category`, whose measures are `measures`, that `wanted`
/// asks for. Each measure and state not asked for falls to the most common
/// value among the builds the earlier choices leave, so a board starts
/// where most builds are; every orientation, latency, and design compete
/// unless asked for, but for a category's one design. A build missing a
/// measure, such as one of unknown width, competes on no board, nor does
/// one not verified as its category. Interleaved builds compete only when
/// asked for, unless every build is. Builds that rank as cells split by no
/// width, and by how closely their copies stack, any way by default.
pub fn board<'a>(
    category: &[&'a Build],
    wanted: &Wanted,
    measures: &[Measure],
) -> Board<'a> {
    let group = category
        .first()
        .map_or_else(Group::default, |s| group_of(&s.category));
    let cells = as_cells(group);
    let (splits, ranked) = split(category, wanted, measures, group);
    let any = !ranked.is_empty();

    // Stateless and stateful builds never compete: a stateful build's
    // latency is no stateless one's.
    let (logics, logic, ranked) = narrow(ranked, wanted.logic, |s| {
        let state = State::of(s);
        (state, state.label().to_owned())
    });
    let (inputs_choices, inputs, ranked) = by_inputs(ranked, wanted.inputs);
    let stacking = wanted.stacking.clone().filter(|_| cells);
    let (stackings, ruler, mut ranked) =
        by_stacking(ranked, stacking.as_deref(), group);
    let orientations = choices(&ranked, |s| {
        (s.specs.orientation, s.specs.orientation.label().to_owned())
    });
    let orientation = wanted.orientation;
    ranked.retain(|s| orientation.is_none_or(|o| s.specs.orientation == o));

    // By the short name people say, such as CCA, where there is one.
    let designs = choices(&ranked, |s| {
        let label = s
            .listed_design()
            .and_then(|listed| listed.abbreviation)
            .map(str::to_owned)
            .or_else(|| s.design_label())
            .unwrap_or_else(|| "Other".to_owned());
        (
            s.specs.design.as_deref().unwrap_or(NO_DESIGN).to_owned(),
            label,
        )
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
            .is_none_or(|d| s.specs.design.as_deref().unwrap_or(NO_DESIGN) == d)
    });
    let latencies = choices(&ranked, |s| {
        let ticks = s.specs.latency.unwrap_or(i64::MAX);
        let label = s
            .specs
            .latency
            .map_or("Unknown".to_owned(), |t| t.to_string());
        (ticks, label)
    });
    let ticks = wanted.ticks;
    ranked.retain(|s| ticks.is_none_or(|t| s.specs.latency == Some(t)));
    let tag_choices = tag_choices(&ranked, cells);
    let tags = wanted.tags.clone();
    let builds = ranked
        .into_iter()
        .filter(|s| {
            let own = ranked_tags(s);
            tags.iter().all(|t| own.iter().any(|tag| &tag.name == t))
        })
        .collect();
    Board {
        group,
        ranked: any,
        splits,
        logics: if logics.len() > 1 { logics } else { Vec::new() },
        logic,
        inputs_choices,
        inputs,
        stackings,
        stacking,
        ruler,
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

/// The verified builds of `category`, whose measures are `measures`, of
/// `group`, narrowed by each measure a board of theirs splits by, as
/// [`board`] does, with each measure's split.
fn split<'a>(
    category: &[&'a Build],
    wanted: &Wanted,
    measures: &[Measure],
    group: Group,
) -> (Vec<Split>, Vec<&'a Build>) {
    let cells = as_cells(group);
    let mut ranked: Vec<&Build> = category
        .iter()
        .copied()
        .filter(|s| s.ranks())
        .filter(|s| {
            !(cells
                && measures.contains(&Measure::Width)
                && s.specs.width.is_none())
        })
        .collect();
    let mut splits = Vec::new();
    for measure in split_by(measures, group) {
        ranked.retain(|s| s.specs.of(measure).is_some());
        let (choices, chosen, left) =
            narrow(ranked, wanted.measure(measure), |s| {
                let n = s.specs.of(measure).unwrap_or_default();
                (n, measure.lead(n))
            });
        ranked = left;
        splits.push(Split {
            measure,
            choices,
            chosen,
        });
    }
    (splits, ranked)
}

/// Narrows `ranked` to the builds of the inputs `wanted`, by default split
/// ones, or interleaved where none is split. Returns each kind's choice,
/// none where no build is interleaved, the kind chosen, and the builds
/// left.
fn by_inputs(
    mut ranked: Vec<&Build>,
    wanted: Option<Inputs>,
) -> (Vec<Choice<Inputs>>, Inputs, Vec<&Build>) {
    let options = choices(&ranked, |s| {
        let inputs = if s.interleaved_inputs() {
            Inputs::Interleaved
        } else {
            Inputs::Split
        };
        (inputs, inputs.label().to_owned())
    });
    let has = |inputs: Inputs| options.iter().any(|c| c.value == inputs);
    let chosen = wanted.unwrap_or(if has(Inputs::Split) {
        Inputs::Split
    } else {
        Inputs::Interleaved
    });
    ranked.retain(|s| chosen.takes(s));
    let options = if has(Inputs::Interleaved) {
        options
    } else {
        Vec::new()
    };
    (options, chosen, ranked)
}

/// Narrows `ranked`, builds of `group`, to those stacking in the pitch
/// class `class`, such as `2-wide`, where one is asked for. Returns, where
/// builds rank as cells, each class's choice, then how the board measures
/// its builds, and the builds left.
fn by_stacking<'a>(
    mut ranked: Vec<&'a Build>,
    class: Option<&str>,
    group: Group,
) -> (Vec<Choice<String>>, Ruler, Vec<&'a Build>) {
    let options = if as_cells(group) {
        stackings(&ranked)
    } else {
        Vec::new()
    };
    let class = class.map(tag::pitch_stackable);
    ranked.retain(|s| {
        class
            .as_ref()
            .is_none_or(|class| ranked_tags(s).iter().any(|t| &t.name == class))
    });
    (options, Ruler::of(group, class.as_deref()), ranked)
}

/// The pitch classes `ranked` stack in, as `2-wide`, each with how many
/// builds do: west first, then up, each tightest first.
fn stackings(ranked: &[&Build]) -> Vec<Choice<String>> {
    let mut counts: BTreeMap<(bool, i64), usize> = BTreeMap::new();
    for s in ranked {
        for tag in ranked_tags(s) {
            if let Some((pitch, shape)) = pitch_class(&tag.name) {
                *counts.entry((shape == "high", pitch)).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .map(|((high, pitch), count)| {
            let class =
                format!("{pitch}-{}", if high { "high" } else { "wide" });
            Choice {
                label: class.clone(),
                value: class,
                count,
            }
        })
        .collect()
}

/// The tags `ranked` have, with how many builds have each, by label, but
/// those chosen apart: a design, an orientation, interleaved inputs, and,
/// where builds rank as `cells`, a pitch class.
fn tag_choices(ranked: &[&Build], cells: bool) -> Vec<Choice<String>> {
    let chosen_apart = |tag: &Tag| {
        matches!(tag.kind, Kind::Design | Kind::Orientation)
            || tag.name == tag::INTERLEAVED_INPUTS
            || (cells && pitch_class(&tag.name).is_some())
    };
    let mut choices: Vec<Choice<String>> = Vec::new();
    for s in ranked {
        for tag in ranked_tags(s) {
            if chosen_apart(&tag) {
                continue;
            }
            match choices.iter_mut().find(|c| c.value == tag.name) {
                Some(choice) => choice.count += 1,
                None => choices.push(Choice {
                    label: tag.label(),
                    value: tag.name,
                    count: 1,
                }),
            }
        }
    }
    choices.sort_by(|a, b| a.label.cmp(&b.label));
    choices
}

/// Narrows `ranked` to the builds of one `(value, label)` of `key`: the
/// `wanted` value, else the most common. Returns every value's choice, the
/// value chosen, and the builds left.
fn narrow<V: Ord + Clone>(
    ranked: Vec<&Build>,
    wanted: Option<V>,
    key: impl Fn(&Build) -> (V, String),
) -> (Vec<Choice<V>>, Option<V>, Vec<&Build>) {
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
    builds: &[&Build],
    key: impl Fn(&Build) -> (V, String),
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
/// they compete at its width; see `Build::widened_to`. A cell's copies
/// make none, as cells compete by no width.
pub fn stacked<'a>(builds: impl IntoIterator<Item = &'a Build>) -> Vec<Build> {
    builds
        .into_iter()
        .filter(|s| !as_cells(group_of(&s.category)))
        .flat_map(|s| s.widened.iter().filter_map(|w| s.widened_to(w)))
        .collect()
}

/// A build with its place: one more than the builds ranked strictly ahead
/// of it, so a tie shares a place.
pub type Placed<'a> = (usize, &'a Build);

/// The builds of known latency, quickest first, a tie going to the
/// smaller, as `ruler` measures them.
pub fn fastest<'a>(builds: &[&'a Build], ruler: Ruler) -> Vec<Placed<'a>> {
    let mut timed: Vec<&Build> = builds
        .iter()
        .copied()
        .filter(|s| s.specs.latency.is_some())
        .collect();
    let key = |s: &Build| (s.specs.latency, ruler.size(s));
    timed.sort_by_key(|s| (key(s), s.entry.added_at.clone()));
    placed(&timed, key)
}

/// Every build, smallest as `ruler` measures them first, a tie going to
/// the quicker and then to one of known latency.
pub fn smallest<'a>(builds: &[&'a Build], ruler: Ruler) -> Vec<Placed<'a>> {
    let mut sorted = builds.to_vec();
    let key = |s: &Build| (ruler.size(s), s.specs.latency.unwrap_or(i64::MAX));
    sorted.sort_by_key(|s| (key(s), s.entry.added_at.clone()));
    placed(&sorted, key)
}

/// The builds nothing beats on both counts: each is smaller, as `ruler`
/// measures them, than every build at least as quick, so the list runs
/// quickest to smallest.
pub fn tradeoffs<'a>(builds: &[&'a Build], ruler: Ruler) -> Vec<&'a Build> {
    let mut best: Option<Size> = None;
    fastest(builds, ruler)
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| {
            let size = ruler.size(s);
            let smaller = best.is_none_or(|best| size < best);
            if smaller {
                best = Some(size);
            }
            smaller
        })
        .collect()
}

/// `sorted` with each build's place by `key`.
fn placed<'a, K: PartialEq>(
    sorted: &[&'a Build],
    key: impl Fn(&Build) -> K,
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

/// Authors by how many builds credit them.
pub fn most_built(builds: &[Build]) -> Vec<Standing> {
    tally(builds.iter().flat_map(credited))
}

/// A build's authors, as people to count.
fn credited(s: &Build) -> impl Iterator<Item = (String, Option<String>)> + '_ {
    s.credits.authors.iter().map(|name| (name.clone(), None))
}

/// People by how many builds they put on the site: an upload counts for
/// its account, under its Minecraft name if it has one, and an import for
/// the in-game name of whoever imported it.
pub fn most_added(builds: &[Build]) -> Vec<Standing> {
    tally(builds.iter().filter_map(|s| {
        match (
            &s.entry.uploader,
            &s.entry.uploader_name,
            &s.entry.imported_by,
        ) {
            (Some(account), Some(name), _) => {
                Some((name.clone(), Some(account.clone())))
            }
            (_, _, Some(name)) => Some((name.clone(), None)),
            _ => None,
        }
    }))
}

/// Authors by records held: each record a build of theirs holds, see
/// [`records`], a tie giving every holder's authors one.
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
/// as its design, orientation, `synced`, `torchless`, a pitch class, as
/// `2-wide_stackable`, or `interleaved_inputs`, but for the flags that only
/// say which lines it has, as a carry in, which never narrow a record's
/// board, and how far apart a register's copies add words, as
/// `3-wide_word_stackable`, shown on its page alone. Of a build that ranks
/// as a cell, `stackable` is left out too, its pitch class saying more.
pub fn ranked_tags(s: &Build) -> Vec<Tag> {
    let cells = as_cells(group_of(&s.category));
    s.every_tag()
        .into_iter()
        .filter(|tag| {
            tag.is_found()
                && !tag.is_unverified()
                && !Flag::LINES.contains(&tag.name.as_str())
                && !tag::is_word_stackable(&tag.name)
                && (!cells || tag.name != tag::STACKABLE)
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
    /// How its board measures its builds' size.
    pub ruler: Ruler,
    /// The builds it ranks.
    pub field: Vec<&'a Build>,
    /// The builds in first place, more than one on a tie.
    pub holders: Vec<&'a Build>,
    /// How many people built the field, whatever the case of their names.
    pub authors: usize,
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

/// Every record `builds` hold, the most contested first: by most authors,
/// then most builds. A board is one of a category's measures and state,
/// narrowed to up to [`MOST_TRAITS`] traits: a latency, and tags the site
/// finds, such as `synced`, a design, an orientation, or a pitch class, as
/// `2-wide_stackable`. Builds that rank as cells have no width among their
/// measures, a board of theirs narrows to one pitch class at most, and
/// they are measured as its [`Ruler`] says. An interleaved build competes
/// only on a board narrowed to `interleaved_inputs`, and only one is. Its
/// first place is a record when the board ranks two or more builds, and
///
/// - its traits each change who is first, so no board with one fewer has
///   the same first place: a design nearly every fast build has makes no
///   fastest board of its own;
/// - as fastest, speed decides it: when builds tie on the fewest ticks,
///   the record is the smallest at that latency, a board of its own;
/// - as fastest, it narrows to no latency, which would rank by size;
/// - of a looser stacking, such as `4-wide_stackable`, or `stackable`
///   alone, whose board takes a tighter one's builds too, its first place
///   is not a tighter one's record, as `2-wide_stackable`'s, its other
///   traits the same: a build first on both holds the tighter one alone.
///
/// Boards of the same builds with the same first place are one record,
/// named by the one that says the most of them: a pitch class, then a
/// design, then a latency, so `Smallest 2-wide stackable 1-bit adder`
/// over `Smallest 6-tick 1-bit adder`.
pub fn records<'a>(builds: &[&'a Build]) -> Vec<Record<'a>> {
    type Base<'a> = (&'a str, Vec<i64>, State);
    let mut bases: BTreeMap<Base, (Vec<Measure>, Vec<&Build>)> =
        BTreeMap::new();
    for &s in builds.iter().filter(|s| s.kind().is_some() && s.ranks()) {
        let Some(values) = ranked_values(s) else {
            continue;
        };
        let (measures, values): (Vec<Measure>, Vec<i64>) =
            values.into_iter().unzip();
        bases
            .entry((&s.category, values, State::of(s)))
            .or_insert_with(|| (measures, Vec::new()))
            .1
            .push(s);
    }
    let mut records = Vec::new();
    for ((category, values, logic), (measures, base)) in &bases {
        let logic_named = bases
            .keys()
            .any(|(c, v, l)| c == category && v == values && l != logic);
        records.extend(records_of(base).into_iter().map(|found| {
            Record {
                category,
                measures: measures
                    .iter()
                    .copied()
                    .zip(values.iter().copied())
                    .collect(),
                logic: *logic,
                logic_named,
                ticks: found.ticks,
                tags: found.tags,
                rank: found.rank,
                ruler: found.ruler,
                authors: tally(found.field.iter().flat_map(|s| credited(s)))
                    .len(),
                field: found.field,
                holders: found.holders,
            }
        }));
    }
    records.sort_by(|a, b| {
        (b.authors, b.field.len()).cmp(&(a.authors, a.field.len()))
    });
    records
}

/// A record as one category's builds at one value of each of its measures
/// and one state find it, before it is named by them; see [`Record`].
struct Found<'a> {
    /// The traits its board narrows to.
    wanted: BTreeSet<Trait>,
    ticks: Option<i64>,
    tags: Vec<Tag>,
    rank: Rank,
    ruler: Ruler,
    field: Vec<&'a Build>,
    holders: Vec<&'a Build>,
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

/// Each board builds of `group` with `traits` make, by the traits it
/// narrows to: a cell's narrows to one pitch class at most, as it is
/// measured by it.
fn boards(
    traits: &[BTreeSet<Trait>],
    group: Group,
) -> BTreeSet<BTreeSet<Trait>> {
    let mut boards = BTreeSet::new();
    for own in traits {
        let own: Vec<&Trait> = own.iter().collect();
        boards_of(&own, 0, &mut Vec::new(), &mut boards);
    }
    if as_cells(group) {
        boards.retain(|wanted| classes_in(wanted).count() <= 1);
    }
    boards
}

/// The pitch classes, as `2-wide_stackable`, that the traits `wanted`
/// narrow to.
fn classes_in(wanted: &BTreeSet<Trait>) -> impl Iterator<Item = &str> {
    wanted.iter().filter_map(|t| match t {
        Trait::Tag(name) if pitch_class(name).is_some() => Some(name.as_str()),
        _ => None,
    })
}

/// Whether the tag `tight` names a tighter stacking than `loose` does,
/// whose builds the board of `loose` takes too: a pitch class tighter than
/// another of the same way, as `2-wide_stackable` is than
/// `4-wide_stackable`, or any pitch class than `stackable` alone.
fn tighter(tight: &str, loose: &str) -> bool {
    match (pitch_class(tight), pitch_class(loose)) {
        (Some(_), None) => loose == tag::STACKABLE,
        (Some((tight, way)), Some((loose, loose_way))) => {
            way == loose_way
                && tight < loose
                && PITCHES.contains(&tight)
                && PITCHES.contains(&loose)
        }
        _ => false,
    }
}

/// The records of `base`, one category's builds of one value of each of
/// its measures and one state; see [`records`].
fn records_of<'a>(base: &[&'a Build]) -> Vec<Found<'a>> {
    let Some(first) = base.first() else {
        return Vec::new();
    };
    let group = group_of(&first.category);
    let interleaved = Trait::Tag(tag::INTERLEAVED_INPUTS.to_owned());
    let tags: Vec<Vec<Tag>> = base.iter().map(|s| ranked_tags(s)).collect();
    let traits: Vec<BTreeSet<Trait>> = base
        .iter()
        .zip(&tags)
        .map(|(s, tags)| {
            tags.iter()
                .map(|tag| Trait::Tag(tag.name.clone()))
                .chain(s.specs.latency.map(Trait::Ticks))
                .collect()
        })
        .collect();
    let boards = boards(&traits, group);
    let ruler = |wanted: &BTreeSet<Trait>| Ruler::of(group, classes_in(wanted));
    // The builds a board of `wanted` ranks `rank`, by their place in base:
    // interleaved builds only where it narrows to them.
    let field = |wanted: &BTreeSet<Trait>, rank: Rank| -> Vec<usize> {
        let interleaving = wanted.contains(&interleaved);
        (0..base.len())
            .filter(|&i| wanted.is_subset(&traits[i]))
            .filter(|&i| base[i].interleaved_inputs() == interleaving)
            .filter(|&i| {
                rank == Rank::Smallest || base[i].specs.latency.is_some()
            })
            .collect()
    };
    // Ticks, then size, for the fastest; size, then ticks, the smallest.
    let key = |i: usize, rank: Rank, ruler: Ruler| {
        let (s, ticks) = (base[i], base[i].specs.latency.unwrap_or(i64::MAX));
        match rank {
            Rank::Fastest => (ticks, ruler.size(s), 0),
            Rank::Smallest => (0, ruler.size(s), ticks),
        }
    };
    // The first places of the board of `wanted` ranking `field`.
    let firsts =
        |wanted: &BTreeSet<Trait>, field: &[usize], rank: Rank| -> Vec<usize> {
            let ruler = ruler(wanted);
            let best = field.iter().map(|&i| key(i, rank, ruler)).min();
            field
                .iter()
                .copied()
                .filter(|&i| Some(key(i, rank, ruler)) == best)
                .collect()
        };
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
            let first = firsts(wanted, &ranked, rank);
            if rank == Rank::Fastest {
                let fewest = base[first[0]].specs.latency;
                if ranked
                    .iter()
                    .filter(|&&i| base[i].specs.latency == fewest)
                    .count()
                    > 1
                {
                    continue;
                }
            }
            let changes = wanted.iter().all(|t| {
                let mut fewer = wanted.clone();
                fewer.remove(t);
                firsts(&fewer, &field(&fewer, rank), rank) != first
            });
            if !changes {
                continue;
            }
            let named = tags[first[0]]
                .iter()
                .filter(|tag| wanted.contains(&Trait::Tag(tag.name.clone())))
                .cloned()
                .collect();
            found.push(Found {
                wanted: wanted.clone(),
                ticks,
                tags: named,
                rank,
                ruler: ruler(wanted),
                field: ranked.iter().map(|&i| base[i]).collect(),
                holders: first.iter().map(|&i| base[i]).collect(),
            });
        }
    }
    shown_once(named_once(found))
}

/// `found` but one record of each set of boards of the same builds with
/// the same first place, where the first of them stood, named by the one
/// whose name says the most of them, as [`says`] weighs it, the first of
/// those that say as much.
fn named_once(found: Vec<Found>) -> Vec<Found> {
    let ids = |builds: &[&Build]| -> Vec<String> {
        builds.iter().map(|s| s.id.clone()).collect()
    };
    let mut kept: Vec<Found> = Vec::new();
    for record in found {
        let same = kept.iter().position(|other| {
            ids(&other.field) == ids(&record.field)
                && ids(&other.holders) == ids(&record.holders)
        });
        match same {
            Some(at) if says(&record) > says(&kept[at]) => kept[at] = record,
            Some(_) => {}
            None => kept.push(record),
        }
    }
    kept
}

/// How much a record's name says of its builds, to choose which of the
/// boards of the same builds with the same first place names it: a pitch
/// class, as `2-wide_stackable`, then a design, then a latency, each
/// before the board without it; boards that say as much keep the first.
fn says(record: &Found) -> (bool, bool, bool) {
    (
        record
            .tags
            .iter()
            .any(|tag| pitch_class(&tag.name).is_some()),
        record.tags.iter().any(|tag| tag.kind == Kind::Design),
        record.ticks.is_some(),
    )
}

/// `found` but each record of a looser stacking whose first place is a
/// tighter one's record too, its other traits the same, as that of
/// `4-wide_stackable` held by the record of `2-wide_stackable`: a build
/// first on both holds the tightest alone.
fn shown_once(found: Vec<Found>) -> Vec<Found> {
    let redundant: Vec<bool> = found
        .iter()
        .map(|record| {
            found.iter().any(|other| {
                let only = |a: &BTreeSet<Trait>, b: &BTreeSet<Trait>| {
                    let mut left = a.difference(b);
                    match (left.next(), left.next()) {
                        (Some(Trait::Tag(name)), None) => Some(name.clone()),
                        _ => None,
                    }
                };
                let swapped = only(&record.wanted, &other.wanted)
                    .zip(only(&other.wanted, &record.wanted));
                other.rank == record.rank
                    && swapped
                        .is_some_and(|(loose, tight)| tighter(&tight, &loose))
                    && other.holders.len() == record.holders.len()
                    && other
                        .holders
                        .iter()
                        .zip(&record.holders)
                        .all(|(a, b)| a.id == b.id)
            })
        })
        .collect();
    found
        .into_iter()
        .zip(redundant)
        .filter(|(_, redundant)| !redundant)
        .map(|(record, _)| record)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        interface::Encoding,
        models::schematic::{Credits, Entry, File, Specs, Widened},
        stack::StackDirection,
    };

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

    /// Interleaved builds compete only where a board asks for them: alone,
    /// or with the rest. A board of nothing else takes them by default.
    #[test]
    fn interleaved_builds_compete_only_when_asked_for() {
        let mut odd = build("odd", 1, None, Some(3), 10);
        odd.operands = crate::models::schematic::Operands::Interleaved;
        let adders = [odd, build("a", 1, None, Some(4), 20)];
        let all: Vec<&Build> = adders.iter().collect();
        let ids = |inputs: Option<Inputs>| {
            let wanted = Wanted {
                inputs,
                ..Wanted::default()
            };
            let board = board(&all, &wanted, &[Measure::Width]);
            assert_eq!(board.inputs_choices.len(), 2);
            board
                .builds
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(None), ["a"]);
        assert_eq!(ids(Some(Inputs::Interleaved)), ["odd"]);
        assert_eq!(ids(Some(Inputs::All)), ["odd", "a"]);
        let wanted = Wanted::parse("inputs=interleaved");
        assert_eq!(wanted.inputs, Some(Inputs::Interleaved));
        assert_eq!(wanted.href("/b"), "/b?inputs=interleaved");
        let only = board(&all[..1], &Wanted::default(), &[Measure::Width]);
        assert_eq!(only.inputs, Inputs::Interleaved);
        assert_eq!(only.builds.len(), 1);
    }

    /// The smaller interleaved build makes no record among the split ones,
    /// but one of its own among the interleaved.
    #[test]
    fn interleaved_builds_hold_records_apart() {
        let interleaved = |id: &str, volume: i64| Build {
            operands: crate::models::schematic::Operands::Interleaved,
            ..build(id, 1, None, Some(3), volume)
        };
        let adders = [
            interleaved("p", 10),
            interleaved("q", 12),
            build("a", 1, None, Some(3), 20),
            build("b", 1, None, Some(3), 30),
        ];
        let builds: Vec<&Build> = adders.iter().collect();
        let found: Vec<(String, Vec<&str>)> = records(&builds)
            .iter()
            .map(|r| {
                (r.name(), r.holders.iter().map(|s| s.id.as_str()).collect())
            })
            .collect();
        assert_eq!(
            found,
            [
                ("Smallest 1-bit adder".to_owned(), vec!["a"]),
                (
                    "Smallest 1-bit interleaved inputs adder".to_owned(),
                    vec!["p"]
                ),
            ]
        );
    }

    /// A gate of `bits`, `size`, stacking `ways`, each `(west, up, pitch)`.
    fn gate(
        id: &str,
        bits: i64,
        size: (i64, i64, i64),
        latency: i64,
        ways: &[(i64, i64, i64)],
    ) -> Build {
        Build {
            category: "xor_gate".to_owned(),
            file: sized(size),
            stacking: ways
                .iter()
                .map(|&(west, up, pitch)| StackDirection {
                    west,
                    up,
                    max_copies: None,
                    faces: None,
                    pitch: Some(pitch),
                })
                .collect(),
            ..build(id, bits, None, Some(latency), 0)
        }
    }

    /// Gates rank as cells: on no board of a width, every width together,
    /// each measured a bit, its box alone where no pitch class is asked
    /// for, and its step along the class's way where one is; copies of a
    /// gate stacked wider compete on none. An adder keeps its widths.
    #[test]
    fn cells_rank_by_no_width_and_crossing_bits_do() {
        let gates = [
            // 1 wide, standing alone in 25 blocks, its copies 2 apart.
            gate("narrow", 1, (1, 5, 5), 3, &[(2, 0, 2)]),
            gate("snug", 1, (2, 5, 4), 2, &[(2, 0, 2)]),
            // 2 bits in 196 blocks, 98 a bit, stacking 4 west.
            gate("pair", 2, (4, 7, 7), 2, &[(4, 0, 2)]),
            gate("alone", 1, (3, 3, 3), 2, &[]),
        ];
        let all: Vec<&Build> = gates.iter().collect();
        assert!(stacked(all.iter().copied()).is_empty());
        let open = board(&all, &Wanted::default(), &[Measure::Width]);
        assert!(open.splits.is_empty());
        assert_eq!(open.builds.len(), 4);
        assert_eq!(
            open.ruler,
            Ruler::Cell {
                west: false,
                up: false
            }
        );
        let order = |placed: Vec<Placed>| {
            placed.iter().map(|(_, s)| s.id.clone()).collect::<Vec<_>>()
        };
        assert_eq!(
            order(smallest(&open.builds, open.ruler)),
            ["narrow", "alone", "snug", "pair"]
        );
        let classes: Vec<&str> =
            open.stackings.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(classes, ["2-wide", "4-wide"]);
        let wanted = Wanted::parse("stacking=2-wide");
        let two = board(&all, &wanted, &[Measure::Width]);
        assert_eq!(
            two.ruler,
            Ruler::Cell {
                west: true,
                up: false
            }
        );
        assert_eq!(
            order(smallest(&two.builds, two.ruler)),
            ["snug", "narrow", "pair"]
        );
        // Builds that cross bits keep their width boards.
        let adders = adders();
        let adders: Vec<&Build> = adders.iter().collect();
        let board = board(&adders, &Wanted::default(), &[Measure::Width]);
        assert_eq!(board.splits.len(), 1);
        assert_eq!(board.ruler, Ruler::Whole);
    }

    /// Of builds that cross bits, as adders, a board of any stacking
    /// takes every pitch class's builds, so its first place, a tighter
    /// class's too, is held once, as the tighter class's record.
    #[test]
    fn a_stackable_record_held_by_a_pitch_classs_holder_is_shown_once() {
        let stacking = |up: i64, pitch: i64, s: Build| Build {
            stacking: vec![StackDirection {
                west: 0,
                up,
                max_copies: None,
                faces: None,
                pitch: Some(pitch),
            }],
            ..s
        };
        let adders = [
            build("a", 8, None, Some(3), 500),
            // 400 blocks, 200 a copy, 2 high.
            stacking(2, 2, build("b", 8, None, Some(4), 200)),
            stacking(8, 8, build("c", 8, None, Some(5), 300)),
            stacking(2, 2, build("d", 8, None, Some(6), 400)),
        ];
        let builds: Vec<&Build> = adders.iter().collect();
        let names: Vec<String> =
            records(&builds).iter().map(Record::name).collect();
        assert_eq!(
            names,
            [
                "Smallest 8-bit adder",
                "Fastest 8-bit adder",
                "Fastest 8-bit 2-high stackable adder",
            ]
        );
    }

    /// Boards of the same builds with the same first place are one record,
    /// named by a pitch class over a design, and a design over a latency.
    #[test]
    fn a_merged_record_is_named_by_its_stacking_then_design_then_ticks() {
        // Copies 2 apart, each as big as its box.
        let stacking = |s: Build| Build {
            file: sized((2, 5, s.file.size.0 / 10)),
            stacking: vec![StackDirection {
                west: 2,
                up: 0,
                max_copies: None,
                faces: None,
                pitch: Some(2),
            }],
            ..s
        };
        let names = |adders: &[Build]| -> Vec<String> {
            let builds: Vec<&Build> = adders.iter().collect();
            records(&builds).iter().map(Record::name).collect()
        };
        // The 6-tick builds are the 2-wide stackable ones, ripple carry.
        let adders = [
            build("x", 1, None, Some(4), 50),
            stacking(build("a", 1, None, Some(6), 100)),
            stacking(build("c", 1, None, Some(6), 150)),
        ];
        assert_eq!(
            names(&adders),
            [
                "Smallest 1-bit adder",
                "Smallest 1-bit 2-wide stackable adder",
            ]
        );
        let ripple = Some("ripple_carry");
        let adders = [
            build("x", 1, None, Some(4), 50),
            build("a", 1, ripple, Some(6), 100),
            build("c", 1, ripple, Some(6), 150),
        ];
        assert_eq!(
            names(&adders),
            ["Smallest 1-bit adder", "Smallest 1-bit RCA adder",]
        );
        let adders = [
            build("x", 1, None, Some(4), 50),
            stacking(build("a", 1, ripple, Some(6), 100)),
            stacking(build("c", 1, ripple, Some(6), 150)),
        ];
        assert_eq!(
            names(&adders),
            [
                "Smallest 1-bit adder",
                "Smallest 1-bit 2-wide stackable adder",
            ]
        );
    }

    /// Registers, cells on shared lines, rank as cells too: an 8-bit one
    /// and a 1-bit one on one board, each measured a bit.
    #[test]
    fn cells_on_shared_lines_rank_a_bit() {
        let register = |id: &str, bits: i64, size: (i64, i64, i64)| {
            let mut register = build(id, bits, None, Some(4), 0);
            register.category = "register".to_owned();
            register.specs.logic = Some(Logic::Stateful);
            register.file.size = size;
            register
        };
        let registers = [
            register("one", 1, (3, 12, 4)),
            register("eight", 8, (3, 19, 7)),
        ];
        let all: Vec<&Build> = registers.iter().collect();
        let board = board(&all, &Wanted::default(), &[Measure::Width]);
        assert_eq!(board.group, Group::SharedLines);
        assert!(board.splits.is_empty());
        let smallest = smallest(&board.builds, board.ruler);
        assert_eq!(ids(&smallest), [(1, "eight"), (2, "one")]);
        let size = board.ruler.size(smallest[0].1);
        assert_eq!((size.blocks, size.bits), (3 * 19 * 7, 8));
        let names: Vec<String> =
            records(&all).iter().map(Record::name).collect();
        assert_eq!(names, ["Smallest register"]);
    }

    /// A looser pitch class's board takes the tighter classes' builds, so
    /// a 2-wide gate may hold the 4-wide record; where it holds the 2-wide
    /// one too, it holds that alone.
    #[test]
    fn a_looser_class_record_held_by_the_tighter_ones_holder_is_shown_once() {
        let gates = [
            // Smallest alone, but its copies need air between them.
            gate("narrow", 1, (1, 5, 5), 3, &[(2, 0, 2)]),
            gate("snug", 1, (2, 5, 4), 3, &[(2, 0, 2)]),
            gate("wide", 1, (3, 5, 4), 3, &[(4, 0, 4)]),
            gate("high", 1, (2, 3, 5), 3, &[(0, 4, 4)]),
            gate("tall", 1, (3, 3, 5), 3, &[(0, 4, 4)]),
        ];
        let builds: Vec<&Build> = gates.iter().collect();
        let found: Vec<(String, Vec<&str>)> = records(&builds)
            .iter()
            .map(|r| {
                (r.name(), r.holders.iter().map(|s| s.id.as_str()).collect())
            })
            .collect();
        assert_eq!(
            found,
            [
                ("Smallest XOR gate".to_owned(), vec!["narrow"]),
                (
                    "Smallest 2-wide stackable XOR gate".to_owned(),
                    vec!["snug"]
                ),
                (
                    "Smallest 4-high stackable XOR gate".to_owned(),
                    vec!["high"]
                ),
            ]
        );
        // Alone on the 2-wide board, so holding no record there, the
        // 2-wide gate holds the 4-wide one.
        let gates = [
            gate("tiny", 1, (1, 3, 3), 3, &[]),
            gates[0].clone(),
            gates[2].clone(),
        ];
        let builds: Vec<&Build> = gates.iter().collect();
        let found: Vec<(String, Vec<&str>)> = records(&builds)
            .iter()
            .map(|r| {
                (r.name(), r.holders.iter().map(|s| s.id.as_str()).collect())
            })
            .collect();
        assert_eq!(
            found,
            [
                ("Smallest XOR gate".to_owned(), vec!["tiny"]),
                (
                    "Smallest 4-wide stackable XOR gate".to_owned(),
                    vec!["narrow"]
                ),
            ]
        );
    }

    /// A register whose copies add words on its shared bus carries how
    /// far apart they do, as `3-wide_word_stackable`, shown on its page,
    /// but no board narrows to it.
    #[test]
    fn word_stacking_splits_no_board() {
        let mut register = build("r", 8, None, Some(3), 10);
        register.category = "register".to_owned();
        register.specs.orientation = Orientation::Vertical;
        register.stacking = vec![StackDirection {
            west: 3,
            up: 0,
            max_copies: Some(5),
            faces: None,
            pitch: None,
        }];
        let named = |tags: Vec<Tag>| {
            tags.iter().any(|tag| {
                tag.name == "3-wide_word_stackable" && tag.is_found()
            })
        };
        assert!(named(register.every_tag()));
        assert!(!named(ranked_tags(&register)));
        // Nor does it make the register stackable on its boards.
        assert!(!ranked_tags(&register).iter().any(|t| t.name == "stackable"));
        // A stacking its author gave says nothing of words.
        register.file.conventional = false;
        assert!(
            !register.every_tag().iter().any(|tag| {
                crate::models::tag::is_word_stackable(&tag.name)
            })
        );
    }

    /// A conventional build's file facts, its box of `size`.
    fn sized(size: (i64, i64, i64)) -> File {
        File {
            size,
            conventional: true,
            ..File::default()
        }
    }

    fn build(
        id: &str,
        width: i64,
        design: Option<&str>,
        latency: Option<i64>,
        volume: i64,
    ) -> Build {
        Build {
            id: id.to_owned(),
            category: "adder".to_owned(),
            specs: Specs {
                width: Some(width),
                orientation: Orientation::Horizontal,
                encoding: Some(Encoding::Binary),
                latency,
                design: design.map(str::to_owned),
                ..Specs::default()
            },
            file: sized((volume, 1, 1)),
            entry: Entry {
                added_at: id.to_owned(),
                public: true,
                ..Entry::default()
            },
            stacking: Vec::new(),
            widened: Vec::new(),
            stacked_from: None,
            credits: Credits::default(),
            built_on: None,
            flags: Vec::new(),
            description: None,
            given_name: None,
            unverified: None,
            status: crate::models::schematic::Status::default(),
            operands: crate::models::schematic::Operands::Split,
            unaligned: false,
            tags: Vec::new(),
        }
    }

    fn adders() -> Vec<Build> {
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
            s.specs.logic = Some(Logic::Stateless);
        }
        adders[0].specs.logic = Some(Logic::Stateful);
        let stateful_id = adders[0].id.clone();
        let all: Vec<&Build> = adders.iter().collect();
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
        let all: Vec<&Build> = adders.iter().collect();
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
        let ram = |id: &str, bytes: Option<i64>| {
            let mut ram = build(id, 8, None, Some(4), 100);
            ram.specs.bytes = bytes;
            ram
        };
        let rams = [
            ram("a", Some(32)),
            ram("b", Some(32)),
            ram("c", Some(256)),
            ram("d", None),
        ];
        let all: Vec<&Build> = rams.iter().collect();
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
        let threshold = |id: &str, at_least: i64| {
            let mut threshold = build(id, 8, None, Some(4), 100);
            threshold.specs.at_least = Some(at_least);
            threshold
        };
        let all = [threshold("a", 3), threshold("b", 5), threshold("c", 5)];
        let all: Vec<&Build> = all.iter().collect();
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
        let all: Vec<&Build> = adders.iter().collect();
        let board = board(&all, &Wanted::default(), &[Measure::Width]);
        assert_eq!(
            ids(&fastest(&board.builds, Ruler::Whole)),
            [(1, "a"), (2, "b"), (3, "d"), (4, "c"), (5, "e")]
        );
        assert_eq!(
            ids(&smallest(&board.builds, Ruler::Whole)),
            [(1, "g"), (2, "a"), (3, "b"), (4, "d"), (5, "e"), (6, "c")]
        );
        let tied = [
            build("x", 8, None, Some(3), 100),
            build("y", 8, None, Some(3), 100),
            build("z", 8, None, Some(2), 100),
        ];
        let tied: Vec<&Build> = tied.iter().collect();
        assert_eq!(
            ids(&fastest(&tied, Ruler::Whole)),
            [(1, "z"), (2, "x"), (2, "y")]
        );
    }

    /// The builds of the tests below, each by one author.
    fn credited_adders() -> Vec<Build> {
        let by = |name: &str, s: Build| Build {
            credits: Credits {
                authors: vec![name.to_owned()],
                ..Credits::default()
            },
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
        let builds: Vec<&Build> = adders.iter().collect();
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
                ("Smallest 8-bit CLA adder".to_owned(), vec!["e"]),
                ("Fastest 8-bit CLA adder".to_owned(), vec!["d"]),
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
        let builds: Vec<&Build> = adders.iter().collect();
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
            Build {
                tags: tags.clone(),
                ..build("a", 8, None, Some(3), 900)
            },
            Build {
                tags,
                ..build("b", 8, None, Some(3), 700)
            },
        ];
        let builds: Vec<&Build> = adders.iter().collect();
        let names: Vec<String> =
            records(&builds).iter().map(Record::name).collect();
        assert_eq!(names, ["Smallest 8-bit adder"]);
    }

    #[test]
    fn people_count_whatever_their_case() {
        let by = |id: &str, authors: &[&str]| {
            let mut s = build(id, 8, None, None, 1);
            s.credits.authors =
                authors.iter().map(|&name| name.to_owned()).collect();
            s
        };
        let mut imported = by("a", &["embizone"]);
        imported.entry.imported_by = Some("embizone".to_owned());
        let mut uploaded = by("b", &["EmbiZone", "alex"]);
        uploaded.entry.uploader = Some("lookaside".to_owned());
        uploaded.entry.uploader_name = Some("EMBIZONE".to_owned());
        let builds = [imported, uploaded];
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
        let slice = Build {
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
            file: sized((3, 4, 5)),
            ..build("s", 1, Some("ripple_carry"), Some(6), 60)
        };
        let adders =
            [slice, build("a", 8, Some("ripple_carry"), Some(25), 1000)];
        let stacks = stacked(&adders);
        let all: Vec<&Build> = adders.iter().chain(&stacks).collect();
        let wanted = Wanted {
            measures: vec![(Measure::Width, 8)],
            ..Wanted::default()
        };
        let board = board(&all, &wanted, &[Measure::Width]);
        let fastest = fastest(&board.builds, board.ruler);
        assert_eq!(ids(&fastest), [(1, "s"), (2, "a")]);
        let stack = fastest[0].1;
        assert_eq!(stack.stacked_from, Some(1));
        assert_eq!(stack.specs.latency, Some(20));
        assert_eq!(stack.file.size, (3 + 2 * 7, 4, 5));
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
        let builds: Vec<&Build> = builds.iter().collect();
        let kept: Vec<&str> = tradeoffs(&builds, Ruler::Whole)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(kept, ["a", "c", "d"]);
    }
}
