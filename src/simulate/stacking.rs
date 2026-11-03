//! Whether copies of a build stack into a wider one of its kind, as
//! stacking would join them. Copies go west or up, never north, east, or
//! down, and only where they touch or overlap: copies with air between
//! them are separate builds side by side; see `stack::touches`.
//!
//! Copies that add bits are tried only at the pitch classes, `PITCHES`:
//! copies 1, 2, or 4 blocks a bit apart, along the way the build's bits
//! run, west for a horizontal build and up for a vertical one, and both
//! ways for a 1-bit build, whose one bit runs neither. The tightest pitch
//! whose copies work is the build's class, and it counts in each looser
//! class too, so no looser one is tried.
//!
//! How copies must work depends on the category's group, as its
//! `category::Copies` says:
//!
//! - Where data crosses bits, as an adder's carry does, copies must chain
//!   into one wider build: a stack is a wider adder, incrementer, or carry
//!   out, tested as one, the cheapest checks first, as `stacked` runs an
//!   adder's.
//! - Where each copy is a cell of its own, as a gate's or a monostable's,
//!   each copy must still compute or pulse alone, its neighbours not
//!   interfering, which a stack's full test of the wider gate or
//!   monostable shows, each bit's case alone and all together.
//! - Where copies share lines, as an inverter's control or a register's or
//!   buffer's, they must work together on them, and the most copies that
//!   do is kept, as a shared line that runs too far fails.
//!
//! A stack tested is long enough that one copy sits between two others
//! and that it is longer than dust carries a signal, 15 blocks, plus a
//! copy, so a path through copies without a repeater or torch to renew it
//! fails; copies sharing lines are added until one more fails. Copies that
//! work then are taken to stack any number of times.
//!
//! Each step is tried with each box its copies may repeat, the box rule's
//! first, then boxes leaving more to the first copy; see `stack::find_box`.
//! The first box whose copies work goes with the step.

use serde::{Deserialize, Serialize};

use super::{
    MAX_STACKED, Report, Test, Value,
    adder::{self, ADD, adder_ports},
    bench::{Case, mask, sweep},
    circuit::{Circuit, input_bits},
};
use crate::{
    category::{Group, Kind},
    import,
    models::schematic::{Faces, Orientation, PITCHES, StackDirection},
    recognize::interface::{self, Interface},
    schem::{Blocks, stack},
};

/// The farthest dust carries a signal, which a stack tested must reach
/// past.
const DUST_REACH: i64 = 15;

/// The most copies stacked of a build whose copies share lines; one whose
/// copies work with this many is taken to stack any number of times.
pub(super) const MOST_COPIES: usize = 16;

/// What stacking copies of a build found.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Stacking {
    /// Copies every `step` chain into one build of `bits` bits, as an
    /// adder's carries do, whose every case came out right.
    Stacks {
        step: Step,
        copies: usize,
        bits: usize,
        cases: u64,
    },
    /// Copies of a register every `step`, words on its shared data bus,
    /// make a register file whose every cell stores and reads right:
    /// `copies` of them, `registers` cells in all, in `cases` cases.
    /// `most` is how many copies work when one more does not, as a shared
    /// bus that runs too far fails; `None` when every number tried worked.
    Registers {
        step: Step,
        copies: usize,
        registers: usize,
        cases: u64,
        most: Option<usize>,
    },
    /// Copies every `step` make a wider build of `bits` bits whose every
    /// case came out right: `copies` of them, in `cases` cases, each a
    /// cell of its own, as a gate's, or sharing lines, as an inverter's
    /// control or a register's. `most` is as for `Registers`, kept only
    /// for copies that share lines.
    Wider {
        step: Step,
        copies: usize,
        bits: usize,
        cases: u64,
        most: Option<usize>,
        /// The other way copies make a wider one, as a 1-bit gate's may
        /// both west and up.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        also: Vec<Apart>,
        /// For a register, how its copies stack as words on its shared
        /// data bus too, if they do.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        words: Option<Apart>,
    },
    /// Copies side by side each doing their own work, as decoders' were
    /// once found to; decoders no longer stack, but a report kept from
    /// before may say so.
    Separate { ways: Vec<Apart> },
    /// They make no working build of its kind, as an adder whose carries do
    /// not chain or a register with no step that works, with the step
    /// tried last when there was one.
    Not { step: Option<Step>, reason: String },
}

