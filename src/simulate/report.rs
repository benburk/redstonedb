//! What a test found: its cases, the ones it got wrong, and what was
//! measured of a build that got them all right. Plain values, as they are
//! stored with each build and shown on its page; what a reading means for
//! a build's category the categories say.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{Broken, Stacking, Strength, Timing, Value, value};
use crate::schem::Pos;

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
    pub(crate) fn of(run: Result<Run>) -> Self {
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
    /// For a run with cases wrong, each result bit, lowest first, with how
    /// often it came out on and was expected on, so a bit stuck off, stuck
    /// on, or inverted shows as such. Empty for a pass, and for a test
    /// that doesn't count them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bit_counts: Vec<BitCount>,
    /// What would make every case right, when one change does, such as
    /// flipping carry in.
    pub hint: Option<String>,
    /// The lines found to read inverted, powered meaning 0, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inverted: Vec<String>,
    /// What set apart the reading of the build its cases came out best
    /// under, which `pattern` names, for a test whose readings mean more
    /// than their names, such as an adder's subtracting. `None` for any
    /// other test.
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
            _ => AdderVariant::default(),
        }
    }

    /// Whether a greater-than comparator's cases read its operands signed.
    pub fn signed(&self) -> bool {
        matches!(self.reading, Some(Reading::GreaterThan { signed: true }))
    }

    /// How many of A's bits turn on a threshold, as its cases found.
    pub fn at_least(&self) -> Option<i64> {
        match self.reading {
            Some(Reading::Threshold { at_least }) => Some(at_least.into()),
            _ => None,
        }
    }

    /// Whether a register's reads show its cells inverted.
    pub fn reads_inverted(&self) -> bool {
        matches!(self.reading, Some(Reading::Register { inverted: true }))
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

/// One result bit over every case a run tried: its name, as `wrong_bits`
/// gives it, how many cases it was wrong in, and how many it came out on
/// and was expected on.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BitCount {
    pub name: String,
    pub wrong: u64,
    pub got_on: u64,
    pub expected_on: u64,
}

impl BitCount {
    /// What the bit did, as a few words, when its cases show a pattern:
    /// `never on`, `always on`, or `inverted`; `None` when it is right, or
    /// wrong only sometimes.
    pub fn pattern(&self, cases: u64) -> Option<&'static str> {
        if self.wrong == 0 {
            None
        } else if self.wrong == cases {
            Some("inverted")
        } else if self.got_on == 0 {
            Some("never on")
        } else if self.got_on == cases {
            Some("always on")
        } else {
            None
        }
    }
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

/// One way an adder's lines may read: as an add, or, B inverted and its
/// carries borrows, a subtract, A − B less its borrow in; and with its
/// carry in inverted, as a torch on that line gives. Inverting a line does
/// not change what a build is, so a subtractor is an adder, its B
/// inverted. An adder's sum inverted is an inverted adder, a category of
/// its own, whose carry out may read either way.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each a line read one way or the other"
)]
pub struct AdderVariant {
    pub subtract: bool,
    pub carry_in: bool,
    /// Whether its sum reads inverted, as an inverted adder's does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sum: bool,
    /// Whether its carry out reads inverted, as a carry-cancel adder's
    /// may, a tick sooner than inverting it again, or an inverted adder's
    /// with its sum.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub carry_out: bool,
}

/// Which way an incrementer counts, which names its category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Counts {
    /// Up, by 1 or by its control line: an incrementer.
    Up,
    /// Down, by 1 or by its control line: a decrementer.
    Down,
    /// Up one way of its control line and down the other: an inc/dec.
    Either,
}

/// How an adder makes its carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Design {
    /// The design's name, as stored: `ripple_carry`, `carry_cancel`,
    /// `carry_lookahead`, `carry_look_everywhere`, `hybrid`, or `other`.
    pub name: String,
    /// A letter per carry, from the carry into bit 1 up to the carry out.
    pub signature: String,
    /// How many components are lookahead terms.
    pub products: usize,
    /// How many bits each group holds, when it has more than one group
    /// and they are alike but for the lowest and highest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<usize>,
    /// Each group's lookahead terms: how many a component holds, of all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<Terms>,
}

impl Design {
    /// The name a report gives an adder whose carries name no design, as
    /// one with a carry no component holds; a build of it is given none.
    pub const OTHER: &'static str = "other";

    /// The design a build of it is given, as its category lists them;
    /// `None` for [`Design::OTHER`], as its carries name none.
    pub fn given(&self) -> Option<&str> {
        (self.name != Self::OTHER).then_some(self.name.as_str())
    }
}

/// A group's lookahead terms: how many have a component of their own, of
/// how many it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terms {
    pub found: usize,
    pub all: usize,
}
