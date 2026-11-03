//! The colours the site shows a build's interface in, by the roles probing
//! finds, see [`crate::schem::lines`]: inputs on cool concrete, outputs on
//! warm concrete or glass. A theme shows them in its own; see
//! [`crate::recognize::palette`]. Colours are only shown: no block's colour
//! says what it is for, nor which blocks are inputs and outputs; see
//! [`crate::recognize::inspect`]. The names lines go by are here too.

use crate::schem::lines::Role;

/// The operand colours of concrete, in order: lime for A, light blue for B,
/// cyan for C, blue for D. Lightest first, going from green to blue.
pub const INPUT_COLOURS: [&str; 4] = ["lime", "light_blue", "cyan", "blue"];

/// The colour of a control line, an input named on its own, such as
/// `carry in`.
pub const CONTROL: &str = "magenta";

/// The output bus colours of concrete or stained glass, in order, the first
/// holding the lowest bits when they make one value: yellow, orange, brown.
/// Lightest first, going from yellow to brown.
pub const OUTPUT_COLOURS: [&str; 3] = ["yellow", "orange", "brown"];

/// The colour of a status line, an output named on its own, such as
/// `carry out`.
pub const STATUS: &str = "pink";

/// The colour the site shows a block of `role` in: an operand's, past D
/// the last; an output bus's, past the third the last; or a control or
/// status line's.
pub fn shown(role: Role) -> &'static str {
    let last = |colours: &[&'static str], n: u8| {
        colours[usize::from(n).min(colours.len() - 1)]
    };
    match role {
        Role::Operand(n) => last(&INPUT_COLOURS, n),
        Role::Control => CONTROL,
        Role::Output(n) => last(&OUTPUT_COLOURS, n),
        Role::Status => STATUS,
    }
}

/// The names an adder's control line, its carry in, may be signed with;
/// an unsigned one goes by the first.
pub const CARRY_IN: [&str; 2] = ["carry in", "cin"];

/// The names an adder's status line, its carry out, may be signed with;
/// an unsigned one goes by the first.
pub const CARRY_OUT: [&str; 2] = ["carry out", "cout"];

/// The names a subtractor's control line, its borrow in, may be signed
/// with; an unsigned one goes by the first.
pub const BORROW_IN: [&str; 2] = ["borrow in", "bin"];

/// The names a subtractor's status line, its borrow out, may be signed
/// with; an unsigned one goes by the first.
pub const BORROW_OUT: [&str; 2] = ["borrow out", "bout"];

/// Whether a block id without its namespace is one a barrel stands in for:
/// a furnace, cauldron, or cake, which a comparator reads a signal strength
/// from.
pub fn barrel_stands_in(name: &str) -> bool {
    name == "furnace"
        || name.ends_with("cauldron")
        || name == "cake"
        || name.ends_with("candle_cake")
}

/// Whether a sign's name marks a status output, such as `carry out`,
/// `cout`, or `overflow`: one line of its own, which is pink.
pub fn is_status_name(name: &str) -> bool {
    name.ends_with(" out")
        || matches!(
            name,
            "cout"
                | "bout"
                | "overflow"
                | "zero"
                | "negative"
                | "ready"
                | "done"
        )
}