impl Stacking {
    /// The ways it stacks, as a build's stacking is kept: each with its
    /// step, its box, the most copies that work, and, for copies that add
    /// bits, their pitch class; none when it does not stack.
    pub fn directions(&self) -> Vec<StackDirection> {
        let per_copy = |bits: usize, copies: usize| bits / copies.max(1);
        match self {
            Stacking::Stacks {
                step, copies, bits, ..
            } => vec![StackDirection {
                pitch: step.pitch(per_copy(*bits, *copies)),
                ..step.direction()
            }],
            Stacking::Registers { step, most, .. } => {
                vec![Apart::of(*step, *most).direction(None)]
            }
            Stacking::Wider {
                step,
                copies,
                bits,
                most,
                also,
                words,
                ..
            } => {
                let bits = Some(per_copy(*bits, *copies));
                [Apart::of(*step, *most)]
                    .iter()
                    .chain(also)
                    .map(|apart| apart.direction(bits))
                    .chain(words.iter().map(|words| words.direction(None)))
                    .collect()
            }
            Stacking::Separate { .. } | Stacking::Not { .. } => Vec::new(),
        }
    }
}

/// One way copies stack: every `step`, `copies` of them, in `cases` cases.
/// `most` is as for `Stacking::Registers`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Apart {
    pub step: Step,
    pub copies: usize,
    pub cases: u64,
    pub most: Option<usize>,
}

impl Apart {
    /// Copies every `step`, `most` of them at most, as a direction is
    /// kept.
    fn of(step: Step, most: Option<usize>) -> Self {
        Apart {
            step,
            copies: 0,
            cases: 0,
            most,
        }
    }

    /// The direction it stacks, with the most copies that work, and the
    /// pitch class of copies each adding `bits`, or none for copies that
    /// add words.
    pub fn direction(self, bits: Option<usize>) -> StackDirection {
        StackDirection {
            max_copies: self.most.and_then(|n| i64::try_from(n).ok()),
            pitch: bits.and_then(|bits| self.step.pitch(bits)),
            ..self.step.direction()
        }
    }
}

/// How far each copy sits from the one before, and the box later copies
/// repeat.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub west: i64,
    pub up: i64,
    /// `None` for the box the box rule finds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<Faces>,
}

impl Step {
    pub fn direction(self) -> StackDirection {
        StackDirection {
            west: self.west,
            up: self.up,
            max_copies: None,
            faces: self.faces,
            pitch: None,
        }
    }

    /// A step `west` and `up` with the box rule's box.
    pub fn new(west: i64, up: i64) -> Self {
        Step {
            west,
            up,
            faces: None,
        }
    }

    /// Its pitch class for copies each adding `bits`: the blocks a bit, 1,
    /// 2, or 4; `None` for any other spacing.
    pub fn pitch(self, bits: usize) -> Option<i64> {
        let bits = i64::try_from(bits).ok()?;
        let length = self.west + self.up;
        PITCHES
            .into_iter()
            .find(|&pitch| bits > 0 && pitch * bits == length)
    }

    /// The step with the box its copies of the build in `data` repeat,
    /// found by `works`, copies `along_bits` or across them, with their
    /// buses as `buses` says; see `stack::find_box`. The `Err` is why the
    /// box rule's box failed.
    pub fn find<T, E>(
        self,
        data: &[u8],
        along_bits: bool,
        buses: stack::Buses,
        mut works: impl FnMut(Step) -> Result<T, E>,
    ) -> Option<Result<(Step, T), E>> {
        let of = |direction: StackDirection| Step {
            faces: direction.faces,
            ..self
        };
        let found = stack::find_box(
            data,
            self.direction(),
            along_bits,
            buses,
            |direction| works(of(direction)),
        )
        .ok()?;
        Some(found.map(|(direction, t)| (of(direction), t)))
    }

