//! How alike two builds are: the share of their redstone components that
//! line up, each the same kind facing the same way, once one build is
//! turned, mirrored, and shifted to line up with the other best. Filler
//! blocks are left out, so a copy rebuilt in other blocks is still a copy,
//! and power is left out, as a resave changes it.
//!
//! Every pair of like components votes for the shift that would line them
//! up, so the shift with the most votes lines up the most, and no shift is
//! guessed. Among the stored adders, copies and small fixes of one build
//! score over 90%, a builder's revisions of a design 70% to 77%, and
//! different builds of one design mostly under 60%.

use std::collections::HashMap;

use crate::schem::{self, Blocks, Pos};

/// The share, in tenths of a percent, from which a build is listed as
/// similar to another: a copy or a small fix of it, not just a build of
/// the same design.
pub const SIMILAR: u32 = 900;

/// The way a component faces, or none, as for a standing torch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Facing {
    None,
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl Facing {
    fn of(state: &str) -> Self {
        match schem::property(state, "facing") {
            Some("north") => Facing::North,
            Some("east") => Facing::East,
            Some("south") => Facing::South,
            Some("west") => Facing::West,
            Some("up") => Facing::Up,
            Some("down") => Facing::Down,
            _ => Facing::None,
        }
    }

    /// A quarter turn clockwise seen from above, as `Skeleton::oriented`
    /// turns positions.
    fn turned(self) -> Self {
        match self {
            Facing::North => Facing::East,
            Facing::East => Facing::South,
            Facing::South => Facing::West,
            Facing::West => Facing::North,
            other => other,
        }
    }

    /// Mirrored east for west.
    fn mirrored(self) -> Self {
        match self {
            Facing::East => Facing::West,
            Facing::West => Facing::East,
            other => other,
        }
    }
}

/// A redstone component and the way it faces.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Part {
    kind: &'static str,
    facing: Facing,
}

/// The blocks compared: those that carry, make, or read a signal.
const KINDS: [&str; 12] = [
    "redstone_wire",
    "repeater",
    "comparator",
    "redstone_torch",
    "redstone_wall_torch",
    "target",
    "observer",
    "piston",
    "sticky_piston",
    "redstone_block",
    "lever",
    "hopper",
];

/// A build's redstone components by position: what is compared.
pub struct Skeleton(Vec<(Pos, Part)>);

impl Skeleton {
    pub fn of(blocks: &Blocks) -> Self {
        Skeleton(
            blocks
                .iter()
                .filter_map(|(pos, state)| {
                    let id = schem::id(state).trim_start_matches("minecraft:");
                    let kind = KINDS.into_iter().find(|kind| *kind == id)?;
                    let facing = Facing::of(state);
                    Some((pos, Part { kind, facing }))
                })
                .collect(),
        )
    }

    /// Its components turned a quarter clockwise `turns` times about the
    /// vertical, mirrored first if `mirror`.
    fn oriented(&self, turns: usize, mirror: bool) -> Vec<(Pos, Part)> {
        self.0
            .iter()
            .map(|&([mut x, y, mut z], part)| {
                let mut facing = part.facing;
                if mirror {
                    x = -x;
                    facing = facing.mirrored();
                }
                for _ in 0..turns {
                    (x, z) = (-z, x);
                    facing = facing.turned();
                }
                ([x, y, z], Part { facing, ..part })
            })
            .collect()
    }
}

/// How alike `a` and `b` are, in tenths of a percent: twice the components
/// that line up, at the turn, mirroring, and shift that lines up the most,
/// over both builds' components together. Builds without components are
/// nothing alike.
pub fn permille(a: &Skeleton, b: &Skeleton) -> u32 {
    let total = a.0.len() + b.0.len();
    if a.0.is_empty() || b.0.is_empty() {
        return 0;
    }
    let mut at: HashMap<Part, Vec<Pos>> = HashMap::new();
    for &(pos, part) in &a.0 {
        at.entry(part).or_default().push(pos);
    }
    let mut most = 0;
    for mirror in [false, true] {
        for turns in 0..4 {
            let mut votes: HashMap<Pos, usize> = HashMap::new();
            for (p, part) in b.oriented(turns, mirror) {
                for q in at.get(&part).into_iter().flatten() {
                    *votes
                        .entry([q[0] - p[0], q[1] - p[1], q[2] - p[2]])
                        .or_default() += 1;
                }
            }
            most = most.max(votes.into_values().max().unwrap_or(0));
        }
    }
    u32::try_from(most * 2 * 1000 / total).unwrap_or(1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: &'static str, facing: Facing) -> Part {
        Part { kind, facing }
    }

    /// A small L of dust ending in a repeater facing east.
    fn build() -> Skeleton {
        Skeleton(vec![
            ([0, 0, 0], part("redstone_wire", Facing::None)),
            ([0, 0, 1], part("redstone_wire", Facing::None)),
            ([0, 0, 2], part("redstone_wire", Facing::None)),
            ([1, 0, 2], part("repeater", Facing::East)),
            ([2, 1, 2], part("comparator", Facing::North)),
        ])
    }

    #[test]
    fn a_build_is_wholly_like_itself_however_it_is_turned_or_mirrored() {
        let original = build();
        assert_eq!(permille(&original, &build()), 1000);
        for turns in 0..4 {
            for mirror in [false, true] {
                let moved = Skeleton(
                    original
                        .oriented(turns, mirror)
                        .into_iter()
                        .map(|([x, y, z], part)| ([x + 7, y + 3, z - 2], part))
                        .collect(),
                );
                assert_eq!(
                    permille(&original, &moved),
                    1000,
                    "{turns} {mirror}"
                );
            }
        }
    }

    #[test]
    fn a_turned_component_no_longer_lines_up() {
        let mut changed = build();
        changed.0[3].1.facing = Facing::West;
        // Four of five line up each way: 8 of 10.
        assert_eq!(permille(&build(), &changed), 800);
    }

    #[test]
    fn builds_without_redstone_are_nothing_alike() {
        assert_eq!(permille(&Skeleton(Vec::new()), &build()), 0);
    }
}
