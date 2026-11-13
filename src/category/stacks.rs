//! How copies of a category's builds are stacked and tested: an adder's
//! chain by their carries, as an incrementer's and a carry out's do, and a
//! gate's each work alone. The stacking itself is the simulator's; see
//! [`crate::simulate::stacking`].

use super::{
    Kind, Test,
    adder::{self, ADD, adder_ports},
};
use crate::{
    interface::{self, Interface},
    schem::Blocks,
    simulate::{
        MAX_STACKED, Report, Stacker, Step, Value,
        bench::{Case, mask, sweep},
        circuit::{Circuit, input_bits},
        stacking::{bit_ways, run_stacked},
    },
    stack,
};

/// How copies of the adder in `data`, a file whose own cases came out
/// right, chain into a wider adder, each taking the carry from the one
/// before; `None` when it is no adder.
pub fn adder(data: &[u8]) -> Option<Stacker<'_>> {
    let blocks = Blocks::read(data).ok()?;
    let mut interface = interface::read(&blocks);
    let ports = adder_ports(&mut interface, &ADD).ok()?;
    let width = input_bits(&interface.inputs[ports.a]);
    // How the copy reads, as a subtractor's, B inverted, so its stack is
    // read alike.
    let variant = Test::Adder
        .check(&blocks, true)
        .map_or_else(|_| adder::Variant::default(), |run| run.adder_variant());
    let carry_in = ports.carry_in.is_some();
    let works = move |step: Step, copies: usize, _| {
        if !carry_in {
            return Err("It has no carry in, so a copy cannot take the carry \
                        from the one before."
                .to_owned());
        }
        if width * copies > MAX_STACKED {
            return Err(format!(
                "{copies} copies would be wider than the simulator tests."
            ));
        }
        stacked(data, step, copies, width, variant)
    };
    let not = "No pitch west or up makes copies that add right.";
    let ways = (width, bit_ways(&interface, width));
    // A hex adder's copies add digits, its pitch the blocks a digit.
    let place = if interface.inputs[ports.a].signal_strength {
        4
    } else {
        1
    };
    Some(
        Stacker::new(data, Kind::Adder.copying(), ways, works, not)
            .placed(place),
    )
}

/// Stacks `copies` copies every `step` and tests them, cheapest first; the
/// number of cases the stack ran, or why it fails.
fn stacked(
    data: &[u8],
    step: Step,
    copies: usize,
    width: usize,
    variant: adder::Variant,
) -> Result<u64, String> {
    let (stacked, _) =
        stack::stack(data, &[(step.direction(), copies)], stack::Buses::own())
            .map_err(|err| {
                format!("Its copies do not fit together: {err:#}.")
            })?;
    let blocks = Blocks::read(&stacked).map_err(|err| err.to_string())?;
    let mut interface = interface::read(&blocks);
    let carries_in = interface
        .inputs
        .iter()
        .filter(|port| port.control())
        .count();
    if carries_in > 1 {
        return Err("The carry does not pass between copies: each keeps its \
                    own carry in, which lands past the copy before."
            .to_owned());
    }
    let bench = adder::adder(&mut interface)
        .map_err(|err| format!("{copies} copies do not make one adder: {err}"))?
        .bench;
    // Its seams read as the copy does, as a subtractor's, B inverted.
    let bits = copies * width;
    let widths = bench.widths(&interface);
    if widths[..2] != [bits, bits] {
        return Err(format!(
            "{copies} copies make A and B of {} and {} bits, not {bits}.",
            widths[0], widths[1]
        ));
    }
    // Loading it settles it, so one that never settles stops here.
    Circuit::new(&blocks, &interface).map_err(|err| err.to_string())?;
    let seams = seam_cases(bits, width, widths.len() == 3);
    let sum = |case: &Case| variant.apply(case, bits);
    let seam = sweep(&blocks, &interface, &bench, &seams, None, sum)
        .map_err(|err| err.to_string())?;
    if let Some(wrong) = seam.examples.first() {
        return Err(format!(
            "The carry does not pass between copies: A {} + B {} + carry \
             in {} gives {}, not {}.",
            wrong.inputs[0],
            wrong.inputs[1],
            wrong.inputs.get(2).copied().unwrap_or(0),
            joined(&wrong.got, bits),
            joined(&wrong.expected, bits),
        ));
    }
    let run = Test::Adder
        .check(&blocks, false)
        .map_err(|err| err.to_string())?;
    if run.wrong > 0 {
        return Err(format!(
            "{copies} copies get {} of {} sums wrong.",
            run.wrong, run.cases
        ));
    }
    Ok(run.cases)
}

