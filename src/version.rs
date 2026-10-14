//! Whether a new file is the same build, fixed, so it may replace the old
//! one as a new version rather than be uploaded as a build of its own.
//!
//! A new version keeps at least 90% of the build's blocks, or changes at
//! most 10, so a small build can be fixed too; its inputs and outputs may
//! move, and its size change, within that. Two different 8-bit
//! carry-cancel adders share at most 14% of their blocks, and a fix such as
//! flipping a lever changes one, so the line sits well between.

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::{
    import::{self, Prepared},
    lint,
    models::schematic::{self, Schematic},
    schem::{Blocks, Pos},
    standardize::Gray,
};

/// The share of blocks, in percent, a new version keeps at least, unless
/// it changes no more than `MAX_CHANGED`.
pub const MIN_SAME_PERCENT: usize = 90;

/// How many blocks a new version may change whatever its share.
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
    pub fn close_enough(self) -> bool {
        self.same * 100 >= (self.same + self.changed) * MIN_SAME_PERCENT
            || self.changed <= MAX_CHANGED
    }
}

/// Compares the blocks of `old` and `new`, other than air, trying each
/// shift of up to three blocks, since a new version may have gained or lost
/// a layer. Power a resave changes, such as dust's strength or a torch
/// being lit, is left out; a lever's position is kept, being an input's
/// setting.
pub fn compare(old: &Blocks, new: &Blocks) -> Comparison {
    let old = solid(old);
    let new = solid(new);
    let shifts = -3..=3;
    let mut best = Comparison {
        same: 0,
        changed: old.len() + new.len(),
    };
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
                if changed < best.changed {
                    best = Comparison { same, changed };
                }
            }
        }
    }
    best
}

/// Each block other than air, by position, without the power a resave
/// changes.
fn solid(blocks: &Blocks) -> HashMap<Pos, String> {
    blocks
        .iter()
        .filter(|&(_, state)| state != crate::schem::AIR)
        .map(|(pos, state)| (pos, unpowered(state)))
        .collect()
}

/// A block state without the properties that follow its power, such as
/// `minecraft:redstone_wire[east=side,north=none,south=up,west=none]`.
fn unpowered(state: &str) -> String {
    let Some((id, properties)) = state.split_once('[') else {
        return state.to_owned();
    };
    let lever = id.ends_with(":lever");
    let kept: Vec<&str> = properties
        .trim_end_matches(']')
        .split(',')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or_default();
            !matches!(key, "power" | "lit" | "locked")
                && (key != "powered" || lever)
        })
        .collect();
    format!("{id}[{}]", kept.join(","))
}

/// Why a file whose blocks compare with the current version's as
/// `comparison` cannot replace it as a new version, as a sentence; empty
/// when it can. Its inputs and outputs may move, and its size change, as
/// long as it keeps enough of the build's blocks.
pub fn refusals(comparison: Comparison) -> Vec<String> {
    if comparison.close_enough() {
        return Vec::new();
    }
    let permille = comparison.same_permille();
    vec![format!(
        "It changes {} blocks, keeping {}.{}% of them; a new version keeps at \
         least {MIN_SAME_PERCENT}%, or changes at most {MAX_CHANGED}. Upload \
         it as a build of its own instead.",
        comparison.changed,
        permille / 10,
        permille % 10
    )]
}

/// A file prepared as the build's next version, how it compares with the
/// current one, and why it cannot replace it.
pub struct Checked {
    pub prepared: Prepared,
    pub comparison: Comparison,
    pub refusals: Vec<String>,
}

/// Prepares `file`, standardized if asked, as build `s`'s next version and
/// checks it against the current one.
pub async fn check(
    db: &SqlitePool,
    s: &Schematic,
    file: &[u8],
    standardize: Option<Gray>,
) -> anyhow::Result<Checked> {
    let keep_paste = schematic::keeping_paste(db).await?.contains(&s.id);
    let prepared =
        import::prepare(file, standardize, s.conventional, keep_paste)?;
    let current = schematic::data(db, &s.id).await?.unwrap_or_default();
    let current = Blocks::read(&current)?;
    let comparison = compare(&current, &prepared.blocks);
    let mut refusals = refusals(comparison);
    // Only conventions the current version keeps count against it, so a
    // build from before a rule can still be fixed.
    let (kept, now) = if s.conventional {
        (
            lint::check_for(&current, &s.category),
            lint::check_for(&prepared.blocks, &s.category),
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let broken: Vec<&str> = now
        .iter()
        .map(|v| v.message.as_str())
        .filter(|message| !kept.iter().any(|k| k.message == *message))
        .collect();
    if let Some(first) = broken.first() {
        refusals.push(format!(
            "It breaks {} building conventions the current version keeps, \
             such as: {first}",
            broken.len()
        ));
    }
    match schematic::id_by_sha256(db, &prepared.sha256).await? {
        Some(other) if other == s.id => {
            refusals
                .push("It is the same file as the current version.".to_owned());
        }
        Some(_) => refusals.push(
            "This exact file is already on the site as another build."
                .to_owned(),
        ),
        None => {}
    }
    Ok(Checked {
        prepared,
        comparison,
        refusals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resave_with_other_power_is_unchanged() {
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
            unpowered("minecraft:lever[face=wall,facing=south,powered=true]"),
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
