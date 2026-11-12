//! What a block is to redstone, told from its id: a component, a whole
//! cube, a block that carries power, or one redstone sees through. Each
//! question has one answer here, though the check of a build's inputs asks
//! a looser one of its own, [`holds_input`], than dust's, [`solid`].

use std::collections::HashMap;

use super::{self as schem, Blocks, Direction, Pos};

/// Redstone components and the other blocks every build may use, by id
/// without its namespace. Besides dust, torches, and the like, this keeps
/// the blocks a comparator reads, such as barrels, since a build uses them
/// for their signal; furnaces, cauldrons, and cakes are swapped for
/// barrels. Every kind of button is a component as well.
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

/// Whether a block id without its namespace is a redstone component, such
/// as dust, a lever, a button, or a sign, rather than a block that only
/// holds or carries power.
pub fn is_component(name: &str) -> bool {
    COMPONENTS.contains(&name) || name.ends_with("_button")
}

/// Whether `name` is a redstone part smaller than a block, such as dust, a
/// torch, a repeater, or a lever; a target, a redstone block, an observer,
/// and the like are whole blocks.
fn partial(name: &str) -> bool {
    matches!(
        name,
        "redstone_wire"
            | "redstone_torch"
            | "redstone_wall_torch"
            | "repeater"
            | "comparator"
            | "lever"
            | "piston_head"
            | "daylight_detector"
            | "tripwire_hook"
            | "tripwire"
    ) || name.ends_with("_button")
        || name.ends_with("_pressure_plate")
}

/// Whether the block in `state` is a whole cube: neither air, a redstone
/// part smaller than a block, a partial block, nor a container shaped
/// otherwise.
pub fn whole(state: &str) -> bool {
    let name = super::short(state);
    !(name == "air"
        || name.ends_with("_slab")
        || name.ends_with("_stairs")
        || name.ends_with("_wall")
        || name.ends_with("_fence")
        || name.ends_with("_pane")
        || name.ends_with("trapdoor")
        || name.ends_with("_carpet")
        || name.ends_with("sign")
        || matches!(
            name,
            "hopper" | "cauldron" | "water_cauldron" | "lava_cauldron" | "cake"
        )
        || partial(name))
}

/// Whether the block in `state` passes power through itself, as a full
/// solid block does; glass, slabs, stairs, glowstone, a redstone block,
/// and redstone parts do not.
pub fn conductor(state: &str) -> bool {
    let name = super::short(state);
    if name.ends_with("_slab") {
        return super::property(state, "type") == Some("double");
    }
    !(name == "air"
        || name.contains("glass")
        || name.ends_with("_stairs")
        || name.ends_with("_leaves")
        || name.ends_with("_wall")
        || name.ends_with("_fence")
        || name.ends_with("trapdoor")
        || name.ends_with("_carpet")
        || name.ends_with("sign")
        || matches!(
            name,
            "glowstone"
                | "redstone_block"
                | "observer"
                | "piston"
                | "sticky_piston"
                | "piston_head"
                | "hopper"
                | "cauldron"
                | "water_cauldron"
                | "lava_cauldron"
                | "cake"
                | "ice"
                | "sea_lantern"
        )
        || partial(name))
}

/// Whether the block in `state` is a full block that carries power as
/// concrete does, such as wool, terracotta, planks, stone, or a lamp.
pub fn solid(state: &str) -> bool {
    whole(state) && conductor(state)
}

/// Whether a block id without its namespace is one a lever of a build's
/// input can hang on and dust stand on, as the check of a build's inputs
/// asks: any block but air, glass, a slab, stairs, a sign, or a redstone
/// component, and a lamp, a full block that carries power as any other
/// does. It asks less than [`solid`]: leaves, ice, and fences pass.
pub fn holds_input(name: &str) -> bool {
    name == "redstone_lamp"
        || !is_component(name)
            && !name.contains("glass")
            && !name.ends_with("slab")
            && !name.ends_with("stairs")
            && !name.ends_with("sign")
            && name != "air"
}

