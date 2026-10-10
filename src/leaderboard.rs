//! Leaderboards: a category's builds ranked against the others that match
//! them in width, orientation, and design, since only those compete. A
//! serial category's builds match in period instead of width.

use std::collections::BTreeMap;

use crate::models::schematic::Schematic;

/// How many builds each ranking shows.
pub const SHOWN: usize = 10;

/// What a category's boards are split by first: width in bits, or for a
/// serial category, period in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    Width,
    Period,
}

impl Measure {
    pub fn of(self, s: &Schematic) -> Option<i64> {
        match self {
            Measure::Width => s.width,
            Measure::Period => s.period,
        }
    }

    /// Such as `8-bit` or `2-tick`.
    pub fn label(self, n: i64) -> String {
        match self {
            Measure::Width => format!("{n}-bit"),
            Measure::Period => format!("{n}-tick"),
        }
    }

    /// Its name as a heading, and in a board's address.
    pub fn name(self) -> &'static str {
        match self {
            Measure::Width => "width",
            Measure::Period => "period",
        }
    }
}

/// What a leaderboard's address asks for; `None` takes the default.
#[derive(Debug, Default)]
pub struct Wanted {
    /// The width or period.
    pub measure: Option<i64>,
    /// A memory's capacity in bytes.
    pub bytes: Option<i64>,
    pub orientation: Option<String>,
    /// A design's name; `None` for every design, where they compete.
    pub design: Option<String>,
}

/// One value a board can be narrowed to, with how many builds have it.
#[derive(Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub count: usize,
}

/// One board: the choices that narrowed it, the ones chosen, and the
/// builds left.
#[derive(Debug)]
pub struct Board<'a> {
    pub by: Measure,
    pub measures: Vec<Choice>,
    pub measure: Option<i64>,
    /// A memory's capacities in bytes; empty for any other category.
    pub capacities: Vec<Choice>,
    pub capacity: Option<i64>,
    pub orientations: Vec<Choice>,
    pub orientation: Option<String>,
    /// Empty when the category has a single design.
    pub designs: Vec<Choice>,
    /// `None` for every design together.
    pub design: Option<String>,
    /// Whether "every design" is a choice.
    pub all_designs: bool,
    pub builds: Vec<&'a Schematic>,
}

