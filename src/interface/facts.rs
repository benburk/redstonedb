//! What a build's lines show of it: the way its bits run, and how its
//! numbers are written on them.

use super::{Interface, Port};

named_values! {
    /// The way a build's bits run.
    Orientation {
        Horizontal = "horz", "Horizontal";
        Vertical = "vert", "Vertical";
        Diagonal = "diag", "Diagonal";
    }
}

/// A build's bits run across unless it says otherwise.
impl Default for Orientation {
    fn default() -> Self {
        Orientation::Horizontal
    }
}

named_values! {
    /// How a build's numbers are written on its lines.
    Encoding {
        /// A bit per line, on levers.
        Binary = "bin", "Binary";
        /// A digit per line, as a signal strength.
        Hex = "hex", "Hex";
    }
}

/// The width and orientation the inputs suggest: the widest operand's bit
/// count, four for each block of a signal-strength input, and which way its
/// bits run. `None` where the inputs say nothing.
pub fn suggested(
    interface: &Interface,
) -> (Option<usize>, Option<Orientation>) {
    let bits = |port: &Port| {
        port.bits.len() * if port.signal_strength { 4 } else { 1 }
    };
    let widest = interface
        .inputs
        .iter()
        .filter(|port| !port.labelled)
        .max_by_key(|port| bits(port));
    let width = widest.map(bits);
    let orientation = widest.and_then(orientation);
    (width, orientation)
}

/// The way a bus runs, from how far it runs across and up. A bus that
/// mostly runs one way, such as a staggered horizontal one, counts as
/// running that way.
pub fn orientation(port: &Port) -> Option<Orientation> {
    let first = port.bits.first()?.pos;
    let last = port.bits.last()?.pos;
    let across = (last[0] - first[0]).abs() + (last[2] - first[2]).abs();
    let up = (last[1] - first[1]).abs();
    if across == 0 && up == 0 {
        None
    } else if across >= 2 * up {
        Some(Orientation::Horizontal)
    } else if up >= 2 * across {
        Some(Orientation::Vertical)
    } else {
        Some(Orientation::Diagonal)
    }
}
