//! The rule the site read inputs and outputs by before it read them by
//! the blocks themselves, kept only to port builds saved under it, as
//! `rsdb convert` and `rsdb outputs` do: an output was a lone block of a
//! kind marking outputs, read through the dust, repeater, or comparator on
//! it, and a signal-strength input a block of an input colour with dust on
//! top. Nothing that recognizes a build reads by it; see
//! [`crate::recognize::inspect`] for the rule it does.

use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap, HashSet},
};

use crate::{
    recognize::{
        colour::{self, CONTROL, INPUT_COLOURS, OUTPUT_COLOURS, STATUS},
        inspect::{self, Problem, Report},
        interface::{self, Feed, Interface},
        repeater_outputs,
    },
    schem::{
        self, Blocks, Direction, Pos, block,
        lines::{Role, Roles},
    },
};

thread_local! {
    /// Whether this thread reads lines by the old rule, inside
    /// [`reading`].
    static OLD: Cell<bool> = const { Cell::new(false) };
}

/// What `run` gives with every read of a build's lines,
/// [`interface::read`] and [`interface::feeds`], by the old rule, as
/// [`read`] reads them: how `rsdb convert` tests a build saved under it,
/// and its port with its taps coloured as its outputs were, by the
/// category's own test.
pub fn reading<T>(run: impl FnOnce() -> T) -> T {
    let before = OLD.replace(true);
    let result = run();
    OLD.set(before);
    result
}

/// Whether this thread reads lines by the old rule; see [`reading`].
pub(crate) fn old() -> bool {
    OLD.get()
}

/// Which kinds of block marked a build's outputs, each only where every
/// block of it is alone with something reading it on top.
#[derive(Clone, Copy, Debug)]
pub enum Marks<'a> {
    /// Any kind that could, as [`could_mark`] says.
    Any,
    /// Only the kinds named, such as `yellow_concrete`.
    Kinds(&'a [&'a str]),
}

impl Marks<'_> {
    /// Whether a block of kind `name` may mark an output.
    fn takes(&self, name: &str) -> bool {
        match self {
            Marks::Any => could_mark(name),
            Marks::Kinds(kinds) => kinds.contains(&name),
        }
    }
}

/// What an uploaded build's outputs were read as: any kind that could mark
/// one.
pub const UPLOAD: Marks<'static> = Marks::Any;

/// The blocks the site marked outputs with in the files it kept: each
/// output colour and the status colour, see [`is_output`], of
/// concrete or stained glass.
pub const SITE: Marks<'static> = Marks::Kinds(&[
    "yellow_concrete",
    "yellow_stained_glass",
    "orange_concrete",
    "orange_stained_glass",
    "brown_concrete",
    "brown_stained_glass",
    "pink_concrete",
    "pink_stained_glass",
]);

/// The check by function of a build saved under the old rule: its inputs
/// as [`inspect::inspect`] finds them, and its outputs the blocks of the
/// kinds `marks` takes, each with dust, a repeater, or a comparator on top
/// reading it out, and no block of its own kind touching it, even
/// diagonally, of a kind every block of which is so. An input's block,
/// under its switch, is never one.
pub fn inspect(blocks: &Blocks, marks: Marks) -> Report {
    let mut report = inspect::inspect(blocks);
    report.errors.retain(|problem| {
        !matches!(problem.rule, "no outputs" | "output at the bottom")
    });
    let found: Vec<Pos> = kinds(blocks, |name| marks.takes(name))
        .into_values()
        .filter(|members| members.iter().all(|&(_, could)| could))
        .flatten()
        .map(|(pos, _)| pos)
        .collect();
    if found.is_empty() {
        report.errors.push(Problem {
            rule: "no outputs",
            at: Vec::new(),
            message: "No outputs".to_owned(),
        });
    }
    report.outputs =
        inspect::runs(&found.iter().map(|&pos| (pos, ())).collect::<Vec<_>>());
    report
}

