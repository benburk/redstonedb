//! Dust's shape as the game gives it among its neighbours, ported from
//! Minecraft's `RedStoneWireBlock.getConnectionState`, which both a block
//! update beside it and the world editor's `//update` run: which sides it
//! joins, and whether it climbs the block beside it. A dot touching nothing
//! stays a dot, as a player can make one.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;

use crate::schem::{
    self, Blocks, Direction, Pos,
    block::{conductor, whole},
};

/// How dust meets one side: not at all, along the ground, or up the block
/// beside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    None,
    Side,
    Up,
}

impl Side {
    /// Its name in a block state, such as `up`.
    pub fn name(self) -> &'static str {
        match self {
            Side::None => "none",
            Side::Side => "side",
            Side::Up => "up",
        }
    }

    fn joined(self) -> bool {
        self != Side::None
    }
}

/// The shape the dust at `pos` in `blocks` takes, its sides in the order
/// of [`Direction::SIDES`]: north, south, west, east.
pub fn shape(blocks: &Blocks, pos: Pos) -> [Side; 4] {
    let state = blocks.get(pos);
    let was_dot = Direction::SIDES.iter().all(|&d| {
        schem::property(state, d.name()).is_none_or(|side| side == "none")
    });
    let above = blocks.get(schem::step(pos, Direction::Up));
    let open_above = !conductor(above);
    let mut sides = Direction::SIDES
        .map(|direction| side(blocks, pos, direction, open_above));
    let joined = sides.map(Side::joined);
    if was_dot && !joined.contains(&true) {
        return sides;
    }
    let [north, south, west, east] = joined;
    let (north_south, west_east) = (!north && !south, !west && !east);
    if !west && north_south {
        sides[2] = Side::Side;
    }
    if !east && north_south {
        sides[3] = Side::Side;
    }
    if !north && west_east {
        sides[0] = Side::Side;
    }
    if !south && west_east {
        sides[1] = Side::Side;
    }
    sides
}

/// How the dust at `pos` meets its side `direction`, `open_above` when the
/// block above it lets dust climb: up the block beside it to dust on top,
/// when that block can hold dust; along the ground to what it connects to,
/// or down to dust below the block beside it when that block conducts
/// nothing; else not at all.
fn side(
    blocks: &Blocks,
    pos: Pos,
    direction: Direction,
    open_above: bool,
) -> Side {
    let beside_pos = schem::step(pos, direction);
    let beside = blocks.get(beside_pos);
    let on_top = blocks.get(schem::step(beside_pos, Direction::Up));
    if open_above
        && (holds_dust(beside) || schem::short(beside).ends_with("trapdoor"))
        && is_dust(on_top)
    {
        return if sturdy_face(beside, direction.opposite()) {
            Side::Up
        } else {
            Side::Side
        };
    }
    let below = blocks.get(schem::step(beside_pos, Direction::Down));
    if !connects_to(beside, direction) && (conductor(beside) || !is_dust(below))
    {
        Side::None
    } else {
        Side::Side
    }
}

/// The file in `data` with its dust shaped as the game shapes it, as the
/// world editor's `//update` leaves it, so a file saved with dust pointing
/// the wrong way is set right. A file whose dust is right comes back as it
/// was.
pub fn update(data: &[u8]) -> Result<Vec<u8>> {
    let changes: BTreeMap<Pos, String> =
        reshaped(&Blocks::read(data)?).into_iter().collect();
    if changes.is_empty() {
        return Ok(data.to_vec());
    }
    crate::schem::edit(data, &changes)
}

/// `blocks`' dust, each at the shape the game gives it, by position, for
/// those whose shape changes.
pub fn reshaped(blocks: &Blocks) -> HashMap<Pos, String> {
    blocks
        .iter()
        .filter(|&(_, state)| is_dust(state))
        .filter_map(|(pos, state)| {
            let sides = shape(blocks, pos);
            let mut now = state.to_owned();
            for (direction, side) in Direction::SIDES.iter().zip(sides) {
                now = schem::with_property(&now, direction.name(), side.name());
            }
            (now != state).then_some((pos, now))
        })
        .collect()
}

