//! What a build's file and its simulation decide of its details, in place
//! of what was typed: worked out here without touching the database, and
//! written by `Derived::save` in the module above.

use crate::{
    category, import,
    models::schematic::{Build, Specs, StackDirection, Widened},
    recognize::{evaluate::Evaluation, interface},
    schem::Blocks,
    simulate::{Edge, Report, Stacking, Test},
};

/// The flag of a monostable whose output rests on, so its pulse turns it
/// off.
pub const INVERTED: &str = "inverted";

/// The flag of a comparator that reads its operands in two's complement.
pub const SIGNED: &str = "signed";

/// The flag of a monostable that pulses as A turns off, not on.
pub const FALLING_EDGE: &str = "falling_edge";

/// The details worked out from a build's file and its simulation, over
/// what was typed, as they stand or are to be.
#[derive(PartialEq)]
pub(super) struct Derived {
    pub(super) specs: Specs,
    pub(super) flags: Vec<String>,
    pub(super) stacking: Vec<StackDirection>,
    /// Its copies stacked into wider builds; see `Widened`.
    pub(super) widened: Vec<Widened>,
}

impl Derived {
    /// Build `s`'s, as stored.
    pub(super) fn of(s: &Build) -> Self {
        Derived {
            specs: s.specs.clone(),
            flags: s.flags.iter().map(|flag| flag.name.to_owned()).collect(),
            stacking: s.stacking.clone(),
            widened: s.widened.clone(),
        }
    }

    /// Everything `e` worked out from build `s`'s file, for a build of a
    /// category whose details all come from `evaluate`: what its file
    /// shows as the category, its ports' width, orientation, encoding,
    /// and flags, whatever the test made of it, and, once the test
    /// recognizes it as the category, what the simulation measured. What
    /// could not be worked out is unknown, not what was typed.
    pub(super) fn evaluated(s: &Build, e: &Evaluation) -> Self {
        let stored = Derived::of(s);
        let mut derived = Derived {
            specs: Specs {
                latency: None,
                synchronized: None,
                throughput: None,
                design: None,
                ..stored.specs
            },
            stacking: Vec::new(),
            widened: Vec::new(),
            ..stored
        };
        derived.read(&s.category, &e.prepared.blocks);
        if e.category == Some(s.category.as_str()) {
            if let Some(orientation) = e.orientation {
                derived.specs.orientation = orientation;
            }
            derived.specs.latency = e.latency;
            derived.specs.synchronized = e.synchronized;
            derived.stacking.clone_from(&e.stacking);
            derived.widened.clone_from(&e.widened);
            derived.flags.clone_from(&e.flags);
            if e.pulse.is_some() {
                derived.specs.pulse = e.pulse;
            }
            if let Some(Report::Ran(run)) = &e.report
                && run.wrong == 0
                && category::behaviour(&s.category).test
                    == Some(Test::Threshold)
            {
                derived.specs.at_least = run.at_least();
            }
            derived.specs.throughput = e.throughput;
            derived.specs.design = e
                .design
                .as_ref()
                .and_then(|design| design.given())
                .map(str::to_owned);
            if e.logic.is_some() {
                derived.specs.logic = e.logic;
            }
        }
        derived
    }

    /// Takes what a build of `category` reads from its file, `blocks`.
    pub(super) fn read(&mut self, category: &str, blocks: &Blocks) {
        let interface = interface::read(blocks);
        let found = import::derived(category, &interface);
        if let Some(width) = found.width {
            self.specs.width = Some(width);
        }
        if let Some(orientation) = found.orientation {
            self.specs.orientation = orientation;
        }
        if let Some(encoding) = found.encoding {
            self.specs.encoding = Some(encoding);
        }
        if let Some(flags) = import::derived_flags(category, &interface) {
            self.flags = flags;
        }
    }