/// Whether a block id without its namespace is one redstone sees through,
/// neither powered nor cutting dust, which standardizing makes white
/// stained glass: glass of any kind, or a block that stands in for it.
pub fn see_through(name: &str) -> bool {
    name.contains("glass") || like_glass(name)
}

/// Whether a block id without its namespace is one redstone sees through
/// as it does glass, though it is not glass: a slab, stairs, or
/// glowstone, which the simulator does not model, so it takes white
/// stained glass in their place.
pub fn like_glass(name: &str) -> bool {
    name.ends_with("_slab") || name.ends_with("_stairs") || name == "glowstone"
}

/// The block `id` acts as, such as `oak_button` for any wooden button: a
/// button or pressure plate goes by what it does, not what it is made of.
pub(crate) fn like(id: &str) -> &str {
    if id.ends_with("_button") {
        if matches!(id, "stone_button" | "polished_blackstone_button") {
            "stone_button"
        } else {
            "oak_button"
        }
    } else if id.ends_with("_pressure_plate") && !id.contains("weighted") {
        "stone_pressure_plate"
    } else {
        id
    }
}

/// The block the component `id` at `pos` rests on or hangs from, and
/// whether it rests on its top; `None` for a block that needs none.
pub(crate) fn support(pos: Pos, state: &str, id: &str) -> Option<(Pos, bool)> {
    let below = Some((schem::step(pos, Direction::Down), true));
    match id {
        "redstone_wire"
        | "redstone_torch"
        | "repeater"
        | "comparator"
        | "stone_pressure_plate"
        | "light_weighted_pressure_plate"
        | "heavy_weighted_pressure_plate" => below,
        "redstone_wall_torch" => Some((schem::torch_block(pos, state)?, false)),
        "lever" | "stone_button" | "oak_button" => {
            match schem::property(state, "face")? {
                "floor" => below,
                "ceiling" => Some((schem::step(pos, Direction::Up), false)),
                _ => {
                    let facing = Direction::facing(state)?;
                    Some((schem::step(pos, facing.opposite()), false))
                }
            }
        }
        _ => None,
    }
}

/// What the block id `name`, without its namespace, counts as among a
/// build's blocks: a redstone part by what it is, a torch on a wall or on
/// the floor alike and dust whatever its shape; every see-through block as
/// one, and every other solid block as one. `None` for air and for signs,
/// which carry only labels.
pub fn block_kind(name: &str) -> Option<&'static str> {
    Some(match name {
        "air" | "cave_air" | "void_air" => return None,
        _ if name.ends_with("_sign") => return None,
        "redstone_wire" => "Dust",
        "redstone_torch" | "redstone_wall_torch" => "Torch",
        "repeater" => "Repeater",
        "comparator" => "Comparator",
        "target" => "Target",
        "barrel" => "Barrel",
        "redstone_block" => "Redstone block",
        "lever" => "Lever",
        _ if name.ends_with("_button") => "Button",
        _ if name.ends_with("_pressure_plate") => "Pressure plate",
        "observer" => "Observer",
        "piston" | "sticky_piston" | "piston_head" | "moving_piston" => {
            "Piston"
        }
        "redstone_lamp" => "Lamp",
        "hopper" => "Hopper",
        "furnace" => "Furnace",
        "cake" => "Cake",
        _ if name.ends_with("cauldron") => "Cauldron",
        _ if see_through(name) => "Transparent",
        _ => "Solid",
    })
}

