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
        colour,
        inspect::{self, Problem, Report},
        interface::{self, Feed, Interface},
    },
    schem::{self, Blocks, Direction, Pos, block},
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
/// output colour and the status colour, see [`colour::is_output`], of
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
            colour::is_input(schem::short(state))
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
            !feeds.contains_key(pos) && colour::is_output(schem::short(state))
        })
        .map(|(pos, _)| pos)
        .collect();
    interface::read_from(blocks, &feeds, outputs)
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
            assert!(colour::is_output(kind), "{kind}");
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
