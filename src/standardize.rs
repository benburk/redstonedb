//! Bringing a downloaded build into the building conventions' palette and
//! shape. Its inputs and outputs must already be marked, since everything
//! else turns white:
//!
//! - Old corner markers are taken out.
//! - Slabs, stairs, and every kind of glass become white stained glass;
//!   every other solid block becomes white concrete. Redstone components,
//!   input blocks, and output blocks are kept.
//! - The box is fitted: empty outer layers are trimmed to leave a one-block
//!   margin, and a face whose outer layer holds more than levers, signs,
//!   input blocks, and torches on input blocks grows a new layer.
//! - Glass goes in all 32 corner marker spots.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail, ensure};

use crate::{
    interface, lint,
    schem::{self, AIR, Blocks, Pos},
};

const WHITE: &str = "minecraft:white_concrete";
const WHITE_GLASS: &str = "minecraft:white_stained_glass";
const GLASS: &str = "minecraft:glass";

pub fn standardize(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let old_markers: HashSet<Pos> = blocks
        .marker_positions()
        .into_iter()
        .filter(|&pos| blocks.get(pos) == GLASS)
        .collect();
    let mut grid: HashMap<Pos, String> = HashMap::new();
    for (pos, state) in blocks.iter() {
        if state != AIR && !old_markers.contains(&pos) {
            grid.insert(pos, restyle(state));
        }
    }

    let (low, high) = fit(&grid, blocks.size);
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    let shift = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] - low[axis]);
    let mut placed: HashMap<Pos, String> = grid
        .into_iter()
        .map(|(pos, state)| (shift(pos), state))
        .collect();
    for pos in schem::marker_positions(size) {
        if let Some(state) = placed.get(&pos) {
            let [x, y, z] = pos;
            bail!(
                "{} sits where a corner marker goes, at ({x}, {y}, {z}); \
                 leave the corners of the box clear",
                interface::block_label(state)
            );
        }
        placed.insert(pos, GLASS.to_owned());
    }

    let mut nbt = schem::decode(data)?;
    let entities = nbt
        .take_entities()
        .iter()
        .filter_map(|entity| {
            let pos = schem::entity_pos(entity)?;
            Some(schem::with_pos(entity, shift(pos)))
        })
        .collect();
    nbt.write_blocks(size, &placed, entities)?;
    let standardized = schem::encode(nbt)?;

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

/// The bounds, inclusive, of a box with a one-block shell around the build.
fn fit(grid: &HashMap<Pos, String>, size: Pos) -> (Pos, Pos) {
    let layer = |axis: usize, at: i32| {
        grid.iter()
            .filter(move |(pos, _)| pos[axis] == at)
            .map(|(pos, state)| (*pos, state.as_str()))
    };
    let empty = |axis: usize, at: i32| layer(axis, at).next().is_none();
    let shell_ok = |axis: usize, at: i32| {
        layer(axis, at).all(|(pos, state)| {
            let support = schem::torch_block(pos, state)
                .map(|block| grid.get(&block).map_or(AIR, String::as_str));
            lint::allowed_in_shell(state, support)
        })
    };
    let mut low = [0; 3];
    let mut high = size.map(|n| n - 1);
    for axis in 0..3 {
        while high[axis] - low[axis] >= 2
            && empty(axis, low[axis])
            && empty(axis, low[axis] + 1)
        {
            low[axis] += 1;
        }
        if !shell_ok(axis, low[axis]) {
            low[axis] -= 1;
        }
        while high[axis] - low[axis] >= 2
            && empty(axis, high[axis])
            && empty(axis, high[axis] - 1)
        {
            high[axis] -= 1;
        }
        if !shell_ok(axis, high[axis]) {
            high[axis] += 1;
        }
    }
    (low, high)
}