    /// The layout copies stacked this way give, as the bits of an adder
    /// whose copies add up run the way it stacks; see
    /// `StackDirection::orientation`.
    pub fn orientation(self) -> Option<Orientation> {
        self.direction().orientation()
    }

    /// Such as `16 up` or `2 west and 2 up`.
    pub fn describe(self) -> String {
        [(self.west, "west"), (self.up, "up")]
            .into_iter()
            .filter(|&(n, _)| n > 0)
            .map(|(n, way)| format!("{n} {way}"))
            .collect::<Vec<_>>()
            .join(" and ")
    }
}

/// A way copies go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    West,
    Up,
}

impl Way {
    /// A step of `n` blocks this way.
    pub fn step(self, n: i64) -> Step {
        match self {
            Way::West => Step::new(n, 0),
            Way::Up => Step::new(0, n),
        }
    }

    /// The way the bits of a build run whose copies this way add bits.
    pub fn orientation(self) -> Orientation {
        match self {
            Way::West => Orientation::Horizontal,
            Way::Up => Orientation::Vertical,
        }
    }

    /// The other way.
    #[must_use]
    pub fn across(self) -> Way {
        match self {
            Way::West => Way::Up,
            Way::Up => Way::West,
        }
    }

    /// The way `step` goes; `None` for a step both ways or neither.
    pub fn of(step: Step) -> Option<Way> {
        match (step.west > 0, step.up > 0) {
            (true, false) => Some(Way::West),
            (false, true) => Some(Way::Up),
            _ => None,
        }
    }
}

/// The ways copies of a build `bits` bits wide add bits: both, west
/// first, for a 1-bit build, whose one bit runs neither way, and for a
/// wider one the way its bits run, as its widest operand in `interface`
/// runs, west for a horizontal build and up for a vertical one, and none
/// for a diagonal build.
pub fn bit_ways(interface: &Interface, bits: usize) -> Vec<Way> {
    if bits <= 1 {
        return vec![Way::West, Way::Up];
    }
    match import::guess(interface).1 {
        None => vec![Way::West, Way::Up],
        Some(Orientation::Horizontal) => vec![Way::West],
        Some(Orientation::Vertical) => vec![Way::Up],
        Some(Orientation::Diagonal) => Vec::new(),
    }
}

/// The cases copies stacked a step ran, every one right, or why not.
type Works<'a> = dyn Fn(Step, usize, stack::Buses) -> Result<u64, String> + 'a;

/// A build's copies stacked and tested as its category's group asks; see
/// the module's notes. [`Stacker::search`] tries each pitch class each
/// way, and [`Stacker::at`] one step.
pub struct Stacker<'a> {
    data: &'a [u8],
    /// Its category, which says what its copies do with its buses and
    /// lines, and how they must work together.
    kind: Kind,
    /// The bits each copy adds.
    bits: usize,
    /// The ways its copies may add bits.
    ways: Vec<Way>,
    /// The cases `copies` stacked `step`, their buses as given, ran, every
    /// one right, or why not.
    works: Box<Works<'a>>,
    /// Why no step works, when none does.
    not: String,
}

impl<'a> Stacker<'a> {
    /// Copies of the build in `data`, of `kind`, each adding `bits` bits,
    /// the ways `ways`, tested by `works`; `not` says why none stack.
    pub(super) fn new(
        data: &'a [u8],
        kind: Kind,
        (bits, ways): (usize, Vec<Way>),
        works: impl Fn(Step, usize, stack::Buses) -> Result<u64, String> + 'a,
        not: &str,
    ) -> Self {
        Stacker {
            data,
            kind,
            bits,
            ways,
            works: Box::new(works),
            not: not.to_owned(),
        }
    }

