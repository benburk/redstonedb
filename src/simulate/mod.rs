//! Runs a build in MCHPRS's redstone compiler, Redpiler, to check what it
//! computes.
//!
//! Redpiler models dust, torches, repeaters, comparators, levers, buttons,
//! and lamps, and the barrels, furnaces, and hoppers a comparator reads a
//! signal strength from, but not pistons, observers, or the order in which
//! the game updates blocks. A build using anything else is refused rather
//! than simulated wrongly.
//!
//! Each [`Test`] is one category's. Most are stateless, and state only what
//! sets them apart as a spec, which `bench` runs; an ALU's operations and a
//! register's or buffer's cells and a monostable's pulse run their own
//! way.

use std::{fmt, sync::atomic::AtomicBool};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    category::{self, Kind},
    recognize::interface::{self, Interface},
    schem::{Blocks, Pos},
};

/// Why a build cannot be what its category says, such as an adder whose
/// operands differ in width. Unlike a block the simulator does not model,
/// this is the build's fault, so it counts as failing.
#[derive(Debug)]
struct Broken(String);

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Broken {}

/// Fails with `Broken` and a message formatted as `format!` does.
macro_rules! broken {
    ($($arg:tt)*) => {
        return Err(Broken(format!($($arg)*)).into())
    };
}

// After `broken!`, which they use.
mod adder;
mod alu;
mod bench;
mod circuit;
mod converters;
mod decoder;
mod design;
mod gate;
mod incrementer;
mod inverter;
mod monostable;
mod multiplier;
mod negator;
mod register;
mod rng;
mod sequential;
mod shifter;
mod stacking;
mod strength;
mod throughput;
mod timing;
mod value;

pub use adder::Variant as AdderVariant;
pub use alu::{flags as alu_flags, op as alu_op};
/// An ALU's operations and status lines, as its test reads them.
pub mod alu_ops {
    pub use super::alu::{Op, carry_out, overflow, shift_out, zero};
}
pub use bench::mask;
pub(crate) use circuit::panic_message;
pub use circuit::{Circuit, Compiled, Link, compiled, settled};
pub use design::{Design, Terms, adder_design, design_of, groups};
pub use gate::Gate;
pub use incrementer::Counts;
pub use shifter::flags as shifter_flags;
pub use stacking::{Stacker, Stacking, Step, Way as StackWay};
pub use strength::Strength;
pub use throughput::throughput;
pub use timing::{
    BitTiming, Edge, Glitch, InputTiming, OutputTiming, Pulse, Start, Takes,
    Timing,
};
pub use value::Value;

/// The test a build of one category gets, which tries every case when they
/// number few enough, else every case of each byte and the test patterns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Test {
    /// The sums of A, B, and any carry in, or, B inverted, the differences
    /// of A and B, less any borrow in, which may read inverted.
    Adder,
    /// A's every value into one bit of an output, the one A names, off
    /// and the rest on.
    Decoder,
    /// A nand B and A xnor B, on outputs of their own.
    NandXnor,
    /// A shifted or rotated by an amount its lines give, the way its other
    /// lines choose.
    BarrelShifter,
    /// A plus 1, or plus its control line.
    Incrementer,
    /// A minus 1, or minus its control line.
    Decrementer,
    /// A plus 1 one way of its control line and minus 1 the other.
    IncDec,
    /// The products of A and B, plus C when there is one.
    Multiplier,
    /// A in binary-coded decimal: each decimal digit in four bits, the
    /// ones lowest.
    BinaryToBcd,
    /// A's number from binary-coded decimal, for each A whose digits are
    /// all decimal and whose number the outputs can hold.
    BcdToBinary,
    /// Each operation an ALU's control lines name, and any other they give.
    Alu,
    /// Each value written into every cell, then read.
    Register,
    /// Each value written into every cell, then shown with no read, as
    /// data in changes.
    Buffer,
    /// A's magnitude, A taken as a signed number.
    AbsoluteValue,
    /// −A in two's complement, or A until its control line turns on.
    Negator,
    /// The carry out of A + B, and a carry in when there is one.
    CarryOut,
    /// Whether A is greater than B, both unsigned or both in two's
    /// complement, whichever the build gets more right.
    GreaterThan,
    /// Whether A equals B.
    Equality,
    /// A pulse as each bit of A turns on, and none as it turns off, or the
    /// other way round.
    Monostable,
    /// A pulse as each bit of A turns on and as it turns off.
    DualEdgeMonostable,
    /// A, or with its control line on, every bit of A inverted.
    Inverter,
    /// How many of A's bits are on.
    Popcount,
    /// A as the strength of one output block.
    BinaryToHex,
    /// A, a hex digit a block as its strength, in binary.
    HexToBinary,
    /// Whether at least some number of A's bits are on.
    Threshold,
    /// Whether an odd number of A's bits are on, the xor of them all, or
    /// an even number, its inverse.
    Parity,
    /// A xor B.
    Xor,
    /// A xor B, every bit of it inverted.
    Xnor,
    /// A and B.
    And,
    /// A and B, every bit of it inverted.
    Nand,
    /// A or B.
    Or,
    /// A or B, every bit of it inverted.
    Nor,
    /// A value held, counted up or down as its control lines pulse.
    Counter,
    /// A value held, shifted toward its high or low bit as its control
    /// lines pulse.
    ShiftRegister,
    /// One bit held, toggled by each pulse of its one control line.
    TFlipFlop,
}

