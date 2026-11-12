//! The one way the site reads a build: what it is and how it performs,
//! worked out from its file alone, trusting nothing stored. The test that
//! recognizes it names its category; that test's cases say whether it
//! works; its copies, stacked, say how it stacks; its timing gives its
//! latency, whether its outputs change together, and its throughput; and
//! for an adder, how its carries are made gives its design.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::{
    category::{self, Logic},
    import::{self, Prepared},
    interface::{self, Encoding, Orientation},
    models::{
        schematic::Widened,
        simulation::{FALLING_EDGE, INVERTED, SIGNED},
    },
    schem::Blocks,
    simulate::{
        self, Design, Edge, Report, Run, Stacking, Test, Timing, adder_design,
    },
    stack::{self, StackDirection},
};

/// The widths a stackable build's copies are stacked to, above its own, to
/// compete at.
pub const STANDARD_WIDTHS: [i64; 4] = [8, 16, 32, 64];

/// How many of a stackable build's stacks are timed, the narrowest; each
/// wider one is worked out from them, see [`extrapolated`].
const MEASURED_STACKS: usize = 2;

/// Everything worked out from a build's file.
pub struct Evaluation {
    /// The file as the site keeps it.
    pub prepared: Prepared,
    /// What its test recognizes it as; `None` when no test fits it.
    pub category: Option<&'static str>,
    pub width: Option<i64>,
    pub orientation: Option<Orientation>,
    pub encoding: Option<Encoding>,
    /// The flags its file shows, such as an adder's `carry_in`.
    pub flags: Vec<String>,
    /// Its test's result, with its timing and how its copies stack when
    /// every case came out right.
    pub report: Option<Report>,
    pub stacking: Vec<StackDirection>,
    /// Its copies stacked into wider builds, as `Evaluation::widen` finds.
    pub widened: Vec<Widened>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    /// The fewest ticks from one input to the next that keep every result
    /// right, no fewer than torch burnout allows.
    pub throughput: Option<i64>,
    /// How an adder makes its carries.
    pub design: Option<Design>,
    /// Whether it holds state: whether what its test leaves on its outputs
    /// differs from a fresh copy given the same inputs.
    pub logic: Option<Logic>,
    /// A monostable's pulse, in ticks.
    pub pulse: Option<i64>,
}

impl Evaluation {
    /// Whether its test ran and every case came out right.
    pub fn passes(&self) -> bool {
        matches!(&self.report, Some(Report::Ran(run)) if run.wrong == 0)
    }
}

/// Reads the build in `file`.
pub fn evaluate(file: &[u8]) -> Result<Evaluation> {
    let prepared = import::prepare_for(
        file,
        import::Purpose::InPlace {
            standardize: false,
            conventional: true,
            keep_paste: true,
        },
    )?;
    let blocks = &prepared.blocks;
    // An adder whose inputs take hex digits, comparators with air behind,
    // is guessed a hex adder.
    let category = simulate::guess(blocks).map(|guess| guess.category);
    let interface = interface::read(blocks);
    let read = category
        .map(|c| import::derived(c, &interface))
        .unwrap_or_default();
    let mut found = Evaluation {
        category,
        width: read.width,
        orientation: read.orientation,
        encoding: read.encoding,
        flags: category
            .and_then(|c| import::derived_flags(c, &interface))
            .unwrap_or_default(),
        report: None,
        stacking: Vec::new(),
        widened: Vec::new(),
        latency: None,
        synchronized: None,
        throughput: None,
        design: None,
        logic: None,
        pulse: None,
        prepared,
    };
    let Some(test) = category.and_then(|c| Test::of(c, found.width)) else {
        return Ok(found);
    };
    let mut report = test.run(&found.prepared.blocks);
    if let Report::Ran(run) = &mut report
        && run.wrong == 0
    {
        let never = category.is_some_and(|c| {
            category::behaviour(c).stacking == category::Stacking::Never
        });
        run.stacking = if never {
            None
        } else {
            test.stacking(&found.prepared.data)
        };
        found.stacks(run.stacking.as_ref());
        let timing =
            test.time(&found.prepared.blocks, &AtomicBool::new(false))?;
        found.latency = timing.latency().map(|t| t.max(0).into());
        found.synchronized = Some(timing.synced);
        found.flagged(test, run, &timing)?;
        // A torch burns out switched too often, which the simulator does
        // not model: the game takes a new input no sooner than every third
        // tick through torches, or every second without.
        let burnout = if found.prepared.facts.torches > 0 {
            3
        } else {
            2
        };
        found.throughput = test
            .throughput(&found.prepared.blocks, &found.prepared.data, &timing)?
            .map(|ticks| i64::from(ticks).max(burnout));
        run.timing = Some(timing);
        if let Some(category) = category {
            found.widen(category, burnout);
        }
        found.logic = test.remembers(&found.prepared.blocks)?.map(|held| {
            if held {
                Logic::Stateful
            } else {
                Logic::Stateless
            }
        });
        if test == Test::Adder {
            let variant = run.adder_variant();
            found.design = adder_design(&found.prepared.blocks, variant)?;
            run.design.clone_from(&found.design);
        }
    }
    found.report = Some(report);
    Ok(found)
}

