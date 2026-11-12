//! What a build's checks are worked out from: what a page knows of the
//! build, how a stage can come out and which stages it needs, and what the
//! check by function finds wrong with its lines, each kind once. No markup
//! is made here.

use std::collections::BTreeMap;

use crate::{
    build::Build,
    interface::problem::{Problem, Rule, place},
    simulate::{BitCount, Report, Run},
    web::{build::detail_view::FileRead, ui::Tone},
};

/// How a stage came out.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Pass,
    /// Passed, with something worth a look that holds nothing up.
    Warn,
    Fail,
    /// Not run, as a stage before it failed or there is nothing to run.
    Skip,
}

impl State {
    pub(crate) fn key(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Warn => "warn",
            State::Fail => "fail",
            State::Skip => "skip",
        }
    }

    pub(crate) fn mark(self) -> &'static str {
        match self {
            State::Pass => "✓",
            State::Warn => "!",
            State::Fail => "✗",
            State::Skip => "–",
        }
    }

    pub(crate) fn tone(self) -> Tone {
        match self {
            State::Pass => Tone::Good,
            State::Warn => Tone::Warn,
            State::Fail => Tone::Bad,
            State::Skip => Tone::Plain,
        }
    }
}

/// What its checks say of the build, stored or being uploaded: its box,
/// its category, why it ranks on no board though it works, whether its
/// buses are unaligned, whether it keeps to the conventions, and the
/// reason stored against it, if any.
pub struct Subject<'a> {
    pub size: (i64, i64, i64),
    pub category: &'a str,
    pub unranked: Option<String>,
    pub unaligned: bool,
    pub conventional: bool,
    pub unverified: Option<&'a str>,
}

impl<'a> Subject<'a> {
    /// Stored build `s`.
    pub fn of(s: &'a Build) -> Subject<'a> {
        Subject {
            size: s.file.size,
            category: &s.category,
            unranked: s.unranked(),
            unaligned: s.unaligned,
            conventional: s.file.conventional,
            unverified: s.unverified.as_deref(),
        }
    }
}

/// What a page knows of a build, which its stages are worked out from.
pub struct Known<'a> {
    pub s: Subject<'a>,
    pub file: &'a Result<FileRead, String>,
    pub report: Option<&'a Report>,
    /// Where its traced glitch is asked for, under `/glitch`; `None` for a
    /// build not yet stored.
    pub trace: Option<&'a str>,
    /// Where the details form asks for its timing, when it is measured
    /// once the page has loaded rather than stored.
    pub timing: Option<&'a str>,
}

/// The stages the one keyed `key` reads from: the test and timing its
/// lines, and stacking a test that passes. Timing needs no right
/// answers, so it runs on a build that gets cases wrong.
pub(crate) fn needs(key: &str) -> &'static [&'static str] {
    match key {
        "lines" => &["file"],
        "test" | "timing" => &["file", "lines"],
        "stacking" => &["file", "lines", "test"],
        _ => &[],
    }
}

/// What the check by function finds wrong with its lines, less the cases
/// its test got wrong, which the test shows: each kind of problem once,
/// with every place it is, as `Lever at (5, 2, 3): touches repeater at (4,
/// 2, 3)` five times over is one problem at five places.
pub(crate) fn problems(known: &Known) -> Vec<Found> {
    let Ok(file) = known.file else {
        return Vec::new();
    };
    // Only a kind that takes digits may take a signal strength.
    let strength = file.strength_inputs
        && !crate::category::Kind::named(known.s.category)
            .is_some_and(crate::category::Kind::digit_inputs);
    let strength = strength.then(|| {
        Problem::said(Rule::WithoutLevers, crate::build::status::WITHOUT_LEVERS)
    });
    let mut kinds: BTreeMap<String, Found> = BTreeMap::new();
    for problem in strength.iter().chain(&file.unrecognized) {
        if problem.message.is_empty() || problem.rule == Rule::WrongCases {
            continue;
        }
        let kind = problem.kind();
        let found = kinds.entry(kind.clone()).or_insert_with(|| Found {
            kind,
            rule: problem.rule,
            places: Vec::new(),
        });
        if !problem.places.is_empty() {
            let places: Vec<String> =
                problem.places.iter().copied().map(place).collect();
            found.places.push(places.join(" → "));
        }
    }
    kinds.into_values().collect()
}

/// One kind of problem: as said with its places left out, the rule it
/// breaks, and each place it is, the places one sentence names together.
pub(crate) struct Found {
    pub(crate) kind: String,
    pub(crate) rule: Rule,
    pub(crate) places: Vec<String>,
}

/// Each result bit's counts, or for a test that doesn't count them, each
/// bit with how often it was wrong alone.
pub(crate) fn bit_counts(run: &Run) -> Vec<BitCount> {
    if !run.bit_counts.is_empty() {
        return run.bit_counts.clone();
    }
    run.outputs
        .iter()
        .flat_map(|line| {
            (0..line.bits).map(move |bit| match line.bits {
                1 => line.name.clone(),
                _ => format!("{} bit {bit}", line.name),
            })
        })
        .map(|name| {
            let wrong = run
                .wrong_bits
                .iter()
                .find(|(bit, _)| *bit == name)
                .map_or(0, |(_, n)| *n);
            // Neither count is known, so no pattern is claimed.
            BitCount {
                name,
                wrong,
                got_on: 1,
                expected_on: 1,
            }
        })
        .collect()
}