/// The widest build tested, whose every value fits one [`Value`] with room
/// for a carry, or a product of two; see `Bench::fits`.
const MAX_WIDTH: usize = 64;

/// The widest stack of copies tested, narrower than [`MAX_WIDTH`], as each
/// copy more slows the run and a few copies show whether they stack.
const MAX_STACKED: usize = 32;

/// What a stateless test drives, reads, and expects of a build, from its
/// file and its interface, whose lines it names for their roles.
type SpecOf = fn(&Blocks, &mut Interface) -> Result<bench::Spec>;

/// How copies of a build, its file, whose own cases came out right, are
/// stacked and tested as its category's group asks; see `Test::stacking`.
type StacksOf = for<'a> fn(&'a [u8]) -> Option<Stacker<'a>>;

/// How a test runs: from a spec, as most stateless ones do, or its own
/// way.
#[derive(Clone, Copy)]
enum Way {
    /// Driven by `spec`, which `bench` runs, its copies stacking as
    /// `stacks` finds, when they are tried.
    Spec {
        spec: SpecOf,
        stacks: Option<StacksOf>,
    },
    /// Each operation an ALU's control lines name.
    Alu,
    /// Each shift a barrel shifter's lines give.
    Shifter,
    /// Each value written into a register's or buffer's every cell.
    Register(register::Kind),
    /// A pulse on one edge of A, or with `dual` on both.
    Monostable { dual: bool },
    /// A value held and changed as control lines pulse.
    Sequential(sequential::Kind),
}

impl Way {
    /// A test driven by `spec` whose copies are never tried stacked.
    const fn spec(spec: SpecOf) -> Way {
        Way::Spec { spec, stacks: None }
    }

    /// A test driven by `spec` whose copies stack as `stacks` finds.
    const fn stacked(spec: SpecOf, stacks: StacksOf) -> Way {
        Way::Spec {
            spec,
            stacks: Some(stacks),
        }
    }
}

/// The interface of the build in `blocks` with its lines named as `spec`
/// names them, and the spec.
fn bench(spec: SpecOf, blocks: &Blocks) -> Result<(Interface, bench::Spec)> {
    let mut interface = interface::read(blocks);
    let spec = spec(blocks, &mut interface)?;
    Ok((interface, spec))
}

/// An incrementer's kind of test, counting `counts`: its unsigned line is
/// named for its role, such as increment, so the cases read `A +
/// increment`.
fn counting(
    kind: Kind,
    counts: Counts,
    blocks: &Blocks,
    interface: &mut Interface,
) -> Result<bench::Spec> {
    interface::name_lines(kind, blocks, interface);
    incrementer::spec(interface, Some(counts))
}

