//! What a category reads from a build's file in place of what was
//! typed: its width, the way its bits run, its encoding, and its flags.

use crate::{
    category::{self, Kind, Lines},
    interface::{self, Encoding, Interface, Orientation, Port},
    schem::Blocks,
};

/// What a build's file says about itself: its inputs and outputs, named as
/// its category names them.
#[derive(Default)]
pub struct Analysis {
    pub interface: Interface,
}

impl Analysis {
    /// `blocks` read as a build of the category named `category`.
    pub fn of(blocks: &Blocks, category: &str) -> Self {
        let mut interface = interface::read(blocks);
        interface::name_lines(
            category::behaviour(category).lines,
            blocks,
            &mut interface,
        );
        Analysis { interface }
    }
}

/// What a category that reads its builds' files, as
/// `category::Behaviour::reads` says, takes from them in place of
/// typed details.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FromFile {
    /// A's bit count, four for each block of a signal-strength input.
    pub width: Option<i64>,
    /// The way A's bits run.
    pub orientation: Option<Orientation>,
    /// Hex when A takes a signal strength, a digit per block, else binary.
    pub encoding: Option<Encoding>,
}

/// What `category` reads from a build with `interface`: nothing for a
/// category that reads nothing from its file; see [`read_from_file`].
pub fn derived(category: &str, interface: &Interface) -> FromFile {
    match category::behaviour(category).reads {
        category::Reads::Output => read_from_output(interface),
        category::Reads::InputA => read_from_file(interface),
        category::Reads::Nothing => FromFile::default(),
    }
}

/// What a counter or shift register reads from a build with `interface`:
/// its width and the way its bits run from its widest output, the value
/// it holds.
fn read_from_output(interface: &Interface) -> FromFile {
    let held = interface
        .outputs
        .iter()
        .filter(|port| !port.status())
        .max_by_key(|port| port.bits.len());
    match held {
        Some(port) => FromFile {
            width: i64::try_from(port.bits.len()).ok(),
            orientation: interface::orientation(port),
            encoding: Some(Encoding::Binary),
        },
        None => FromFile::default(),
    }
}

/// What a category that reads its builds' files, such as adder, reads
/// from one with `interface`: its width, orientation, and encoding from
/// its A, the unsigned first operand; nothing without one.
pub fn read_from_file(interface: &Interface) -> FromFile {
    let a = interface
        .inputs
        .iter()
        .find(|port| !port.labelled && port.a());
    match a {
        Some(a) => FromFile {
            width: i64::try_from(
                a.bits.len() * if a.signal_strength { 4 } else { 1 },
            )
            .ok(),
            orientation: interface::orientation(a),
            encoding: Some(if a.signal_strength {
                Encoding::Hex
            } else {
                Encoding::Binary
            }),
        },
        None => FromFile::default(),
    }
}

/// The width `category` reads from a build with `interface`; see
/// `derived`.
pub fn derived_width(category: &str, interface: &Interface) -> Option<i64> {
    derived(category, interface).width
}

/// A multiplier's flags, from its ports: `fma` for a third operand, as a
/// fused multiply-add's, and `upper_product` for outputs holding more bits
/// than its wider operand, the upper half of the product as well.
fn multiplier_flags(interface: &Interface) -> Vec<String> {
    let operands: Vec<usize> = interface
        .inputs
        .iter()
        .filter(|port| !port.control())
        .map(|port| port.bits.len())
        .collect();
    let product: usize = interface
        .outputs
        .iter()
        .filter(|port| !port.status())
        .map(|port| port.bits.len())
        .sum();
    let mut flags = Vec::new();
    if product > operands.iter().copied().max().unwrap_or(0) {
        flags.push("upper_product".to_owned());
    }
    if operands.len() > 2 {
        flags.push("fma".to_owned());
    }
    flags
}

/// The flags a category takes from its builds' files rather than their
/// details, as `category::Behaviour::flags_from_file` says: one for a
/// control line and one for a status line, named as its lone lines are, such as an adder's `carry_in` and `carry_out` or a
/// subtractor's `borrow_in` and `borrow_out`; a multiplier's from its
/// operands and product. `None` for other categories.
pub fn derived_flags(
    category: &str,
    interface: &Interface,
) -> Option<Vec<String>> {
    let behaviour = category::behaviour(category);
    if !behaviour.flags_from_file {
        return None;
    }
    if Kind::named(category) == Some(Kind::Multiplier) {
        return Some(multiplier_flags(interface));
    }
    let Lines::Lone {
        control, status, ..
    } = behaviour.lines
    else {
        return None;
    };
    let flags = [
        (interface.inputs.iter().any(Port::control), control),
        (interface.outputs.iter().any(Port::status), status),
    ];
    Some(
        flags
            .into_iter()
            .filter(|(has, _)| *has)
            .map(|(_, line)| line.replace(' ', "_"))
            .collect(),
    )
}
