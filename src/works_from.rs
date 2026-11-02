//! The oldest Minecraft: Java Edition release a build works in, from the
//! redstone components and mechanics it uses. Building blocks do not count,
//! as any solid block stands in for white concrete and any transparent one
//! for white stained glass. Where a component rests on a transparent block
//! does count, as one could not always: the oldest transparent block it
//! could rest on sets the release. Every build on the site works in the
//! latest release, so there is no newest one to find.

use crate::{
    schem::{self, Blocks, Direction, Pos},
    standardize,
};

/// A release that added redstone components or mechanics a build may use.
pub struct Release {
    /// As players name it, such as `Beta 1.3` or `1.16`.
    pub name: &'static str,
    /// The block ids it added, without their namespace.
    pub blocks: &'static [&'static str],
}

/// Each release that added redstone components, oldest first, as the
/// Minecraft Wiki's history sections give them. A barrel or furnace is left
/// out: it matters only when a comparator reads it, which sets 1.5 already,
/// and in an older release a chest or furnace stands in for a barrel, as a
/// barrel stands in for the furnaces, cauldrons, and cakes uploads swap.
/// Buttons and pressure plates go by what they do, a wooden button from
/// 1.4.2 and any other from Alpha.
pub const RELEASES: &[Release] = &[
    Release {
        name: "Alpha",
        blocks: &[
            "redstone_wire",
            "redstone_torch",
            "redstone_wall_torch",
            "lever",
            "stone_button",
            "stone_pressure_plate",
            "oak_pressure_plate",
        ],
    },
    Release {
        name: "Beta 1.2",
        blocks: &["dispenser", "note_block"],
    },
    Release {
        name: "Beta 1.3",
        blocks: &["repeater"],
    },
    Release {
        name: "Beta 1.7",
        blocks: &["piston", "sticky_piston"],
    },
    Release {
        name: "1.2.1",
        blocks: &["redstone_lamp"],
    },
    Release {
        name: "1.3.1",
        blocks: &["tripwire_hook"],
    },
    Release {
        name: "1.4.2",
        blocks: &["oak_button"],
    },
    Release {
        name: "1.5",
        blocks: &[
            "comparator",
            "redstone_block",
            "hopper",
            "dropper",
            "daylight_detector",
            "trapped_chest",
            "light_weighted_pressure_plate",
            "heavy_weighted_pressure_plate",
        ],
    },
    Release {
        name: "1.6.1",
        blocks: &["cauldron", "water_cauldron", "lava_cauldron"],
    },
    Release {
        name: "1.8",
        blocks: &["slime_block", "cake"],
    },
    Release {
        name: "1.9",
        blocks: &[],
    },
    Release {
        name: "1.11",
        blocks: &["observer"],
    },
    Release {
        name: "1.14",
        blocks: &["lectern", "composter"],
    },
    Release {
        name: "1.15",
        blocks: &["honey_block"],
    },
    Release {
        name: "1.16",
        blocks: &["target"],
    },
    Release {
        name: "1.17",
        blocks: &["lightning_rod"],
    },
    Release {
        name: "1.19",
        blocks: &["sculk_sensor"],
    },
    Release {
        name: "1.20",
        blocks: &["calibrated_sculk_sensor"],
    },
    Release {
        name: "1.21",
        blocks: &["crafter", "copper_bulb"],
    },
];

/// When repeaters could be locked, by a repeater or comparator powering
/// one's side.
const LOCKING: &str = "1.4.2";

/// When a block of redstone could power a comparator's side.
const REDSTONE_BLOCK_SIDE: &str = "1.9";

/// When lone dust could be a dot, powering none of its sides.
const DOT: &str = "1.16";

/// When dust, or a standing torch, could rest on a transparent block: dust
/// on glowstone, which it passes signals only up through, as through
/// glass, and a torch on glass.
const ON_TRANSPARENT: &str = "1.2.1";

/// When a repeater, lever, or pressure plate could rest on a transparent
/// block: on an upside-down slab.
const ON_TRANSPARENT_SLAB: &str = "1.3.1";

/// When a torch, lever, or button could hang from a transparent block's
/// side or underside, glass's among them.
const ON_TRANSPARENT_SIDE: &str = "1.14";

/// The oldest release a build works in, and the component or mechanic that
/// sets it, such as `comparator`; `None` for a build of nothing newer than
/// the earliest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorksFrom {
    pub release: &'static str,
    pub because: Option<String>,
}

