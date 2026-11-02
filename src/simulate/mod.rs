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
    recognize::{
        category, colour,
        interface::{self, Interface},
    },
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
mod register;
mod rng;
mod sequential;
mod shifter;
mod stacking;
mod strength;
mod throughput;
mod timing;

pub use adder::Variant as AdderVariant;
pub use alu::{flags as alu_flags, op as alu_op};
pub(crate) use circuit::panic_message;
pub use circuit::{Circuit, Compiled, Link, compiled, settled};
pub use design::{Design, Terms, adder_design, design_of, groups};
pub use gate::Gate;
pub use incrementer::Counts;
pub use shifter::flags as shifter_flags;
pub use stacking::{Stacking, Step};
pub use strength::Strength;
pub use throughput::throughput;
pub use timing::{
    BitTiming, Edge, Glitch, InputTiming, OutputTiming, Pulse, Start, Takes,
    Timing,
};

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
    /// Whether at least some number of A's bits are on.
    Threshold,
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

/// The widest build tested, whose every value fits one number with room
/// for a carry.
const MAX_WIDTH: i64 = 32;

impl Test {
    /// Every test.
    pub const ALL: [Test; 32] = [
        Test::Adder,
        Test::Decoder,
        Test::NandXnor,
        Test::BarrelShifter,
        Test::Incrementer,
        Test::Decrementer,
        Test::IncDec,
        Test::Multiplier,
        Test::BinaryToBcd,
        Test::BcdToBinary,
        Test::Alu,
        Test::Register,
        Test::Buffer,
        Test::AbsoluteValue,
        Test::CarryOut,
        Test::GreaterThan,
        Test::Equality,
        Test::Monostable,
        Test::DualEdgeMonostable,
        Test::Inverter,
        Test::Popcount,
        Test::BinaryToHex,
        Test::Threshold,
        Test::Xor,
        Test::Xnor,
        Test::And,
        Test::Nand,
        Test::Or,
        Test::Nor,
        Test::Counter,
        Test::ShiftRegister,
        Test::TFlipFlop,
    ];

    /// The test a build of `category` and `width` gets: its category's,
    /// when it has one and the build is no wider than `MAX_WIDTH`.
    pub fn of(category: &str, width: Option<i64>) -> Option<Self> {
        if !width.is_some_and(|w| (1..=MAX_WIDTH).contains(&w)) {
            return None;
        }
        category::behaviour(category).test
    }

    /// Whether `guess` may take a build for one of this test's category,
    /// a decrementer or inc/dec by the incrementer's run; an ALU only when
    /// its control lines' signs name operations.
    pub fn guessed(self) -> bool {
        matches!(self, Test::Decrementer | Test::IncDec)
            || GUESSED.contains(&self)
    }