/// A stack's result from its outputs' values, the sum below its carry out.
fn joined(values: &[Value], bits: usize) -> Value {
    values[0] | values.get(1).copied().unwrap_or(0) << bits
}

/// Sums whose carry crosses each seam between copies, `width` bits apart:
/// all ones below it plus one, and its lowest bits each doubled, with
/// carry in each way; and none and all ones plus one across the whole.
fn seam_cases(bits: usize, width: usize, carry_in: bool) -> Vec<Case> {
    let carries: &[Value] = if carry_in { &[0, 1] } else { &[0] };
    let mut cases = vec![[0, 0, 0], [mask(bits), 1, 0]];
    for seam in (width..bits).step_by(width) {
        let below = 1 << (seam - 1);
        for &carry in carries {
            cases.push([mask(seam), 1 - carry, carry]);
            cases.push([below, below, carry]);
        }
    }
    cases
}

/// How copies of the incrementer, decrementer, or carry out in `data`
/// chain into a wider one under `test`: each copy's carry out, landing on
/// the next's control line, takes its place, as an adder's carry does. One
/// with no carry out keeps every copy's own line, so none does.
pub(super) fn chained(data: &[u8], test: Test) -> Option<Stacker<'_>> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    let width = interface.inputs.first().map_or(0, |a| a.bits.len());
    let works = move |step: Step, copies: usize, buses| {
        run_stacked(data, (step, copies), buses, |blocks| {
            // Copies that overlap may read as a bus wider than any test, so
            // only a stack of the bits its copies add is run.
            let stacked = interface::read(blocks);
            let wide = |port: &interface::Port| port.bits.len() > MAX_STACKED;
            let a = stacked.inputs.first().map_or(0, |a| a.bits.len());
            if a != width * copies || stacked.inputs.iter().any(wide) {
                return Err(format!(
                    "{copies} copies make A of {a} bits, not {}.",
                    width * copies
                ));
            }
            match test.run(blocks) {
                Report::Ran(run) if run.wrong == 0 => Ok(run.cases),
                Report::Ran(run) => Err(format!(
                    "{copies} copies get {} of {} cases wrong.",
                    run.wrong, run.cases
                )),
                Report::Skipped { reason } | Report::Broken { reason } => {
                    Err(reason)
                }
            }
        })
    };
    let not = "No pitch west or up makes copies whose carries chain into a \
               wider one.";
    let ways = (width, bit_ways(&interface, width));
    Some(Stacker::new(data, test.kind().copying(), ways, works, not))
}

