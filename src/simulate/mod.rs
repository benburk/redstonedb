//! Runs a build in MCHPRS's redstone compiler, Redpiler, to check what it
//! computes.
//!
//! Redpiler models dust, torches, repeaters, comparators, levers, buttons,
//! and lamps, and the barrels, furnaces, and hoppers a comparator reads a
//! signal strength from, but not pistons, observers, or the order in which
//! the game updates blocks. A build using anything else is refused rather
//! than simulated wrongly.
//!
//! This is the engine: the compiled circuit, the bench that drives one
//! through a spec's cases, timing, throughput, and stacking, and what a
//! test found. Each category's test is with the categories; see
//! [`crate::category::Test`].

use std::fmt;

pub use crate::interface::problem::Rule;

/// Why a build cannot be what its category says, such as an adder whose
/// operands differ in width, with the rule that breaks when it is one
/// with a fix of its own. Unlike a block the simulator does not model,
/// this is the build's fault, so it counts as failing.
#[derive(Debug)]
pub(crate) struct Broken {
    pub(crate) rule: Option<Rule>,
    pub(crate) reason: String,
}

impl Broken {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Broken {
            rule: None,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Broken {}

/// Why a build could not be tested, though it may be what its category
/// says, when the reason breaks a rule with a fix of its own; any other
/// is a plain error.
#[derive(Debug)]
pub(crate) struct Untested {
    pub(crate) rule: Rule,
    pub(crate) reason: String,
}

impl fmt::Display for Untested {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Untested {}

/// The rule the error of a test that could not run says it breaks, if it
/// names one.
pub fn rule_of(err: &anyhow::Error) -> Option<Rule> {
    match err.downcast_ref::<Broken>() {
        Some(broken) => broken.rule,
        None => err.downcast_ref::<Untested>().map(|untested| untested.rule),
    }
}

/// Fails with `Broken` and a message formatted as `format!` does; after
/// `rule: R;`, as breaking rule `R`.
macro_rules! broken {
    (rule: $rule:expr; $($arg:tt)*) => {
        return Err($crate::simulate::Broken {
            rule: Some($rule),
            reason: format!($($arg)*),
        }
        .into())
    };
    ($($arg:tt)*) => {
        return Err($crate::simulate::Broken::new(format!($($arg)*)).into())
    };
}
pub(crate) use broken;

/// Fails with `Untested`, as breaking `rule`, and a message formatted as
/// `format!` does.
macro_rules! untested {
    ($rule:expr; $($arg:tt)*) => {
        return Err($crate::simulate::Untested {
            rule: $rule,
            reason: format!($($arg)*),
        }
        .into())
    };
}
pub(crate) use untested;

pub(crate) mod bench;
pub(crate) mod circuit;
mod report;
pub(crate) mod rng;
pub(crate) mod stacking;
mod strength;
pub(crate) mod throughput;
pub(crate) mod timing;
pub mod trace;
pub(crate) mod value;

pub use bench::mask;
pub(crate) use circuit::panic_message;
pub use circuit::{Circuit, Compiled, Link, compiled, settled};
pub use report::{
    Action, AdderVariant, BitCount, CellControls, Counts, Design, Reading,
    Report, Run, Signal, Terms, Wrong,
};
pub use stacking::{Copying, Group, Stacker, Stacking, Step, Way as StackWay};
pub use strength::Strength;
pub use throughput::throughput;
pub use timing::{
    BitTiming, Edge, Glitch, InputTiming, OutputTiming, Pulse, Start, Takes,
    Timing,
};
pub use value::Value;

/// The widest build tested, whose every value fits one [`Value`] with room
/// for a carry, or a product of two; see `Bench::fits`.
pub(crate) const MAX_WIDTH: usize = 64;

/// The widest stack of copies tested, narrower than [`MAX_WIDTH`], as each
/// copy more slows the run and a few copies show whether they stack.
pub(crate) const MAX_STACKED: usize = 32;