impl Test {
    /// The category the test is made for, which its results are stored
    /// under the name of, and how it runs.
    #[allow(clippy::too_many_lines, reason = "a table, one arm per test")]
    fn made(self) -> (Kind, Way) {
        match self {
            Test::Adder => (
                Kind::Adder,
                Way::stacked(|_, i| adder::adder(i), stacking::adder),
            ),
            Test::Decoder => {
                (Kind::Decoder, Way::spec(|_, i| decoder::spec(i)))
            }
            Test::NandXnor => (
                Kind::NandXnorGate,
                Way::stacked(
                    |_, i| gate::nand_xnor(i),
                    gate::nand_xnor_stacker,
                ),
            ),
            Test::BarrelShifter => (Kind::BarrelShifter, Way::Shifter),
            Test::Incrementer => (
                Kind::Incrementer,
                Way::stacked(
                    |b, i| counting(Kind::Incrementer, Counts::Up, b, i),
                    |data| stacking::chained(data, Test::Incrementer),
                ),
            ),
            Test::Decrementer => (
                Kind::Decrementer,
                Way::stacked(
                    |b, i| counting(Kind::Decrementer, Counts::Down, b, i),
                    |data| stacking::chained(data, Test::Decrementer),
                ),
            ),
            Test::IncDec => (
                Kind::IncDec,
                Way::spec(|b, i| counting(Kind::IncDec, Counts::Either, b, i)),
            ),
            Test::Multiplier => (
                Kind::Multiplier,
                // Its lone control line is named for its role, calculate.
                Way::spec(|b, i| {
                    interface::name_lines(Kind::Multiplier, b, i);
                    multiplier::spec(i)
                }),
            ),
            Test::BinaryToBcd => (
                Kind::BinaryToBcd,
                Way::spec(|_, i| converters::binary_to_bcd(i)),
            ),
            Test::BcdToBinary => (
                Kind::BcdToBinary,
                Way::spec(|_, i| converters::bcd_to_binary(i)),
            ),
            Test::Alu => (Kind::Alu, Way::Alu),
            Test::Register => {
                (Kind::Register, Way::Register(register::Kind::Register))
            }
            Test::Buffer => {
                (Kind::Buffer, Way::Register(register::Kind::Buffer))
            }
            Test::AbsoluteValue => (
                Kind::AbsoluteValue,
                Way::spec(|_, i| converters::absolute_value(i)),
            ),
            Test::Negator => (
                Kind::Negator,
                // Its lone control line is named for its role, negate.
                Way::spec(|b, i| {
                    interface::name_lines(Kind::Negator, b, i);
                    negator::spec(i)
                }),
            ),
            Test::CarryOut => (
                Kind::CarryOut,
                Way::stacked(
                    |_, i| adder::carry_out(i),
                    |data| stacking::chained(data, Test::CarryOut),
                ),
            ),
            Test::GreaterThan => {
                (Kind::GreaterThan, Way::spec(|_, i| adder::greater_than(i)))
            }
            Test::Equality => {
                (Kind::Equality, Way::spec(|_, i| adder::equality(i)))
            }
            Test::Monostable => {
                (Kind::Monostable, Way::Monostable { dual: false })
            }
            Test::DualEdgeMonostable => {
                (Kind::DualEdgeMonostable, Way::Monostable { dual: true })
            }
            Test::Inverter => (
                Kind::Inverter,
                // Its lone control line is named for its role, invert.
                Way::stacked(
                    |b, i| {
                        interface::name_lines(Kind::Inverter, b, i);
                        inverter::spec(i)
                    },
                    inverter::stacker,
                ),
            ),
            Test::Popcount => {
                (Kind::Popcount, Way::spec(|_, i| converters::popcount(i)))
            }
            Test::BinaryToHex => (
                Kind::BinaryToHex,
                Way::spec(|_, i| converters::binary_to_hex(i)),
            ),
            Test::HexToBinary => (
                Kind::HexToBinary,
                Way::spec(|_, i| converters::hex_to_binary(i)),
            ),
            Test::Threshold => {
                (Kind::Threshold, Way::spec(|_, i| converters::threshold(i)))
            }
            Test::Parity => {
                (Kind::Parity, Way::spec(|_, i| converters::parity(i)))
            }
            Test::Xor => (
                Kind::XorGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Xor),
                    |data| gate::stacker(data, Gate::Xor),
                ),
            ),
            Test::Xnor => (
                Kind::XnorGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Xnor),
                    |data| gate::stacker(data, Gate::Xnor),
                ),
            ),
            Test::And => (
                Kind::AndGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::And),
                    |data| gate::stacker(data, Gate::And),
                ),
            ),
            Test::Nand => (
                Kind::NandGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Nand),
                    |data| gate::stacker(data, Gate::Nand),
                ),
            ),
            Test::Or => (
                Kind::OrGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Or),
                    |data| gate::stacker(data, Gate::Or),
                ),
            ),
            Test::Nor => (
                Kind::NorGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Nor),
                    |data| gate::stacker(data, Gate::Nor),
                ),
            ),
            Test::Counter => {
                (Kind::Counter, Way::Sequential(sequential::Kind::Counter))
            }
            Test::ShiftRegister => (
                Kind::ShiftRegister,
                Way::Sequential(sequential::Kind::ShiftRegister),
            ),
            Test::TFlipFlop => (
                Kind::TFlipFlop,
                Way::Sequential(sequential::Kind::TFlipFlop),
            ),
        }
    }

    /// The category it is made for, whose name its results are stored
    /// under: an adder's for a hex adder's and a subtractor's too.
    pub fn kind(self) -> Kind {
        self.made().0
    }

    fn way(self) -> Way {
        self.made().1
    }

    /// Every test, each its own category's, in the order of their
    /// categories.
    pub fn all() -> impl Iterator<Item = Test> {
        Kind::all().filter_map(|kind| kind.test().filter(|t| t.kind() == kind))
    }

    /// The test a build of `category` and `width` gets: its category's,
    /// when it has one and the build is no wider than `MAX_WIDTH`.
    pub fn of(category: &str, width: Option<i64>) -> Option<Self> {
        let tested = |w: i64| {
            usize::try_from(w).is_ok_and(|w| (1..=MAX_WIDTH).contains(&w))
        };
        if !width.is_some_and(tested) {
            return None;
        }
        category::behaviour(category).test
    }

    /// The name stored with its result, which is its category's.
    pub fn name(self) -> &'static str {
        self.kind().name()
    }

    /// Runs the test's cases on `blocks` and checks each result.
    pub fn run(self, blocks: &Blocks) -> Report {
        Report::of(self.check(blocks, false))
    }

    /// Runs the test's cases on `blocks` through `interface`, as
    /// [`Test::check_with`] does, as a report.
    pub fn run_with(self, blocks: &Blocks, interface: Interface) -> Report {
        Report::of(self.check_with(blocks, interface, false))
    }

    /// A few telling cases of the test run on `blocks`, as its colours mark
    /// its lines: enough to tell one reading of a build from another.
    pub fn quick(self, blocks: &Blocks) -> Result<Run> {
        self.check(blocks, true)
    }

    /// The test's cases run on `blocks` through `interface`, as probing
    /// finds it rather than as colours mark it, or with `quick` a few
    /// telling ones. Only a test driven by a spec, such as an adder's,
    /// runs so; any other fails.
    pub fn check_with(
        self,
        blocks: &Blocks,
        mut interface: Interface,
        quick: bool,
    ) -> Result<Run> {
        let Way::Spec { spec, .. } = self.way() else {
            anyhow::bail!("{} runs its own way", self.kind().a());
        };
        let spec = spec(blocks, &mut interface)?;
        spec.check(blocks, &interface, quick)
    }

    /// The test's cases run on `blocks`, or with `quick` a few telling
    /// ones, quick enough for a guess at what a build is.
    fn check(self, blocks: &Blocks, quick: bool) -> Result<Run> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, spec) = bench(spec, blocks)?;
                let mut run = spec.check(blocks, &interface, quick)?;
                // An adder's kinds name the lines it reads inverted.
                if self == Test::Adder {
                    run.inverted = run.adder_variant().lines(&run);
                }
                Ok(run)
            }
            Way::Alu => alu::check(blocks, quick),
            Way::Shifter => shifter::check(blocks, quick),
            Way::Register(kind) => register::check(blocks, kind, quick),
            Way::Monostable { dual } => monostable::check(blocks, dual),
            Way::Sequential(kind) => sequential::check(blocks, kind),
        }
    }

    /// Whether the build in `blocks`, whose cases all came out right, holds
    /// state, as `bench::remembers` tells: what is left of each case once
    /// its presses are over, against a fresh copy given only the case's
    /// inputs. A register's or buffer's cases are what it holds once its
    /// data bus has changed, so passing them is holding state, as a
    /// counter's, shift register's, or T flip-flop's are; an ALU's is
    /// told of each of its operations; a monostable, a pulse generator,
    /// holds none, its pulse being its input's change, not a held value.
    pub fn remembers(self, blocks: &Blocks) -> Result<Option<bool>> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, spec) = bench(spec, blocks)?;
                bench::remembers(blocks, &interface, &spec.bench).map(Some)
            }
            Way::Register(_) | Way::Sequential(_) => Ok(Some(true)),
            Way::Alu => alu::remembers(blocks).map(Some),
            Way::Shifter => shifter::remembers(blocks).map(Some),
            Way::Monostable { .. } => Ok(Some(false)),
        }
    }

    /// Whether copies of the build in `data`, whose own cases came out
    /// right, stack into a wider one of its kind, tried at each pitch
    /// class as its category's group asks: an adder's, an incrementer's,
    /// and a carry out's copies must chain; a gate's, a monostable's, and a
    /// T flip-flop's each work alone beside the others; an inverter's, a register's, and
    /// a buffer's share lines, as many copies as work counted, and a
    /// register's copies may also add words on its shared bus. `None` for
    /// any other, and for a category whose builds never stack, as a
    /// decoder's, whose copies side by side are separate decoders.
    pub fn stacking(self, data: &[u8]) -> Option<Stacking> {
        if self.kind().facts().behaviour.stacking
            == crate::category::Stacking::Never
        {
            return None;
        }
        match self.way() {
            Way::Register(kind) => register::stacking(data, kind),
            _ => Some(self.stacker(data)?.stacking()),
        }
    }

    /// How copies of the build in `data` are stacked and tested, as
    /// [`Test::stacking`] tries each pitch, for one step at a time; see
    /// [`Stacker::at`]. `None` for a test whose copies are never stacked.
    pub fn stacker(self, data: &[u8]) -> Option<Stacker<'_>> {
        match self.way() {
            Way::Spec { stacks, .. } => stacks.and_then(|stacks| stacks(data)),
            Way::Register(kind) => register::stacker(data, kind),
            Way::Monostable { dual } => monostable::stacker(data, dual),
            Way::Sequential(sequential::Kind::TFlipFlop) => {
                sequential::toggle_stacker(data)
            }
            Way::Alu | Way::Shifter | Way::Sequential(_) => None,
        }
    }

    /// Whether `copies` of the build in `data`, whose own cases came out
    /// right, stacked `step` make a wider one of its kind, as its own
    /// `stacking` tests each step, the box rule's box first, then boxes
    /// leaving out more of the first copy: the step with the box that
    /// works and the cases run, or why not. Copies apart, as
    /// `stack::touches` has them, never stack. A register's copies are tried as more words on
    /// its shared bus, then as more bits. `None` for a test whose copies
    /// are never stacked.
    pub fn stacks_at(
        self,
        data: &[u8],
        step: Step,
        copies: usize,
    ) -> Option<Result<(Step, u64), String>> {
        if let Way::Register(kind) = self.way() {
            return register::stacks_at(data, kind, step, copies)
                .map(|found| found.map(|(step, cases, _)| (step, cases)));
        }
        Some(self.stacker(data)?.at(step, copies))
    }

    /// How often the build in `blocks`, its file `data`, timed as
    /// `timing`, takes a new case, as `throughput::throughput` streams
    /// them, through its lines named as its test names them, as its timing
    /// names them, so each goes in at its own tick; a register's cases are
    /// its accesses, as `register::throughput` streams them. Meant for a
    /// build whose cases all came out right.
    pub fn throughput(
        self,
        blocks: &Blocks,
        data: &[u8],
        timing: &Timing,
    ) -> Result<Option<i32>> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, _) = bench(spec, blocks)?;
                throughput::throughput(blocks, &interface, timing)
            }
            Way::Register(kind) => register::throughput(data, timing, kind),
            Way::Monostable { .. } => monostable::throughput(blocks, timing),
            Way::Alu => alu::throughput(blocks, timing),
            Way::Shifter => shifter::throughput(blocks, timing),
            // How fast one may be clocked isn't measured yet.
            Way::Sequential(_) => Ok(None),
        }
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, spec) = bench(spec, blocks)?;
                spec.time(blocks, &interface, cancel)
            }
            Way::Alu => alu::time(blocks, cancel),
            Way::Shifter => shifter::time(blocks, cancel),
            Way::Register(kind) => register::time(blocks, kind, cancel),
            Way::Monostable { dual } => monostable::time(blocks, dual, cancel),
            Way::Sequential(kind) => sequential::time(blocks, kind, cancel),
        }
    }
}

