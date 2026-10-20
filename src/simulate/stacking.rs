//! Whether copies of an adder stack into a wider adder, as stacking would
//! join them: the step from one copy to the next comes from how its bits
//! are spaced. Enough copies are stacked and tested that one sits between
//! two others, taking a carry and passing one on, and that the stack is
//! longer than dust carries a signal, 15 blocks, plus a copy, so a path
//! that runs through copies without a repeater or torch to renew it fails.
//! An adder whose copies add right then is taken to stack any number of
//! times.
//!
//! The checks run cheapest first and stop at the first that fails: a carry
//! in to take the carry from the copy before, the spacing of the bits, then
//! whether the copies, stacked as `//rstack` stacks them, fit together and
//! read as one adder, then whether the
//! stack settles, then a few sums whose carry crosses from one copy to the
//! next, and only then the stack's full test.

use serde::{Deserialize, Serialize};

use super::{
    MAX_WIDTH, Report, Test,
    adder::{self, ADD, AdderPorts, adder_ports},
    bench::{Case, mask, sweep},
    circuit::{Circuit, input_bits},
};
use crate::{
    interface::{self, Interface},
    lint,
    models::schematic::{Orientation, StackDirection},
    schem::{Blocks, Pos},
    stack,
};

/// The farthest dust carries a signal, which a stack tested must reach
/// past.
const DUST_REACH: i64 = 15;

/// What stacking copies of an adder or a register found.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Stacking {
    /// Copies every `step` make an adder of `bits` bits whose every case
    /// came out right.
    Stacks {
        step: Step,
        copies: usize,
        bits: usize,
        cases: u64,
    },
    /// Copies of a register every `step` make a register file whose every
    /// cell stores and reads right: `copies` of them, `registers` cells in
    /// all, in `cases` cases. `most` is how many copies work when one more
    /// does not, as a shared bus that runs too far fails; `None` when
    /// every number tried worked.
    Registers {
        step: Step,
        copies: usize,
        registers: usize,
        cases: u64,
        most: Option<usize>,
    },
    /// Copies of a register or buffer every `step` make a wider one, of
    /// `bits` bits, whose every cell stores and reads right, sharing its
    /// control lines: `copies` of them, in `cases` cases. `most` is as for
    /// `Registers`.
    Wider {
        step: Step,
        copies: usize,
        bits: usize,
        cases: u64,
        most: Option<usize>,
    },
    /// Copies every `step` side by side each do their own work, as more
    /// decoders, each with its own input and outputs: `copies` of them, in
    /// `cases` cases. `most` is as for `Registers`.
    Separate {
        step: Step,
        copies: usize,
        cases: u64,
        most: Option<usize>,
    },
    /// They make no working build of its kind, as an adder whose carries do
    /// not chain or a register with no step west that works, with the step
    /// tried when there was one.
    Not { step: Option<Step>, reason: String },
}

/// How far each copy sits from the one before.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub west: i64,
    pub up: i64,
    pub north: i64,
}

impl Step {
    pub fn direction(self) -> StackDirection {
        StackDirection {
            west: self.west,
            up: self.up,
            north: self.north,
            max_copies: None,
        }
    }

    /// The layout copies stacked this way give, as the bits of an adder
    /// whose copies add up run the way it stacks; see
    /// `StackDirection::orientation`.
    pub fn orientation(self) -> Option<Orientation> {
        self.direction().orientation()
    }

    /// Such as `16 up` or `2 west and 2 up`.
    pub fn describe(self) -> String {
        [(self.west, "west"), (self.up, "up"), (self.north, "north")]
            .into_iter()
            .filter(|&(n, _)| n > 0)
            .map(|(n, way)| format!("{n} {way}"))
            .collect::<Vec<_>>()
            .join(" and ")
    }
}