/// Where `name`, such as `1.5`, falls among the releases, oldest 0.
fn place(name: &str) -> usize {
    RELEASES
        .iter()
        .position(|release| release.name == name)
        .unwrap_or(0)
}

/// The oldest release `blocks` works in.
pub fn of(blocks: &Blocks) -> WorksFrom {
    let mut found = WorksFrom {
        release: RELEASES[0].name,
        because: None,
    };
    let mut raise = |release: &'static str, because: String| {
        if place(release) > place(found.release) {
            found = WorksFrom {
                release,
                because: Some(because),
            };
        }
    };
    for (pos, state) in blocks.iter() {
        let id = like(schem::id(state).trim_start_matches("minecraft:"));
        if let Some(release) =
            RELEASES.iter().find(|release| release.blocks.contains(&id))
        {
            raise(release.name, named(id));
        }
        if id == "repeater" && locked(blocks, pos, state) {
            raise(LOCKING, "repeater locking".to_owned());
        }
        if id == "redstone_wire" && dot(blocks, pos, state) {
            raise(DOT, "a dot of dust".to_owned());
        }
        if let Some((on, top)) = support(pos, state, id)
            && standardize::see_through(schem::short(blocks.get(on)))
        {
            let what = match id {
                "redstone_wire" => "dust",
                "redstone_torch" | "redstone_wall_torch" => "a redstone torch",
                "repeater" => "a repeater",
                "comparator" => "a comparator",
                "lever" => "a lever",
                "stone_button" | "oak_button" => "a button",
                _ => "a pressure plate",
            };
            let release = match (id, top) {
                (_, false) => ON_TRANSPARENT_SIDE,
                ("redstone_wire" | "redstone_torch", true) => ON_TRANSPARENT,
                _ => ON_TRANSPARENT_SLAB,
            };
            let place = if top { "on" } else { "under or beside" };
            raise(release, format!("{what} {place} a transparent block"));
        }
        if id == "comparator" && redstone_block_beside(blocks, pos, state) {
            raise(
                REDSTONE_BLOCK_SIDE,
                "a block of redstone beside a comparator".to_owned(),
            );
        }
    }
    found
}

