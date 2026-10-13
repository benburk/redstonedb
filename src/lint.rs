//! Checks a schematic against `conventions.md`.

use std::{collections::BTreeMap, fmt};

use crate::{
    interface,
    schem::{self, Blocks, DIRECTIONS, Pos},
};

/// The block palette from `conventions.md`, besides the input and output
/// colours and gray: white for everything that is none of them.
const PALETTE: [&str; 2] = ["white_concrete", "white_stained_glass"];

/// The colour of logic only a stack's first copy keeps, such as what its
/// least significant bit alone needs.
const FIRST_COPY: &str = "gray";

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
/// such as barrels, since a build uses them for their signal; furnaces,
/// cauldrons, and cakes are swapped for barrels. Every kind of button is
/// allowed as well.
const COMPONENTS: [&str; 43] = [
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
    "blast_furnace",
    "smoker",
    "chest",
    "trapped_chest",
    "barrel",
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

/// Blocks a lever must not touch, since it would power them directly. A
/// torch is not one: only the block it hangs on turns it off.
const POWERABLE: [&str; 7] = [
    "redstone_wire",
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

///
/// A control or status line with no sign naming it breaks a convention,
/// except the only line of its colour, which an adder takes as its carry in
/// or carry out; whether a build may leave it unsigned depends on its
/// category, which `check_for` knows.
pub fn check(blocks: &Blocks) -> Vec<Violation> {
    let mut found = Vec::new();
    palette(blocks, &mut found);
    levers(blocks, &mut found);
    inputs(blocks, &mut found);
    names(blocks, &mut found);
    order_marks(blocks, &mut found);
    found.extend(
        unsigned(blocks)
            .into_iter()
            .filter(|(_, lone)| !lone)
            .map(|(violation, _)| violation),
    );
    found
}

/// The categories whose builds' lone control line and lone status line
/// need no sign, their roles being plain: an adder's carry in and carry
/// out.
pub const UNSIGNED_LINES: [&str; 1] = ["adder"];

/// The names an adder's control line, its carry in, may be signed with;
/// an unsigned one goes by the first.
pub const CARRY_IN: [&str; 2] = ["carry in", "cin"];

/// The names an adder's status line, its carry out, may be signed with;
/// an unsigned one goes by the first.
pub const CARRY_OUT: [&str; 2] = ["carry out", "cout"];

/// As `check`, for a build of `category`: one not in `UNSIGNED_LINES` must
/// sign even a lone control or status line.
pub fn check_for(blocks: &Blocks, category: &str) -> Vec<Violation> {
    let mut found = check(blocks);
    if !UNSIGNED_LINES.contains(&category) {
        found.extend(
            unsigned(blocks)
                .into_iter()
                .filter(|(_, lone)| *lone)
                .map(|(violation, _)| violation),
        );
    }
    found
}

/// Each control or status line with no sign naming it, and whether it is
/// the only line of its colour.
fn unsigned(blocks: &Blocks) -> Vec<(Violation, bool)> {
    let signs = blocks.sign_texts();
    // Lines named by one sign on their row.
    let rowed = interface::rowed(blocks);
    let mut lines: BTreeMap<bool, Vec<Pos>> = BTreeMap::new();
    let mut unsigned = Vec::new();
    for (pos, state) in blocks.iter() {
        let block = short(state);
        let control = block == format!("{CONTROL}_concrete");
        if !control && output_colour(block) != Some(STATUS) {
            continue;
        }
        lines.entry(control).or_default().push(pos);
        let named = signs.get(&pos).and_then(|l| interface::sign_name(l));
        if named.is_none() && !rowed.contains(&pos) {
            unsigned.push((pos, control));
        }
    }
    unsigned
        .into_iter()
        .map(|(pos, control)| {
            let what = if control { "control" } else { "status" };
            let violation = Violation {
                pos: Some(pos),
                message: format!(
                    "{what} line has no sign naming it: sign it, or sign one \
                     line of its row to name the row"
                ),
            };
            (violation, lines[&control].len() == 1)
        })
        .collect()
}

fn say(found: &mut Vec<Violation>, pos: Option<Pos>, message: String) {
    found.push(Violation { pos, message });
}

fn palette(blocks: &Blocks, found: &mut Vec<Violation>) {
    let mut outside: BTreeMap<&str, (Pos, usize)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = short(state);
        if !PALETTE.contains(&name)
            && !first_copy_only(name)
            && !is_input(name)
            && !is_output(name)
            && !COMPONENTS.contains(&name)
            && !name.ends_with("_button")
        {
            outside.entry(name).or_insert((pos, 0)).1 += 1;
        }
    }
    for (name, (pos, count)) in outside {
        let swapped = barrel_stands_in(name);
        let name = name.replace('_', " ");
        let mut message = if count == 1 {
            format!("{name} is not in the block palette")
        } else {
            format!("{name} is not in the block palette ({count} blocks)")
        };
        if swapped {
            message.push_str(
                "; a barrel gives a signal strength instead, and adding \
                 the file swaps it for one giving the same",
            );
        }
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

/// Each input block takes a lever, or for a control line a button, or for
/// a signal-strength input dust on top with an open end to drive it from.
/// One colour is one bus, so its blocks are all driven the same way.
fn inputs(blocks: &Blocks, found: &mut Vec<Violation>) {
    let levers = blocks.switches();
    let dust = interface::dust_inputs(blocks);
    // Each operand colour's first levered block and first dust-fed one.
    let mut kinds: BTreeMap<&str, (Option<Pos>, Option<Pos>)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = short(state);
        if !is_input(name) {
            continue;
        }
        let colour = name.strip_suffix("_concrete").unwrap_or(name);
        let kind = kinds.entry(colour).or_default();
        let switch = if colour == CONTROL {
            "lever or button"
        } else {
            "lever"
        };
        match levers.iter().filter(|lever| lever.block == pos).count() {
            1 => {
                kind.0.get_or_insert(pos);
            }
            0 => match dust.get(&pos) {
                None => say(
                    found,
                    Some(pos),
                    format!(
                        "input block has no {switch}, nor dust on top for a \
                         signal-strength input"
                    ),
                ),
                Some(None) => say(
                    found,
                    Some(pos),
                    "signal-strength input's dust has no one open end to \
                     drive it from"
                        .to_owned(),
                ),
                // The dust may enter from any side: the simulator drives
                // it from wherever there is room.
                Some(Some(_)) => {
                    kind.1.get_or_insert(pos);
                }
            },
            n => say(
                found,
                Some(pos),
                format!("input block has {n} levers and buttons"),
            ),
        }
    }
    for (colour, kind) in kinds {
        if colour != CONTROL
            && let (Some(_), Some(pos)) = kind
        {
            say(
                found,
                Some(pos),
                format!(
                    "{} has both levers and signal-strength dust; one colour \
                     is one bus, driven one way",
                    colour.replace('_', " ")
                ),
            );
        }
    }
}

/// Signs name the interface: each control and status line by its own sign,
/// or a row of them by one sign on the row, and each bus colour by a sign on any of its blocks, so a bus colour
/// takes one name. A status name belongs on a pink status line, and a name
/// on a block that is neither an input nor an output names nothing.
/// An `lsb` or `msb` sign belongs on an end of its bus, where it says which
/// end is least significant.
fn order_marks(blocks: &Blocks, found: &mut Vec<Violation>) {
    let interface::Interface { inputs, outputs } = interface::read(blocks);
    for port in inputs.iter().chain(&outputs) {
        let middle = port.bits.iter().skip(1).rev().skip(1);
        for bit in middle {
            if let Some(mark) = interface::order_mark(blocks, bit.pos) {
                say(
                    found,
                    Some(bit.pos),
                    format!(
                        "an {mark} sign is on a middle bit of {}; it marks an \
                         end of the bus",
                        port.name
                    ),
                );
            }
        }
    }
}

fn names(blocks: &Blocks, found: &mut Vec<Violation>) {
    let signs = blocks.sign_texts();
    let held = interface::feeds(blocks);
    let mut buses: BTreeMap<&str, (Pos, Vec<String>)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let block = short(state);
        let name = signs.get(&pos).and_then(|l| interface::sign_name(l));
        let line = block == format!("{CONTROL}_concrete")
            || output_colour(block) == Some(STATUS);
        // Unsigned lines are `unsigned`'s to report.
        if line {
            continue;
        }
        let Some(name) = name else { continue };
        let bus =
            (is_input(block) && held.contains_key(&pos)) || is_output(block);
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
/// output block, or a block only a stack's first copy keeps.
pub fn keeps(name: &str) -> bool {
    COMPONENTS.contains(&name)
        || name.ends_with("_button")
        || is_output(name)
        || first_copy_only(name)
}

/// Whether a block id without its namespace is one a barrel stands in for:
/// a furnace, cauldron, or cake, which a comparator reads a signal strength
/// from.
pub fn barrel_stands_in(name: &str) -> bool {
    name == "furnace"
        || name.ends_with("cauldron")
        || name == "cake"
        || name.ends_with("candle_cake")
}

/// Whether a block id without its namespace is one only a stack's first
/// copy keeps, with what rests on it: gray concrete or stained glass. A
/// barrel on gray gives a signal strength the first copy alone needs; any
/// other repeats in every copy.
pub fn first_copy_only(name: &str) -> bool {
    name.strip_suffix("_concrete")
        .or_else(|| name.strip_suffix("_stained_glass"))
        == Some(FIRST_COPY)
}

/// Whether a block id without its namespace, such as `lime_concrete`, is
/// an input colour of concrete; it is an input once a lever is on it, or
/// dust for a signal-strength input.
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