impl Evaluation {
    /// Takes the stacking its search found: each way, its step, the most
    /// copies that work, and its pitch class, and the way its bits run, as
    /// copies that add bits run the way its bits do, which a 1-bit build's
    /// lone input cannot show. A register file's copies, more words, show
    /// nothing of how its bits run.
    fn stacks(&mut self, stacking: Option<&Stacking>) {
        let Some(stacking) = stacking else {
            return;
        };
        self.stacking = stacking.directions();
        if let Stacking::Stacks { step, .. } | Stacking::Wider { step, .. } =
            stacking
            && let Some(orientation) = step.orientation()
        {
            self.orientation = Some(orientation);
        }
    }

    /// Stacks its copies, along the way its bits run, into each of
    /// `STANDARD_WIDTHS` above its own that a whole number of them make
    /// and its most copies allow. The narrowest [`MEASURED_STACKS`] are
    /// timed by the test patterns, their throughput no fewer than
    /// `burnout` ticks, as for the build itself; each wider one is worked
    /// out from the last two, as each copy more adds what a copy added
    /// between them, see [`extrapolated`]. Their cases aren't run again:
    /// copies its stacking found to work, in a stack longer than dust
    /// carries a signal, are taken to stack any number of times. The first
    /// width whose copies can't be stacked or measured ends them.
    fn widen(&mut self, category: &str, burnout: i64) {
        let (Some(width), Some(orientation)) = (self.width, self.orientation)
        else {
            return;
        };
        let Some(&along) =
            self.stacking.iter().find(|d| d.adds_bits(orientation))
        else {
            return;
        };
        for wider in STANDARD_WIDTHS {
            if wider <= width || wider % width != 0 {
                continue;
            }
            let copies = wider / width;
            if along.max_copies.is_some_and(|most| copies > most) {
                break;
            }
            if self.widened.len() >= MEASURED_STACKS
                && let [.., before, last] = self.widened.as_slice()
            {
                let widened =
                    extrapolated(before, last, wider, copies, burnout);
                self.widened.push(widened);
                continue;
            }
            let measured = || -> Result<Option<Widened>> {
                let Some(test) = Test::of(category, Some(wider)) else {
                    return Ok(None);
                };
                let (data, _) = stack::stack(
                    &self.prepared.data,
                    &[(along, usize::try_from(copies)?)],
                    stack::Buses::of(
                        category::behaviour(category).copies,
                        orientation,
                    ),
                )?;
                let blocks = Blocks::read(&data)?;
                let timing =
                    test.time_by_patterns(&blocks, &AtomicBool::new(false))?;
                let throughput = test
                    .throughput(&blocks, &data, &timing)?
                    .map(|ticks| i64::from(ticks).max(burnout));
                Ok(Some(Widened {
                    width: wider,
                    copies,
                    latency: timing.latency().map(|t| t.max(0).into()),
                    synchronized: Some(timing.synced),
                    throughput,
                }))
            };
            match measured() {
                Ok(Some(widened)) => self.widened.push(widened),
                _ => break,
            }
        }
    }

