//! Changing the blocks a file holds: putting blocks in place and taking
//! them out, growing or fitting its box, and moving its signs and other
//! block entities along.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;

use super::{self as schem, AIR, Blocks, Pos};

/// The file with each block of `changes` put in place, air taking one out,
/// its box and paste offset as they were, so `tidy` fits it afterwards.
pub fn edit(data: &[u8], changes: &BTreeMap<Pos, String>) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    for (&pos, state) in changes {
        if state == AIR {
            grid.remove(&pos);
        } else {
            grid.insert(pos, state.clone());
        }
    }
    rebox(data, &grid, [0, 0, 0], blocks.size)
}

/// The file with each block of `changes` put in place, as `edit` does, its
/// box grown to hold any outside it; and how far every old position moved.
pub fn edit_grown(
    data: &[u8],
    changes: &BTreeMap<Pos, String>,
) -> Result<(Vec<u8>, Pos)> {
    edit_grown_moving(data, changes, &HashMap::new())
}

/// As [`edit_grown`], each block entity at a key of `moves`, such as a
/// sign's, moved to its value, `changes` putting its block there.
pub fn edit_grown_moving<S: std::hash::BuildHasher>(
    data: &[u8],
    changes: &BTreeMap<Pos, String>,
    moves: &HashMap<Pos, Pos, S>,
) -> Result<(Vec<u8>, Pos)> {
    let blocks = Blocks::read(data)?;
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let (mut low, mut high) = ([0; 3], blocks.size.map(|n| n - 1));
    for (&pos, state) in changes {
        for axis in 0..3 {
            low[axis] = low[axis].min(pos[axis]);
            high[axis] = high[axis].max(pos[axis]);
        }
        if state == AIR {
            grid.remove(&pos);
        } else {
            grid.insert(pos, state.clone());
        }
    }
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    let file = rebox_with(data, &grid, low, size, &HashMap::new(), moves)?;
    Ok((file, low.map(|n| -n)))
}

/// `data` as the 3D viewer, schematic-renderer, should be given it: it
/// turns a wall sign facing east or west the wrong way round, onto the far
/// side of its own space, a block from what it hangs on, so each such sign
/// is given facing the other way.
pub fn for_viewer(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let mut swapped = false;
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| {
            let wall_sign = schem::id(state).ends_with("_wall_sign");
            let facing = schem::property(state, "facing");
            let turned = match facing {
                Some("east") if wall_sign => {
                    state.replace("facing=east", "facing=west")
                }
                Some("west") if wall_sign => {
                    state.replace("facing=west", "facing=east")
                }
                _ => return (pos, state.to_owned()),
            };
            swapped = true;
            (pos, turned)
        })
        .collect();
    if !swapped {
        return Ok(data.to_vec());
    }
    rebox(data, &grid, [0, 0, 0], blocks.size)
}

/// Writes `grid`, in the file's own positions, into a box of `size` whose
/// first block is at `low`, moving signs and other block entities along,
/// each sign's lines plain text; see `schem::plain_sign`.
/// Entities left outside the box are dropped.
pub fn rebox<G: std::hash::BuildHasher>(
    data: &[u8],
    grid: &HashMap<Pos, String, G>,
    low: Pos,
    size: Pos,
) -> Result<Vec<u8>> {
    rebox_with(data, grid, low, size, &HashMap::new(), &HashMap::new())
}

/// As `rebox`, each comparator at a position of `strengths`, in the old
/// box, putting out its strength, and each block entity at a key of
/// `moves` moved to its value.
pub(crate) fn rebox_with<
    G: std::hash::BuildHasher,
    S: std::hash::BuildHasher,
>(
    data: &[u8],
    grid: &HashMap<Pos, String, G>,
    low: Pos,
    size: Pos,
    strengths: &HashMap<Pos, u8>,
    moves: &HashMap<Pos, Pos, S>,
) -> Result<Vec<u8>> {
    let shift = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] - low[axis]);
    let inside =
        |pos: Pos| (0..3).all(|axis| (0..size[axis]).contains(&pos[axis]));
    let placed: HashMap<Pos, String> = grid
        .iter()
        .map(|(&pos, state)| (shift(pos), state.clone()))
        .collect();
    let mut nbt = schem::decode(data)?;
    nbt.move_roles(|pos| Some(shift(pos)));
    // A block entity whose block was taken out goes with it, and a sign's
    // lines are written plain.
    let entities = nbt
        .take_entities()
        .iter()
        .filter_map(|entity| {
            let was = schem::entity_pos(entity)?;
            let pos = shift(moves.get(&was).copied().unwrap_or(was));
            let entity = match strengths.get(&was) {
                Some(&strength) => schem::with_output_signal(entity, strength),
                None => entity.clone(),
            };
            (inside(pos) && placed.contains_key(&pos))
                .then(|| schem::with_pos(&schem::plain_sign(&entity), pos))
        })
        .collect();
    nbt.write_blocks(low, size, &placed, entities)?;
    schem::encode(nbt)
}
