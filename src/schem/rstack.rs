//! `//rstack` as the `RedstoneTools` plugin runs it, block for block, to
//! measure how copies of a build meet, unlike `stack`, which joins copies
//! the way the site lays a stack out. `//rstack` hands `WorldEdit` a
//! forward copy of the selection, repeated `count` times, each repetition
//! one step further along, the step the direction times the spacing, 2
//! unless given. Every repetition copies the original selection, never
//! what the one before pasted, in order, and only its blocks that aren't
//! air (`-w` would copy air too), so a later repetition's block stands
//! over an earlier one's and air never takes one out. A player then runs
//! `//update` so dust takes its shape and every component settles; see
//! [`settle`].

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Result, ensure};

use crate::schem::{self, AIR, Blocks, Pos};

/// The most copies one stack makes.
const MAX_COPIES: usize = 64;

/// A copy's block put over another copy's block that differs from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overwrite {
    /// Where, in the stacked file.
    pub at: Pos,
    /// The copy pasted there, 0 the original.
    pub copy: usize,
    /// The copy whose block it covered.
    pub over: usize,
    /// The block covered and the block now there.
    pub was: String,
    pub now: String,
}

/// Blocks by position, each with the copy that put it there.
pub type Pasted = BTreeMap<Pos, (String, usize)>;

/// `blocks`, the original at copy 0, pasted `copies - 1` more times, copy
/// `i` moved `i` times `step`, as `//rstack` pastes them: each block not
/// air put over what is there, air never; with every block put over a
/// different one. Blocks the same but for their power, which `//update`
/// settles, cover nothing.
pub fn paste(
    blocks: &BTreeMap<Pos, String>,
    step: Pos,
    copies: usize,
) -> (Pasted, Vec<Overwrite>) {
    let mut grid: Pasted = BTreeMap::new();
    let mut overwrites = Vec::new();
    for copy in 0..copies {
        let by = i32::try_from(copy).unwrap_or(i32::MAX);
        for (&pos, state) in blocks {
            if state == AIR {
                continue;
            }
            let to = [0, 1, 2].map(|axis| pos[axis] + by * step[axis]);
            if let Some((was, over)) = grid.get(&to)
                && schem::as_placed(was) != schem::as_placed(state)
            {
                overwrites.push(Overwrite {
                    at: to,
                    copy,
                    over: *over,
                    was: was.clone(),
                    now: state.clone(),
                });
            }
            grid.insert(to, (state.clone(), copy));
        }
    }
    (grid, overwrites)
}

/// The build in `data` `//rstack`ed: `copies` in all, the original and
/// `copies - 1` repetitions, each `step` from the one before.
pub struct Stacked {
    /// The stacked file, not yet settled.
    pub data: Vec<u8>,
    /// Where copy 0's first block is in it: copy `i`'s block at `pos` in
    /// the original is at `origin + i * step + pos`.
    pub origin: Pos,
    pub step: Pos,
    pub copies: usize,
    /// Each block put over another copy's that differs, in the stacked
    /// file's places.
    pub overwrites: Vec<Overwrite>,
    /// Each block's copy, in the stacked file's places.
    pub copy_of: HashMap<Pos, usize>,
}

impl Stacked {
    /// Where copy `copy`'s block at `pos` in the original lands.
    pub fn place(&self, copy: usize, pos: Pos) -> Pos {
        let by = i32::try_from(copy).unwrap_or(i32::MAX);
        [0, 1, 2]
            .map(|axis| self.origin[axis] + by * self.step[axis] + pos[axis])
    }
}

/// The build in `data` stacked as `//rstack` stacks the whole of it, a
/// step `step` apart, `copies` in all; its block entities, such as signs,
/// copied with their blocks, a later copy's standing, and one whose block
/// a later copy covered with another going with it.
pub fn rstack(data: &[u8], step: Pos, copies: usize) -> Result<Stacked> {
    ensure!(
        (1..=MAX_COPIES).contains(&copies),
        "a stack has 1 to {MAX_COPIES} copies"
    );
    let blocks = Blocks::read(data)?;
    let original: BTreeMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let (pasted, overwrites) = paste(&original, step, copies);
    let last = i32::try_from(copies - 1)?;
    // The stacked box, from the original's and the last copy's corners.
    let low: Pos = [0, 1, 2].map(|axis| (last * step[axis]).min(0));
    let high: Pos = [0, 1, 2]
        .map(|axis| blocks.size[axis] - 1 + (last * step[axis]).max(0));
    let size: Pos = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    let origin = low.map(|n| -n);
    let to_file = |pos: Pos| schem::add(pos, origin);
    let grid: HashMap<Pos, String> = pasted
        .iter()
        .map(|(&pos, (state, _))| (to_file(pos), state.clone()))
        .collect();
    let copy_of: HashMap<Pos, usize> = pasted
        .iter()
        .map(|(&pos, &(_, copy))| (to_file(pos), copy))
        .collect();
    let mut nbt = schem::decode(data)?;
    let entities = nbt.take_entities();
    let mut placed: HashMap<Pos, (String, fastnbt::Value)> = HashMap::new();
    for copy in 0..copies {
        let by = i32::try_from(copy)?;
        for entity in &entities {
            let Some(pos) = schem::entity_pos(entity) else {
                continue;
            };
            let to = [0, 1, 2]
                .map(|axis| pos[axis] + by * step[axis] + origin[axis]);
            let block = schem::id(blocks.get(pos)).to_owned();
            placed.insert(to, (block, schem::with_pos(entity, to)));
        }
    }
    let kept = placed
        .into_iter()
        .filter(|(pos, (block, _))| {
            grid.get(pos).is_some_and(|state| schem::id(state) == block)
        })
        .map(|(_, (_, entity))| entity)
        .collect();
    nbt.write_blocks(low, size, &grid, kept)?;
    Ok(Stacked {
        data: schem::encode(nbt)?,
        origin,
        step,
        copies,
        overwrites: overwrites
            .into_iter()
            .map(|o| Overwrite {
                at: to_file(o.at),
                ..o
            })
            .collect(),
        copy_of,
    })
}

