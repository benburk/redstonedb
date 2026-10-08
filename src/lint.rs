//! Checks a schematic against `conventions.md`.

use std::{collections::BTreeMap, fmt};

use crate::{
    interface,
    schem::{self, Blocks, DIRECTIONS, Pos},
};

/// The block palette from `conventions.md`, besides the input and output
/// colours: white for everything that is neither.
const PALETTE: [&str; 2] = ["white_concrete", "white_stained_glass"];

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

/// Redstone components and the other blocks every build may use. Besides
/// dust, torches, and the like, this keeps the blocks a comparator reads,
/// such as furnaces, cauldrons, and cakes, since a build uses them for
/// their signal. Every kind of button is allowed as well.
const COMPONENTS: [&str; 49] = [
    "air",
    "redstone_wire",
    "redstone_torch",
    "redstone_wall_torch",
    "repeater",
    "comparator",
    "lever",
    "oak_sign",
    "oak_wall_sign",
    "piston",
    "sticky_piston",
    "piston_head",
    "observer",
    "redstone_block",
    "redstone_lamp",
    "hopper",
    "target",
    "note_block",
    "dropper",
    "dispenser",
    "daylight_detector",
    "tripwire_hook",
    "tripwire",
    "stone_pressure_plate",
    "light_weighted_pressure_plate",
    "heavy_weighted_pressure_plate",
    "furnace",
    "blast_furnace",
    "smoker",
    "chest",
    "trapped_chest",
    "barrel",
    "cauldron",
    "water_cauldron",
    "lava_cauldron",
    "powder_snow_cauldron",
    "cake",
    "composter",
    "jukebox",
    "lectern",
    "brewing_stand",
    "beehive",
    "bee_nest",
    "respawn_anchor",
    "end_portal_frame",
    "chiseled_bookshelf",
    "crafter",
    "sculk_sensor",
    "calibrated_sculk_sensor",
];

/// Blocks a lever must not touch, since it would power them directly.
const POWERABLE: [&str; 9] = [
    "redstone_wire",
    "redstone_torch",
    "redstone_wall_torch",
    "repeater",
    "comparator",
    "redstone_lamp",
    "piston",
    "sticky_piston",
    "observer",
];

