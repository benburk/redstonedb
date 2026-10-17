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
    category,
    interface::{self, Interface},
    lint,
    schem::Blocks,
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
mod incrementer;
mod monostable;
mod multiplier;
mod register;
mod stacking;
mod timing;

pub use adder::SIGNED_GREATER_THAN;
pub use alu::op as alu_op;
pub use circuit::Circuit;
pub(crate) use circuit::panic_message;
pub use stacking::{Stacking, Step};
pub use timing::{
    BitTiming, Edge, Glitch, InputTiming, OutputTiming, Pulse, Start, Takes,
    Timing,
};

/// The test a build of one category gets, which tries every case when they
/// number few enough, else every case of each byte and the test patterns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Test {
    /// The sums of A, B, and any carry in.
    Adder,
    /// The differences of A and B, less any borrow in.
    Subtractor,
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
}

/// The widest build tested, whose every value fits one number with room
/// for a carry.
const MAX_WIDTH: i64 = 32;

impl Test {
    /// Every test.
    pub const ALL: [Test; 17] = [
        Test::Adder,
        Test::Subtractor,
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
            Test::Subtractor => "subtractor",
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
            Test::CarryOut => "carry_out",
            Test::GreaterThan => "greater_than",
            Test::Equality => "equality",
            Test::Monostable => "monostable",
            Test::DualEdgeMonostable => "dual_edge_monostable",
        }
    }

    /// The version stored with its result. Raising it, as when a change to
    /// the test, its stacking, or its timing would change what a stored
    /// result says, makes every stored result of the test stale, so each
    /// runs again.
    pub fn version(self) -> i64 {
        match self {
            Test::Adder
            | Test::Subtractor
            | Test::Incrementer
            | Test::Decrementer
            | Test::IncDec
            | Test::Multiplier
            | Test::BinaryToBcd
            | Test::BcdToBinary
            | Test::Alu
            | Test::Buffer
            | Test::AbsoluteValue
            | Test::CarryOut
            | Test::Equality
            | Test::Monostable
            | Test::DualEdgeMonostable => 1,
            // A register's timing says it counts from a read, where it once
            // said so as a flag; a signed comparator passes as a Greater
            // Than with the flag Signed, where it was once a category of its
            // own.
            Test::Register | Test::GreaterThan => 2,
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

    /// The test's cases run on `blocks`, or with `quick` a few telling
    /// ones, quick enough for a guess at what a build is.
    fn check(self, blocks: &Blocks, quick: bool) -> Result<Run> {
        match self {
            Test::Alu => alu::check(blocks, quick),
            Test::Register => {
                register::check(blocks, register::Kind::Register, quick)
            }
            Test::Buffer => {
                register::check(blocks, register::Kind::Buffer, quick)
            }
            Test::Monostable => monostable::check(blocks, false),
            Test::DualEdgeMonostable => monostable::check(blocks, true),
            _ => {
                let mut interface = interface::read(blocks);
                let spec = self.spec(blocks, &mut interface)?;
                spec.check(blocks, &interface, quick)
            }
        }
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
            Test::Subtractor => adder::subtractor(interface),
            Test::CarryOut => adder::carry_out(interface),
            Test::GreaterThan => adder::greater_than(interface),
            Test::Equality => adder::equality(interface),
            Test::Incrementer | Test::Decrementer | Test::IncDec => {
                // An unsigned line is named for its role, such as
                // increment, so the cases read `A + increment`.
                interface::name_lines(self.name(), blocks, interface);
                incrementer::spec(interface, Some(self.name()))
            }
            Test::Multiplier => multiplier::spec(interface),
            Test::BinaryToBcd => converters::binary_to_bcd(interface),
            Test::BcdToBinary => converters::bcd_to_binary(interface),
            Test::AbsoluteValue => converters::absolute_value(interface),
            Test::Alu
            | Test::Register
            | Test::Buffer
            | Test::Monostable
            | Test::DualEdgeMonostable => {
                unreachable!(
                    "an ALU, a register, a buffer, and a monostable run their \
                     own way"
                )
            }
        }
    }

    /// Whether copies of the build in `data`, whose own cases came out
    /// right, stack into a wider one of its kind: tried on an adder, whose
    /// copies' carries must chain, a register, whose copies may add cells
    /// or bits, and a buffer and a monostable, whose copies add bits;
    /// `None` for any other.
    pub fn stacking(self, data: &[u8]) -> Option<Stacking> {
        match self {
            Test::Adder => stacking::check(data),
            Test::Register => {
                register::stacking(data, register::Kind::Register)
            }
            Test::Buffer => register::stacking(data, register::Kind::Buffer),
            Test::Monostable => monostable::stacking(data, false),
            Test::DualEdgeMonostable => monostable::stacking(data, true),
            _ => None,
        }
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        match self {
            Test::Alu => alu::time(blocks, cancel),
            Test::Register => {
                register::time(blocks, register::Kind::Register, cancel)
            }
            Test::Buffer => {
                register::time(blocks, register::Kind::Buffer, cancel)
            }
            Test::Monostable => monostable::time(blocks, false, cancel),
            Test::DualEdgeMonostable => monostable::time(blocks, true, cancel),
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
    /// When its outputs settle; measured after the cases, and only for a
    /// build that gets them all right.
    pub timing: Option<Timing>,
    /// Whether copies of a build that gets them all right stack into a
    /// wider one, as `Test::stacking` tries; `None` for a build of another
    /// test, or one too wide to try.
    pub stacking: Option<Stacking>,
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

/// The tests `guess` tries. One run tries every kind of incrementer, and
/// the kind it gets most right names its category.
const GUESSED: [Test; 15] = [
    Test::Adder,
    Test::Subtractor,
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
}

/// What the build in `blocks` seems to be: of the tests its inputs and
/// outputs fit, the one whose telling values come out right most often, if
/// three in four do. Quick enough to run before anyone says what it is,
/// even for a hex build, which runs slower.
pub fn guess(blocks: &Blocks) -> Option<Guess> {
    // An ALU's lines are tried only when their signs name operations, as
    // any build with a control line might otherwise pass for one.
    let names_ops = interface::read(blocks)
        .inputs
        .iter()
        .filter(|port| port.block.starts_with(lint::CONTROL))
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
            })
        })
        // Three in four right, so a build with a few wrong bits is still
        // known for what it is.
        .filter(|guess| (guess.cases - guess.wrong) * 4 >= guess.cases * 3)
        .min_by_key(|guess| guess.wrong * 1000 / guess.cases.max(1))
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