/// What a test found.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Report {
    Ran(Box<Run>),
    /// The build could not be simulated, such as for a block the simulator
    /// does not model.
    Skipped {
        reason: String,
    },
    /// The build cannot be what its category says, such as an adder whose
    /// operands differ in width.
    Broken {
        reason: String,
    },
}

impl Report {
    /// A run's report: its cases, or why it broke or was skipped.
    fn of(run: Result<Run>) -> Self {
        match run {
            Ok(run) => Report::Ran(Box::new(run)),
            Err(err) => match err.downcast_ref::<Broken>() {
                Some(Broken(reason)) => Report::Broken {
                    reason: reason.clone(),
                },
                None => Report::Skipped {
                    reason: err.to_string(),
                },
            },
        }
    }

    pub fn passed(&self) -> bool {
        matches!(self, Report::Ran(run) if run.wrong == 0)
    }

    /// Whether the build got cases wrong or is broken; one that was skipped
    /// has not failed.
    pub fn failed(&self) -> bool {
        match self {
            Report::Ran(run) => run.wrong > 0,
            Report::Broken { .. } => true,
            Report::Skipped { .. } => false,
        }
    }

    /// How weakly its data inputs may be driven, once measured.
    pub fn strength(&self) -> Option<&Strength> {
        match self {
            Report::Ran(run) => run.strength.as_ref(),
            _ => None,
        }
    }

