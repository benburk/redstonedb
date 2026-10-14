//! Copies of a stackable build joined into one schematic, as `//rstack`
//! joins them in-game.
//!
//! Copy 0 sits at the south-east-bottom corner and each later copy one
//! step further along, pasted over the copies before it: copies may
//! overlap, and where they do, the later copy's block stands, though air
//! never replaces anything and dust that meets joins. A build may rely on
//! that, as a carry out capping one copy is covered by the next.
//!
//! Later copies leave out what only the first copy keeps: gray blocks and
//! what rests on them, such as logic the least significant bit alone needs,
//! or a barrel giving the signal strength it alone takes; and the inputs
//! the copy before feeds, such as a carry in, with their levers and signs;
//! see `shared_inputs`.
//!
//! Every copy but the last turns its labelled outputs and status lines,
//! signed or not, white and drops their signs, since the next copy takes
//! them over. A later copy's solid block on a different solid block of the
//! copy before, one of them an input or output, means the step is wrong,
//! and is refused. Several directions
//! stack one after another.

use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

use anyhow::{Result, bail, ensure};

use crate::{
    interface, lint,
    models::schematic::StackDirection,
    schem::{self, AIR, Blocks, Pos},
};

/// The most copies along one direction.
const MAX_COPIES: usize = 64;
/// The most blocks in a stacked schematic.
const MAX_VOLUME: i64 = 16 * 1024 * 1024;

const WHITE: &str = "minecraft:white_concrete";

/// The stored file stacked with `copies` along each direction, with the
/// result's size, as [`interface::measured`] finds it.
pub fn stack(
    data: &[u8],
    copies: &[(StackDirection, usize)],
) -> Result<(Vec<u8>, Pos)> {
    let mut data = data.to_vec();
    for &(direction, count) in copies {
        data = along(&data, direction, count)?;
    }
    let (low, high) = interface::measured(&Blocks::read(&data)?);
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    Ok((data, size))
}

/// Stacks two copies each way, so a wrong step is refused when it is saved
/// rather than when someone downloads a stack. The `Err` is a sentence for
/// the person.
pub fn check(data: &[u8], directions: &[StackDirection]) -> Result<(), String> {
    for &direction in directions {
        along(data, direction, 2).map_err(|err| {
            format!("Two copies stacked {} don't fit: {err}.", direction.name())
        })?;
    }
    Ok(())
}

/// Pairs each of `directions` with its count in `wanted`, keyed by
/// direction name such as `west`. A direction left out gets one copy.
pub fn plan<S: BuildHasher>(
    directions: &[StackDirection],
    wanted: &HashMap<String, usize, S>,
) -> Result<Vec<(StackDirection, usize)>> {
    for name in wanted.keys() {
        ensure!(
            directions.iter().any(|d| d.name() == *name),
            "this build does not stack {name}"
        );
    }
    Ok(directions
        .iter()
        .map(|&d| (d, wanted.get(&d.name()).copied().unwrap_or(1)))
        .collect())
}

/// `copies` copies of the file along one direction.
fn along(
    data: &[u8],
    direction: StackDirection,
    copies: usize,
) -> Result<Vec<u8>> {
    ensure!(copies >= 1, "a stack needs at least one copy");
    // Anyone can ask for a stack, so cap the work one request can make.
    ensure!(
        copies <= MAX_COPIES,
        "a stack has at most {MAX_COPIES} copies each way"
    );
    if let Some(max) = direction.max_copies {
        ensure!(
            i64::try_from(copies)? <= max,
            "this build stacks at most {max} times {}",
            direction.name()
        );
    }
    if copies == 1 {
        return Ok(data.to_vec());
    }
    let blocks = Blocks::read(data)?;
    let [west, up, north] =
        [direction.west, direction.up, direction.north].map(i32::try_from);
    let (west, up, north) = (west?, up?, north?);
    let last = i32::try_from(copies - 1)?;
    let step = [-west, up, -north];
    let [w, h, l] = blocks.size;
    let size = [w + last * west, h + last * up, l + last * north];
    let volume: i64 = size.iter().map(|&n| i64::from(n)).product();
    ensure!(volume <= MAX_VOLUME, "that stack would be too large");
    // Where copy `i`'s blocks move to in the stacked box.
    let shift = |i: i32| [(last - i) * west, i * up, (last - i) * north];

    let mut left_out = first_copy_only(&blocks);
    left_out.extend(shared_inputs(&blocks, step));
    let outputs = labelled_outputs(&blocks);

    let mut grid: HashMap<Pos, String> = HashMap::new();
    for i in 0..=last {
        let first = i == 0;
        let final_copy = i == last;
        for (pos, state) in blocks.iter() {
            if state == AIR
                || (!first && left_out.contains(&pos))
                || (!final_copy && outputs.signs.contains(&pos))
            {
                continue;
            }
            let state = if !final_copy && outputs.blocks.contains(&pos) {
                WHITE
            } else {
                state
            };
            place(&mut grid, add(pos, shift(i)), state)?;
        }
    }

    let mut nbt = schem::decode(data)?;
    let entities = nbt.take_entities();
    // Where copies overlap, a later copy's entity stands, as its block
    // does, and one whose block a later copy covered goes with it.
    let mut placed: HashMap<Pos, (String, fastnbt::Value)> = HashMap::new();
    for i in 0..=last {
        for entity in &entities {
            let Some(pos) = schem::entity_pos(entity) else {
                continue;
            };
            let dropped = (i != 0 && left_out.contains(&pos))
                || (i != last && outputs.signs.contains(&pos));
            if !dropped {
                let to = add(pos, shift(i));
                let block = schem::id(blocks.get(pos)).to_owned();
                placed.insert(to, (block, schem::with_pos(entity, to)));
            }
        }
    }
    let stacked_entities = placed
        .into_iter()
        .filter(|(pos, (block, _))| {
            grid.get(pos).is_some_and(|state| schem::id(state) == block)
        })
        .map(|(_, (_, entity))| entity)
        .collect();
    // The first copy stays at the south-east-bottom end and the rest go
    // west, north, and up, so the box's first block moved by this.
    let low = [-(last * west), 0, -(last * north)];
    nbt.write_blocks(low, size, &grid, stacked_entities)?;
    schem::encode(nbt)
}