/// Stacks copies of the adder in `data`, a file whose own cases came out
/// right, and tests them; `None` when two copies would be wider than the
/// simulator tests.
pub fn check(data: &[u8]) -> Option<Stacking> {
    let not = |step, reason: String| Stacking::Not { step, reason };
    let blocks = match Blocks::read(data) {
        Ok(blocks) => blocks,
        Err(err) => return Some(not(None, err.to_string())),
    };
    let mut interface = interface::read(&blocks);
    let ports = match adder_ports(&mut interface, &ADD) {
        Ok(ports) => ports,
        Err(err) => return Some(not(None, err.to_string())),
    };
    if ports.carry_in.is_none() {
        let reason = "It has no carry in, so a copy cannot take the carry \
                      from the one before.";
        return Some(not(None, reason.to_owned()));
    }
    let width = input_bits(&interface.inputs[ports.a]);
    // How the copy reads, as a subtractor's, B inverted, so its stack is
    // read alike.
    let variant = Test::Adder.check(&blocks, true).map_or_else(
        |_| adder::Variant::default(),
        |run| adder::Variant::of(&run.pattern),
    );
    // A 1-bit adder's carry out need not be where the next copy's carry in
    // goes, as when the copy above covers it, or it may have none, its carry
    // leaving by dust that carry in lands on; short steps are tried with
    // the one from its carry in to its carry out.
    if width == 1 {
        return Some(search(
            data,
            &blocks,
            step(&interface, &ports).ok(),
            variant,
        ));
    }
    let step = match step(&interface, &ports) {
        Ok(step) => step,
        Err(reason) => return Some(not(None, reason)),
    };
    let copies = copies(step, width)?;
    Some(match stacked(data, step, copies, width, variant) {
        Ok(cases) => Stacking::Stacks {
            step,
            copies,
            bits: copies * width,
            cases,
        },
        Err(reason) => not(Some(step), reason),
    })
}

/// The shortest step whose copies of 1-bit adder `data` add right, of
/// `first`, if given, which wins a tie, and the steps up, west, or north as
/// far as the build reaches. The shortest that works packs the bits
/// closest, and each wrong one mostly fails a cheap check, so this is
/// quick.
fn search(
    data: &[u8],
    blocks: &Blocks,
    first: Option<Step>,
    variant: adder::Variant,
) -> Stacking {
    let (low, high) = interface::core(blocks);
    let reach = |axis: usize| high[axis] - low[axis] + 1;
    let longest = (0..3).map(reach).max().unwrap_or(1);
    let short = (1..=i64::from(longest)).flat_map(|n| {
        [
            (
                1,
                Step {
                    west: 0,
                    up: n,
                    north: 0,
                },
            ),
            (
                0,
                Step {
                    west: n,
                    up: 0,
                    north: 0,
                },
            ),
            (
                2,
                Step {
                    west: 0,
                    up: 0,
                    north: n,
                },
            ),
        ]
        .into_iter()
        .filter(move |&(axis, _)| n <= i64::from(reach(axis)))
        .map(|(_, step)| step)
    });
    let mut steps: Vec<Step> = first.into_iter().chain(short).collect();
    steps.sort_by_key(|step| step.west + step.up + step.north);
    steps.dedup();
    for step in steps {
        let Some(copies) = copies(step, 1) else {
            continue;
        };
        if let Ok(cases) = stacked(data, step, copies, 1, variant) {
            return Stacking::Stacks {
                step,
                copies,
                bits: copies,
                cases,
            };
        }
    }
    Stacking::Not {
        step: None,
        reason: "No step up, west, or north as far as it reaches makes \
                 copies that add right."
            .to_owned(),
    }
}

/// How many copies to test: three, so one sits between two others, and
/// enough that the stack reaches past what dust carries by a copy; but no
/// wider than the simulator tests, and `None` when two would be.
fn copies(step: Step, width: usize) -> Option<usize> {
    let length = u64::try_from(step.west + step.up + step.north).ok()?;
    let reach = u64::try_from(DUST_REACH + 1).ok()?;
    let reaching = usize::try_from(reach.div_ceil(length) + 1).ok()?;
    let fitting = usize::try_from(MAX_WIDTH).ok()? / width;
    let copies = reaching.max(3).min(fitting);
    (copies >= 2).then_some(copies)
}

