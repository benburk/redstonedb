//! Copies of a build stacked as the site's stacking test stacks them,
//! which is `//rstack` as a player runs it on the site's box, with where
//! each copy's blocks went, to measure how copies meet. The stacked file is
//! the site's own, from [`stack::stack`]: the first copy whole, then each
//! later copy only the box, the build less the layers only the first copy
//! keeps, such as a carry in's lever and its block, one step further
//! along. `//rstack` repeats the selection that way, each repetition of the
//! original rather than of what the one before pasted, in order, and only
//! its blocks that aren't air, so a later copy's block stands over an
//! earlier one's and air never takes one out; dust meeting dust joins, as
//! `//update` reshapes it. [`paste`] lays the same copies out block by
//! block, to say which copy each block came from and where one covered
//! another. A player then runs `//update`; see [`settle`].
//!
//! The site stacks west, up, and north. A stack east or south is the
//! mirror image's stacked west or north, mirrored back.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Result, bail, ensure};

use crate::{
    models::schematic::{Faces, StackDirection},
    schem::{
        self, AIR, Blocks, Pos,
        orient::{Turn, turned},
        stack,
    },
};

/// The most copies one stack makes.
const MAX_COPIES: usize = 64;

/// A copy's block put over another copy's block that differs from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overwrite {
    /// Where, in the stacked file.
    pub at: Pos,
    /// The copy pasted there, 0 the first.
    pub copy: usize,
    /// The copy whose block it covered.
    pub over: usize,
    /// The block covered and the block now there.
    pub was: String,
    pub now: String,
}

/// Blocks by position, each with the copy that put it there.
pub type Pasted = BTreeMap<Pos, (String, usize)>;

