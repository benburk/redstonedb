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
    let measures = measures
        .into_iter()
        .map(|c| Choice {
            value: c.value.trim_start_matches('0').to_owned(),
            ..c
        })
        .collect();
    let ranked: Vec<&Schematic> =
        ranked.into_iter().filter(|s| by.of(s) == measure).collect();

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
            flags: Vec::new(),
            description: None,
            given_name: None,
            uploader: None,
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
    fn a_board_starts_at_the_most_common_width_with_every_design() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board = board(&all, &Wanted::default(), Measure::Width, true);
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
    fn designs_that_differ_never_share_a_board() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board = board(&all, &Wanted::default(), Measure::Width, false);
        assert!(!board.all_designs);
        assert_eq!(board.design.as_deref(), Some("carry_cancel"));
        assert_eq!(board.builds.len(), 2);
    }

    #[test]
    fn rankings_share_a_place_on_a_tie_and_skip_unknown_latency() {
        let adders = adders();
        let all: Vec<&Schematic> = adders.iter().collect();
        let board = board(&all, &Wanted::default(), Measure::Width, true);
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
