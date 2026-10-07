//! Copies of a stackable build joined into one schematic.
//!
//! Copy 0 sits at the south-east-bottom corner and each later copy one
//! step further along. A copy's blocks inside the corner-marker shell are
//! its slice; its blocks in the shell that reach into the previous copy's
//! slice, such as the lever, sign, and torch feeding a carry in or a
//! control line, belong to the edge of the stack and are dropped, along
//! with any lever or sign on them. The previous copy's blocks stand there.
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
const GLASS: &str = "minecraft:glass";

/// The stored file stacked with `copies` along each direction, with the
/// inner size of the result.
pub fn stack(
    data: &[u8],
    copies: &[(StackDirection, usize)],
) -> Result<(Vec<u8>, Pos)> {
    let mut data = data.to_vec();
    for &(direction, count) in copies {
        data = along(&data, direction, count)?;
    }
    let size = Blocks::read(&data)?.size.map(|n| n - 2);
    Ok((data, size))
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
    let markers: HashSet<Pos> = blocks.marker_positions().into_iter().collect();

    let mut grid: HashMap<Pos, String> = HashMap::new();
    for i in 0..=last {
        let first = i == 0;
        let final_copy = i == last;
        for (pos, state) in blocks.iter() {
            if state == AIR
                || markers.contains(&pos)
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
    for pos in schem::marker_positions(size) {
        place(&mut grid, pos, GLASS)?;
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
    nbt.write_blocks(size, &grid, stacked_entities)?;
    schem::encode(nbt)
}

/// Blocks in the shell around the slice that a step lands inside the
/// slice, so on the previous copy, with the levers and signs on them.
fn edge_blocks(blocks: &Blocks, step: Pos) -> HashSet<Pos> {
    let [w, h, l] = blocks.size;
    let inside = |[x, y, z]: Pos| {
        (1..w - 1).contains(&x)
            && (1..h - 1).contains(&y)
            && (1..l - 1).contains(&z)
    };
    let mut edge: HashSet<Pos> = blocks
        .iter()
        .map(|(pos, _)| pos)
        .filter(|&pos| !inside(pos) && inside(add(pos, step)))
        .collect();
    let levers: Vec<Pos> = blocks
        .levers()
        .into_iter()
        .filter(|lever| edge.contains(&lever.block))
        .map(|lever| lever.pos)
        .collect();
    let signs: Vec<Pos> = blocks
        .signs
        .keys()
        .copied()
        .filter(|&sign| {
            blocks.sign_block(sign).is_some_and(|b| edge.contains(&b))
        })
        .collect();
    edge.extend(levers);
    edge.extend(signs);
    edge
}

struct Outputs {
    blocks: HashSet<Pos>,
    signs: HashSet<Pos>,
}

/// The blocks of labelled outputs, such as a carry out, and their signs.
fn labelled_outputs(blocks: &Blocks) -> Outputs {
    let interface = interface::read(blocks);
    // Interface positions count from inside the corner markers.
    let outputs: HashSet<Pos> = interface
        .outputs
        .iter()
        .filter(|port| port.labelled)
        .flat_map(|port| port.bits.iter().map(|bit| bit.pos.map(|n| n + 1)))
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
