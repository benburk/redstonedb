//! Checks a schematic against `conventions.md`.

use std::{collections::BTreeMap, fmt};

use crate::{
    category, interface,
    schem::{self, Blocks, Direction, Pos},
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

/// Components no build may use: an observer, whose updates the simulator
/// does not model, and a button but a stone one, as a wooden one, which it
/// does not press, so a build with one could never be checked.
fn forbids(name: &str) -> bool {
    name == "observer" || name.ends_with("_button") && name != "stone_button"
}

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
/// or carry out, and an incrementer or decrementer as its own; whether a
/// build may leave it unsigned depends on its category, which `check_for`
/// knows.
pub fn check(blocks: &Blocks) -> Vec<Violation> {
    let mut found = Vec::new();
    palette(blocks, &mut found);
    forbidden(blocks, &mut found);
    levers(blocks, &mut found);
    inputs(blocks, &mut found);
    input_wiring(blocks, &mut found);
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

/// As `check`, for a build of `category`: one whose lines' roles are not
/// plain, as `category::Lines` says, must sign even a lone control or
/// status line.
pub fn check_for(blocks: &Blocks, category: &str) -> Vec<Violation> {
    let mut found = check(blocks);
    if !category::behaviour(category).lines.lone_unsigned() {
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
/// the only line of its colour. Unsigned control lines with buttons and
/// unsigned ones with levers, together, are a memory's writes and reads,
/// as a register's cells take them, and need no sign.
fn unsigned(blocks: &Blocks) -> Vec<(Violation, bool)> {
    let signs = blocks.sign_texts();
    // Lines named by one sign on their row.
    let rowed = interface::rowed(blocks);
    let mut lines: BTreeMap<bool, Vec<Pos>> = BTreeMap::new();
    let mut unsigned = Vec::new();
    for (pos, state) in blocks.iter() {
        let block = schem::short(state);
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
    let switches: BTreeMap<Pos, bool> = blocks
        .switches()
        .into_iter()
        .map(|switch| (switch.block, switch.button))
        .collect();
    let kinds: Vec<Option<bool>> = unsigned
        .iter()
        .filter(|(_, control)| *control)
        .map(|(pos, _)| switches.get(pos).copied())
        .collect();
    let writes_and_reads = kinds.contains(&Some(true))
        && kinds.contains(&Some(false))
        && !kinds.contains(&None);
    unsigned
        .into_iter()
        .filter(|&(_, control)| !(control && writes_and_reads))
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

fn forbidden(blocks: &Blocks, found: &mut Vec<Violation>) {
    let mut seen: BTreeMap<&str, (Pos, usize)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        if forbids(name) {
            seen.entry(name).or_insert((pos, 0)).1 += 1;
        }
    }
    for (name, (pos, count)) in seen {
        let blocks = if count == 1 {
            String::new()
        } else {
            format!(" ({count} blocks)")
        };
        let name = name.replace('_', " ");
        let a = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        say(
            found,
            Some(pos),
            format!(
                "{a} {name} is not allowed{blocks}, as the simulator can't run \
                 one, so the build could never be checked"
            ),
        );
    }
}

fn palette(blocks: &Blocks, found: &mut Vec<Violation>) {
    let mut outside: BTreeMap<&str, (Pos, usize)> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
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
        let block = schem::short(blocks.get(lever.block));
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
        // A's levers face south, so builds line up; the other inputs' may
        // be on any side, but not on top or underneath.
        let operand = block
            .strip_suffix("_concrete")
            .is_some_and(|colour| INPUT_COLOURS.contains(&colour));
        let input = is_input(block);
        let primary = block == format!("{}_concrete", INPUT_COLOURS[0]);
        if primary && lever.side != Direction::South {
            say(
                found,
                Some(lever.pos),
                format!(
                    "A's lever is on the {} side of its block, not south",
                    lever.side
                ),
            );
        } else if input && matches!(lever.side, Direction::Up | Direction::Down)
        {
            let who = if operand { "operand" } else { "control line's" };
            say(
                found,
                Some(lever.pos),
                format!(
                    "{who} {what} is on the {} of its block; it goes on a \
                     side, north, south, east, or west",
                    if lever.side == Direction::Up {
                        "top"
                    } else {
                        "bottom"
                    }
                ),
            );
        }
        for direction in Direction::ALL {
            let next = schem::step(lever.pos, direction);
            let touched = schem::short(blocks.get(next));
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

/// What touches an input block with a lever or button: its switch powers
/// it, and it feeds at most two dusts, on its other sides, so wiring
/// it up puts one driver into one wire. Dust on top or underneath, a second
/// dust, anything else that would power it, as a repeater or comparator
/// pointed into it or a torch beneath it, and any other component, such as
/// a redstone block beside it, break that. Torches hung on it or standing
/// on it, a target or redstone block on top, repeaters and comparators that read it, and other
/// inputs' levers and buttons, which power only their own blocks, are
/// fine.
fn input_wiring(blocks: &Blocks, found: &mut Vec<Violation>) {
    for switch in blocks.switches() {
        let pos = switch.block;
        if !is_input(schem::short(blocks.get(pos))) {
            continue;
        }
        let mut dust = 0;
        for direction in Direction::ALL {
            let next = schem::step(pos, direction);
            if next == switch.pos {
                continue;
            }
            let state = blocks.get(next);
            let name = schem::short(state);
            let pointed = || Direction::facing(state) == Some(direction);
            let problem = match name {
                "redstone_wire" => match direction {
                    Direction::Up => Some("has dust on top".to_owned()),
                    Direction::Down => Some("has dust under it".to_owned()),
                    _ => {
                        dust += 1;
                        None
                    }
                },
                "redstone_torch" | "redstone_wall_torch"
                    if direction == Direction::Down =>
                {
                    Some("has a torch under it, which powers it".to_owned())
                }
                "repeater" | "comparator" if pointed() => Some(format!(
                    "has a {name} pointed into it, which powers it"
                )),
                "redstone_torch"
                | "redstone_wall_torch"
                | "repeater"
                | "comparator"
                | "lever"
                | "air" => None,
                "target" | "redstone_block" if direction == Direction::Up => {
                    None
                }
                _ if name.ends_with("sign")
                    || name.ends_with("_button")
                    || !is_component(name)
                    || schem::slots(state).is_some() =>
                {
                    None
                }
                _ => Some(format!("touches a {}", name.replace('_', " "))),
            };
            if let Some(problem) = problem {
                say(
                    found,
                    Some(pos),
                    format!(
                        "input block {problem}; an input feeds at most two \
                         dusts, on its sides, besides any torches"
                    ),
                );
            }
        }
        if dust > 2 {
            say(
                found,
                Some(pos),
                format!(
                    "input block touches {dust} dust; an input feeds at most \
                     two dusts, on its sides, besides any torches"
                ),
            );
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
        let name = schem::short(state);
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
        let block = schem::short(state);
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
                    "a sign reading \"{name}\" is on {}, which is neither an \
                     input nor an output; signs carry only labels, so \
                     standardizing takes it out",
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
    let name = schem::short(state);
    name == "lever"
        || name.ends_with("_button")
        || name.ends_with("sign")
        || is_input(name)
        || (name.contains("torch")
            && support.is_some_and(|s| is_input(schem::short(s))))
}

/// Whether a block id without its namespace is a redstone component, such
/// as dust, a lever, a button, or a sign, rather than a block that only
/// holds or carries power.
pub fn is_component(name: &str) -> bool {
    COMPONENTS.contains(&name) || name.ends_with("_button")
}

/// Whether standardizing keeps `name` as it is: a redstone component, an
/// output block, or a block only a stack's first copy keeps.
pub fn keeps(name: &str) -> bool {
    is_component(name) || is_output(name)
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