/// The file in `data` as `//update` leaves it: its dust shaped as the game
/// shapes it, then every component at rest, each lever as it was.
pub fn settle(data: &[u8]) -> Result<Vec<u8>> {
    let data = schem::dust::update(data)?;
    let blocks = Blocks::read(&data)?;
    let on: BTreeSet<Pos> = blocks
        .switches()
        .into_iter()
        .filter(|switch| switch.on && !switch.button)
        .map(|switch| switch.pos)
        .collect();
    schem::standardize::with_levers(&data, &on)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(blocks: &[(Pos, &str)]) -> BTreeMap<Pos, String> {
        blocks
            .iter()
            .map(|&(pos, state)| (pos, format!("minecraft:{state}")))
            .collect()
    }

    /// Copies are pasted in order, so where two copies' blocks land in one
    /// place, the later copy's stands, and each such place is listed with
    /// the block covered.
    #[test]
    fn a_later_copy_stands_over_an_earlier_one() {
        let original =
            grid(&[([0, 0, 0], "stone"), ([1, 0, 0], "yellow_concrete")]);
        let (pasted, overwrites) = paste(&original, [1, 0, 0], 3);
        assert_eq!(pasted.len(), 4);
        assert_eq!(pasted[&[1, 0, 0]], ("minecraft:stone".to_owned(), 1));
        assert_eq!(pasted[&[2, 0, 0]], ("minecraft:stone".to_owned(), 2));
        assert_eq!(
            pasted[&[3, 0, 0]],
            ("minecraft:yellow_concrete".to_owned(), 2)
        );
        let covered: Vec<(Pos, usize, usize)> =
            overwrites.iter().map(|o| (o.at, o.copy, o.over)).collect();
        assert_eq!(covered, [([1, 0, 0], 1, 0), ([2, 0, 0], 2, 1)]);
    }

    /// A copy's air is never pasted, so it leaves an earlier copy's block
    /// where it lands: only blocks are copied.
    #[test]
    fn air_never_covers_a_block() {
        let original = grid(&[([0, 0, 0], "stone"), ([1, 0, 0], "air")]);
        let (pasted, overwrites) = paste(&original, [-1, 0, 0], 2);
        assert_eq!(pasted.len(), 2);
        assert_eq!(pasted[&[0, 0, 0]], ("minecraft:stone".to_owned(), 0));
        assert!(overwrites.is_empty());
    }

    /// Every repetition copies the original, not what the one before it
    /// pasted: a block a copy covered is not carried on by the next.
    #[test]
    fn each_copy_is_of_the_original() {
        let original = grid(&[
            ([0, 0, 0], "repeater[delay=1,facing=west]"),
            ([1, 0, 0], "lever[face=floor,facing=north,powered=false]"),
        ]);
        let (pasted, overwrites) = paste(&original, [1, 0, 0], 3);
        // Copy 1's repeater covers copy 0's lever, and copy 2's repeater
        // copy 1's lever; copy 2's lever stands alone at the end.
        assert_eq!(overwrites.len(), 2);
        assert!(pasted[&[1, 0, 0]].0.contains("repeater"));
        assert!(pasted[&[2, 0, 0]].0.contains("repeater"));
        assert!(pasted[&[3, 0, 0]].0.contains("lever"));
    }

    /// A block that differs only in its power covers nothing: `//update`
    /// sets its power again.
    #[test]
    fn a_block_differing_only_in_power_covers_nothing() {
        let original = grid(&[
            ([0, 0, 0], "redstone_torch[lit=true]"),
            ([1, 0, 0], "redstone_torch[lit=false]"),
        ]);
        let (_, overwrites) = paste(&original, [1, 0, 0], 2);
        assert!(overwrites.is_empty());
    }

    /// The stacked file holds every copy, the box grown toward the step,
    /// and each copy's block is where `place` says.
    #[test]
    fn a_stack_west_grows_the_box_west() {
        let data =
            std::fs::read("tests/fixtures/rca-1-repeater-a.schem").unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let stacked = rstack(&data, [-2, 0, 0], 3).unwrap();
        let after = Blocks::read(&stacked.data).unwrap();
        assert_eq!(
            after.size,
            [blocks.size[0] + 4, blocks.size[1], blocks.size[2]]
        );
        assert_eq!(stacked.origin, [4, 0, 0]);
        // The last copy's blocks all stand, nothing pasted after it.
        for (pos, state) in blocks.iter().filter(|(_, s)| *s != AIR) {
            assert_eq!(after.get(stacked.place(2, pos)), state);
        }
    }
}