    /// Takes the flags its `test`'s cases, `run`, and its `timing` show: a
    /// comparator's whether it reads its operands signed, an ALU's its
    /// operations and plain lines, a parity circuit's whether it is on for
    /// an even number, and a monostable's pulse and whether it rests on or
    /// pulses as A turns off.
    fn flagged(
        &mut self,
        test: Test,
        run: &Run,
        timing: &Timing,
    ) -> Result<()> {
        if test == Test::GreaterThan && run.signed() {
            self.flags.push(SIGNED.to_owned());
        }
        if test == Test::Alu {
            self.flags = simulate::alu_flags(&self.prepared.blocks)?;
        }
        // A barrel shifter's are the shifts its lines give.
        if test == Test::BarrelShifter {
            self.flags = simulate::shifter_flags(&self.prepared.blocks)?;
        }
        // An adder's carry in may read inverted, and its B, as a
        // subtractor's does, and a register's reads may show its cells
        // inverted.
        if test == Test::Adder {
            let variant = run.adder_variant();
            self.flags
                .extend(variant.flags().into_iter().map(str::to_owned));
        }
        if test == Test::Register && run.reads_inverted() {
            self.flags.push(INVERTED.to_owned());
        }
        if let Some(pulse) = timing.pulse {
            self.pulse = Some(pulse.ticks.into());
            if pulse.inverted {
                self.flags.push(INVERTED.to_owned());
            }
            if pulse.edge == Edge::Falling {
                self.flags.push(FALLING_EDGE.to_owned());
            }
        }
        Ok(())
    }
}

/// Copies stacked `copies` times into `width` bits, worked out from two
/// narrower stacks, `before` and `last`, rather than timed: each copy more
/// adds the latency and throughput a copy added between them, rounded to
/// the nearest tick, the throughput no fewer than `burnout`, and whether it
/// is synced is the last's. A stack's latency and throughput grow by a
/// fixed number of ticks a copy: of the live builds stacked to three widths
/// or more, the narrowest two gave every wider width's latency, and all
/// but one's throughput, a 4-bit hex adder's 2-copy stack being a tick and
/// a half faster than its copies after.
fn extrapolated(
    before: &Widened,
    last: &Widened,
    width: i64,
    copies: i64,
    burnout: i64,
) -> Widened {
    let added = last.copies - before.copies;
    let grown = |was: Option<i64>, is: Option<i64>| {
        let (was, is) = (was?, is?);
        let more = (copies - last.copies) * (is - was);
        // To the nearest tick, a half rounded up.
        Some(is + (2 * more + added).div_euclid(2 * added))
    };
    Widened {
        width,
        copies,
        latency: grown(before.latency, last.latency).map(|t| t.max(0)),
        synchronized: last.synchronized,
        throughput: grown(before.throughput, last.throughput)
            .map(|t| t.max(burnout)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(
        width: i64,
        copies: i64,
        latency: i64,
        throughput: i64,
    ) -> Widened {
        Widened {
            width,
            copies,
            latency: Some(latency),
            synchronized: Some(false),
            throughput: Some(throughput),
        }
    }

    /// Wider stacks are worked out from the narrowest two as the live
    /// builds measured them: a 1-bit ripple-carry adder's, 223d6d81, and an
    /// 8-bit carry-cancel adder's, 4961b144, exactly; a 4-bit hex adder's,
    /// 7ef9818c, its latency exactly and its throughput two ticks short at
    /// 32 bits, its 2-copy stack a tick and a half faster than the copies
    /// after.
    #[test]
    fn wider_stacks_are_worked_out_from_the_narrowest_two() {
        let worked = |before: Widened, last: Widened, width, copies| {
            extrapolated(&before, &last, width, copies, 2)
        };
        let rca = (stack(8, 8, 20, 18), stack(16, 16, 36, 34));
        assert_eq!(worked(rca.0, rca.1, 32, 32), stack(32, 32, 68, 66));
        assert_eq!(worked(rca.0, rca.1, 64, 64), stack(64, 64, 132, 130));
        let cca = (stack(16, 2, 4, 3), stack(32, 4, 6, 5));
        assert_eq!(worked(cca.0, cca.1, 64, 8), stack(64, 8, 10, 9));
        let hex = (stack(8, 2, 10, 7), stack(16, 4, 14, 10));
        assert_eq!(worked(hex.0, hex.1, 32, 8), stack(32, 8, 22, 16));
    }

    /// What a copy adds may be in halves, rounded to the nearest tick, a
    /// half up; a throughput never falls below `burnout`.
    #[test]
    fn a_worked_out_stack_rounds_to_the_nearest_tick() {
        let (before, last) = (stack(8, 2, 4, 3), stack(12, 3, 5, 3));
        let wider = extrapolated(&before, &last, 16, 4, 2);
        assert_eq!((wider.latency, wider.throughput), (Some(6), Some(3)));
        let (before, last) = (stack(8, 2, 9, 9), stack(16, 4, 6, 3));
        let wider = extrapolated(&before, &last, 64, 16, 2);
        assert_eq!((wider.latency, wider.throughput), (Some(0), Some(2)));
    }
}
