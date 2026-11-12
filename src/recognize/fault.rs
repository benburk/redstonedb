//! The reasons a build is stored as unverified for, as
//! `details::unrecognized` gives them.

/// Whether `reason` says a build gives wrong outputs, so it is failing,
/// rather than that its inputs, outputs, or parts don't fit its
/// category's, which leaves it untested.
pub fn wrong_outputs(reason: &str) -> bool {
    let reason = reason.trim();
    reason == crate::recognize::fault::WRONG_OUTPUTS
        || reason.contains("cases wrong")
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
