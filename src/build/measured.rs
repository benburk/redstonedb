//! What a build's file and its simulation decide of its details, rather
//! than a person.

use super::{Build, Encoding, Logic, Orientation};

/// What a build's file and its simulation decide of its details, rather
/// than a person: its width, orientation, and encoding as its file shows
/// them; its latency, whether it is synchronized, a monostable's pulse, and
/// a threshold's count as the simulation measured them; and, for a category
/// whose details are all evaluated, its throughput, design, and whether it
/// holds state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub width: Option<i64>,
    pub orientation: Orientation,
    pub encoding: Option<Encoding>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    /// A monostable's pulse, in ticks.
    pub pulse: Option<i64>,
    /// A threshold's count of A's bits on that turns it on.
    pub at_least: Option<i64>,
    pub throughput: Option<i64>,
    /// One of its category's designs; `None` for none.
    pub design: Option<String>,
    /// Whether it holds state, where set on the build itself; `None`
    /// follows its category.
    pub logic: Option<Logic>,
}

impl Measured {
    /// Build `s`'s, as stored.
    pub fn of(s: &Build) -> Self {
        Measured {
            width: s.width,
            orientation: s.orientation,
            encoding: s.encoding,
            latency: s.latency,
            synchronized: s.synchronized,
            pulse: s.pulse,
            at_least: s.at_least,
            throughput: s.throughput,
            design: s.design.clone(),
            logic: s.own_logic,
        }
    }
}