    /// The name stored with its result, which is its category's.
    pub fn name(self) -> &'static str {
        match self {
            Test::Adder => "adder",
            Test::Decoder => "decoder",
            Test::NandXnor => "nand_xnor_gate",
            Test::BarrelShifter => "barrel_shifter",
            Test::Incrementer => "incrementer",
            Test::Decrementer => "decrementer",
            Test::IncDec => "inc_dec",
            Test::Multiplier => "multiplier",
            Test::BinaryToBcd => "binary_to_bcd",
            Test::BcdToBinary => "bcd_to_binary",
            Test::Alu => "alu",
            Test::Register => "register",
            Test::Buffer => "buffer",
            Test::AbsoluteValue => "absolute_value",
            Test::Popcount => "popcount",
            Test::BinaryToHex => "binary_to_hex",
            Test::Threshold => "threshold",
            Test::CarryOut => "carry_out",
            Test::GreaterThan => "greater_than",
            Test::Equality => "equality",
            Test::Monostable => "monostable",
            Test::DualEdgeMonostable => "dual_edge_monostable",
            Test::Inverter => "inverter",
            Test::Xor => "xor_gate",
            Test::Xnor => "xnor_gate",
            Test::And => "and_gate",
            Test::Nand => "nand_gate",
            Test::Or => "or_gate",
            Test::Nor => "nor_gate",
            Test::Counter => "counter",
            Test::ShiftRegister => "shift_register",
            Test::TFlipFlop => "t_flip_flop",
        }
    }

    /// Runs the test's cases on `blocks` and checks each result.
    pub fn run(self, blocks: &Blocks) -> Report {
        match self.check(blocks, false) {
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

    /// A few telling cases of the test run on `blocks`, as its colours mark
    /// its lines: enough to tell one reading of a build from another.
    pub fn quick(self, blocks: &Blocks) -> Result<Run> {
        self.check(blocks, true)
    }

    /// The test's cases run on `blocks` through `interface`, as probing
    /// finds it rather than as colours mark it, or with `quick` a few
    /// telling ones. For the tests driven by a spec only, such as an
    /// adder's.
    pub fn check_with(
        self,
        blocks: &Blocks,
        mut interface: Interface,
        quick: bool,
    ) -> Result<Run> {
        let spec = self.spec(blocks, &mut interface)?;
        spec.check(blocks, &interface, quick)
    }

    /// The test's cases run on `blocks`, or with `quick` a few telling
    /// ones, quick enough for a guess at what a build is.
    fn check(self, blocks: &Blocks, quick: bool) -> Result<Run> {
        match self {
            Test::Alu => alu::check(blocks, quick),
            Test::BarrelShifter => shifter::check(blocks, quick),
            Test::Register => {
                register::check(blocks, register::Kind::Register, quick)
            }
            Test::Buffer => {
                register::check(blocks, register::Kind::Buffer, quick)
            }
            Test::Monostable => monostable::check(blocks, false),
            Test::DualEdgeMonostable => monostable::check(blocks, true),
            Test::Counter => {
                sequential::check(blocks, sequential::Kind::Counter)
            }
            Test::ShiftRegister => {
                sequential::check(blocks, sequential::Kind::ShiftRegister)
            }
            Test::TFlipFlop => {
                sequential::check(blocks, sequential::Kind::TFlipFlop)
            }
            _ => {
                let mut interface = interface::read(blocks);
                let spec = self.spec(blocks, &mut interface)?;
                let mut run = spec.check(blocks, &interface, quick)?;
                // An adder's kinds name the lines it reads inverted.
                if self == Test::Adder {
                    run.inverted = run.adder_variant().lines(&run);
                }
                Ok(run)
            }
        }
    }

    /// Whether the build in `blocks`, whose cases all came out right, holds
    /// state, as `bench::remembers` tells: what is left of each case once
    /// its presses are over, against a fresh copy given only the case's
    /// inputs. A register's or buffer's cases are what it holds once its
    /// data bus has changed, so passing them is holding state; an ALU's is
    /// told of each of its operations; a monostable, a pulse generator,
    /// holds none.
    pub fn remembers(self, blocks: &Blocks) -> Result<Option<bool>> {
        if matches!(
            self,
            Test::Register
                | Test::Buffer
                | Test::Counter
                | Test::ShiftRegister
                | Test::TFlipFlop
        ) {
            return Ok(Some(true));
        }
        if self == Test::Alu {
            return alu::remembers(blocks).map(Some);
        }
        if self == Test::BarrelShifter {
            return shifter::remembers(blocks).map(Some);
        }
        // A pulse generator's pulse is its input's change, not a held
        // value, so it holds no state.
        if matches!(self, Test::Monostable | Test::DualEdgeMonostable) {
            return Ok(Some(false));
        }
        let mut interface = interface::read(blocks);
        let spec = self.spec(blocks, &mut interface)?;
        bench::remembers(blocks, &interface, &spec.bench).map(Some)
    }

    /// What a stateless test drives, reads, and expects of a build with
    /// `interface`, its lines named for their roles.
    fn spec(
        self,
        blocks: &Blocks,
        interface: &mut Interface,
    ) -> Result<bench::Spec> {
        match self {
            Test::Adder => adder::adder(interface),
            Test::Decoder => decoder::spec(interface),
            Test::NandXnor => gate::nand_xnor(interface),
            Test::CarryOut => adder::carry_out(interface),
            Test::GreaterThan => adder::greater_than(interface),
            Test::Equality => adder::equality(interface),
            Test::Incrementer | Test::Decrementer | Test::IncDec => {
                // An unsigned line is named for its role, such as
                // increment, so the cases read `A + increment`.
                interface::name_lines(self.name(), blocks, interface);
                incrementer::spec(interface, Some(self.name()))
            }
            Test::Multiplier => {
                // Its lone control line is named for its role, calculate.
                interface::name_lines(self.name(), blocks, interface);
                multiplier::spec(interface)
            }
            Test::BinaryToBcd => converters::binary_to_bcd(interface),
            Test::BcdToBinary => converters::bcd_to_binary(interface),
            Test::AbsoluteValue => converters::absolute_value(interface),
            Test::Popcount => converters::popcount(interface),
            Test::BinaryToHex => converters::binary_to_hex(interface),
            Test::Threshold => converters::threshold(interface),
            Test::Inverter => {
                // Its lone control line is named for its role, invert.
                interface::name_lines(self.name(), blocks, interface);
                inverter::spec(interface)
            }
            Test::Xor
            | Test::Xnor
            | Test::And
            | Test::Nand
            | Test::Or
            | Test::Nor => {
                gate::spec(interface, Gate::of(self).expect("a gate's test"))
            }
            Test::Alu
            | Test::BarrelShifter
            | Test::Register
            | Test::Buffer
            | Test::Monostable
            | Test::DualEdgeMonostable
            | Test::Counter
            | Test::ShiftRegister
            | Test::TFlipFlop => {
                unreachable!(
                    "an ALU, a register, a buffer, a monostable, a counter, \
                     a shift register, and a T flip-flop run their own way"
                )
            }
        }
    }

    /// Whether copies of the build in `data`, whose own cases came out
    /// right, stack into a wider one of its kind: tried on an adder, whose
    /// copies' carries must chain, a register, whose copies may add cells
    /// or bits, and a buffer, a monostable, an inverter, and a gate,
    /// whose copies add bits; `None` for any other.
    pub fn stacking(self, data: &[u8]) -> Option<Stacking> {
        match self {
            Test::Adder => stacking::check(data),
            Test::Register => {
                register::stacking(data, register::Kind::Register)
            }
            Test::Buffer => register::stacking(data, register::Kind::Buffer),
            Test::Monostable => monostable::stacking(data, false),
            Test::DualEdgeMonostable => monostable::stacking(data, true),
            Test::Inverter => inverter::stacking(data),
            Test::Xor
            | Test::Xnor
            | Test::And
            | Test::Nand
            | Test::Or
            | Test::Nor => gate::stacking(data, Gate::of(self)?),
            Test::NandXnor => gate::nand_xnor_stacking(data),
            Test::Decoder => decoder::stacking(data),
            Test::Incrementer | Test::Decrementer | Test::CarryOut => {
                Some(stacking::chained(data, self))
            }
            _ => None,
        }
    }

    /// How often the build in `blocks`, its file `data`, timed as
    /// `timing`, takes a new case, as `throughput::throughput` streams
    /// them; a register's cases are its accesses, as `register::throughput`
    /// streams them. Meant for a build whose cases all came out right.
    pub fn throughput(
        self,
        blocks: &Blocks,
        data: &[u8],
        timing: &Timing,
    ) -> Result<Option<i32>> {
        match self {
            Test::Register => {
                register::throughput(data, timing, register::Kind::Register)
            }
            Test::Buffer => {
                register::throughput(data, timing, register::Kind::Buffer)
            }
            Test::Monostable | Test::DualEdgeMonostable => {
                monostable::throughput(blocks, timing)
            }
            Test::Alu => alu::throughput(blocks, timing),
            Test::BarrelShifter => shifter::throughput(blocks, timing),
            // How fast one may be clocked isn't measured yet.
            Test::Counter | Test::ShiftRegister | Test::TFlipFlop => Ok(None),
            _ => {
                // Its lines named as its test names them, as its timing
                // names them, so each goes in at its own tick.
                let mut interface = interface::read(blocks);
                self.spec(blocks, &mut interface)?;
                throughput::throughput(blocks, &interface, timing)
            }
        }
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        match self {
            Test::Alu => alu::time(blocks, cancel),
            Test::BarrelShifter => shifter::time(blocks, cancel),
            Test::Register => {
                register::time(blocks, register::Kind::Register, cancel)
            }
            Test::Buffer => {
                register::time(blocks, register::Kind::Buffer, cancel)
            }
            Test::Monostable => monostable::time(blocks, false, cancel),
            Test::DualEdgeMonostable => monostable::time(blocks, true, cancel),
            Test::Counter => {
                sequential::time(blocks, sequential::Kind::Counter, cancel)
            }
            Test::ShiftRegister => sequential::time(
                blocks,
                sequential::Kind::ShiftRegister,
                cancel,
            ),
            Test::TFlipFlop => {
                sequential::time(blocks, sequential::Kind::TFlipFlop, cancel)
            }
            _ => {
                let mut interface = interface::read(blocks);
                let spec = self.spec(blocks, &mut interface)?;
                spec.time(blocks, &interface, cancel)
            }
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
}

impl Run {
    /// How an adder's cases read its lines.
    pub fn adder_variant(&self) -> AdderVariant {
        match self.reading {
            Some(Reading::Adder(variant)) => variant,
            _ => stored::adder_variant(&self.pattern),
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
    Write { cell: usize, value: u64 },
    /// The cell's first read turned on and its bus, by its place in
    /// `Run::outputs`, read, then the read turned off.
    Read {
        cell: usize,
        bus: usize,
        expected: u64,
        got: u64,
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
    pub fn got(&self) -> Option<u64> {
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
    pub inputs: Vec<u64>,
    pub expected: Vec<u64>,
    pub got: Vec<u64>,
}

/// The tests `guess` tries, the first winning a tie. One run tries every
/// kind of incrementer, and the kind it gets most right names its
/// category. An inverter comes before it, as a 1-bit incrementer without a
/// carry out is A xor its line, as a 1-bit inverter is. An XNOR gate comes
/// before an equality comparator, as a 1-bit one is A xnor B. A
/// popcount and a threshold come before the BCD converters, whose telling
/// values a popcount's strength output can pass.
const GUESSED: [Test; 30] = [
    Test::Adder,
    Test::Decoder,
    Test::NandXnor,
    Test::BarrelShifter,
    Test::Inverter,
    Test::Xor,
    Test::Xnor,
    Test::And,
    Test::Nand,
    Test::Or,
    Test::Nor,
    Test::Popcount,
    Test::BinaryToHex,
    Test::Threshold,
    Test::Incrementer,
    Test::Multiplier,
    Test::BinaryToBcd,
    Test::BcdToBinary,
    Test::Register,
    Test::Buffer,
    Test::AbsoluteValue,
    Test::CarryOut,
    Test::GreaterThan,
    Test::Equality,
    Test::Monostable,
    Test::DualEdgeMonostable,
    Test::Alu,
    Test::Counter,
    Test::ShiftRegister,
    Test::TFlipFlop,
];

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
        .filter(|port| port.block.starts_with(colour::CONTROL))
        .flat_map(|port| port.words.iter())
        .any(|word| alu::Op::named(word).is_some());
    // Inputs taking a hex digit per block through dust are a hex adder's.
    let hex = !interface::dust_inputs(blocks).is_empty();
    GUESSED
        .into_iter()
        .filter(|&test| test != Test::Alu || names_ops)
        .filter_map(|test| {
            let run = if test == Test::Incrementer {
                let interface = interface::read(blocks);
                incrementer::spec(&interface, None)
                    .and_then(|spec| spec.check(blocks, &interface, true))
            } else {
                test.check(blocks, true)
            }
            .ok()?;
            let category = if test == Test::Incrementer {
                match run.reading {
                    Some(Reading::Incrementer { counts }) => counts.category(),
                    _ => "incrementer",
                }
            } else if test == Test::Adder && run.adder_variant().subtract {
                // An adder whose B is subtracted is a subtractor.
                "subtractor"
            } else if test == Test::Adder && hex {
                "hex_adder"
            } else {
                test.name()
            };
            Some(Guess {
                category,
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

    #[test]
    fn builds_up_to_32_bits_are_tested() {
        assert_eq!(Test::of("adder", Some(16)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(32)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(33)), None);
        assert_eq!(Test::of("adder", None), None);
        assert_eq!(Test::of("register", Some(8)), Some(Test::Register));
        assert_eq!(Test::of("ram", Some(8)), None);
        assert_eq!(Test::of("multiplier", Some(33)), None);
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