/// The board of `category` that `wanted` asks for. Each choice not asked
/// for falls to the most common value among the builds the earlier choices
/// leave, so a board starts where most builds are. A build whose width, or
/// period, is unknown competes on no board.
pub fn board<'a>(
    category: &[&'a Schematic],
    wanted: &Wanted,
    by: Measure,
    by_bytes: bool,
    designs_interchangeable: bool,
) -> Board<'a> {
    let ranked: Vec<&Schematic> = category
        .iter()
        .copied()
        .filter(|s| by.of(s).is_some())
        .collect();

    let measures = choices(&ranked, |s| {
        let n = by.of(s).unwrap_or_default();
        // Zero-padded, so the choices sort by number.
        (format!("{n:08}"), by.label(n))
    });
    let measure = wanted.measure.or_else(|| {
        most_common(&measures).and_then(|value| value.parse().ok())
    });
    let measures = unpadded(measures);
    let ranked: Vec<&Schematic> =
        ranked.into_iter().filter(|s| by.of(s) == measure).collect();

    // A memory's boards split by capacity too, and one of unknown capacity
    // competes on none.
    let (capacities, capacity, ranked) = if by_bytes {
        let ranked: Vec<&Schematic> =
            ranked.into_iter().filter(|s| s.bytes.is_some()).collect();
        let capacities = choices(&ranked, |s| {
            let b = s.bytes.unwrap_or_default();
            (format!("{b:012}"), format!("{b} bytes"))
        });
        let capacity = wanted.bytes.or_else(|| {
            most_common(&capacities).and_then(|value| value.parse().ok())
        });
        let ranked =
            ranked.into_iter().filter(|s| s.bytes == capacity).collect();
        (unpadded(capacities), capacity, ranked)
    } else {
        (Vec::new(), None, ranked)
    };

    let orientations = choices(&ranked, |s| {
        (s.orientation.clone(), s.orientation_label().to_owned())
    });
    let orientation = wanted
        .orientation
        .clone()
        .or_else(|| most_common(&orientations).map(str::to_owned));
    let ranked: Vec<&Schematic> = ranked
        .into_iter()
        .filter(|s| Some(&s.orientation) == orientation.as_ref())
        .collect();

    // By the short name people say, such as CCA, where there is one.
    let designs = choices(&ranked, |s| {
        let label = s
            .design_abbreviation
            .clone()
            .or_else(|| s.design_label())
            .unwrap_or_else(|| "Other".to_owned());
        (s.design.clone(), label)
    });
    let all_designs = designs_interchangeable && designs.len() > 1;
    let design = wanted.design.clone().or_else(|| {
        (!all_designs)
            .then(|| most_common(&designs).map(str::to_owned))
            .flatten()
    });
    let builds = ranked
        .into_iter()
        .filter(|s| design.as_ref().is_none_or(|d| &s.design == d))
        .collect();
    Board {
        by,
        measures,
        measure,
        capacities,
        capacity,
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

/// `choices` with the zeros that sorted their values by number taken off.
fn unpadded(choices: Vec<Choice>) -> Vec<Choice> {
    choices
        .into_iter()
        .map(|c| Choice {
            value: c.value.trim_start_matches('0').to_owned(),
            ..c
        })
        .collect()
}

/// Each distinct `(value, label)` of `builds`, in value order, with its
/// count.
fn choices(
    builds: &[&Schematic],
    key: impl Fn(&Schematic) -> (String, String),
) -> Vec<Choice> {
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
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
fn most_common(choices: &[Choice]) -> Option<&str> {
    choices
        .iter()
        .rev()
        .max_by_key(|c| c.count)
        .map(|c| c.value.as_str())
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
/// one width, or period, capacity where it splits by bytes, orientation,
/// and design, `split` saying which for a category;
/// each builder of a record holder counts it, and a tie gives every holder
/// one.
pub fn most_records(
    builds: &[Schematic],
    split: impl Fn(&str) -> (Measure, bool),
) -> Vec<Standing> {
    type Key<'a> = (&'a str, i64, Option<i64>, &'a str, &'a str);
    let mut boards: BTreeMap<Key, Vec<&Schematic>> = BTreeMap::new();
    for s in builds.iter().filter(|s| s.category_label.is_some()) {
        let (by, by_bytes) = split(&s.category);
        let Some(n) = by.of(s) else { continue };
        if by_bytes && s.bytes.is_none() {
            continue;
        }
        boards
            .entry((&s.category, n, s.bytes, &s.orientation, &s.design))
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
            orientation: "horz".to_owned(),
            encoding: Some("bin".to_owned()),
            latency,
            synchronized: None,
            period: None,
            bytes: None,
            size: (volume, 1, 1),
            stacking: Vec::new(),
            builders: Vec::new(),
            contributors: Vec::new(),
            built_on: None,
            imported_by: None,
            conventional: true,
            design: design.to_owned(),
            design_abbreviation: None,
            design_given_label: None,
            named_by_design: false,
            flags: Vec::new(),
            description: None,
            given_name: None,
            uploader: None,
            uploader_name: None,
            sha256: String::new(),
            added_at: id.to_owned(),
            edited_at: None,
            downloads: 0,
            fails_simulation: false,
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
    fn a_board_starts_at_the_most_common_width_with_every_design() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board =
            board(&all, &Wanted::default(), Measure::Width, false, true);
        assert_eq!(board.measure, Some(8));
        assert_eq!(
            board
                .measures
                .iter()
                .map(|c| c.value.as_str())
                .collect::<Vec<_>>(),
            ["8", "16"]
        );
        assert_eq!(board.orientation.as_deref(), Some("horz"));
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
        let board = board(&all, &Wanted::default(), Measure::Width, true, true);
        assert_eq!(board.capacity, Some(32));
        assert_eq!(
            board
                .capacities
                .iter()
                .map(|c| c.value.as_str())
                .collect::<Vec<_>>(),
            ["32", "256"]
        );
        assert_eq!(board.builds.len(), 2);
    }

    #[test]
    fn designs_that_differ_never_share_a_board() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board =
            board(&all, &Wanted::default(), Measure::Width, false, false);
        assert!(!board.all_designs);
        assert_eq!(board.design.as_deref(), Some("carry_cancel"));
        assert_eq!(board.builds.len(), 2);
    }

    #[test]
    fn rankings_share_a_place_on_a_tie_and_skip_unknown_latency() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board =
            board(&all, &Wanted::default(), Measure::Width, false, true);
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
        let counts: Vec<(String, usize)> =
            most_records(&builds, |_| (Measure::Width, false))
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
