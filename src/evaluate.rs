//! The one way the site reads a build: what it is and how it performs,
//! worked out from its file alone, trusting nothing stored. The test that
//! recognizes it names its category; that test's cases say whether it
//! works; its copies, stacked, say how it stacks; its timing gives its
//! latency, whether its outputs change together, and its throughput; and
//! for an adder, how its carries are made gives its design.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::{
    import::{self, Prepared},
    interface,
    models::{
        schematic::{Encoding, Logic, Orientation, StackDirection, Widened},
        simulation::{FALLING_EDGE, INVERTED, SIGNED},
    },
    schem::Blocks,
    simulate::{
        self, AdderVariant, Design, Edge, Report, Run, Stacking, Test, Timing,
        adder_design,
    },
    stack,
};

/// The widths a stackable build's copies are stacked to, above its own, to
/// compete at.
pub const STANDARD_WIDTHS: [i64; 3] = [8, 16, 32];

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
    let prepared = import::prepare(file, false, true, true, false)?;
    let blocks = &prepared.blocks;
    // An adder whose inputs take hex digits through dust is a hex adder.
    let hex = !interface::dust_inputs(blocks).is_empty();
    let category = simulate::guess(blocks).map(|guess| match guess.category {
        "adder" if hex => "hex_adder",
        category => category,
    });
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
            crate::category::behaviour(c).stacking
                == crate::category::Stacking::Never
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
        if matches!(category, Some("adder" | "hex_adder")) {
            let variant = AdderVariant::of(&run.pattern);
            found.design = adder_design(&found.prepared.blocks, variant)?;
            run.design.clone_from(&found.design);
        }
    }
    found.report = Some(report);
    Ok(found)
}

impl Evaluation {
    /// Takes the stacking its search found: its step, the most copies that
    /// work, and the way its bits run, as copies that add bits run the way
    /// its bits do, which a 1-bit build's lone input cannot show.
    fn stacks(&mut self, stacking: Option<&Stacking>) {
        // Copies side by side, as more decoders, stack each way they do,
        // and show nothing of how its bits run.
        if let Some(Stacking::Separate { ways }) = stacking {
            self.stacking = ways.iter().map(|way| way.direction()).collect();
            return;
        }
        let stepped = match stacking {
            Some(Stacking::Stacks { step, .. }) => Some((step, None, true)),
            Some(Stacking::Wider { step, most, .. }) => {
                Some((step, *most, true))
            }
            // A register file's copies are more cells, not more bits, so
            // they show nothing of how its bits run.
            Some(Stacking::Registers { step, most, .. }) => {
                Some((step, *most, false))
            }
            _ => None,
        };
        if let Some((step, most, bits)) = stepped {
            self.stacking = vec![StackDirection {
                max_copies: most.and_then(|n| i64::try_from(n).ok()),
                ..step.direction()
            }];
            if let Some(orientation) = step.orientation().filter(|_| bits) {
                self.orientation = Some(orientation);
            }
        }
    }

    /// Stacks its copies, along the way its bits run, into each of
    /// `STANDARD_WIDTHS` above its own that a whole number of them make
    /// and its most copies allow, and tests and times each, its throughput
    /// no fewer than `burnout` ticks, as for the build itself. The first
    /// width whose copies get a case wrong, or can't be stacked or
    /// measured, ends them.
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
            let measured = || -> Result<Option<Widened>> {
                let Some(test) = Test::of(category, Some(wider)) else {
                    return Ok(None);
                };
                let (data, _) = stack::stack(
                    &self.prepared.data,
                    &[(along, usize::try_from(copies)?)],
                    stack::Buses::of(category, orientation),
                )?;
                let blocks = Blocks::read(&data)?;
                if !test.run(&blocks).passed() {
                    return Ok(None);
                }
                let timing = test.time(&blocks, &AtomicBool::new(false))?;
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
    /// gate's whether it gives its inverse, as an XNOR, a comparator's whether it reads its
    /// operands signed, an ALU's its operations and plain lines, and a
    /// monostable's pulse and whether it rests on or pulses as A turns off.
    fn flagged(
        &mut self,
        test: Test,
        run: &Run,
        timing: &Timing,
    ) -> Result<()> {
        if let Some(gate) = simulate::Gate::of(test)
            && run.pattern == gate.patterns()[1]
        {
            self.flags.push(gate.flag().to_owned());
        }
        if test == Test::GreaterThan
            && run.pattern == simulate::SIGNED_GREATER_THAN
        {
            self.flags.push(SIGNED.to_owned());
        }
        if test == Test::Alu {
            self.flags = simulate::alu_flags(&self.prepared.blocks)?;
        }
        // A barrel shifter's are the shifts its lines give, and a
        // decoder's whether its outputs are inverted.
        if test == Test::BarrelShifter {
            self.flags = simulate::shifter_flags(&self.prepared.blocks)?;
        }
        if test == Test::Decoder && run.pattern == simulate::DECODER_INVERTED {
            self.flags.push(INVERTED.to_owned());
        }
        // An adder's lines read inverted, as a subtractor's B is, and a
        // register's or buffer's reads.
        if test == Test::Adder {
            let variant = AdderVariant::of(&run.pattern);
            self.flags
                .extend(variant.flags().into_iter().map(str::to_owned));
        }
        if matches!(test, Test::Register | Test::Buffer)
            && run.pattern.ends_with(" inverted")
        {
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