    /// The group its copies are in.
    fn group(&self) -> Group {
        self.kind.facts().behaviour.group
    }

    /// Whether its copies share lines, so they are added until one more
    /// fails, rather than as many as reach past what dust carries.
    fn shared(&self) -> bool {
        self.group() == Group::SharedLines
    }

    /// What copies stacked `way` do with the build's buses and lines.
    fn buses(&self, way: Way) -> stack::Buses {
        stack::Buses::of(self.kind, way.orientation())
    }

    /// The bits each copy adds.
    pub fn bits(&self) -> usize {
        self.bits
    }

    /// How many copies to test stacked `step`, reaching past what dust
    /// carries; two for copies that share lines, which [`Stacker::search`]
    /// adds to. `None` when two would be wider than the simulator tests.
    fn copies(&self, step: Step) -> Option<usize> {
        if self.shared() {
            (2 * self.bits <= MAX_STACKED).then_some(2)
        } else {
            copies(step, self.bits)
        }
    }

    /// Whether `copies` stacked `step` work, the box rule's box first,
    /// then boxes leaving out more of the first copy: the step with the
    /// box that works and the cases run, or why not. Copies that leave air
    /// between them never stack.
    pub fn at(&self, step: Step, copies: usize) -> Result<(Step, u64), String> {
        let touching = stack::touches(self.data, step.direction())
            .map_err(|err| err.to_string())?;
        if !touching {
            return Err(format!(
                "Copies {} apart leave air between them, so they are \
                 separate builds side by side rather than a stack.",
                step.describe()
            ));
        }
        let buses = self.buses(Way::of(step).unwrap_or(Way::West));
        step.find(self.data, true, buses, |step| {
            (self.works)(step, copies, buses)
        })
        .unwrap_or_else(|| Err("Its file could not be read.".to_owned()))
    }

    /// Each way its copies add bits, at the tightest pitch class that
    /// works, with the most copies that do where they share lines, the
    /// tightest first and west on a tie; or the last step tried whose
    /// copies touch and why it failed, or, where none touch, why not.
    pub fn search(&self) -> Result<Vec<Apart>, (Option<Step>, String)> {
        let mut found: Vec<(i64, Apart)> = Vec::new();
        let mut last = (None, self.not.clone());
        let mut tested = false;
        let bits = i64::try_from(self.bits).unwrap_or(i64::MAX);
        for &way in &self.ways {
            for pitch in PITCHES {
                let step = way.step(pitch * bits);
                let Some(copies) = self.copies(step) else {
                    continue;
                };
                let touching = stack::touches(self.data, step.direction())
                    .unwrap_or(false);
                if tested && !touching {
                    continue;
                }
                tested |= touching;
                match self.at(step, copies) {
                    Ok((step, cases)) => {
                        let apart = Apart {
                            step,
                            copies,
                            cases,
                            most: None,
                        };
                        found.push((pitch, self.most(apart)));
                        break;
                    }
                    Err(reason) => last = (Some(step), reason),
                }
            }
        }
        found.sort_by_key(|&(pitch, _)| pitch);
        if found.is_empty() {
            return Err(last);
        }
        Ok(found.into_iter().map(|(_, apart)| apart).collect())
    }

    /// `apart` with as many copies as work, for copies that share lines,
    /// up to [`MOST_COPIES`] and as wide as the simulator tests; `most`
    /// the copies that work when one more does not.
    fn most(&self, mut apart: Apart) -> Apart {
        if !self.shared() {
            return apart;
        }
        let fitting = MAX_STACKED / self.bits.max(1);
        while apart.copies < MOST_COPIES.min(fitting) {
            let Ok((_, cases)) = self.at(apart.step, apart.copies + 1) else {
                apart.most = Some(apart.copies);
                break;
            };
            apart.copies += 1;
            apart.cases = cases;
        }
        apart
    }

