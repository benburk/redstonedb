//! The roles the site's old colours mark on a build's blocks, as the site
//! coloured a build's lines before it kept their roles apart from its
//! blocks, and as the test fixtures are coloured: lime, light blue, cyan,
//! and blue its operands, magenta a control line, yellow, orange, and
//! brown its output buses, and pink a status line. An author's operand
//! colours are read too, as the order of operands probing finds
//! interchangeable. Lines themselves are read by the rule for them; see
//! [`crate::interface::inspect`].

use std::collections::HashSet;

use crate::{
    interface::{
        self,
        colour::{self, CONTROL, INPUT_COLOURS, OUTPUT_COLOURS, STATUS},
        repeater_outputs,
    },
    schem::{
        self, Blocks, Direction, Pos,
        lines::{Role, Roles},
    },
};

/// The roles the colours of the blocks at `positions` marked, as the site
/// coloured them before it kept roles apart from a build's blocks: lime,
/// light blue, cyan, and blue its operands, magenta a control line,
/// yellow, orange, and brown its output buses, and pink a status line.
/// A block of any other colour marks none.
fn marked(blocks: &Blocks, positions: impl IntoIterator<Item = Pos>) -> Roles {
    let mut roles = Roles::default();
    for pos in positions {
        if let Some(role) = mark(blocks.get(pos)) {
            roles.mark(role, [&pos]);
        }
    }
    roles
}

/// The role the colour of `state` marked; see [`marked`].
fn mark(state: &str) -> Option<Role> {
    let name = schem::short(state);
    let colour = name
        .strip_suffix("_concrete")
        .or_else(|| name.strip_suffix("_stained_glass"))?;
    let place = |colours: &[&str]| {
        colours
            .iter()
            .position(|c| *c == colour)
            .and_then(|n| u8::try_from(n).ok())
    };
    if colour == CONTROL {
        Some(Role::Control)
    } else if colour == STATUS {
        Some(Role::Status)
    } else if let Some(n) = place(&INPUT_COLOURS) {
        Some(Role::Operand(n))
    } else {
        place(&OUTPUT_COLOURS).map(Role::Output)
    }
}

/// The roles a file in the site's old colours marks on the blocks the rule
/// for inputs and outputs finds, as a stored build's file was coloured
/// before the site kept its roles apart, and as the test fixtures are:
/// each lever's or button's block and each block under a signal-strength
/// input's comparator by its input colour, and each block under an output
/// by its output colour. `None` for a file the site never coloured, none
/// of its signal-strength inputs or outputs on a block of a role colour.
pub fn colour_roles(blocks: &Blocks) -> Option<Roles> {
    let held = |at: Pos| at[1] > 0;
    let digits: Vec<Pos> = repeater_outputs::strength_inputs(blocks)
        .into_iter()
        .filter(|input| held(input.at))
        .map(|input| schem::step(input.at, Direction::Down))
        .collect();
    let outputs: Vec<Pos> = repeater_outputs::find(blocks)
        .into_iter()
        .filter(|output| held(output.at))
        .map(repeater_outputs::Output::block)
        .collect();
    let input = |pos: &Pos| mark(blocks.get(*pos)).is_some_and(Role::input);
    let output = |pos: &Pos| mark(blocks.get(*pos)).is_some_and(|r| !r.input());
    if !digits.iter().any(input) && !outputs.iter().any(output) {
        return None;
    }
    let levered: HashSet<Pos> = blocks
        .switches()
        .iter()
        .map(|switch| switch.block)
        .collect();
    let inputs = digits.into_iter().filter(input).chain(levered.clone());
    let mut roles = marked(blocks, inputs);
    roles.0.retain(|_, role| role.input());
    let outputs = outputs
        .into_iter()
        .filter(|pos| output(pos) && !levered.contains(pos));
    roles.0.extend(marked(blocks, outputs).0);
    // A status line older builds marked on an output bus's colour, its sign
    // naming a status output, as the site turned them pink.
    for (pos, lines) in interface::line_signs(blocks) {
        if roles.get(pos).is_some_and(|role| !role.input())
            && interface::sign_name(lines)
                .is_some_and(|name| colour::is_status_name(&name))
        {
            roles.mark(Role::Status, [&pos]);
        }
    }
    Some(roles)
}

/// The operands a file in the site's old colours marks, A lime, B light
/// blue, and on, by the block of each lever or button, and under each
/// signal-strength input's comparator, of an input colour: the order its
/// author, or the site colouring it, gave its operands, which a build
/// whose outputs, and so [`colour_roles`], no longer read the old colours
/// keeps. Only the order of operands probing finds interchangeable
/// is read from it, see [`crate::recognize::probe::Order`].
pub fn operand_marks(blocks: &Blocks) -> Roles {
    let digits = repeater_outputs::strength_inputs(blocks)
        .into_iter()
        .map(|input| schem::step(input.at, Direction::Down));
    let levered = blocks.switches().into_iter().map(|switch| switch.block);
    let mut roles = marked(blocks, digits.chain(levered));
    roles.0.retain(|_, role| matches!(role, Role::Operand(_)));
    roles
}

/// The file in `data` carrying the roles its colours mark, as
/// [`colour_roles`] reads them, in place of any it carried; as it was for
/// a file the site never coloured.
pub fn with_colour_roles(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    match colour_roles(&blocks) {
        Some(roles) => schem::lines::with(data, Some(&roles)),
        None => Ok(data.to_vec()),
    }
}