/// How copies of the build in `data`, a gate or another whose bits each
/// work alone, stack into more bits: a stack works when every case of
/// `test`, the wider build's, comes out right and its A, the input `a`
/// finds, has every copy's bits, no more than [`MAX_STACKED`] of them.
/// Copies of an inverter share its control line, as its category's group
/// says. `None` when `a` finds no A in the build itself; `not` says why
/// none stacks.
pub(super) fn bitwise<'a>(
    data: &'a [u8],
    test: Test,
    a: impl Fn(&Interface) -> Option<usize> + Copy + 'a,
    not: &str,
) -> Option<Stacker<'a>> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    let width = interface.inputs[a(&interface)?].bits.len();
    let works = move |step: Step, copies: usize, buses| {
        run_stacked(data, (step, copies), buses, |blocks| {
            if width * copies > MAX_STACKED {
                return Err(format!(
                    "{copies} copies would be wider than the simulator tests."
                ));
            }
            let interface = interface::read(blocks);
            let a = a(&interface)
                .ok_or_else(|| format!("{copies} copies have no input A."))?;
            match test.run(blocks) {
                Report::Ran(run)
                    if run.wrong == 0
                        && interface.inputs[a].bits.len() == width * copies =>
                {
                    Ok(run.cases)
                }
                Report::Ran(run) if run.wrong > 0 => Err(format!(
                    "{copies} copies get {} of {} cases wrong.",
                    run.wrong, run.cases
                )),
                Report::Ran(_) => Err(format!(
                    "{copies} copies do not make A of {} bits.",
                    width * copies
                )),
                Report::Skipped { reason } | Report::Broken { reason } => {
                    Err(reason)
                }
            }
        })
    };
    let ways = (width, bit_ways(&interface, width));
    Some(Stacker::new(data, test.kind().copying(), ways, works, not))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ingest::{
            prepare::{self, Purpose},
            publish,
        },
        simulate::stacking::Way,
        stack::PITCHES,
    };

    /// The 1-wide inverted monostable, as an upload standardizes it.
    fn one_wide() -> Vec<u8> {
        let raw = std::fs::read("tests/fixtures/monostable-3-inverted.schem")
            .unwrap();
        let raw = crate::recognize::legacy::with_colour_roles(&raw).unwrap();
        prepare::prepare_for(&raw, Purpose::Standard { keep_paste: false })
            .unwrap()
            .data
    }

    /// Copies of a build 1 wide, 2 apart, a block of air between each,
    /// are tested as its group asks: as cells, they cannot meet, so they
    /// are taken to stack with no stack built or run; chained, as an
    /// adder's, or sharing lines, as an inverter's, they must reach across
    /// the air, so they are tested as any copies are. Farther apart, cells
    /// still stack untested, and a chain's copies are separate builds.
    #[test]
    fn only_cells_with_air_between_stack_untested() {
        let data = one_wide();
        let tested = std::cell::Cell::new(0);
        let stacker = |kind| {
            let works = |_: Step, _: usize, _| {
                tested.set(tested.get() + 1);
                Err("tested".to_owned())
            };
            Stacker::new(
                &data,
                Kind::copying(kind),
                (1, vec![Way::West]),
                works,
                "none",
            )
        };
        let two = Step::new(2, 0);
        let found = stacker(Kind::Monostable).at(two, 9);
        assert_eq!(found.map(|(step, cases)| (step.west, cases)), Ok((2, 0)));
        assert_eq!(tested.get(), 0);
        for kind in [Kind::Adder, Kind::Inverter] {
            let found = stacker(kind).at(two, 9);
            assert_eq!(found, Err("tested".to_owned()), "{kind:?}");
        }
        assert!(tested.get() > 0);
        // Copies 1 apart touch, so cells are tested as ever.
        tested.set(0);
        let one = stacker(Kind::Monostable).at(Step::new(1, 0), 9);
        assert!(one.is_err() && tested.get() > 0, "{one:?}");
        // Cells farther apart stack untested; a chain's are no stack.
        tested.set(0);
        let four = Step::new(4, 0);
        let found = stacker(Kind::Monostable).at(four, 9);
        assert_eq!(found.map(|(_, cases)| cases), Ok(0));
        let chain = stacker(Kind::Adder).at(four, 9);
        assert!(chain.is_err_and(|why| why.contains("air")));
        assert_eq!(tested.get(), 0);
    }

    /// Only the adder test's builds' copies are tried end to end, from the
    /// narrowest the box rule's box cuts the core to: the 1-bit ripple
    /// adder's core is 6 wide, but copies 5 west repeat its east layer
    /// over the copy before's west one, so it is tried 5 to 9 west, its
    /// whole width, after the classes; a gate the same shape only at the
    /// classes.
    #[test]
    fn only_adders_are_tried_end_to_end() {
        let raw = std::fs::read("tests/fixtures/rca-1-5-wide.schem").unwrap();
        let data = publish::with_roles(&raw, "adder").unwrap();
        let works = |_: Step, _: usize, _| Err("untried".to_owned());
        let steps = |kind| {
            Stacker::new(
                &data,
                Kind::copying(kind),
                (1, vec![Way::West]),
                works,
                "none",
            )
            .steps(Way::West)
        };
        for kind in [Kind::Adder, Kind::Subtractor, Kind::HexAdder] {
            assert_eq!(steps(kind), [1, 2, 4, 5, 6, 7, 8, 9], "{kind:?}");
        }
        for kind in [Kind::XorGate, Kind::Incrementer, Kind::Monostable] {
            assert_eq!(steps(kind), PITCHES, "{kind:?}");
        }
    }
}