/// The block the component `id` at `pos` rests on or hangs from, and
/// whether it rests on its top; `None` for a block that needs none.
fn support(pos: Pos, state: &str, id: &str) -> Option<(Pos, bool)> {
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

/// A component as people name it, such as `target block` for `target`.
fn named(id: &str) -> String {
    match id {
        "target" => "target block".to_owned(),
        "redstone_block" => "block of redstone".to_owned(),
        other => other.replace('_', " "),
    }
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

/// The unit step toward the level direction `facing` names, such as
/// `[0, 0, -1]` for north.
fn toward(facing: &str) -> Option<Pos> {
    Direction::parse(facing)
        .filter(|d| Direction::SIDES.contains(d))
        .map(Direction::offset)
}

/// Whether the repeater at `pos` has a repeater or comparator pointing
/// into its side, which locks it. A repeater's or comparator's `facing` is
/// the way its input is, so it outputs the other way.
fn locked(blocks: &Blocks, pos: Pos, state: &str) -> bool {
    let Some([x, _, z]) = schem::property(state, "facing").and_then(toward)
    else {
        return false;
    };
    [[z, 0, x], [-z, 0, -x]].into_iter().any(|side| {
        let beside = schem::add(pos, side);
        let other = blocks.get(beside);
        let id = schem::id(other).trim_start_matches("minecraft:");
        matches!(id, "repeater" | "comparator")
            && schem::property(other, "facing")
                .and_then(toward)
                .is_some_and(|input| {
                    schem::add(beside, input)
                        == schem::add(pos, side.map(|n| 2 * n))
                })
    })
}

/// Whether the comparator at `pos` has a block of redstone beside it, on a
/// side rather than behind it, which powers that side.
fn redstone_block_beside(blocks: &Blocks, pos: Pos, state: &str) -> bool {
    let Some([x, _, z]) = schem::property(state, "facing").and_then(toward)
    else {
        return false;
    };
    [[z, 0, x], [-z, 0, -x]].into_iter().any(|side| {
        schem::id(blocks.get(schem::add(pos, side)))
            == "minecraft:redstone_block"
    })
}

/// Whether the dust at `pos` is a dot, joined to none of its sides, with a
/// block beside it that dust from before the dot would have powered.
fn dot(blocks: &Blocks, pos: Pos, state: &str) -> bool {
    let joined = Direction::SIDES.iter().any(|side| {
        schem::property(state, side.name()).is_some_and(|value| value != "none")
    });
    !joined
        && Direction::SIDES
            .into_iter()
            .any(|side| blocks.get(schem::step(pos, side)) != schem::AIR)
}

/// A release's place in time, as `[era, numbers...]`: Alpha 0, Beta 1,
/// and full releases 2, as in `[2, 1, 16, 0]` for `1.16`.
fn key(name: &str) -> [u32; 4] {
    let (era, rest) = if let Some(rest) = name.strip_prefix("Beta ") {
        (1, rest)
    } else if name.starts_with("Alpha") {
        (0, "")
    } else {
        (2, name)
    };
    let mut key = [era, 0, 0, 0];
    for (place, part) in key[1..].iter_mut().zip(rest.split('.')) {
        *place = part.parse().unwrap_or(0);
    }
    key
}

/// Every release search knows by name: each that added a component, and
/// each from 1.0 to the newest, as in `version:1.12`.
pub fn known() -> Vec<String> {
    let newest = RELEASES.last().map_or(0, |r| key(r.name)[2]);
    let mut names: Vec<String> =
        RELEASES.iter().map(|r| r.name.to_owned()).collect();
    names.extend((0..=newest).map(|minor| format!("1.{minor}")));
    names.sort_by_key(|name| key(name));
    names.dedup();
    names
}

/// Whether a build that works from `from` works in `release` too, as it
/// does in every later one.
pub fn works_in(from: &str, release: &str) -> bool {
    key(release) >= key(from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_are_in_order_and_later_ones_keep_working() {
        let keys: Vec<[u32; 4]> =
            RELEASES.iter().map(|release| key(release.name)).collect();
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(works_in("Alpha", "Beta 1.3"));
        assert!(works_in("1.5", "1.12"));
        assert!(works_in("1.4.2", "1.5"));
        assert!(!works_in("1.16", "1.12"));
        assert!(!works_in("Beta 1.7", "Beta 1.3"));
        let known = known();
        assert_eq!(known.first().map(String::as_str), Some("Alpha"));
        assert!(known.contains(&"1.12".to_owned()));
        assert_eq!(known.last().map(String::as_str), Some("1.21"));
    }

    /// A build of one block with `parts` on it, by offset from the block,
    /// whose own state is `block`.
    fn on(block: &str, parts: &[(Pos, &str)]) -> WorksFrom {
        use std::collections::HashMap;
        let mut grid: HashMap<Pos, String> = parts
            .iter()
            .map(|&(offset, state)| {
                (schem::add([1, 1, 1], offset), format!("minecraft:{state}"))
            })
            .collect();
        grid.insert([1, 1, 1], format!("minecraft:{block}"));
        of(&Blocks::from_grid([3, 3, 3], &grid))
    }

    #[test]
    fn a_component_on_a_transparent_block_waits_for_one_it_could_rest_on() {
        let dust =
            "redstone_wire[east=none,north=none,power=0,south=none,west=none]";
        // A lone dot of dust on nothing beside is no dot, as in Alpha.
        assert_eq!(on("white_concrete", &[([0, 1, 0], dust)]).release, "Alpha");
        let glass = on("white_stained_glass", &[([0, 1, 0], dust)]);
        assert_eq!(glass.release, "1.2.1");
        assert_eq!(
            glass.because.as_deref(),
            Some("dust on a transparent block")
        );
        let torch = "redstone_torch[lit=true]";
        assert_eq!(on("glass", &[([0, 1, 0], torch)]).release, "1.2.1");
        let lever = "lever[face=floor,facing=north,powered=false]";
        assert_eq!(on("oak_slab", &[([0, 1, 0], lever)]).release, "1.3.1");
        let wall = "redstone_wall_torch[facing=south,lit=true]";
        assert_eq!(
            on("white_stained_glass", &[([0, 0, 1], wall)]).release,
            "1.14"
        );
        assert_eq!(on("white_concrete", &[([0, 0, 1], wall)]).release, "Alpha");
    }

    #[test]
    fn a_button_or_plate_goes_by_what_it_does() {
        assert_eq!(like("birch_button"), "oak_button");
        assert_eq!(like("polished_blackstone_button"), "stone_button");
        assert_eq!(like("crimson_pressure_plate"), "stone_pressure_plate");
        assert_eq!(
            like("heavy_weighted_pressure_plate"),
            "heavy_weighted_pressure_plate"
        );
    }
}
