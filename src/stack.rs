//! Copies of a stackable build joined into one schematic.
//!
//! Copy 0 sits at the south-east-bottom corner and each later copy one
//! step further along. A copy's slice is the part that repeats: its core,
//! as [`interface::core`] finds it, cut along each stacking axis to as many
//! layers as the step, at the end away from the previous copy. Its blocks
//! outside the slice that reach into the previous copy's slice, such as
//! the magenta block feeding a carry in or a control line, belong to the
//! edge of the stack and are dropped, along with the levers, signs, and
//! torches on them. A lever, sign, or torch on a block that stays, stays
//! too, unless it would land on one of the previous copy's blocks. The previous copy's blocks stand there.
//! Every copy but the last turns its labelled outputs white and drops their
//! signs, since the next copy takes them over. Several directions stack one
//! after another.

use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

use anyhow::{Result, bail, ensure};

use crate::{
    interface,
    models::schematic::StackDirection,
    schem::{self, AIR, Blocks, Pos},
};

/// The most copies along one direction.
const MAX_COPIES: usize = 64;
/// The most blocks in a stacked schematic.
const MAX_VOLUME: i64 = 16 * 1024 * 1024;

const WHITE: &str = "minecraft:white_concrete";

/// The stored file stacked with `copies` along each direction, with the
/// size of the result's core.
pub fn stack(
    data: &[u8],
    copies: &[(StackDirection, usize)],
) -> Result<(Vec<u8>, Pos)> {
    let mut data = data.to_vec();
    for &(direction, count) in copies {
        data = along(&data, direction, count)?;
    }
    let (low, high) = interface::core(&Blocks::read(&data)?);
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

    let edge = edge_blocks(&blocks, step);
    let outputs = labelled_outputs(&blocks);

    let mut grid: HashMap<Pos, String> = HashMap::new();
    for i in 0..=last {
        let first = i == 0;
        let final_copy = i == last;
        for (pos, state) in blocks.iter() {
            if state == AIR
                || (!first && edge.contains(&pos))
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
    let mut stacked_entities = Vec::new();
    for i in 0..=last {
        for entity in &entities {
            let Some(pos) = schem::entity_pos(entity) else {
                continue;
            };
            let dropped = (i != 0 && edge.contains(&pos))
                || (i != last && outputs.signs.contains(&pos));
            if !dropped {
                stacked_entities
                    .push(schem::with_pos(entity, add(pos, shift(i))));
            }
        }
    }
    // The first copy stays at the south-east-bottom end and the rest go
    // west, north, and up, so the box's first block moved by this.
    let low = [-(last * west), 0, -(last * north)];
    nbt.write_blocks(low, size, &grid, stacked_entities)?;
    schem::encode(nbt)
}

/// Blocks outside the slice that a step lands inside the slice, so on the
/// previous copy, and the levers, signs, torches, and input dust on them. A
/// lever, sign, torch, or input dust on a block that stays, stays too,
/// unless it lands on one of the previous copy's blocks.
fn edge_blocks(blocks: &Blocks, step: Pos) -> HashSet<Pos> {
    let (mut low, mut high) = interface::core(blocks);
    // The previous copy lies the opposite way from `step`, so the slice is
    // the `step` layers at the core's far end.
    for axis in 0..3 {
        match step[axis] {
            n if n < 0 => high[axis] = high[axis].min(low[axis] - n - 1),
            n if n > 0 => low[axis] = low[axis].max(high[axis] - n + 1),
            _ => {}
        }
    }
    let inside = |pos: Pos| {
        (0..3).all(|axis| (low[axis]..=high[axis]).contains(&pos[axis]))
    };
    let lands_inside = |pos: Pos| !inside(pos) && inside(add(pos, step));
    // What each lever, sign, torch, and input's dust hangs on.
    let mut support: HashMap<Pos, Pos> = blocks
        .switches()
        .into_iter()
        .map(|lever| (lever.pos, lever.block))
        .collect();
    support.extend(
        blocks
            .signs
            .keys()
            .filter_map(|&sign| Some((sign, blocks.sign_block(sign)?))),
    );
    support.extend(blocks.iter().filter_map(|(pos, state)| {
        Some((pos, schem::torch_block(pos, state)?))
    }));
    support.extend(
        interface::dust_inputs(blocks)
            .into_keys()
            .map(|block| (schem::step(block, "up"), block)),
    );
    let mut edge: HashSet<Pos> = blocks
        .iter()
        .filter(|&(pos, state)| {
            state != AIR && !support.contains_key(&pos) && lands_inside(pos)
        })
        .map(|(pos, _)| pos)
        .collect();
    let hanging: Vec<Pos> = support
        .iter()
        .filter(|&(&pos, on)| {
            edge.contains(on)
                || (lands_inside(pos) && blocks.get(add(pos, step)) != AIR)
        })
        .map(|(&pos, _)| pos)
        .collect();
    edge.extend(hanging);
    edge
}

struct Outputs {
    blocks: HashSet<Pos>,
    signs: HashSet<Pos>,
}

/// The blocks of labelled outputs, such as a carry out, and their signs.
fn labelled_outputs(blocks: &Blocks) -> Outputs {
    let interface = interface::read(blocks);
    let outputs: HashSet<Pos> = interface
        .outputs
        .iter()
        .filter(|port| port.labelled)
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

/// Puts `state` at `pos`, refusing to overwrite a different block: copies
/// that collide mean the stacking step is wrong.
fn place(grid: &mut HashMap<Pos, String>, pos: Pos, state: &str) -> Result<()> {
    match grid.get(&pos) {
        Some(existing) if existing != state => {
            let [x, y, z] = pos;
            bail!(
                "copies collide at ({x}, {y}, {z}): {existing} and {state}; \
                 check the stacking step"
            );
        }
        _ => {
            grid.insert(pos, state.to_owned());
            Ok(())
        }
    }
}

fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
