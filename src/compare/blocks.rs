//! How two files' blocks compare, block for block, at the shift that
//! lines them up best: whether a new file is the same build, fixed.
//!
//! A new version that keeps less than 90% of the build's blocks, and
//! changes more than 10, is warned of as maybe another build, but may
//! replace it all the same; its inputs and outputs may move, and its
//! size change, unwarned. A block other than a redstone component,
//! sign, or container counts only as solid or see-through, so
//! recolouring one changes nothing. So compared, different builds of
//! one category share at most 68% of their blocks, and every stored
//! version kept 95% or more of the one before, so the line sits well
//! between.

use crate::schem::{self, Blocks, Pos};
use std::collections::HashMap;

/// The share of blocks, in percent, a new version keeps at least before it
/// is warned of, unless it changes no more than `MAX_CHANGED`.
pub const MIN_SAME_PERCENT: usize = 90;

/// How many blocks a new version may change, whatever its share, unwarned.
pub const MAX_CHANGED: usize = 10;

/// How two files' blocks compare, at the shift that lines them up best.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comparison {
    /// Places holding the same block in both.
    pub same: usize,
    /// Places holding a block in either, but not the same one in both.
    pub changed: usize,
}

impl Comparison {
    /// The share of blocks kept, in tenths of a percent.
    pub fn same_permille(self) -> usize {
        // Two empty files are wholly the same.
        (self.same * 1000)
            .checked_div(self.same + self.changed)
            .unwrap_or(1000)
    }

    /// Whether few enough blocks changed for a new version.
    fn close_enough(self) -> bool {
        self.same * 100 >= (self.same + self.changed) * MIN_SAME_PERCENT
            || self.changed <= MAX_CHANGED
    }
}

/// Compares the blocks of `old` and `new`, other than air, trying each
/// shift of up to three blocks, since a new version may have gained or lost
/// a layer. A redstone component, a sign, or a container is compared as it
/// is, without the power a resave changes, such as dust's strength or a
/// torch being lit; a lever's position is kept, being an input's setting.
/// Any other block is compared only as a block, so one recoloured, such as
/// marked as an input, or turned from solid to see-through
/// or back, as glass to show the circuit, is unchanged.
pub fn compare(old: &Blocks, new: &Blocks) -> Comparison {
    aligned(old, new).0
}

/// How `old` and `new` compare, as [`compare`] says, with the shift that
/// lines them up so: the block at `pos` in `old` is at `pos` plus the
/// shift in `new`.
pub fn aligned(old: &Blocks, new: &Blocks) -> (Comparison, Pos) {
    let old = solid(old);
    let new = solid(new);
    let shifts = -3..=3;
    let mut best = Comparison {
        same: 0,
        changed: old.len() + new.len(),
    };
    let mut best_shift = [0, 0, 0];
    for dx in shifts.clone() {
        for dy in shifts.clone() {
            for dz in shifts.clone() {
                let (mut same, mut both) = (0, 0);
                for (&[x, y, z], state) in &old {
                    if let Some(theirs) = new.get(&[x + dx, y + dy, z + dz]) {
                        both += 1;
                        same += usize::from(theirs == state);
                    }
                }
                let changed = old.len() + new.len() - both - same;
                // No shift wins a tie, as most versions keep their box.
                let nearer =
                    changed == best.changed && [dx, dy, dz] == [0, 0, 0];
                if changed < best.changed || nearer {
                    best = Comparison { same, changed };
                    best_shift = [dx, dy, dz];
                }
            }
        }
    }
    (best, best_shift)
}

/// Each block other than air, by position, as `compare` compares it.
fn solid(blocks: &Blocks) -> HashMap<Pos, String> {
    blocks
        .iter()
        .filter(|&(_, state)| state != schem::AIR)
        .map(|(pos, state)| (pos, compared(state)))
        .collect()
}

/// `state` as a new version is compared by: a component, sign, or
/// container without its power, and any other block only as a block.
pub fn compared(state: &str) -> String {
    let name = schem::short(state);
    if schem::block::is_component(name)
        || name.ends_with("_sign")
        || schem::slots(state).is_some()
    {
        schem::unpowered(state)
    } else {
        "block".to_owned()
    }
}

/// What to warn of a file whose blocks compare with the current version's
/// as `comparison`, taken as a new version: changing so much of it that it
/// may be another build. It may replace it all the same, as its uploader
/// knows whether it fixes the build. Its inputs and outputs may move, and
/// its size change, unwarned.
pub fn warnings(comparison: Comparison) -> Vec<String> {
    if comparison.close_enough() {
        return Vec::new();
    }
    let permille = comparison.same_permille();
    vec![format!(
        "It changes {} blocks, keeping {}.{}% of them, where a fix most often \
         keeps at least {MIN_SAME_PERCENT}%. If it is another build rather \
         than this one fixed, upload it as a build of its own instead.",
        comparison.changed,
        permille / 10,
        permille % 10
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resave_with_other_power_is_unchanged() {
        use schem::unpowered;
        assert_eq!(
            unpowered("minecraft:redstone_wire[east=side,power=15,south=up]"),
            "minecraft:redstone_wire[east=side,south=up]"
        );
        assert_eq!(
            unpowered(
                "minecraft:repeater[delay=2,facing=east,locked=false,powered=true]"
            ),
            "minecraft:repeater[delay=2,facing=east]"
        );
    }

    #[test]
    fn a_levers_position_counts() {
        assert_eq!(
            schem::unpowered(
                "minecraft:lever[face=wall,facing=south,powered=true]"
            ),
            "minecraft:lever[face=wall,facing=south,powered=true]"
        );
    }

    #[test]
    fn a_small_build_may_change_a_few_blocks() {
        let small = Comparison {
            same: 55,
            changed: 8,
        };
        assert!(small.close_enough());
        let big = Comparison {
            same: 900,
            changed: 40,
        };
        assert!(big.close_enough());
        let reworked = Comparison {
            same: 900,
            changed: 100,
        };
        assert!(reworked.close_enough(), "90% kept is enough");
        let rebuilt = Comparison {
            same: 890,
            changed: 110,
        };
        assert!(!rebuilt.close_enough());
    }
}