/// How many of each kind of block, as `block_kind` counts them, `blocks`
/// holds, most first.
pub fn block_counts(blocks: &Blocks) -> Vec<(&'static str, usize)> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for (_, state) in blocks.iter() {
        if let Some(kind) = block_kind(schem::short(state)) {
            *counts.entry(kind).or_default() += 1;
        }
    }
    let mut counts: Vec<(&'static str, usize)> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What each question makes of `state`: whether it is a component,
    /// whole, a conductor, solid, holds an input, and is seen through.
    fn kind(state: &str) -> [bool; 6] {
        let name = crate::schem::short(state);
        [
            is_component(name),
            whole(state),
            conductor(state),
            solid(state),
            holds_input(name),
            see_through(name),
        ]
    }

    const T: bool = true;
    const F: bool = false;

    #[test]
    fn plain_full_blocks_are_solid() {
        for state in [
            "minecraft:white_concrete",
            "minecraft:red_wool",
            "minecraft:stone",
            "minecraft:oak_planks",
            "minecraft:terracotta",
        ] {
            assert_eq!(kind(state), [F, T, T, T, T, F], "{state}");
        }
    }

    #[test]
    fn glass_is_seen_through_and_holds_nothing() {
        for state in [
            "minecraft:glass",
            "minecraft:white_stained_glass",
            "minecraft:tinted_glass",
        ] {
            assert_eq!(kind(state), [F, T, F, F, F, T], "{state}");
        }
        // A pane is no whole block either.
        assert_eq!(kind("minecraft:glass_pane"), [F, F, F, F, F, T]);
    }

    #[test]
    fn a_slab_conducts_only_doubled() {
        let bottom = "minecraft:stone_slab[type=bottom,waterlogged=false]";
        let double = "minecraft:stone_slab[type=double,waterlogged=false]";
        assert_eq!(kind(bottom), [F, F, F, F, F, T]);
        assert_eq!(kind(double), [F, F, T, F, F, T]);
        assert_eq!(
            kind("minecraft:oak_stairs[facing=north,half=top]"),
            [F, F, F, F, F, T]
        );
        assert_eq!(kind("minecraft:glowstone"), [F, T, F, F, T, T]);
    }

    /// Leaves, ice, and fences carry no power, so dust's rules find them
    /// no solid block, but the check of an input lets a lever hang on
    /// them.
    #[test]
    fn leaves_ice_and_fences_hold_an_input_but_are_not_solid() {
        assert_eq!(
            kind("minecraft:oak_leaves[persistent=true]"),
            [F, T, F, F, T, F]
        );
        assert_eq!(kind("minecraft:ice"), [F, T, F, F, T, F]);
        assert_eq!(kind("minecraft:oak_fence"), [F, F, F, F, T, F]);
        assert_eq!(kind("minecraft:sea_lantern"), [F, T, F, F, T, F]);
    }

    #[test]
    fn components_are_neither_solid_nor_hold_an_input() {
        for state in [
            "minecraft:redstone_wire",
            "minecraft:repeater[delay=1,facing=north]",
            "minecraft:lever[face=wall,facing=north]",
            "minecraft:stone_button[face=wall,facing=north]",
            "minecraft:redstone_torch",
        ] {
            assert_eq!(kind(state), [T, F, F, F, F, F], "{state}");
        }
        // A sign holds nothing up and carries no power.
        assert_eq!(kind("minecraft:oak_sign"), [T, F, F, F, F, F]);
        assert_eq!(kind("minecraft:air"), [T, F, F, F, F, F]);
    }

    /// A lamp, a target, and an observer are whole blocks among the
    /// components; a lamp alone carries power as concrete does and holds
    /// an input.
    #[test]
    fn a_lamp_is_the_one_solid_component() {
        assert_eq!(
            kind("minecraft:redstone_lamp[lit=false]"),
            [T, T, T, T, T, F]
        );
        assert_eq!(kind("minecraft:target"), [T, T, T, T, F, F]);
        assert_eq!(kind("minecraft:observer"), [T, T, F, F, F, F]);
        assert_eq!(kind("minecraft:redstone_block"), [T, T, F, F, F, F]);
        assert_eq!(kind("minecraft:hopper"), [T, F, F, F, F, F]);
    }
}