/// `first`, the first copy at copy 0, then `rest`, the box later copies
/// repeat, pasted `copies - 1` more times, copy `i` moved `i` times
/// `step`, as `//rstack` pastes them: each block not air put over what is
/// there, air never; with every block put over a different one. Blocks the
/// same but for their power, or dust on dust, which `//update` settles,
/// cover nothing.
pub fn paste(
    first: &BTreeMap<Pos, String>,
    rest: &BTreeMap<Pos, String>,
    step: Pos,
    copies: usize,
) -> (Pasted, Vec<Overwrite>) {
    let mut grid: Pasted = BTreeMap::new();
    let mut overwrites = Vec::new();
    for copy in 0..copies {
        let by = i32::try_from(copy).unwrap_or(i32::MAX);
        let blocks = if copy == 0 { first } else { rest };
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

/// A way to stack, as `rsdb stack-check` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    West,
    East,
    Up,
    North,
    South,
}

impl Way {
    /// The way named `name`, such as `west`.
    pub fn named(name: &str) -> Result<Way> {
        Ok(match name {
            "west" => Way::West,
            "east" => Way::East,
            "up" => Way::Up,
            "north" => Way::North,
            "south" => Way::South,
            _ => bail!(
                "no way {name}: the site stacks west, up, or north, and east \
                 or south as a mirror image"
            ),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Way::West => "west",
            Way::East => "east",
            Way::Up => "up",
            Way::North => "north",
            Way::South => "south",
        }
    }

    /// How the build is mirrored to stack this way west or north, which
    /// mirrors it back too.
    fn mirror(self) -> Turn {
        match self {
            Way::East => Turn {
                mirrored: true,
                quarters: 0,
            },
            // Mirrored east for west and turned a half turn, it is mirrored
            // north for south.
            Way::South => Turn {
                mirrored: true,
                quarters: 2,
            },
            Way::West | Way::Up | Way::North => Turn::default(),
        }
    }

    /// The build in `data` as the site stacks it this way: mirrored for
    /// east or south, so it stacks west or north.
    pub fn lying(self, data: &[u8]) -> Result<Vec<u8>> {
        let mirror = self.mirror();
        if mirror.is_none() {
            Ok(data.to_vec())
        } else {
            turned(data, mirror)
        }
    }

    /// Whether the site stacks it as its mirror image: east or south.
    pub fn mirrored(self) -> bool {
        self.flipped().is_some()
    }

    /// The axis a mirror flips: x for east, z for south.
    fn flipped(self) -> Option<usize> {
        match self {
            Way::East => Some(0),
            Way::South => Some(2),
            Way::West | Way::Up | Way::North => None,
        }
    }

    /// The site's direction for `spacing` blocks this way, or for east or
    /// south its mirror's, with the box `faces` leaves, as the build lies.
    pub fn direction(
        self,
        spacing: i64,
        faces: Option<Faces>,
    ) -> StackDirection {
        let [west, up, north] = match self {
            Way::West | Way::East => [spacing, 0, 0],
            Way::Up => [0, spacing, 0],
            Way::North | Way::South => [0, 0, spacing],
        };
        StackDirection {
            west,
            up,
            north,
            max_copies: None,
            faces: faces.map(|faces| self.flip_faces(faces)),
        }
    }

    /// `faces` as the mirror image has them, or back: its low and high
    /// layers swapped on the flipped axis.
    pub fn flip_faces(self, mut faces: Faces) -> Faces {
        if let Some(axis) = self.flipped() {
            std::mem::swap(&mut faces.low[axis], &mut faces.high[axis]);
        }
        faces
    }
}

/// The build in `data` stacked as the site stacks it: `copies` in all,
/// each `step` from the one before, the first whole and the rest the box.
pub struct Stacked {
    /// The stacked file, the site's own, not yet settled.
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
    /// The blocks, in the original's places, only the first copy keeps.
    pub first_only: BTreeSet<Pos>,
}

impl Stacked {
    /// Where copy `copy`'s block at `pos` in the original lands.
    pub fn place(&self, copy: usize, pos: Pos) -> Pos {
        let by = i32::try_from(copy).unwrap_or(i32::MAX);
        [0, 1, 2]
            .map(|axis| self.origin[axis] + by * self.step[axis] + pos[axis])
    }
}

/// The build in `data` stacked `way`, `spacing` apart, `copies` in all,
/// as the site's stacking test stacks it with `buses`, its box the one
/// `faces` leaves, or the box rule's with `None`; see [`stack::stack`].
pub fn stack_as_site(
    data: &[u8],
    way: Way,
    spacing: i64,
    copies: usize,
    buses: stack::Buses,
    faces: Option<Faces>,
) -> Result<Stacked> {
    ensure!(
        (1..=MAX_COPIES).contains(&copies),
        "a stack has 1 to {MAX_COPIES} copies"
    );
    let mirror = way.mirror();
    let lying = way.lying(data)?;
    let direction = way.direction(spacing, faces);
    let (stacked, _) = stack::stack(&lying, &[(direction, copies)], buses)?;
    let blocks = |data: &[u8]| -> Result<BTreeMap<Pos, String>> {
        Ok(Blocks::read(data)?
            .iter()
            .filter(|(_, state)| *state != AIR)
            .map(|(pos, state)| (pos, state.to_owned()))
            .collect())
    };
    let whole = blocks(&lying)?;
    let boxed = blocks(&stack::part(&lying, direction, false)?)?;
    let first_only: BTreeSet<Pos> = whole
        .keys()
        .filter(|pos| !boxed.contains_key(*pos))
        .copied()
        .collect();
    let [west, up, north] =
        [direction.west, direction.up, direction.north].map(i32::try_from);
    let step = [-west?, up?, -north?];
    let (pasted, overwrites) = paste(&whole, &boxed, step, copies);
    let last = i32::try_from(copies - 1)?;
    let origin: Pos = [0, 1, 2].map(|axis| -(last * step[axis]).min(0));
    let size = Blocks::read(&lying)?.size;
    let stacked_size: Pos =
        [0, 1, 2].map(|axis| size[axis] + last * step[axis].abs());
    // A place in the mirror image's stack, as the stack lies.
    let back = |pos: Pos| -> Pos {
        let mut pos = schem::add(pos, origin);
        if let Some(axis) = way.flipped() {
            pos[axis] = stacked_size[axis] - 1 - pos[axis];
        }
        pos
    };
    // A place in the build, as its mirror image has it, and back.
    let mirrored = |mut pos: Pos| -> Pos {
        if let Some(axis) = way.flipped() {
            pos[axis] = size[axis] - 1 - pos[axis];
        }
        pos
    };
    let data = if mirror.is_none() {
        stacked
    } else {
        turned(&stacked, mirror)?
    };
    // Mirrored, copies step the other way along the flipped axis.
    let start = back(mirrored([0, 0, 0]));
    let mut forward = step;
    if let Some(axis) = way.flipped() {
        forward[axis] = -forward[axis];
    }
    Ok(Stacked {
        data,
        origin: start,
        step: forward,
        copies,
        overwrites: overwrites
            .into_iter()
            .map(|o| Overwrite {
                at: back(o.at),
                ..o
            })
            .collect(),
        copy_of: pasted
            .iter()
            .map(|(&pos, &(_, copy))| (back(pos), copy))
            .collect(),
        first_only: first_only.into_iter().map(mirrored).collect(),
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
        let (pasted, overwrites) = paste(&original, &original, [1, 0, 0], 3);
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
        let (pasted, overwrites) = paste(&original, &original, [-1, 0, 0], 2);
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
        let (pasted, overwrites) = paste(&original, &original, [1, 0, 0], 3);
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
        let (_, overwrites) = paste(&original, &original, [1, 0, 0], 2);
        assert!(overwrites.is_empty());
    }

    /// Later copies repeat only the box: a block only the first copy
    /// keeps, as a carry in's lever, is pasted once.
    #[test]
    fn later_copies_repeat_only_the_box() {
        let first = grid(&[
            ([0, 0, 0], "stone"),
            ([1, 0, 0], "lever[face=floor,facing=north,powered=false]"),
        ]);
        let rest = grid(&[([0, 0, 0], "stone")]);
        let (pasted, overwrites) = paste(&first, &rest, [-1, 0, 0], 3);
        assert!(overwrites.is_empty());
        let levers = pasted.values().filter(|(s, _)| s.contains("lever"));
        assert_eq!(levers.count(), 1);
        assert_eq!(pasted[&[-2, 0, 0]], ("minecraft:stone".to_owned(), 2));
    }

    /// Stacked west, the box grows west and the first copy sits at the
    /// east end; stacked east, as the mirror image stacked west mirrored
    /// back, it sits at the west end, and every copy's repeaters stand
    /// where `place` says, facing as they did.
    #[test]
    fn a_stack_east_is_the_mirror_of_one_west() {
        let data =
            std::fs::read("tests/fixtures/rca-1-repeater-a.schem").unwrap();
        let blocks = Blocks::read(&data).unwrap();
        for (way, origin, step) in [
            (Way::West, [4, 0, 0], [-2, 0, 0]),
            (Way::East, [0, 0, 0], [2, 0, 0]),
        ] {
            let stacked =
                stack_as_site(&data, way, 2, 3, stack::Buses::own(), None)
                    .unwrap();
            assert_eq!((stacked.origin, stacked.step), (origin, step));
            let after = Blocks::read(&stacked.data).unwrap();
            assert_eq!(
                after.size,
                [blocks.size[0] + 4, blocks.size[1], blocks.size[2]]
            );
            for (pos, state) in blocks.iter() {
                if schem::short(state) == "repeater" {
                    assert_eq!(
                        schem::as_placed(after.get(stacked.place(2, pos))),
                        schem::as_placed(state),
                        "{way:?} {pos:?}"
                    );
                }
            }
        }
    }
}