    /// What stacking its copies finds: a chain of them, or a wider build,
    /// each way they work; or why none do.
    pub fn stacking(&self) -> Stacking {
        match self.search() {
            Err((step, reason)) => Stacking::Not { step, reason },
            Ok(ways) if self.group() == Group::CrossesBits => {
                let first = ways[0];
                Stacking::Stacks {
                    step: first.step,
                    copies: first.copies,
                    bits: first.copies * self.bits,
                    cases: first.cases,
                }
            }
            Ok(mut ways) => {
                let first = ways.remove(0);
                Stacking::Wider {
                    step: first.step,
                    copies: first.copies,
                    bits: first.copies * self.bits,
                    cases: first.cases,
                    most: first.most,
                    also: ways,
                    words: None,
                }
            }
        }
    }
}

/// The cases a stack of `copies` of the build in `data`, stacked `step`
/// with its buses as `buses` says, ran under `works`, or why not.
pub(super) fn run_stacked(
    data: &[u8],
    (step, copies): (Step, usize),
    buses: stack::Buses,
    works: impl FnOnce(&Blocks) -> Result<u64, String>,
) -> Result<u64, String> {
    let (stacked, _) = stack::stack(data, &[(step.direction(), copies)], buses)
        .map_err(|err| format!("Its copies do not fit together: {err:#}."))?;
    let blocks = Blocks::read(&stacked).map_err(|err| err.to_string())?;
    works(&blocks)
}

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
    Some(Stacker::new(data, Kind::Adder, ways, works, not))
}

/// How many copies to test: three, so one sits between two others, and
/// enough that the stack reaches past what dust carries by a copy; but no
/// wider than the simulator tests, and `None` when two would be.
fn copies(step: Step, width: usize) -> Option<usize> {
    let length = u64::try_from(step.west + step.up).ok()?;
    let reach = u64::try_from(DUST_REACH + 1).ok()?;
    let reaching = usize::try_from(reach.div_ceil(length) + 1).ok()?;
    let fitting = MAX_STACKED / width;
    let copies = reaching.max(3).min(fitting);
    (copies >= 2).then_some(copies)
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
    let sum = |case: &Case| variant.apply(case);
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
    Some(Stacker::new(data, test.kind(), ways, works, not))
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
    Some(Stacker::new(data, test.kind(), ways, works, not))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_lies_the_way_it_steps() {
        let step = Step::new;
        assert_eq!(step(0, 2).orientation(), Some(Orientation::Vertical));
        assert_eq!(step(5, 0).orientation(), Some(Orientation::Horizontal));
        assert_eq!(step(1, 1).orientation(), Some(Orientation::Diagonal));
    }

    #[test]
    fn enough_copies_reach_past_what_dust_carries() {
        let step = |west, up| Step::new(west, up);
        // A 1-bit slice 2 wide: 9 copies, 18 blocks.
        assert_eq!(copies(step(2, 0), 1), Some(9));
        // An 8-bit adder 16 high: three, one in the middle.
        assert_eq!(copies(step(0, 16), 8), Some(3));
        // A 16-bit one: three would be 48 bits, so two.
        assert_eq!(copies(step(0, 32), 16), Some(2));
        assert_eq!(copies(step(0, 64), 32), None);
    }

    #[test]
    fn a_step_is_classed_by_the_blocks_a_bit() {
        // A 1-bit XOR every 2 and an 8-bit adder every 16 up: pitch 2.
        assert_eq!(Step::new(2, 0).pitch(1), Some(2));
        assert_eq!(Step::new(0, 16).pitch(8), Some(2));
        assert_eq!(Step::new(0, 8).pitch(8), Some(1));
        // A 1-bit adder every 8, or every 7: no class.
        assert_eq!(Step::new(8, 0).pitch(1), None);
        assert_eq!(Step::new(7, 0).pitch(1), None);
    }
}