/// The step that puts the next copy's bits where the bus would go on: the
/// bus's spacing, which may repeat in a pattern, as a staggered bus's
/// does, carried on past its last bit. A 1-bit adder steps from its carry
/// in to its carry out, which the next copy's carry in continues.
fn step(interface: &Interface, ports: &AdderPorts) -> Result<Step, String> {
    let bits = |port: &interface::Port| -> Vec<Pos> {
        port.bits.iter().map(|bit| bit.pos).collect()
    };
    let a = bits(&interface.inputs[ports.a]);
    let offset = if a.len() >= 2 {
        let along = |name: &str, positions: &[Pos]| {
            next_after(positions).ok_or_else(|| {
                format!("{name}'s bits are not spaced in a repeating pattern")
            })
        };
        let offset = along("A", &a)?;
        let b = bits(&interface.inputs[ports.b]);
        let output = bits(&interface.outputs[ports.sum]);
        let output = &output[..output.len().min(a.len())];
        if along("B", &b)? != offset || along("The output", output)? != offset {
            return Err("A, B, and the output are not spaced alike".to_owned());
        }
        offset
    } else {
        let line = |port: Option<usize>, ports: &[interface::Port]| {
            Some(ports[port?].bits.first()?.pos)
        };
        let (Some(carry_in), Some(carry_out)) = (
            line(ports.carry_in, &interface.inputs),
            line(ports.carry_out, &interface.outputs),
        ) else {
            return Err(
                "A 1-bit adder needs a carry in and a carry out to chain."
                    .to_owned(),
            );
        };
        [0, 1, 2].map(|axis| carry_out[axis] - carry_in[axis])
    };
    let step = Step {
        west: (-offset[0]).into(),
        up: offset[1].into(),
        north: (-offset[2]).into(),
    };
    if step.west < 0 || step.up < 0 || step.north < 0 {
        return Err(
            "Its bits count east, down, or south, but copies go west, up, \
             or north."
                .to_owned(),
        );
    }
    if step
        == (Step {
            west: 0,
            up: 0,
            north: 0,
        })
    {
        return Err("Its copies would sit on each other.".to_owned());
    }
    Ok(step)
}

