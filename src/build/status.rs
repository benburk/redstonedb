//! How a build stands against its category's test, and how its operands
//! lie.

use crate::models::tag;

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
    /// whatever reason was stored before; else the reason, wrong outputs
    /// failing and any other not tested; else no test. A result skipped
    /// says no more than the reason, as an unverified build is only
    /// tested to say what keeps it from being recognized.
    pub fn of(
        outcome: Option<&str>,
        failed: bool,
        reason: Option<&str>,
    ) -> Status {
        match (outcome, reason) {
            (Some("ran"), _) if failed => Status::Failing,
            (Some("ran"), _) => Status::Passing,
            (Some("broken"), _) => Status::NotTested,
            (_, Some(reason))
                if crate::recognize::fault::wrong_outputs(reason) =>
            {
                Status::Failing
            }
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