    /// Takes what build `s`'s result `report` decides: the latency and
    /// synchronized of a result that measured its timing, a monostable's
    /// pulse and whether it is inverted, and, for a
    /// conventional build of a category whose stacking the simulation
    /// decides, how it stacks. An adder takes the step its copies added
    /// right at, with no limit on copies, or none. A register takes the
    /// step west its copies stored and read right at, with the most copies
    /// that did, in place of its author's west step and beside their
    /// others, which the search does not try; it keeps its author's
    /// stacking when none was found. A result that did not stack copies,
    /// such as one with wrong cases, leaves the stacking as it was.
    pub(super) fn decide(&mut self, s: &Build, report: &Report) {
        if let Some(timing) = report.timing()
            && let Some(latency) = timing.latency()
        {
            self.specs.latency = Some(latency.max(0).into());
            self.specs.synchronized = Some(timing.synced);
        }
        // A comparator's cases say whether it reads its operands signed.
        if let Report::Ran(run) = report
            && run.wrong == 0
            && category::behaviour(&s.category).test == Some(Test::GreaterThan)
        {
            self.flags.retain(|flag| flag != SIGNED);
            if run.signed() {
                self.flags.push(SIGNED.to_owned());
            }
        }
        // A threshold's cases say how many of A's bits turn it on.
        if let Report::Ran(run) = report
            && run.wrong == 0
            && category::behaviour(&s.category).test == Some(Test::Threshold)
        {
            self.specs.at_least = run.at_least();
        }
        if let Some(pulse) = report.timing().and_then(|timing| timing.pulse) {
            self.specs.pulse = Some(pulse.ticks.into());
            self.flags
                .retain(|flag| flag != INVERTED && flag != FALLING_EDGE);
            if pulse.inverted {
                self.flags.push(INVERTED.to_owned());
            }
            if pulse.edge == Edge::Falling {
                self.flags.push(FALLING_EDGE.to_owned());
            }
        }
        let policy = category::behaviour(&s.category).stacking;
        let Report::Ran(run) = report else {
            return;
        };
        // A category whose builds never stack keeps no stacking, as a
        // decoder's, whose copies side by side are separate decoders.
        if policy == category::Stacking::Never {
            self.stacking = Vec::new();
            return;
        }
        if !s.file.conventional || policy == category::Stacking::Given {
            return;
        }
        if let Some(stacking) = &run.stacking {
            self.stacking = stacking.directions();
        }
        // Copies that add up as a wider adder, or store as a wider register,
        // run the way its bits do, which a 1-bit build's lone input cannot
        // show.
        if let Some(
            Stacking::Stacks { step, .. } | Stacking::Wider { step, .. },
        ) = &run.stacking
            && let Some(orientation) = step.orientation()
        {
            self.specs.orientation = orientation;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{models::schematic::Orientation, search::tests::schematic};

    /// A result that ran every case of a test, `wrong` of them wrong, read
    /// as `reading` says, with `timing` if it was measured.
    fn ran(
        reading: &serde_json::Value,
        wrong: u64,
        timing: &serde_json::Value,
    ) -> Report {
        serde_json::from_value(json!({
            "outcome": "ran",
            "pattern": "its cases",
            "reading": reading,
            "cases": 4,
            "exhaustive": true,
            "patterns": 0,
            "by_digit": false,
            "wrong": wrong,
            "inputs": [],
            "outputs": [],
            "wrong_bits": [],
            "examples": [],
            "hint": null,
            "timing": timing,
            "stacking": null,
        }))
        .expect("a result")
    }

    /// A timing whose one data output first changes on tick `first` and
    /// settles by `last`, all its bits together, with `pulse` if any.
    fn timing(
        first: i32,
        last: i32,
        pulse: &serde_json::Value,
    ) -> serde_json::Value {
        json!({
            "inputs": [],
            "outputs": [{
                "name": "output",
                "status": false,
                "latency": last,
                "first_change": first,
                "carried": 0,
                "glitches": 0,
                "staggered": 0,
                "glitch": null,
                "bits": [],
            }],
            "synced": true,
            "median": last,
            "p99": last,
            "slowest": last,
            "slowest_case": [],
            "changes": 1,
            "patterns": false,
            "write": null,
            "data": null,
            "takes": "open",
            "pulse": pulse,
        })
    }

    fn decided(s: &Build, flags: &[&str], report: &Report) -> Derived {
        let mut derived = Derived {
            flags: flags.iter().map(|&flag| flag.to_owned()).collect(),
            ..Derived::of(s)
        };
        derived.decide(s, report);
        derived
    }

    #[test]
    fn a_comparators_cases_say_whether_it_reads_signed() {
        let s = schematic("greater_than", Orientation::Horizontal, &[]);
        let reads =
            |signed: bool| json!({"of": "greater_than", "signed": signed});
        let signed = ran(&reads(true), 0, &json!(null));
        assert_eq!(decided(&s, &[], &signed).flags, [SIGNED]);
        let unsigned = ran(&reads(false), 0, &json!(null));
        assert!(decided(&s, &[SIGNED], &unsigned).flags.is_empty());
        // Wrong cases say nothing of how it reads its operands.
        let failing = ran(&reads(false), 1, &json!(null));
        assert_eq!(decided(&s, &[SIGNED], &failing).flags, [SIGNED]);
    }

    #[test]
    fn a_thresholds_cases_say_how_many_bits_turn_it_on() {
        let s = schematic("threshold", Orientation::Horizontal, &[]);
        let reading = json!({"of": "threshold", "at_least": 3});
        let report = ran(&reading, 0, &json!(null));
        assert_eq!(decided(&s, &[], &report).specs.at_least, Some(3));
    }

    #[test]
    fn a_monostables_timing_gives_its_pulse_and_flags() {
        let s = schematic("monostable", Orientation::Horizontal, &[]);
        let pulse = json!({"ticks": 4, "inverted": true, "edge": "falling"});
        let report = ran(&json!(null), 0, &timing(2, 6, &pulse));
        let derived = decided(&s, &[], &report);
        assert_eq!(derived.specs.pulse, Some(4));
        assert_eq!(derived.flags, [INVERTED, FALLING_EDGE]);
        // A pulse's latency is when the output first changes.
        assert_eq!(derived.specs.latency, Some(2));
        assert_eq!(derived.specs.synchronized, Some(true));
        // One that rests off and pulses as A turns on loses both flags.
        let pulse = json!({"ticks": 4, "inverted": false, "edge": "rising"});
        let report = ran(&json!(null), 0, &timing(2, 6, &pulse));
        let derived = decided(&s, &[INVERTED, FALLING_EDGE], &report);
        assert!(derived.flags.is_empty());
    }

    #[test]
    fn a_result_that_did_not_run_decides_nothing() {
        let s = schematic("greater_than", Orientation::Horizontal, &[]);
        let skipped = Report::Skipped {
            reason: "a piston".to_owned(),
        };
        let derived = decided(&s, &[SIGNED], &skipped);
        assert_eq!(derived.flags, [SIGNED]);
        assert_eq!(derived.specs.latency, s.specs.latency);
        assert_eq!(derived.stacking, s.stacking);
    }
}
