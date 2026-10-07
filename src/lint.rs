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
    "glass",
    "lime_concrete",
    "light_blue_concrete",
    "magenta_concrete",
    "yellow_concrete",
];

/// Redstone components and the other blocks every build may use.
const COMPONENTS: [&str; 16] = [
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

/// One broken convention, at a position inside the corner markers when it
/// has one.
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
    markers(blocks, &mut found);
    shell(blocks, &mut found);
    palette(blocks, &mut found);
    levers(blocks, &mut found);
    inputs(blocks, &mut found);
    outputs(blocks, &mut found);
    found
}

/// Records a violation at a position in the file, converting it to one
/// inside the corner markers.
fn say(found: &mut Vec<Violation>, pos: Option<Pos>, message: String) {
    found.push(Violation {
        pos: pos.map(|p| p.map(|n| n - 1)),
        message,
    });
}

fn markers(blocks: &Blocks, found: &mut Vec<Violation>) {
    for pos in blocks.missing_markers() {
        say(found, Some(pos), "corner marker is not glass".to_owned());
    }
}

/// Only a build's interface reaches into the shell around it: levers,
/// signs, input blocks, and torches hanging on input blocks. Plain glass,
/// in the shell or inside, is for the corner markers alone.
fn shell(blocks: &Blocks, found: &mut Vec<Violation>) {
    let [w, h, l] = blocks.size;
    let markers: std::collections::HashSet<Pos> =
        blocks.marker_positions().into_iter().collect();
    for (pos @ [x, y, z], state) in blocks.iter() {
        let in_shell = x == 0
            || y == 0
            || z == 0
            || x == w - 1
            || y == h - 1
            || z == l - 1;
        let name = short(state);
        if markers.contains(&pos) || name == "air" {
            continue;
        }
        if !in_shell {
            if name == "glass" {
                say(
                    found,
                    Some(pos),
                    "glass inside the build; plain glass is for the corner \
                     markers, so use white stained glass"
                        .to_owned(),
                );
            }
            continue;
        }
        let allowed = name == "lever"
            || name.ends_with("sign")
            || is_input(name)
            || schem::torch_block(pos, state)
                .is_some_and(|block| is_input(short(blocks.get(block))));
        if allowed {
            continue;
        }
        let message = if name == "glass" {
            "glass in the shell outside the corner markers".to_owned()
        } else if name.contains("torch") {
            "torch in the shell is not on an input block".to_owned()
        } else {
            format!(
                "{} in the shell; only levers, signs, input blocks, and \
                 torches on input blocks belong there",
                name.replace('_', " ")
            )
        };
        say(found, Some(pos), message);
    }
}

fn palette(blocks: &Blocks, found: &mut Vec<Violation>) {
    let mut outside: BTreeMap<&str, (Pos, usize)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = short(state);
        if !PALETTE.contains(&name) && !COMPONENTS.contains(&name) {
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
    for lever in blocks.levers() {
        let block = short(blocks.get(lever.block));
        if !is_input(block) {
            let block = block.replace('_', " ");
            say(
                found,
                Some(lever.pos),
                format!("lever is on {block}, not an input block"),
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
                        "lever touches {touched}, which it would power directly"
                    ),
                );
            }
        }
    }
}

fn inputs(blocks: &Blocks, found: &mut Vec<Violation>) {
    let levers = blocks.levers();
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
        match levers.iter().filter(|lever| lever.block == pos).count() {
            1 => {}
            0 => say(found, Some(pos), "input block has no lever".to_owned()),
            n => say(found, Some(pos), format!("input block has {n} levers")),
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
        // Interface positions are already inside the markers.
        let pos = port.bits.first().map(|bit| bit.pos.map(|n| n + 1));
        if port.block != "yellow concrete" {
            say(
                found,
                pos,
                format!("{} is {}, not yellow concrete", port.name, port.block),
            );
        }
    }
}

fn is_input(block: &str) -> bool {
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
