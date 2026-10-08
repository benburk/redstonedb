//! Shaping a build's file: fitting its box tightly, adding corner markers
//! for `WorldEdit` to a download, and bringing a downloaded build into the
//! building conventions' palette.
//!
//! Standardizing needs the build's inputs and outputs already marked, since
//! everything else turns white:
//!
//! - Old corner markers are taken out: any group of plain glass that
//!   touches nothing but air, wherever it sits.
//! - Slabs, stairs, and every kind of glass become white stained glass;
//!   every other solid block becomes white concrete. Redstone components,
//!   input blocks, and output blocks are kept.
//! - The box is fitted tightly around what is left.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, ensure};

use crate::{
    interface, lint,
    schem::{self, AIR, Blocks, DIRECTIONS, Pos},
};

const WHITE: &str = "minecraft:white_concrete";
const WHITE_GLASS: &str = "minecraft:white_stained_glass";
const GLASS: &str = "minecraft:glass";

pub fn standardize(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let old_markers = loose_glass(&blocks, false);
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(pos, state)| *state != AIR && !old_markers.contains(pos))
        .map(|(pos, state)| (pos, restyle(state)))
        .collect();
    let restyled = rebox(data, &grid, [0, 0, 0], blocks.size)?;
    let standardized = tidy(&restyled)?;

    let found = interface::read(&Blocks::read(&standardized)?);
    ensure!(
        !found.inputs.is_empty(),
        "no inputs found: mark each input with a lever on a lime, light \
         blue, or magenta block before standardizing"
    );
    ensure!(
        !found.outputs.is_empty(),
        "no outputs found: mark each output with yellow concrete or yellow \
         stained glass before standardizing"
    );
    Ok(standardized)
}

/// The file with any corner markers taken out, its box fitted tightly
/// around the blocks left, and its bytes written the one way the site
/// writes them, so the same build always gives the same file. A marker is
/// loose glass, as [`loose_glass`] finds it, lying on the box's edges: the
/// old corner markers, a corner and its three neighbours, or `WorldEdit`'s
/// single corner blocks.
pub fn tidy(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let markers = loose_glass(&blocks, true);
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(pos, state)| *state != AIR && !markers.contains(pos))
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    ensure!(!grid.is_empty(), "the schematic has no blocks");
    let bound = |axis: usize, pick: fn(i32, i32) -> i32| {
        grid.keys().map(|pos| pos[axis]).reduce(pick).unwrap_or(0)
    };
    let low = [0, 1, 2].map(|axis| bound(axis, i32::min));
    let high = [0, 1, 2].map(|axis| bound(axis, i32::max));
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    rebox(data, &grid, low, size)
}

/// The build with a plain glass block at each corner of a box one block
/// bigger on every side, for `WorldEdit` to select it by. The glass sits
/// diagonally off the build's corners, so it never touches the build.
pub fn with_corner_markers(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let [w, h, l] = blocks.size;
    for x in [-1, w] {
        for y in [-1, h] {
            for z in [-1, l] {
                grid.insert([x, y, z], GLASS.to_owned());
            }
        }
    }
    rebox(data, &grid, [-1, -1, -1], [w + 2, h + 2, l + 2])
}

/// Writes `grid`, in the file's own positions, into a box of `size` whose
/// first block is at `low`, moving signs and other block entities along.
/// Entities left outside the box are dropped.
fn rebox(
    data: &[u8],
    grid: &HashMap<Pos, String>,
    low: Pos,
    size: Pos,
) -> Result<Vec<u8>> {
    let shift = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] - low[axis]);
    let inside =
        |pos: Pos| (0..3).all(|axis| (0..size[axis]).contains(&pos[axis]));
    let placed: HashMap<Pos, String> = grid
        .iter()
        .map(|(&pos, state)| (shift(pos), state.clone()))
        .collect();
    let mut nbt = schem::decode(data)?;
    let entities = nbt
        .take_entities()
        .iter()
        .filter_map(|entity| {
            let pos = shift(schem::entity_pos(entity)?);
            inside(pos).then(|| schem::with_pos(entity, pos))
        })
        .collect();
    nbt.write_blocks(size, &placed, entities)?;
    schem::encode(nbt)
}

/// Plain glass that is no part of the build: old corner markers. A group
/// of glass is loose when it touches nothing but air and levers and signs
/// hanging on other blocks; glass touching anything else may carry dust, so
/// it stays. With `on_edges`, only groups lying wholly on the box's edges
/// count, as markers do; without, misplaced ones count too.
fn loose_glass(blocks: &Blocks, on_edges: bool) -> HashSet<Pos> {
    let size = blocks.size;
    // On an edge of the box: at its first or last block along two axes.
    let on_edge = |pos: Pos| {
        (0..3)
            .filter(|&axis| pos[axis] == 0 || pos[axis] == size[axis] - 1)
            .count()
            >= 2
    };
    let glass: HashSet<Pos> = blocks
        .iter()
        .filter(|&(_, state)| state == GLASS)
        .map(|(pos, _)| pos)
        .collect();
    // The blocks a lever or sign hangs on, which must stay.
    let mut held: HashSet<Pos> = blocks
        .levers()
        .into_iter()
        .map(|lever| lever.block)
        .collect();
    held.extend(
        blocks
            .signs
            .keys()
            .filter_map(|&sign| blocks.sign_block(sign)),
    );
    let harmless = |state: &str| {
        let id = schem::id(state);
        state == AIR || id == "minecraft:lever" || id.ends_with("sign")
    };
    let mut loose = HashSet::new();
    let mut seen = HashSet::new();
    for &start in &glass {
        if !seen.insert(start) {
            continue;
        }
        // Gather the group of glass connected to `start`.
        let mut group = vec![start];
        let mut touches_build = false;
        let mut next = 0;
        while next < group.len() {
            let pos = group[next];
            next += 1;
            for direction in DIRECTIONS {
                let neighbour = schem::step(pos, direction);
                if glass.contains(&neighbour) {
                    if seen.insert(neighbour) {
                        group.push(neighbour);
                    }
                } else if !harmless(blocks.get(neighbour)) {
                    touches_build = true;
                }
            }
        }
        let carries = group.iter().any(|pos| held.contains(pos));
        if !touches_build
            && !carries
            && (!on_edges || group.iter().all(|&p| on_edge(p)))
        {
            loose.extend(group);
        }
    }
    loose
}

/// The block a standardized build uses in place of `state`.
fn restyle(state: &str) -> String {
    let id = schem::id(state);
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    if lint::keeps(name) {
        state.to_owned()
    } else if name.contains("glass")
        || name.ends_with("_slab")
        || name.ends_with("_stairs")
        || name == "glowstone"
    {
        WHITE_GLASS.to_owned()
    } else {
        WHITE.to_owned()
    }
}
