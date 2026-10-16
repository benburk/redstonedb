//! Leaderboards: a category's builds ranked against the others that match
//! them in its measures, such as width, then state, encoding, orientation,
//! and design, since only those compete: a hex adder never meets a binary
//! one, nor a stateful multiplier a stateless one.

use std::{collections::BTreeMap, str::FromStr};

use crate::models::{
    catalogue::Measure,
    schematic::{Encoding, Logic, Orientation, Schematic},
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

/// What a leaderboard's address asks for; `None` takes the default.
#[derive(Debug, Default)]
pub struct Wanted {
    /// A value of each measure asked for, such as a width of 8.
    pub measures: Vec<(Measure, i64)>,
    pub logic: Option<State>,
    pub encoding: Option<Encoding>,
    pub orientation: Option<Orientation>,
    /// A design's name; `None` for every design, where they compete.
    pub design: Option<String>,
}

impl Wanted {
    fn measure(&self, measure: Measure) -> Option<i64> {
        self.measures
            .iter()
            .find(|(m, _)| *m == measure)
            .map(|(_, n)| *n)
    }
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
    /// Binary and hex; empty when the builds count only one way.
    pub encodings: Vec<Choice<Encoding>>,
    pub encoding: Option<Encoding>,
    pub orientations: Vec<Choice<Orientation>>,
    pub orientation: Option<Orientation>,
    /// Empty when the category has a single design.
    pub designs: Vec<Choice<String>>,
    /// `None` for every design together.
    pub design: Option<String>,
    /// Whether "every design" is a choice.
    pub all_designs: bool,
    pub builds: Vec<&'a Schematic>,
}

/// The board of `category`, whose measures are `measures`, that `wanted`
/// asks for. Each choice not asked for falls to the most common value
/// among the builds the earlier choices leave, so a board starts where
/// most builds are. A build missing a measure, such as one of unknown
/// width, competes on no board.
pub fn board<'a>(
    category: &[&'a Schematic],
    wanted: &Wanted,
    measures: &[Measure],
) -> Board<'a> {
    let mut ranked: Vec<&Schematic> = category.to_vec();
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
    let (logics, logic, ranked) = narrow(ranked, wanted.logic, |s| {
        let state = State::of(s);
        (state, state.label().to_owned())
    });
    // Hex and binary builds never compete.
    let (encodings, encoding, ranked) = narrow(ranked, wanted.encoding, |s| {
        let encoding = encoding(s);
        (encoding, encoding.label().to_owned())
    });
    let (orientations, orientation, ranked) =
        narrow(ranked, wanted.orientation, |s| {
            (s.orientation, s.orientation.label().to_owned())
        });

    // By the short name people say, such as CCA, where there is one.
    let designs = choices(&ranked, |s| {
        let label = s
            .design_abbreviation
            .clone()
            .or_else(|| s.design_label())
            .unwrap_or_else(|| "Other".to_owned());
        (s.design.clone(), label)
    });
    let all_designs = designs.len() > 1;
    let design = wanted.design.clone().or_else(|| {
        (!all_designs)
            .then(|| most_common(&designs).cloned())
            .flatten()
    });
    let builds = ranked
        .into_iter()
        .filter(|s| design.as_ref().is_none_or(|d| &s.design == d))
        .collect();
    Board {
        splits,
        logics: if logics.len() > 1 { logics } else { Vec::new() },
        logic,
        encodings: if encodings.len() > 1 {
            encodings
        } else {
            Vec::new()
        },
        encoding,
        orientations,
        orientation,
        designs: if designs.len() > 1 {
            designs
        } else {
            Vec::new()
        },
        design,
        all_designs,
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

/// How a build counts: in hex, or in binary, as one of unknown encoding
/// is taken to.
fn encoding(s: &Schematic) -> Encoding {
    s.encoding.unwrap_or(Encoding::Binary)
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

/// The builds in first place, more than one on a tie.
fn firsts(placed: Vec<Placed<'_>>) -> impl Iterator<Item = &Schematic> {
    placed
        .into_iter()
        .take_while(|(place, _)| *place == 1)
        .map(|(_, s)| s)
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

/// Builders by records held: first place, as fastest or as smallest, on a
/// board of two or more builds. A board is a standard category's builds of
/// one value of each of its measures, logic, encoding, orientation, and
/// design; each builder of a record holder counts it, and a tie gives every
/// holder one.
pub fn most_records(builds: &[Schematic]) -> Vec<Standing> {
    type Key<'a> = (&'a str, Vec<i64>, State, Encoding, Orientation, &'a str);
    let mut boards: BTreeMap<Key, Vec<&Schematic>> = BTreeMap::new();
    for s in builds.iter().filter(|s| s.category_label.is_some()) {
        let Some(values) = s
            .measures
            .iter()
            .map(|m| m.of(s))
            .collect::<Option<Vec<i64>>>()
            .filter(|values| !values.is_empty())
        else {
            continue;
        };
        boards
            .entry((
                &s.category,
                values,
                State::of(s),
                encoding(s),
                s.orientation,
                &s.design,
            ))
            .or_default()
            .push(s);
    }
    let holders =
        boards
            .values()
            .filter(|board| board.len() >= 2)
            .flat_map(|board| {
                firsts(fastest(board)).chain(firsts(smallest(board)))
            });
    tally(holders.flat_map(credited))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(
        id: &str,
        width: i64,
        design: &str,
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
            measures: vec![Measure::Width],
            aliases: Vec::new(),
            torches: None,
            works_from: None,
            works_from_by: None,
            bytes: None,
            size: (volume, 1, 1),
            stacking: Vec::new(),
            builders: Vec::new(),
            contributors: Vec::new(),
            built_on: None,
            imported_by: None,
            conventional: true,
            keeps_paste: false,
            design: design.to_owned(),
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
            tags: Vec::new(),
        }
    }

    fn adders() -> Vec<Schematic> {
        vec![
            build("a", 8, "carry_cancel", Some(3), 684),
            build("b", 8, "carry_cancel", Some(3), 2640),
            build("c", 8, "carry_lookahead", Some(4), 6498),
            build("d", 8, "carry_look_everywhere", Some(4), 3696),
            build("e", 8, "carry_lookahead", Some(8), 3762),
            build("f", 16, "carry_cancel", Some(4), 1960),
            build("g", 8, "ripple_carry", None, 500),
        ]
    }

    fn ids<'a>(placed: &[Placed<'a>]) -> Vec<(usize, &'a str)> {
        placed.iter().map(|(n, s)| (*n, s.id.as_str())).collect()
    }

    #[test]
    fn hex_and_binary_builds_never_share_a_board() {
        let mut adders = adders();
        adders[0].encoding = Some(Encoding::Hex);
        let hex_id = adders[0].id.clone();
        let all: Vec<&Schematic> = adders.iter().collect();
        let binary = board(&all, &Wanted::default(), &[Measure::Width]);
        assert_eq!(binary.encoding, Some(Encoding::Binary));
        assert!(binary.builds.iter().all(|s| s.id != hex_id));
        let labels: Vec<&str> =
            binary.encodings.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Binary", "Hex"]);
        let wanted = Wanted {
            encoding: Some(Encoding::Hex),
            ..Wanted::default()
        };
        let hex = board(&all, &wanted, &[Measure::Width]);
        let ids: Vec<&str> = hex.builds.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, [hex_id.as_str()]);
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
    fn a_board_starts_at_the_most_common_width_with_every_design() {
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
        assert_eq!(board.orientation, Some(Orientation::Horizontal));
        assert_eq!(board.design, None);
        assert!(board.all_designs);
        assert_eq!(board.builds.len(), 6);
    }

    #[test]
    fn memories_compete_only_at_one_capacity() {
        let ram = |id: &str, bytes: Option<i64>| Schematic {
            bytes,
            ..build(id, 8, "other", Some(4), 100)
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
            ..build(id, 8, "other", Some(4), 100)
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
            build("x", 8, "other", Some(3), 100),
            build("y", 8, "other", Some(3), 100),
            build("z", 8, "other", Some(2), 100),
        ];
        let tied: Vec<&Schematic> = tied.iter().collect();
        assert_eq!(ids(&fastest(&tied)), [(1, "z"), (2, "x"), (2, "y")]);
    }

    #[test]
    fn records_are_first_places_on_boards_of_two_or_more() {
        let by = |name: &str, s: Schematic| Schematic {
            builders: vec![name.to_owned()],
            ..s
        };
        let builds = [
            // 8-bit CCAs: a is fastest and smallest.
            by("Alex", build("a", 8, "carry_cancel", Some(3), 684)),
            by("bob", build("b", 8, "carry_cancel", Some(4), 900)),
            // Alone on its board, so no record.
            by("bob", build("c", 16, "carry_cancel", Some(4), 1960)),
            // 8-bit CLAs: d is fastest, e smallest.
            by("alex", build("d", 8, "carry_lookahead", Some(4), 6498)),
            by("Bob", build("e", 8, "carry_lookahead", Some(8), 3762)),
        ];
        let counts: Vec<(String, usize)> = most_records(&builds)
            .into_iter()
            .map(|s| (s.name, s.count))
            .collect();
        assert_eq!(counts, [("Alex".to_owned(), 3), ("Bob".to_owned(), 1)]);
    }

    #[test]
    fn people_count_whatever_their_case() {
        let builds = [
            Schematic {
                builders: vec!["embizone".to_owned()],
                imported_by: Some("embizone".to_owned()),
                ..build("a", 8, "other", None, 1)
            },
            Schematic {
                builders: vec!["EmbiZone".to_owned(), "alex".to_owned()],
                uploader: Some("lookaside".to_owned()),
                uploader_name: Some("EMBIZONE".to_owned()),
                ..build("b", 8, "other", None, 1)
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

    #[test]
    fn tradeoffs_keep_each_build_smaller_than_all_quicker_ones() {
        let builds = [
            build("a", 8, "other", Some(3), 900),
            build("b", 8, "other", Some(4), 950),
            build("c", 8, "other", Some(5), 400),
            build("d", 8, "other", Some(9), 380),
        ];
        let builds: Vec<&Schematic> = builds.iter().collect();
        let kept: Vec<&str> =
            tradeoffs(&builds).iter().map(|s| s.id.as_str()).collect();
        assert_eq!(kept, ["a", "c", "d"]);
    }
}
