//! The blocks the site colours a build's interface with, once probing
//! finds each line's role, and how it reads them back: inputs on cool
//! concrete, outputs on warm concrete or glass. See `docs/conventions.md`.

use crate::schem;

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