/// The inputs a later copy leaves out, a `step` along, since the copy
/// before feeds them: a control line that lands within it, such as a carry
/// in, and an input block of a layer outside the build's core, as
/// [`interface::core`] finds it, that lands in the core of the copy before,
/// as a memory's input layer does; with the levers, buttons, and signs on
/// them. A torch on one stays where the copy before has a solid block in
/// its place to hang on, as one inverting a carry in hangs on the carry out
/// it continues. A control line that lands beyond the copy before, as a
/// register stacked into more registers keeps its own, stays, as does an
/// adder's next bit, which lands in its own input layer.
fn shared_inputs(blocks: &Blocks, step: Pos) -> HashSet<Pos> {
    let (low, high) = interface::core(blocks);
    let in_core = |pos: Pos| {
        (0..3).all(|axis| (low[axis]..=high[axis]).contains(&pos[axis]))
    };
    let within = |pos: Pos| {
        (0..3).all(|axis| (0..blocks.size[axis]).contains(&pos[axis]))
    };
    let inputs: HashSet<Pos> = blocks
        .iter()
        .filter(|&(pos, state)| {
            let name = short(state);
            let landing = add(pos, step);
            (name == format!("{}_concrete", lint::CONTROL) && within(landing))
                || (lint::is_input(name) && !in_core(pos) && in_core(landing))
        })
        .map(|(pos, _)| pos)
        .collect();
    let mut shared = inputs.clone();
    shared.extend(
        blocks
            .switches()
            .into_iter()
            .filter(|switch| inputs.contains(&switch.block))
            .map(|switch| switch.pos),
    );
    shared.extend(blocks.signs.keys().copied().filter(|&sign| {
        blocks
            .sign_block(sign)
            .is_some_and(|on| inputs.contains(&on))
    }));
    shared.extend(blocks.iter().filter_map(|(pos, state)| {
        let on = schem::torch_block(pos, state)?;
        (inputs.contains(&on) && !solid(blocks.get(add(on, step))))
            .then_some(pos)
    }));
    shared
}

/// The blocks only the first copy keeps: gray blocks, and the dust,
/// torches, repeaters, comparators, levers, buttons, signs, and containers
/// such as barrels resting on them.
fn first_copy_only(blocks: &Blocks) -> HashSet<Pos> {
    let mut only: HashSet<Pos> = blocks
        .iter()
        .filter(|&(_, state)| lint::first_copy_only(short(state)))
        .map(|(pos, _)| pos)
        .collect();
    let mut resting: Vec<(Pos, Pos)> = blocks
        .iter()
        .filter_map(|(pos, state)| {
            let on = match short(state) {
                "redstone_wire" | "repeater" | "comparator" | "barrel"
                | "hopper" | "furnace" => Some(schem::step(pos, "down")),
                _ => schem::torch_block(pos, state),
            };
            Some((pos, on?))
        })
        .collect();
    resting.extend(blocks.switches().into_iter().map(|s| (s.pos, s.block)));
    resting.extend(
        blocks
            .signs
            .keys()
            .filter_map(|&sign| Some((sign, blocks.sign_block(sign)?))),
    );
    let on_them: Vec<Pos> = resting
        .into_iter()
        .filter(|(_, on)| only.contains(on))
        .map(|(pos, _)| pos)
        .collect();
    only.extend(on_them);
    only
}

struct Outputs {
    blocks: HashSet<Pos>,
    signs: HashSet<Pos>,
}