/// One broken convention, at a position in the file when it has one.
#[derive(Clone)]
pub struct Violation {
    pub pos: Option<Pos>,
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pos {
            Some([x, y, z]) => write!(f, "({x}, {y}, {z}): {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

pub fn check(blocks: &Blocks) -> Vec<Violation> {
    let mut found = Vec::new();
    palette(blocks, &mut found);
    levers(blocks, &mut found);
    inputs(blocks, &mut found);
    names(blocks, &mut found);
    found
}

fn say(found: &mut Vec<Violation>, pos: Option<Pos>, message: String) {
    found.push(Violation { pos, message });
}

fn palette(blocks: &Blocks, found: &mut Vec<Violation>) {
    let mut outside: BTreeMap<&str, (Pos, usize)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = short(state);
        if !PALETTE.contains(&name)
            && !is_input(name)
            && !is_output(name)
            && !COMPONENTS.contains(&name)
            && !name.ends_with("_button")
        {
            outside.entry(name).or_insert((pos, 0)).1 += 1;
        }
    }
    for (name, (pos, count)) in outside {
        let name = name.replace('_', " ");
        let message = if count == 1 {
            format!("{name} is not in the block palette")
        } else {
            format!("{name} is not in the block palette ({count} blocks)")
        };
        say(found, Some(pos), message);
    }
}

fn levers(blocks: &Blocks, found: &mut Vec<Violation>) {
    for lever in blocks.switches() {
        let what = if lever.button { "button" } else { "lever" };
        let block = short(blocks.get(lever.block));
        if !is_input(block) {
            let block = block.replace('_', " ");
            say(
                found,
                Some(lever.pos),
                format!(
                    "{what} is on {block}; an input is a {what} on lime, light \
                     blue, cyan, blue, or magenta concrete"
                ),
            );
        } else if lever.button && block != format!("{CONTROL}_concrete") {
            say(
                found,
                Some(lever.pos),
                "button on an operand bit; only control lines take a button, \
                 so use a lever"
                    .to_owned(),
            );
        }
        // A's levers face south, so builds line up; the other operands'
        // may be on any side, but not on top or underneath.
        let operand = block
            .strip_suffix("_concrete")
            .is_some_and(|colour| INPUT_COLOURS.contains(&colour));
        let primary = block == format!("{}_concrete", INPUT_COLOURS[0]);
        if primary && lever.side != "south" {
            say(
                found,
                Some(lever.pos),
                format!(
                    "A's lever is on the {} side of its block, not south",
                    lever.side
                ),
            );
        } else if operand && matches!(lever.side.as_str(), "up" | "down") {
            say(
                found,
                Some(lever.pos),
                format!(
                    "operand lever is on the {} of its block; it goes on a \
                     side, north, south, east, or west",
                    if lever.side == "up" { "top" } else { "bottom" }
                ),
            );
        }
        for direction in DIRECTIONS {
            let next = schem::step(lever.pos, direction);
            let touched = short(blocks.get(next));
            if next != lever.block && POWERABLE.contains(&touched) {
                let touched = touched.replace('_', " ");
                say(
                    found,
                    Some(lever.pos),
                    format!(
                        "{what} touches {touched}, which it would power \
                         directly"
                    ),
                );
            }
        }
    }
}

fn inputs(blocks: &Blocks, found: &mut Vec<Violation>) {
    let levers = blocks.switches();
    for (pos, state) in blocks.iter() {
        let name = short(state);
        if !is_input(name) {
            continue;
        }
        let switch = if name == format!("{CONTROL}_concrete") {
            "lever or button"
        } else {
            "lever"
        };
        match levers.iter().filter(|lever| lever.block == pos).count() {
            1 => {}
            0 => say(found, Some(pos), format!("input block has no {switch}")),
            n => say(
                found,
                Some(pos),
                format!("input block has {n} levers and buttons"),
            ),
        }
    }
}

/// Signs name the interface: each control and status line by its own sign,
/// and each bus colour by a sign on any of its blocks, so a bus colour
/// takes one name. A status name belongs on a pink status line, and a name
/// on a block that is neither an input nor an output names nothing.
fn names(blocks: &Blocks, found: &mut Vec<Violation>) {
    let signs = blocks.sign_texts();
    let held: std::collections::HashSet<Pos> =
        blocks.switches().iter().map(|lever| lever.block).collect();
    let mut buses: BTreeMap<&str, (Pos, Vec<String>)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let block = short(state);
        let name = signs.get(&pos).and_then(|l| interface::sign_name(l));
        let line = block == format!("{CONTROL}_concrete")
            || output_colour(block) == Some(STATUS);
        if line {
            if name.is_none() {
                let what = if is_input(block) { "control" } else { "status" };
                say(
                    found,
                    Some(pos),
                    format!("{what} line has no sign naming it"),
                );
            }
            continue;
        }
        let Some(name) = name else { continue };
        let bus = (is_input(block) && held.contains(&pos)) || is_output(block);
        if !bus {
            say(
                found,
                Some(pos),
                format!(
                    "a sign names {}, which is neither an input nor an output",
                    block.replace('_', " ")
                ),
            );
            continue;
        }
        if is_output(block) && is_status_name(&name) {
            say(
                found,
                Some(pos),
                format!(
                    "{name} is a status output, so it is pink concrete or \
                     glass, not {}",
                    block.replace('_', " ")
                ),
            );
            continue;
        }
        let colour = output_colour(block)
            .or_else(|| block.strip_suffix("_concrete"))
            .unwrap_or(block);
        let entry = buses.entry(colour).or_insert((pos, Vec::new()));
        if !entry.1.contains(&name) {
            entry.1.push(name);
        }
    }
    for (colour, (pos, names)) in buses {
        if names.len() > 1 {
            say(
                found,
                Some(pos),
                format!(
                    "{} is named both {}; one colour is one bus, so give it \
                     one name",
                    colour.replace('_', " "),
                    names.join(" and ")
                ),
            );
        }
    }
}

/// Whether `state` is part of a build's interface: a lever or button, a
/// sign, an input block, or a torch on an input block. `support` is the block a
/// torch stands on or hangs from.
pub fn is_interface(state: &str, support: Option<&str>) -> bool {
    let name = short(state);
    name == "lever"
        || name.ends_with("_button")
        || name.ends_with("sign")
        || is_input(name)
        || (name.contains("torch")
            && support.is_some_and(|s| is_input(short(s))))
}

/// Whether standardizing keeps `name` as it is: a redstone component, an
/// input block, or an output block.
pub fn keeps(name: &str) -> bool {
    COMPONENTS.contains(&name) || name.ends_with("_button") || is_output(name)
}

/// Whether a block id without its namespace, such as `lime_concrete`, is
/// an input colour of concrete; it is an input once a lever is on it.
pub fn is_input(block: &str) -> bool {
    block.strip_suffix("_concrete").is_some_and(|colour| {
        colour == CONTROL || INPUT_COLOURS.contains(&colour)
    })
}

/// Whether a block id without its namespace marks an output: yellow,
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

/// The input colour of a solid coloured block, such as `cyan` for cyan
/// terracotta, which standardizing turns into that colour's concrete when
/// a lever is on it.
pub fn input_colour(block: &str) -> Option<&str> {
    ["_concrete_powder", "_concrete", "_terracotta", "_wool"]
        .iter()
        .find_map(|suffix| block.strip_suffix(suffix))
        .filter(|colour| *colour == CONTROL || INPUT_COLOURS.contains(colour))
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

/// A block state's id without its namespace, such as `lever`.
fn short(state: &str) -> &str {
    let id = schem::id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}
