//! How a build stands against its category's test, and how its operands
//! lie.

use std::{fmt, ops::Deref};

use crate::build::tag;

/// Why a build isn't verified as its category: a short sentence about it,
/// without its full stop, such as `Couldn't recognize inputs without
/// levers`, and whether its test ran and found it wrong, which makes it
/// failing rather than untested. It reads as its sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    pub why: String,
    pub wrong: bool,
}

impl Reason {
    /// A reason its test couldn't run, or wasn't.
    pub fn untested(why: impl Into<String>) -> Reason {
        Reason {
            why: why.into(),
            wrong: false,
        }
    }

    /// A reason its test gave, running and finding it wrong.
    pub fn failing(why: impl Into<String>) -> Reason {
        Reason {
            why: why.into(),
            wrong: true,
        }
    }
}

impl Deref for Reason {
    type Target = str;

    fn deref(&self) -> &str {
        &self.why
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.why)
    }
}

/// Why a hex adder or hex-to-binary converter whose inputs are levers is
/// not verified as one.
pub const NO_HEX_INPUTS: &str = "Couldn't find inputs taking hex digits";

/// Why an adder whose B is subtracted is not verified as an adder.
pub const SUBTRACTS: &str = "It subtracts";

/// Why an adder that adds is not verified as a subtractor.
pub const ADDS: &str = "It adds";

/// Why an inverted adder is not verified as an adder.
pub const INVERTS_SUM: &str = "Its sum reads inverted, as an inverted adder's";

/// Why an adder is not verified as an inverted adder.
pub const KEEPS_SUM: &str = "Its sum reads as it is, as an adder's";

/// Why a build its test finds broken is not verified, and a build that got
/// some cases of its test wrong was, before each said which.
pub const WRONG_OUTPUTS: &str = "Gives wrong outputs for some inputs";

/// Why a build whose inputs take a signal strength, rather than a lever's
/// on or off, is not recognized.
pub const WITHOUT_LEVERS: &str = "Couldn't recognize inputs without levers";

/// How a build stands against its category's test, which says whether it
/// works: passing, failing, not tested though its category has a test, or
/// of a category with none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    /// Its test got every case right.
    Passing,
    /// Its test ran and got cases wrong.
    Failing,
    /// Its test couldn't run on it, as when its inputs and outputs don't
    /// fit its category's, or never has; `unverified` says why.
    NotTested,
    /// Its category has no test, so nothing says it doesn't work.
    #[default]
    NoTest,
}

impl Status {
    /// How a build stands whose current file's stored result is `outcome`,
    /// as a report's `outcome` names it, `failed` or not, and which is
    /// stored as unverified for `reason`: its result, when it ran or broke,
    /// whatever reason was stored before; else the reason, one its test
    /// found it wrong for failing and any other not tested; else no test.
    /// A result skipped says no more than the reason, as an unverified
    /// build is only tested to say what keeps it from being recognized.
    pub fn of(
        outcome: Option<&str>,
        failed: bool,
        reason: Option<&Reason>,
    ) -> Status {
        match (outcome, reason) {
            (Some("ran"), _) if failed => Status::Failing,
            (Some("ran"), _) => Status::Passing,
            (Some("broken"), _) => Status::NotTested,
            (_, Some(reason)) if reason.wrong => Status::Failing,
            (_, Some(_)) => Status::NotTested,
            (_, None) => Status::NoTest,
        }
    }

    /// Its tag, if it isn't verified.
    pub fn unverified(self) -> Option<tag::Unverified> {
        match self {
            Status::Failing => Some(tag::Unverified::Failing),
            Status::NotTested => Some(tag::Unverified::Untested),
            Status::Passing | Status::NoTest => None,
        }
    }

    /// Its badge, such as `Passing`.
    pub fn label(self) -> &'static str {
        match self {
            Status::Passing => "Passing",
            Status::Failing => "Failing",
            Status::NotTested => "Not tested",
            Status::NoTest => "No test",
        }
    }
}

/// How a build's operands' bits lie, A's and B's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Operands {
    /// Each operand's bits together, as one bus above the other, as most
    /// builds' are, or no two operands.
    #[default]
    Split,
    /// Alternating along one row; see `Kept::interleaved`.
    Interleaved,
}