    /// The lines its test found read inverted, by name.
    pub fn inverted(&self) -> &[String] {
        match self {
            Report::Ran(run) => &run.inverted,
            _ => &[],
        }
    }

    /// When its outputs settle, once measured.
    pub fn timing(&self) -> Option<&Timing> {
        match self {
            Report::Ran(run) => run.timing.as_ref(),
            _ => None,
        }
    }
}

/// A test that ran, case by case.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Run {
    /// What each case is, such as `A + B + carry in`.
    pub pattern: String,
    /// The cases tried.
    pub cases: u64,
    /// Whether those were every combination of inputs. When not, they were
    /// every case of each byte and every pairing of the test patterns.
    pub exhaustive: bool,
    /// How many of the cases were pairings of the test patterns.
    pub patterns: u64,
    /// Whether, not trying every combination, each hex digit took its
    /// every case rather than each byte, as for a hex adder.
    pub by_digit: bool,
    /// How many cases were wrong.
    pub wrong: u64,
    /// What each case sets, in order, such as A, B, and carry in.
    pub inputs: Vec<Signal>,
    /// What each case reads, in order, such as the output and carry out.
    pub outputs: Vec<Signal>,
    /// Each result bit that was ever wrong, such as `output bit 7` or
    /// `carry out`, with how many cases it was wrong in.
    pub wrong_bits: Vec<(String, u64)>,
    /// The first few wrong cases.
    pub examples: Vec<Wrong>,
    /// What would make every case right, when one change does, such as
    /// flipping carry in.
    pub hint: Option<String>,
    /// The lines found to read inverted, powered meaning 0, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inverted: Vec<String>,
    /// What set apart the reading of the build its cases came out best
    /// under, which `pattern` names, for a test whose readings mean more
    /// than their names, such as an adder's subtracting. `None` for any
    /// other test, and for a report stored before readings were kept,
    /// whose pattern alone says; see `Run::adder_variant` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<Reading>,
    /// For a stateless build that gets every case right, how weakly its
    /// data inputs may be driven, and how weak its data outputs then get.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<Strength>,
    /// For a failing register or buffer, its cells' controls, by the
    /// number its faults give each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellControls>,
    /// For a failing register or buffer, each fault its cells show, as the
    /// writes and reads that show it from the build as saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faults: Vec<Vec<Action>>,
    /// When its outputs settle; measured after the cases, and only for a
    /// build that gets them all right.
    pub timing: Option<Timing>,
    /// Whether copies of a build that gets them all right stack into a
    /// wider one, as `Test::stacking` tries; `None` for a build of another
    /// test, or one too wide to try.
    pub stacking: Option<Stacking>,
    /// For an adder that gets them all right, how it makes its carries:
    /// its design, the letter of each carry, its groups and their terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<Design>,
}

