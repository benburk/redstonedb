//! Checks a schematic against `conventions.md`.

use std::{collections::BTreeMap, fmt};

use crate::{
    interface::{self, Interface},
    schem::{self, Blocks, DIRECTIONS, Pos},
};

/// The block palette from `conventions.md`.
const PALETTE: [&str; 7] = [
    "white_concrete",
    "white_stained_glass",
    "lime_concrete",
    "light_blue_concrete",
    "magenta_concrete",
    "yellow_concrete",
    "yellow_stained_glass",
];

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

/// The blocks marking A and B input bits.
const OPERANDS: [&str; 2] = ["lime_concrete", "light_blue_concrete"];

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
    outputs(blocks, &mut found);
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
                format!("{what} is on {block}, not an input block"),
            );
        } else if lever.button && OPERANDS.contains(&block) {
            say(
                found,
                Some(lever.pos),
                "button on an operand bit; only control lines take a button, \
                 so use a lever"
                    .to_owned(),
            );
        }
        if OPERANDS.contains(&block) && lever.side != "south" {
            say(
                found,
                Some(lever.pos),
                format!(
                    "operand lever is on the {} side of its block, not south",
                    lever.side
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
    let signs = blocks.sign_texts();
    let first_line = |pos: Pos| {
        signs
            .get(&pos)
            .and_then(|lines| lines.first())
            .map(|line| line.trim().to_owned())
            .unwrap_or_default()
    };
    for (pos, state) in blocks.iter() {
        let name = short(state);
        if !is_input(name) {
            continue;
        }
        let switch = if name == "magenta_concrete" {
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
        if name == "magenta_concrete" && first_line(pos).is_empty() {
            say(
                found,
                Some(pos),
                "control line has no sign naming it".to_owned(),
            );
        }
    }
}

fn outputs(blocks: &Blocks, found: &mut Vec<Violation>) {
    let Interface { outputs, .. } = interface::read(blocks);
    for port in &outputs {
        let pos = port.bits.first().map(|bit| bit.pos);
        if !matches!(
            port.block.as_str(),
            "yellow concrete" | "yellow stained glass"
        ) {
            say(
                found,
                pos,
                format!(
                    "{} is {}, not yellow concrete or yellow stained glass",
                    port.name, port.block
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
    COMPONENTS.contains(&name)
        || name.ends_with("_button")
        || is_input(name)
        || matches!(name, "yellow_concrete" | "yellow_stained_glass")
}

/// Whether a block id without its namespace, such as `lime_concrete`,
/// marks an input.
pub fn is_input(block: &str) -> bool {
    matches!(
        block,
        "lime_concrete" | "light_blue_concrete" | "magenta_concrete"
    )
}

/// A block state's id without its namespace, such as `lever`.
fn short(state: &str) -> &str {
    let id = schem::id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}