/// How far the bus at `positions` reaches past its last bit to where the
/// next copy's first bit goes: its spacing, the shortest pattern of gaps
/// that repeats along it, carried on. `None` for gaps that do not repeat.
fn next_after(positions: &[Pos]) -> Option<Pos> {
    let gaps: Vec<Pos> = positions
        .windows(2)
        .map(|pair| [0, 1, 2].map(|axis| pair[1][axis] - pair[0][axis]))
        .collect();
    let period = (1..=gaps.len().max(1)).find(|&period| {
        gaps.iter()
            .enumerate()
            .skip(period)
            .all(|(i, gap)| *gap == gaps[i - period])
    })?;
    // A pattern longer than half the bus repeats too little to trust.
    if period > positions.len() / 2 {
        return None;
    }
    let (first, last) = (positions.first()?, positions.last()?);
    let next = gaps[(positions.len() - 1) % period];
    Some([0, 1, 2].map(|axis| last[axis] - first[axis] + next[axis]))
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
    let control = format!("{} concrete", lint::CONTROL);
    let carries_in = interface
        .inputs
        .iter()
        .filter(|port| port.block == control)
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
    let sum_bits = interface.outputs[bench.outputs[0]].bits.len();
    let carry_out = bench.outputs.len() > 1;
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
    let sum = |case: &Case| variant.apply(sum_bits, carry_out, case);
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
fn joined(values: &[u64], bits: usize) -> u64 {
    values[0] | values.get(1).copied().unwrap_or(0) << bits
}

/// Sums whose carry crosses each seam between copies, `width` bits apart:
/// all ones below it plus one, and its lowest bits each doubled, with
/// carry in each way; and none and all ones plus one across the whole.
fn seam_cases(bits: usize, width: usize, carry_in: bool) -> Vec<Case> {
    let carries: &[u64] = if carry_in { &[0, 1] } else { &[0] };
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

/// The shortest step up, west, or north, as far as the build reaches,
/// whose copies of the incrementer, decrementer, or carry out in `data`
/// make a wider one under `test`: each copy's carry out, landing on the
/// next's control line, takes its place, as an adder's carry does. One with no carry out
/// keeps every copy's own line, so none does. Copies are as many as
/// `copies` gives, so the stack reaches past what dust carries.
pub(super) fn chained(data: &[u8], test: Test) -> Stacking {
    let not = |reason: &str| Stacking::Not {
        step: None,
        reason: reason.to_owned(),
    };
    let Ok(blocks) = Blocks::read(data) else {
        return not("Its file could not be read.");
    };
    let width = interface::read(&blocks)
        .inputs
        .first()
        .map_or(0, |a| a.bits.len());
    let (low, high) = interface::core(&blocks);
    let reach = |axis: usize| i64::from(high[axis] - low[axis] + 1) + 2;
    let mut steps: Vec<Step> = (1..=reach(1))
        .map(|up| Step {
            west: 0,
            up,
            north: 0,
        })
        .chain((1..=reach(0)).map(|west| Step {
            west,
            up: 0,
            north: 0,
        }))
        .chain((1..=reach(2)).map(|north| Step {
            west: 0,
            up: 0,
            north,
        }))
        .collect();
    steps.sort_by_key(|step| step.west + step.up + step.north);
    let works = |step: Step, copies: usize| {
        let (stacked, _) = stack::stack(
            data,
            &[(step.direction(), copies)],
            stack::Buses::own(),
        )
        .ok()?;
        let blocks = Blocks::read(&stacked).ok()?;
        // Copies that overlap may read as a bus wider than any test, so
        // only a stack of the bits its copies add is run.
        let stacked = interface::read(&blocks);
        let wide = |port: &interface::Port| {
            port.bits.len() > usize::try_from(MAX_WIDTH).unwrap_or(0)
        };
        if stacked.inputs.first()?.bits.len() != width * copies
            || stacked.inputs.iter().any(wide)
        {
            return None;
        }
        let Report::Ran(run) = test.run(&blocks) else {
            return None;
        };
        (run.wrong == 0).then_some(run.cases)
    };
    for step in steps {
        let Some(copies) = copies(step, width) else {
            continue;
        };
        if let Some(cases) = works(step, copies) {
            return Stacking::Stacks {
                step,
                copies,
                bits: width * copies,
                cases,
            };
        }
    }
    not(
        "No step up, west, or north as far as it reaches makes copies \
         whose carries chain into a wider one.",
    )
}

/// The most copies stacked into more bits; a build whose copies work with
/// this many is taken to stack any number of times.
const MOST_COPIES: usize = 16;

/// Whether copies of the build in `data`, whose own cases came out right,
/// stack west into more bits, each copy one more bit as `buses` stacks
/// them: the shortest step west, up to `reach` blocks, at which two copies
/// work, then the most copies that do, up to `MOST_COPIES`. `works` tests
/// a stack of so many copies, giving the cases it ran when every one came
/// out right; `not` says why none stacks.
pub(super) fn west_into_bits(
    data: &[u8],
    reach: i64,
    bits: usize,
    buses: stack::Buses,
    works: impl Fn(&Blocks, usize) -> Option<u64>,
    not: &str,
) -> Stacking {
    let tried = |step: Step, copies: usize| {
        let (stacked, _) =
            stack::stack(data, &[(step.direction(), copies)], buses).ok()?;
        works(&Blocks::read(&stacked).ok()?, copies)
    };
    let found = (1..=reach)
        .map(|west| Step {
            west,
            up: 0,
            north: 0,
        })
        .find(|&step| tried(step, 2).is_some());
    let Some(step) = found else {
        return Stacking::Not {
            step: None,
            reason: not.to_owned(),
        };
    };
    let mut most = None;
    let mut copies = 2;
    let mut cases = tried(step, copies).unwrap_or_default();
    while copies < MOST_COPIES {
        let Some(more) = tried(step, copies + 1) else {
            most = Some(copies);
            break;
        };
        copies += 1;
        cases = more;
    }
    Stacking::Wider {
        step,
        copies,
        bits: bits * copies,
        cases,
        most,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_lies_the_way_it_steps() {
        let step = |west, up, north| Step { west, up, north };
        assert_eq!(step(0, 2, 0).orientation(), Some(Orientation::Vertical));
        assert_eq!(step(5, 0, 0).orientation(), Some(Orientation::Horizontal));
        assert_eq!(step(1, 1, 0).orientation(), Some(Orientation::Diagonal));
        assert_eq!(step(0, 0, 4).orientation(), None);
    }

    #[test]
    fn enough_copies_reach_past_what_dust_carries() {
        let step = |west, up| Step { west, up, north: 0 };
        // A 1-bit slice 2 wide: 9 copies, 18 blocks.
        assert_eq!(copies(step(2, 0), 1), Some(9));
        // An 8-bit adder 16 high: three, one in the middle.
        assert_eq!(copies(step(0, 16), 8), Some(3));
        // A 16-bit one: three would be 48 bits, so two.
        assert_eq!(copies(step(0, 32), 16), Some(2));
        assert_eq!(copies(step(0, 64), 32), None);
    }

    #[test]
    fn an_even_bus_goes_on_by_its_spacing() {
        let bus: Vec<Pos> = (0..8).map(|i| [0, 2 * i, 0]).collect();
        assert_eq!(next_after(&bus), Some([0, 16, 0]));
    }

    #[test]
    fn a_staggered_bus_goes_on_by_its_pattern() {
        // Each other bit a block north: gaps alternate.
        let bus: Vec<Pos> = (0..8).map(|i| [0, 2 * i, -(i % 2)]).collect();
        assert_eq!(next_after(&bus), Some([0, 16, 0]));
    }

    #[test]
    fn a_bus_with_one_bit_out_of_line_has_no_spacing() {
        let mut bus: Vec<Pos> = (0..8).map(|i| [0, 2 * i, 0]).collect();
        bus[0] = [0, 1, 1];
        assert_eq!(next_after(&bus), None);
    }
}