/// What sets one reading of a build apart from a test's others, where more
/// than its cases' name depends on it. A kind's cases' name is made from
/// it, never the other way round.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(tag = "of", rename_all = "snake_case")]
pub enum Reading {
    /// How an adder reads its lines: B subtracted, its carry in inverted.
    Adder(AdderVariant),
    /// Whether a greater-than comparator reads its operands in two's
    /// complement.
    GreaterThan { signed: bool },
    /// How many of A's bits turn a threshold on.
    Threshold { at_least: u32 },
    /// Whether an incrementer counts up, down, or either way.
    Incrementer { counts: Counts },
    /// Whether a register's reads show its cells inverted.
    Register { inverted: bool },
    /// A reading a report kept that no test reads builds by any more, as
    /// whether a parity circuit was even once was: as good as none.
    #[serde(other)]
    Retired,
}

impl Run {
    /// The case `wrong` as evidence: its operation, if any, each input's
    /// value, and each output it got wrong as got and as expected, a bus
    /// in binary, such as `A=3 B=5 carry in=0 → output=0b0110, expected
    /// 0b1000`.
    pub fn case(&self, wrong: &Wrong) -> String {
        let value = |signal: &Signal, value: Value| {
            if signal.bits > 1 {
                format!("0b{value:0width$b}", width = signal.bits)
            } else {
                value.to_string()
            }
        };
        let inputs: Vec<String> = self
            .inputs
            .iter()
            .zip(&wrong.inputs)
            .map(|(signal, value)| format!("{}={value}", signal.name))
            .collect();
        let outputs: Vec<String> = self
            .outputs
            .iter()
            .zip(wrong.got.iter().zip(&wrong.expected))
            .filter(|(_, (got, expected))| got != expected)
            .map(|(signal, (&got, &expected))| {
                format!(
                    "{}={}, expected {}",
                    signal.name,
                    value(signal, got),
                    value(signal, expected)
                )
            })
            .collect();
        let op = wrong
            .op
            .as_deref()
            .map(|op| format!("{op}: "))
            .unwrap_or_default();
        format!("{op}{} → {}", inputs.join(" "), outputs.join("; "))
    }