fn is_dust(state: &str) -> bool {
    schem::short(state) == "redstone_wire"
}

/// Whether dust connects to the block in `state` beside it in `direction`:
/// dust, a repeater along its line, an observer facing it, or any other
/// block that gives power, such as a lever, torch, comparator, or target.
fn connects_to(state: &str, direction: Direction) -> bool {
    let name = schem::short(state);
    let facing = Direction::facing(state);
    match name {
        "repeater" => {
            facing.is_some_and(|f| f == direction || f == direction.opposite())
        }
        "observer" => facing == Some(direction),
        "redstone_wire"
        | "comparator"
        | "lever"
        | "redstone_torch"
        | "redstone_wall_torch"
        | "redstone_block"
        | "target"
        | "daylight_detector"
        | "tripwire_hook"
        | "trapped_chest"
        | "lectern"
        | "sculk_sensor" => true,
        _ => name.ends_with("_button") || name.ends_with("_pressure_plate"),
    }
}

/// Whether the block in `state` can hold dust on top: its top face whole,
/// as a full block's, glass's, a top or double slab's, or upside-down
/// stairs'; or a hopper.
fn holds_dust(state: &str) -> bool {
    let name = schem::short(state);
    if name.ends_with("_slab") {
        return schem::property(state, "type") != Some("bottom");
    }
    if name.ends_with("_stairs") {
        return schem::property(state, "half") == Some("top");
    }
    name == "hopper" || whole(state)
}

/// Whether the block in `state` has a whole face toward `face`, as dust
/// climbing it draws up its side.
fn sturdy_face(state: &str, face: Direction) -> bool {
    let name = schem::short(state);
    if name.ends_with("_slab") {
        return schem::property(state, "type") == Some("double");
    }
    if name.ends_with("_stairs") {
        return Direction::facing(state) == Some(face);
    }
    whole(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "minecraft:redstone_wire[east=none,north=side,power=0,south=side,west=none]";
    const DOT: &str = "minecraft:redstone_wire[east=none,north=none,power=0,south=none,west=none]";

    fn shaped(states: &[(Pos, &str)], at: Pos) -> [&'static str; 4] {
        let grid: HashMap<Pos, String> = states
            .iter()
            .map(|&(pos, state)| (pos, state.to_owned()))
            .collect();
        shape(&Blocks::from_grid([3, 3, 3], &grid), at).map(Side::name)
    }

    /// Dust touching nothing is a cross, as placed, unless it was made a
    /// dot, which an update leaves.
    #[test]
    fn lone_dust_is_a_cross_but_a_dot_stays_one() {
        let ground = ([1, 0, 1], "minecraft:white_concrete");
        assert_eq!(
            shaped(&[ground, ([1, 1, 1], LINE)], [1, 1, 1]),
            ["side"; 4]
        );
        assert_eq!(shaped(&[ground, ([1, 1, 1], DOT)], [1, 1, 1]), ["none"; 4]);
    }

    /// Dust beside a whole block with dust on top climbs it, and beside a
    /// lever points at it: joined two ways, it turns the corner between.
    #[test]
    fn dust_climbs_to_dust_above_and_points_at_a_lever() {
        let states = [
            ([1, 0, 1], "minecraft:white_concrete"),
            ([1, 1, 1], LINE),
            ([2, 1, 1], "minecraft:target"),
            ([2, 2, 1], LINE),
            (
                [1, 1, 0],
                "minecraft:lever[face=floor,facing=north,powered=false]",
            ),
        ];
        // North, south, west, east.
        assert_eq!(shaped(&states, [1, 1, 1]), ["side", "none", "none", "up"]);
    }
}
