//! A build's specs: the facts it is named, searched, and ranked by.

use super::{Encoding, Logic, Orientation};
use crate::category::Measure;

/// A build's specs: its width, how its bits run and are written, its
/// timing, and its design, as its file and its simulation decide them, or,
/// where they decide none, as its author gave them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Specs {
    pub width: Option<i64>,
    pub orientation: Orientation,
    /// `None` for a category off the list, such as a piston door.
    pub encoding: Option<Encoding>,
    pub latency: Option<i64>,
    /// Whether every output bit settles on the same tick.
    pub synchronized: Option<bool>,
    /// The fewest ticks from one input to the next that still gives right
    /// outputs; for a serial build, from one bit to the next.
    pub throughput: Option<i64>,
    /// For a threshold, the fewest inputs on that turn it on.
    pub at_least: Option<i64>,
    /// For a monostable, the ticks its pulse lasts.
    pub pulse: Option<i64>,
    /// For a memory, how many bytes it holds.
    pub bytes: Option<i64>,
    /// The design within its category, such as `carry_lookahead`; `None`
    /// for none.
    pub design: Option<String>,
    /// Whether it holds state, where the build says so itself; `None`
    /// follows its category's, see [`super::Build::logic`].
    pub logic: Option<Logic>,
}

impl Specs {
    /// Its value of `measure`, when it has one.
    pub fn of(&self, measure: Measure) -> Option<i64> {
        match measure {
            Measure::Width => self.width,
            Measure::Bytes => self.bytes,
            Measure::Throughput => self.throughput,
            Measure::AtLeast => self.at_least,
            Measure::Pulse => self.pulse,
        }
    }
}