/// The blocks of labelled outputs and status lines, such as a carry out,
/// signed or not, and their signs.
fn labelled_outputs(blocks: &Blocks) -> Outputs {
    let interface = interface::read(blocks);
    let outputs: HashSet<Pos> = interface
        .outputs
        .iter()
        .filter(|port| port.labelled || port.block.starts_with(lint::STATUS))
        .flat_map(|port| port.bits.iter().map(|bit| bit.pos))
        .collect();
    let signs = blocks
        .signs
        .keys()
        .copied()
        .filter(|&sign| {
            blocks
                .sign_block(sign)
                .is_some_and(|b| outputs.contains(&b))
        })
        .collect();
    Outputs {
        blocks: outputs,
        signs,
    }
}

/// Puts `state` at `pos`, over what an earlier copy put there, as
/// `//rstack` does: dust that meets joins every way either joined. A solid
/// block on a different solid block, one of them an input or output, is
/// refused, since copies that collide so mean the stacking step is wrong.
fn place(grid: &mut HashMap<Pos, String>, pos: Pos, state: &str) -> Result<()> {
    let marks = |state: &str| {
        lint::is_input(short(state)) || lint::is_output(short(state))
    };
    let placed = match grid.get(&pos) {
        Some(existing)
            if solid(existing)
                && solid(state)
                && (marks(existing) || marks(state))
                && replaces(existing, state) =>
        {
            let [x, y, z] = pos;
            bail!(
                "copies collide at ({x}, {y}, {z}): {existing} and {state}; \
                 check the stacking step"
            );
        }
        Some(existing)
            if short(existing) == "redstone_wire"
                && short(state) == "redstone_wire" =>
        {
            joined(existing, state)
        }
        _ => state.to_owned(),
    };
    grid.insert(pos, placed);
    Ok(())
}

/// Whether `state` is a solid block rather than a component standing on
/// or beside one, such as dust, a torch, or a lever.
fn solid(state: &str) -> bool {
    let name = short(state);
    !(name == "air"
        || name == "redstone_wire"
        || name == "repeater"
        || name == "comparator"
        || name.ends_with("torch")
        || name == "lever"
        || name.ends_with("_button")
        || name.ends_with("sign")
        || name == "tripwire_hook")
}

/// Whether `state` would replace a different block at `existing`: anything
/// but air or the same block, though in another state of power, which the
/// game works out again.
fn replaces(existing: &str, state: &str) -> bool {
    existing != AIR && unpowered(existing) != unpowered(state)
}

/// A block state without what depends on its power or neighbours: dust's
/// strength and joins, and whether a component is powered or lit.
fn unpowered(state: &str) -> String {
    if short(state) == "redstone_wire" {
        return schem::id(state).to_owned();
    }
    let Some((id, properties)) = state.split_once('[') else {
        return state.to_owned();
    };
    let kept: Vec<&str> = properties
        .trim_end_matches(']')
        .split(',')
        .filter(|p| !p.starts_with("powered=") && !p.starts_with("lit="))
        .collect();
    format!("{id}[{}]", kept.join(","))
}

/// Dust joined every way either `a` or `b` joins, `up` over `side` over
/// `none`, at the greater strength.
fn joined(a: &str, b: &str) -> String {
    let rank = |join: &str| match join {
        "up" => 2,
        "side" => 1,
        _ => 0,
    };
    let joins = ["north", "south", "east", "west"].map(|side| {
        let (x, y) = (
            schem::property(a, side).unwrap_or("none"),
            schem::property(b, side).unwrap_or("none"),
        );
        format!("{side}={}", if rank(y) > rank(x) { y } else { x })
    });
    let power = |s: &str| {
        schem::property(s, "power").and_then(|p| p.parse::<u8>().ok())
    };
    let power = power(a).max(power(b)).unwrap_or(0);
    let [north, south, east, west] = joins;
    format!(
        "minecraft:redstone_wire[{east},{north},power={power},{south},{west}]"
    )
}

/// A block's id without its namespace, such as `redstone_wire`.
fn short(state: &str) -> &str {
    let id = schem::id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dust_where_copies_meet_joins_both_ways() {
        let below = "minecraft:redstone_wire[east=up,north=none,power=4,\
                     south=side,west=side]";
        let above = "minecraft:redstone_wire[east=up,north=none,power=0,\
                     south=none,west=up]";
        assert!(!replaces(below, above));
        assert_eq!(
            joined(below, above),
            "minecraft:redstone_wire[east=up,north=none,power=4,south=side,\
             west=up]"
        );
    }

    #[test]
    fn only_another_block_collides() {
        let lit = "minecraft:redstone_wall_torch[facing=north,lit=true]";
        let unlit = "minecraft:redstone_wall_torch[facing=north,lit=false]";
        assert!(!replaces(lit, unlit));
        assert!(!replaces(AIR, WHITE));
        assert!(replaces(WHITE, unlit));
        assert!(replaces(
            "minecraft:repeater[delay=1,facing=west]",
            "minecraft:repeater[delay=1,facing=east]"
        ));
    }
}
