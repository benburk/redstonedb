//! Changing the blocks a file holds: putting blocks in place and taking
//! them out, growing or fitting its box, and moving its signs and other
//! block entities along. Each change gives the blocks anew, in memory; the
//! functions that take a file's bytes read it, change it, and write it
//! back.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;

use super::{self as schem, AIR, Blocks, Pos};

impl Blocks {
    /// Every block but air, by position.
    pub fn grid(&self) -> HashMap<Pos, String> {
        self.iter()
            .filter(|(_, state)| *state != AIR)
            .map(|(pos, state)| (pos, state.to_owned()))
            .collect()
    }

    /// The blocks with each of `changes` put in place, air taking one out,
    /// their box and paste offset as they were.
    pub fn edited(&self, changes: &BTreeMap<Pos, String>) -> Result<Blocks> {
        let mut grid = self.grid();
        for (&pos, state) in changes {
            if state == AIR {
                grid.remove(&pos);
            } else {
                grid.insert(pos, state.clone());
            }
        }
        self.reboxed(&grid, [0, 0, 0], self.size)
    }

    /// The blocks with each of `changes` put in place, as [`Blocks::edited`]
    /// does, their box grown to hold any outside it, and each block entity
    /// at a key of `moves`, such as a sign's, moved to its value, `changes`
    /// putting its block there; and how far every old position moved.
    pub fn edited_grown<S: std::hash::BuildHasher>(
        &self,
        changes: &BTreeMap<Pos, String>,
        moves: &HashMap<Pos, Pos, S>,
    ) -> Result<(Blocks, Pos)> {
        let mut grid = self.grid();
        let (mut low, mut high) = ([0; 3], self.size.map(|n| n - 1));
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
        let grown =
            self.reboxed_with(&grid, low, size, &HashMap::new(), moves)?;
        Ok((grown, low.map(|n| -n)))
    }

    /// `grid`, in these blocks' own positions, in a box of `size` whose
    /// first block is at `low`, signs and other block entities moved along,
    /// each sign's lines plain text; see `schem::plain_sign`. Entities left
    /// outside the box are dropped.
    pub fn reboxed<G: std::hash::BuildHasher>(
        &self,
        grid: &HashMap<Pos, String, G>,
        low: Pos,
        size: Pos,
    ) -> Result<Blocks> {
        self.reboxed_with(grid, low, size, &HashMap::new(), &HashMap::new())
    }

    /// As [`Blocks::reboxed`], each comparator at a position of
    /// `strengths`, in the old box, putting out its strength, and each
    /// block entity at a key of `moves` moved to its value.
    pub(crate) fn reboxed_with<
        G: std::hash::BuildHasher,
        S: std::hash::BuildHasher,
    >(
        &self,
        grid: &HashMap<Pos, String, G>,
        low: Pos,
        size: Pos,
        strengths: &HashMap<Pos, u8>,
        moves: &HashMap<Pos, Pos, S>,
    ) -> Result<Blocks> {
        let shift = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] - low[axis]);
        let inside =
            |pos: Pos| (0..3).all(|axis| (0..size[axis]).contains(&pos[axis]));
        let placed: HashMap<Pos, String> = grid
            .iter()
            .map(|(&pos, state)| (shift(pos), state.clone()))
            .collect();
        let mut nbt = self.nbt()?;
        nbt.move_roles(|pos| Some(shift(pos)));
        // A block entity whose block was taken out goes with it, and a
        // sign's lines are written plain.
        let entities = nbt
            .take_entities()
            .iter()
            .filter_map(|entity| {
                let was = schem::entity_pos(entity)?;
                let pos = shift(moves.get(&was).copied().unwrap_or(was));
                let entity = match strengths.get(&was) {
                    Some(&strength) => {
                        schem::with_output_signal(entity, strength)
                    }
                    None => entity.clone(),
                };
                (inside(pos) && placed.contains_key(&pos))
                    .then(|| schem::with_pos(&schem::plain_sign(&entity), pos))
            })
            .collect();
        nbt.write_blocks(low, size, &placed, entities)?;
        Blocks::of(nbt)
    }
}

/// The file with each block of `changes` put in place, air taking one out,
/// its box and paste offset as they were, so `tidy` fits it afterwards.
pub fn edit(data: &[u8], changes: &BTreeMap<Pos, String>) -> Result<Vec<u8>> {
    Blocks::read(data)?.edited(changes)?.encode()
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
    let (grown, shift) = Blocks::read(data)?.edited_grown(changes, moves)?;
    Ok((grown.encode()?, shift))
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
    blocks.reboxed(&grid, [0, 0, 0], blocks.size)?.encode()
}

/// Writes `grid`, in the file's own positions, into a box of `size` whose
/// first block is at `low`; see [`Blocks::reboxed`].
pub fn rebox<G: std::hash::BuildHasher>(
    data: &[u8],
    grid: &HashMap<Pos, String, G>,
    low: Pos,
    size: Pos,
) -> Result<Vec<u8>> {
    Blocks::read(data)?.reboxed(grid, low, size)?.encode()
}