    /// How many cases it got wrong, of how many, with the first as
    /// evidence, such as `3 of 512 cases wrong: A=3 B=5 → output=0b0110,
    /// expected 0b1000`.
    pub fn wrong_cases(&self) -> String {
        let counted = format!("{} of {} cases wrong", self.wrong, self.cases);
        match self.examples.first() {
            Some(first) => format!("{counted}: {}", self.case(first)),
            None => counted,
        }
    }

    /// How an adder's cases read its lines.
    pub fn adder_variant(&self) -> AdderVariant {
        match self.reading {
            Some(Reading::Adder(variant)) => variant,
            _ => stored::adder_variant(&self.pattern),
        }
    }

    /// The category of a build the adder's test ran as this: a subtractor
    /// when it subtracts B, else a hex adder when its inputs take `hex`
    /// digits, else an adder.
    pub fn adder_category(&self, hex: bool) -> Kind {
        if self.adder_variant().subtract {
            Kind::Subtractor
        } else if hex {
            Kind::HexAdder
        } else {
            Kind::Adder
        }
    }

    /// Whether a greater-than comparator's cases read its operands signed.
    pub fn signed(&self) -> bool {
        match self.reading {
            Some(Reading::GreaterThan { signed }) => signed,
            _ => stored::signed(&self.pattern),
        }
    }

    /// How many of A's bits turn on a threshold, as its cases found.
    pub fn at_least(&self) -> Option<i64> {
        match self.reading {
            Some(Reading::Threshold { at_least }) => Some(at_least.into()),
            _ => stored::at_least(&self.pattern),
        }
    }

    /// Whether a register's reads show its cells inverted.
    pub fn reads_inverted(&self) -> bool {
        match self.reading {
            Some(Reading::Register { inverted }) => inverted,
            _ => stored::reads_inverted(&self.pattern),
        }
    }
}

/// Reading a report stored before `Run::reading` was kept, whose reading
/// only its cases' name, `Run::pattern`, says. Each answers as the name a
/// fresh run of that reading gives it would.
mod stored {
    use super::AdderVariant;

    /// The way an adder whose cases are named `pattern` reads.
    pub fn adder_variant(pattern: &str) -> AdderVariant {
        AdderVariant {
            subtract: pattern.starts_with("A − "),
            carry_in: pattern.contains("B + not ")
                || pattern.contains("B − not "),
        }
    }

    /// Whether a comparison whose cases are named `pattern` reads its
    /// operands in two's complement.
    pub fn signed(pattern: &str) -> bool {
        pattern == super::adder::SIGNED_GREATER_THAN
    }

    /// How many of A's bits a threshold's cases, named `pattern`, turn it
    /// on at; `None` for any other cases.
    pub fn at_least(pattern: &str) -> Option<i64> {
        pattern
            .strip_prefix("at least ")?
            .strip_suffix(" of A on")?
            .parse()
            .ok()
    }

    /// Whether a register whose cases are named `pattern` reads its cells
    /// inverted.
    pub fn reads_inverted(pattern: &str) -> bool {
        pattern.ends_with(" inverted")
    }
}

/// A register's or buffer's cell, by the block each control is on: its
/// write's, and its first read's, which a buffer's cell has none of.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CellControls {
    pub write: Pos,
    pub read: Option<Pos>,
}

/// One step of a register's fault, on a cell by its place in `Run::cells`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Action {
    /// Data in set to `value`, then the cell's write pressed, or its lever
    /// flicked on and off; data in stays on `value`.
    Write {
        cell: usize,
        #[serde(with = "value")]
        value: Value,
    },
    /// The cell's first read turned on and its bus, by its place in
    /// `Run::outputs`, read, then the read turned off.
    Read {
        cell: usize,
        bus: usize,
        #[serde(with = "value")]
        expected: Value,
        #[serde(with = "value")]
        got: Value,
    },
}

impl Action {
    /// Whether it read what it should; a write always does.
    pub fn right(&self) -> bool {
        match self {
            Action::Write { .. } => true,
            Action::Read { expected, got, .. } => expected == got,
        }
    }

    /// What a read showed; a write shows nothing.
    pub fn got(&self) -> Option<Value> {
        match self {
            Action::Write { .. } => None,
            Action::Read { got, .. } => Some(*got),
        }
    }
}

/// A port a test sets or reads, by the build's name for it.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Signal {
    pub name: String,
    pub bits: usize,
}

/// One case a build got wrong: each input's value, and each output's
/// expected and actual value, in the order of `Run`'s.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Wrong {
    /// The operation it was on, for a build of several, such as an ALU.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(with = "value::many")]
    pub inputs: Vec<Value>,
    #[serde(with = "value::many")]
    pub expected: Vec<Value>,
    #[serde(with = "value::many")]
    pub got: Vec<Value>,
}