/// Each kind of block `wanted` takes but the inputs', with its blocks, and
/// whether each could mark an output: alone, with dust, a repeater, or a
/// comparator on.
fn kinds(
    blocks: &Blocks,
    wanted: impl Fn(&str) -> bool,
) -> BTreeMap<&str, Vec<(Pos, bool)>> {
    let switched: HashSet<Pos> =
        blocks.switches().iter().map(|s| s.block).collect();
    let mut kinds: BTreeMap<&str, Vec<(Pos, bool)>> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        if !wanted(name) || switched.contains(&pos) {
            continue;
        }
        let read = reads_out(blocks, pos);
        kinds
            .entry(name)
            .or_default()
            .push((pos, read && lone(blocks, pos, name)));
    }
    kinds
}

/// Whether what stands on the block at `pos` reads it out of the build:
/// dust, or a repeater or comparator but one powering a block in front of
/// it.
fn reads_out(blocks: &Blocks, pos: Pos) -> bool {
    let top = schem::step(pos, Direction::Up);
    match schem::short(blocks.get(top)) {
        "redstone_wire" => true,
        "repeater" | "comparator" => {
            let powers = Direction::facing(blocks.get(top)).is_some_and(|f| {
                block::solid(blocks.get(schem::step(top, f.opposite())))
            });
            !powers
        }
        _ => false,
    }
}

/// Whether a block of kind `name` could mark an output read as an
/// upload's: any full block that carries power but a redstone component,
/// or stained glass of any colour but white.
fn could_mark(name: &str) -> bool {
    if let Some(colour) = name.strip_suffix("_stained_glass") {
        return colour != "white";
    }
    !block::is_component(name)
        && !name.ends_with("glass")
        && block::solid(&format!("minecraft:{name}"))
}

/// Whether no block of kind `name` touches `pos`, even at a corner.
fn lone(blocks: &Blocks, pos: Pos, name: &str) -> bool {
    let near = -1..=1;
    !near.clone().any(|dx| {
        near.clone().any(|dy| {
            near.clone().any(|dz| {
                [dx, dy, dz] != [0, 0, 0]
                    && schem::short(blocks.get([
                        pos[0] + dx,
                        pos[1] + dy,
                        pos[2] + dz,
                    ])) == name
            })
        })
    })
}

/// The signal-strength inputs as the old rule read them: input-colour
/// concrete with no lever or button on it and dust on top, each with the
/// side its signal enters through.
pub fn dust_inputs(blocks: &Blocks) -> HashMap<Pos, Option<Direction>> {
    let switched: HashSet<Pos> = blocks
        .switches()
        .iter()
        .map(|switch| switch.block)
        .collect();
    blocks
        .iter()
        .filter(|&(pos, state)| {
            is_input(schem::short(state))
                && !switched.contains(&pos)
                && schem::short(blocks.get(schem::step(pos, Direction::Up)))
                    == "redstone_wire"
        })
        .map(|(pos, _)| {
            (pos, open_end(blocks, schem::step(pos, Direction::Up)))
        })
        .collect()
}

/// The side of the dust at `pos` that leads nowhere, where its driver
/// goes; `None` unless exactly one side does.
fn open_end(blocks: &Blocks, pos: Pos) -> Option<Direction> {
    let state = blocks.get(pos);
    let mut open = Direction::SIDES.into_iter().filter(|&side| {
        let next = schem::step(pos, side);
        schem::property(state, side.name()) == Some("side")
            && blocks.get(next) == schem::AIR
            && schem::short(blocks.get(schem::step(next, Direction::Down)))
                != "redstone_wire"
    });
    match (open.next(), open.next()) {
        (Some(side), None) => Some(side),
        _ => None,
    }
}

