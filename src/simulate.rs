//! Runs a build in MCHPRS's redstone compiler, Redpiler, to check what it
//! computes.
//!
//! Redpiler models dust, torches, repeaters, comparators, levers, buttons,
//! and lamps, and the barrels, furnaces, and hoppers a comparator reads a
//! signal strength from, but not pistons, observers, or
//! the order in which the game updates blocks. A build using anything else
//! is refused rather than simulated wrongly.
//!
//! Each [`Test`] is one category's. Most are stateless, and state only what
//! sets them apart as a spec, which `bench` runs; an ALU's operations and a
//! register's or buffer's cells and a monostable's pulse run their own
//! way.

use std::{fmt, sync::atomic::AtomicBool};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    category, colour,
    interface::{self, Interface},
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
mod sequential;
mod shifter;
mod stacking;
mod strength;
mod throughput;
mod timing;

pub use adder::{SIGNED_GREATER_THAN, Variant as AdderVariant};
pub use alu::{flags as alu_flags, op as alu_op};
pub(crate) use circuit::panic_message;
pub use circuit::{Circuit, Compiled, Link, compiled, settled};
pub use converters::at_least;
pub use decoder::INVERTED as DECODER_INVERTED;
pub use design::{Design, Terms, adder_design, design_of, groups};
pub use gate::Gate;
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
    /// of A and B, less any borrow in, any line read inverted.
    Adder,
    /// A's every value into one bit of an output, the one A names.
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
    /// Whether at least some number of A's bits are on.
    Threshold,
    /// A xor B, or every bit of it inverted, an XNOR gate's.
    XorXnor,
    /// A and B, or every bit of it inverted, a NAND gate's.
    AndNand,
    /// A or B, or every bit of it inverted, a NOR gate's.
    OrNor,
    /// A value held, counted up or down as its control lines pulse.
    Counter,
    /// A value held, shifted toward its high or low bit as its control
    /// lines pulse.
    ShiftRegister,
}

/// The widest build tested, whose every value fits one number with room
/// for a carry.
const MAX_WIDTH: i64 = 32;

impl Test {
    /// Every test.
    pub const ALL: [Test; 27] = [
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
        Test::Threshold,
        Test::XorXnor,
        Test::AndNand,
        Test::OrNor,
        Test::Counter,
        Test::ShiftRegister,
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
            Test::Threshold => "threshold",
            Test::CarryOut => "carry_out",
            Test::GreaterThan => "greater_than",
            Test::Equality => "equality",
            Test::Monostable => "monostable",
            Test::DualEdgeMonostable => "dual_edge_monostable",
            Test::Inverter => "inverter",
            Test::XorXnor => "xor_xnor_gate",
            Test::AndNand => "and_nand_gate",
            Test::OrNor => "or_nor_gate",
            Test::Counter => "counter",
            Test::ShiftRegister => "shift_register",
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
            _ => {
                let mut interface = interface::read(blocks);
                let spec = self.spec(blocks, &mut interface)?;
                let mut run = spec.check(blocks, &interface, quick)?;
                // An adder's kinds name the lines it reads inverted.
                if self == Test::Adder {
                    run.inverted = adder::Variant::of(&run.pattern).lines(&run);
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
            Test::Register | Test::Buffer | Test::Counter | Test::ShiftRegister
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
            Test::Threshold => converters::threshold(interface),
            Test::Inverter => {
                // Its lone control line is named for its role, invert.
                interface::name_lines(self.name(), blocks, interface);
                inverter::spec(interface)
            }
            Test::XorXnor | Test::AndNand | Test::OrNor => {
                gate::spec(interface, Gate::of(self).expect("a gate's test"))
            }
            Test::Alu
            | Test::BarrelShifter
            | Test::Register
            | Test::Buffer
            | Test::Monostable
            | Test::DualEdgeMonostable
            | Test::Counter
            | Test::ShiftRegister => {
                unreachable!(
                    "an ALU, a register, a buffer, a monostable, a counter, \
                     and a shift register run their own way"
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
            Test::XorXnor | Test::AndNand | Test::OrNor => {
                gate::stacking(data, Gate::of(self)?)
            }
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
            Test::Counter | Test::ShiftRegister => Ok(None),
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
/// carry out is A xor its line, as a 1-bit inverter is. An XOR or XNOR gate
/// comes before an equality comparator, as a 1-bit one is A xnor B.
const GUESSED: [Test; 25] = [
    Test::Adder,
    Test::Decoder,
    Test::NandXnor,
    Test::BarrelShifter,
    Test::Inverter,
    Test::XorXnor,
    Test::AndNand,
    Test::OrNor,
    Test::Incrementer,
    Test::Multiplier,
    Test::BinaryToBcd,
    Test::BcdToBinary,
    Test::Register,
    Test::Buffer,
    Test::AbsoluteValue,
    Test::Popcount,
    Test::Threshold,
    Test::CarryOut,
    Test::GreaterThan,
    Test::Equality,
    Test::Monostable,
    Test::DualEdgeMonostable,
    Test::Alu,
    Test::Counter,
    Test::ShiftRegister,
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

/// What the build in `blocks` seems to be: of the tests its inputs and
/// outputs fit, the one whose telling values come out right most often, if
/// three in four do, reading the fewest lines inverted on a tie, as a
/// comparator is a carry out with B inverted. Quick enough to run before anyone says what it is,
/// even for a hex build, which runs slower.
pub fn guess(blocks: &Blocks) -> Option<Guess> {
    // An ALU's lines are tried only when their signs name operations, as
    // any build with a control line might otherwise pass for one.
    let names_ops = interface::read(blocks)
        .inputs
        .iter()
        .filter(|port| port.block.starts_with(colour::CONTROL))
        .flat_map(|port| port.words.iter())
        .any(|word| alu::op(word).is_some());
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
                let line = run.inputs.get(1).map(|signal| signal.name.as_str());
                incrementer::changes(line)
                    .into_iter()
                    .find(|change| change.name == run.pattern)
                    .map_or("incrementer", |change| change.category)
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
        .min_by_key(|guess| {
            (guess.wrong * 1000 / guess.cases.max(1), guess.inverted)
        })
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
}