/// What a build seems to be, from a few telling values run through each
/// test its inputs and outputs fit.
#[derive(Debug, Clone)]
pub struct Guess {
    /// Its category, such as `adder`.
    pub category: &'static str,
    /// What its cases are, such as `A + B + carry in`.
    pub pattern: String,
    pub cases: u64,
    pub wrong: u64,
    /// How many of its lines it reads inverted.
    pub inverted: usize,
}

impl Guess {
    /// The share of its cases it got wrong, in thousandths, which ranks one
    /// guess against another.
    pub fn wrong_share(&self) -> u64 {
        self.wrong * 1000 / self.cases.max(1)
    }
}

/// What the build in `blocks` seems to be: of the tests its inputs and
/// outputs fit, the one whose telling values come out right most often, if
/// three in four do, reading the fewest control lines inverted on a tie.
/// Quick enough to run before anyone says what it is, even for a hex
/// build, which runs slower.
pub fn guess(blocks: &Blocks) -> Option<Guess> {
    // An ALU's lines are tried only when their signs name operations, as
    // any build with a control line might otherwise pass for one.
    let names_ops = interface::read(blocks)
        .inputs
        .iter()
        .filter(|port| port.control())
        .flat_map(|port| port.words.iter())
        .any(|word| alu::Op::named(word).is_some());
    // Inputs taking a hex digit per block, a signal strength, are a hex
    // adder's.
    let hex = interface::takes_digits(blocks);
    category::Kind::guessed()
        .into_iter()
        .filter(|&(_, test)| test != Test::Alu || names_ops)
        .filter_map(|(kind, test)| {
            // One run tries every kind of incrementer, and the kind it gets
            // most right names its category.
            let run = if test == Test::Incrementer {
                let interface = interface::read(blocks);
                incrementer::spec(&interface, None)
                    .and_then(|spec| spec.check(blocks, &interface, true))
            } else {
                test.check(blocks, true)
            }
            .ok()?;
            let kind = match run.reading {
                Some(Reading::Incrementer { counts }) => counts.kind(),
                _ if test == Test::Adder => run.adder_category(hex),
                _ => kind,
            };
            Some(Guess {
                category: kind.name(),
                pattern: run.pattern,
                cases: run.cases,
                wrong: run.wrong,
                inverted: run.inverted.len(),
            })
        })
        // Three in four right, so a build with a few wrong bits is still
        // known for what it is.
        .filter(|guess| (guess.cases - guess.wrong) * 4 >= guess.cases * 3)
        .min_by_key(|guess| (guess.wrong_share(), guess.inverted))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A report kept with a reading no test gives any more, as a parity
    /// circuit's evenness, still reads, its reading as good as none.
    #[test]
    fn a_retired_reading_still_reads() {
        let reading: Reading =
            serde_json::from_str(r#"{"of":"parity","even":false}"#).unwrap();
        assert_eq!(reading, Reading::Retired);
        let kept: Reading =
            serde_json::from_str(r#"{"of":"register","inverted":true}"#)
                .unwrap();
        assert_eq!(kept, Reading::Register { inverted: true });
    }

    #[test]
    fn builds_up_to_64_bits_are_tested() {
        assert_eq!(Test::of("adder", Some(16)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(64)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(65)), None);
        assert_eq!(Test::of("adder", None), None);
        assert_eq!(Test::of("register", Some(8)), Some(Test::Register));
        assert_eq!(Test::of("ram", Some(8)), None);
        assert_eq!(Test::of("multiplier", Some(65)), None);
    }

    /// A report stored before readings were kept is read from its cases'
    /// name, so the name each reading gives must read back as it.
    #[test]
    fn a_stored_pattern_reads_as_the_reading_that_named_it() {
        for subtract in [false, true] {
            for carry_in in [false, true] {
                let variant = AdderVariant { subtract, carry_in };
                for line in [None, Some("carry in"), Some("cin")] {
                    let pattern = variant.name(line);
                    let read = stored::adder_variant(&pattern);
                    // With no carry in, there is none to read inverted.
                    let carry_in = carry_in && line.is_some();
                    assert_eq!(read, AdderVariant { subtract, carry_in });
                }
            }
        }
        assert!(stored::signed(adder::SIGNED_GREATER_THAN));
        assert!(!stored::signed("A > B"));
        for k in 1..=8 {
            let pattern = converters::threshold_pattern(k);
            assert_eq!(stored::at_least(&pattern), Some(k.into()));
        }
        assert!(stored::reads_inverted("written, then read inverted"));
        assert!(!stored::reads_inverted(
            "each of 2 cells written, then read"
        ));
    }
}