/// A build's lines as the old rule read them from its colours: its levers
/// and buttons, and [`dust_inputs`], grouped by the colour of their
/// blocks, and its outputs every block of an output colour.
pub fn read(blocks: &Blocks) -> Interface {
    let feeds = feeds(blocks);
    let outputs: Vec<Pos> = blocks
        .iter()
        .filter(|(pos, state)| {
            !feeds.contains_key(pos) && is_output(schem::short(state))
        })
        .map(|(pos, _)| pos)
        .collect();
    let roles = marked(blocks, feeds.keys().chain(&outputs).copied());
    interface::read_from(blocks, &feeds, outputs, &roles)
}

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
/// How `rsdb convert` carries an old build's roles to its port.
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
/// author, or the site colouring it, gave its operands, which a port
/// keeps where its outputs, and so [`colour_roles`], no longer read the
/// old colours. Only the order of operands probing finds interchangeable
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

/// Whether a block id without its namespace, such as `lime_concrete`, is
/// an input colour of concrete, which marked an input's role once a lever
/// was on it.
pub fn is_input(block: &str) -> bool {
    block.strip_suffix("_concrete").is_some_and(|colour| {
        colour == CONTROL || INPUT_COLOURS.contains(&colour)
    })
}

/// The input colour a block marked an input's role with, such as `lime`
/// for lime stained glass: concrete, or stained glass for a
/// signal-strength input's comparator standing on glass.
pub fn input_mark(block: &str) -> Option<&str> {
    block
        .strip_suffix("_concrete")
        .or_else(|| block.strip_suffix("_stained_glass"))
        .filter(|colour| *colour == CONTROL || INPUT_COLOURS.contains(colour))
}

/// Whether a block id without its namespace marked an output: yellow,
/// orange, brown, or pink concrete or stained glass.
pub fn is_output(block: &str) -> bool {
    output_colour(block).is_some()
}

/// The colour of an output block, such as `yellow` for yellow stained
/// glass; `None` for any other block.
pub fn output_colour(block: &str) -> Option<&str> {
    block
        .strip_suffix("_concrete")
        .or_else(|| block.strip_suffix("_stained_glass"))
        .filter(|colour| *colour == STATUS || OUTPUT_COLOURS.contains(colour))
}

/// Every input block the old rule read, with what feeds it: a lever or
/// button, or for [`dust_inputs`] the dust on top.
pub fn feeds(blocks: &Blocks) -> HashMap<Pos, Feed> {
    let mut feeds: HashMap<Pos, Feed> = dust_inputs(blocks)
        .into_iter()
        .map(|(pos, entry)| (pos, Feed::Strength { entry }))
        .collect();
    feeds.extend(
        blocks
            .switches()
            .into_iter()
            .map(|switch| (switch.block, Feed::Switch(switch))),
    );
    feeds
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many outputs a lone block of kind `block` with dust on top is,
    /// read as `marks` says.
    fn lone_outputs(block: &str, marks: Marks) -> usize {
        let states: HashMap<Pos, String> = [
            ([0, 0, 0], block),
            (
                [0, 1, 0],
                "redstone_wire[east=none,north=side,power=0,south=side,west=none]",
            ),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        inspect(&Blocks::from_grid([1, 2, 1], &states), marks)
            .outputs
            .len()
    }

    /// Read as the site kept them, only its output colours mark outputs;
    /// read as an upload, any lone block but plain or white glass, which
    /// holds dust up, and a component.
    #[test]
    fn the_old_rule_read_outputs_by_their_blocks_colours() {
        let Marks::Kinds(kinds) = SITE else {
            panic!("the site's marks are its colours");
        };
        for kind in kinds {
            assert!(is_output(kind), "{kind}");
            assert_eq!(lone_outputs(kind, SITE), 1, "{kind}");
        }
        for block in ["light_gray_stained_glass", "lime_concrete", "stone"] {
            assert_eq!(lone_outputs(block, SITE), 0, "{block}");
            assert_eq!(lone_outputs(block, UPLOAD), 1, "{block}");
        }
        for block in ["glass", "white_stained_glass", "target", "redstone_lamp"]
        {
            assert_eq!(lone_outputs(block, UPLOAD), 0, "{block}");
        }
    }
}
