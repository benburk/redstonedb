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

/// Why a build cannot be what its category says, such as an adder whose
/// operands differ in width. Unlike a block the simulator does not model,
/// this is the build's fault, so it counts as failing.
#[derive(Debug)]
pub(crate) struct Broken(pub(crate) String);

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
pub(crate) use broken;

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
