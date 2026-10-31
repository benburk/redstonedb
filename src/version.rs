//! Whether a new file is the same build, fixed, so it may replace the old
//! one as a new version rather than be uploaded as a build of its own.
//!
//! A new version keeps at least 90% of the build's blocks, or changes at
//! most 10, so a small build can be fixed too; its inputs and outputs may
//! move, and its size change, within that. A block other than a redstone
//! component, sign, or container counts only as solid or see-through, so
//! recolouring one changes nothing. So compared, different builds of one
//! category share at most 68% of their blocks, and every stored version
//! kept 95% or more of the one before, so the line sits well between.

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::{
    colour,
    import::{self, Prepared},
    inspect,
    models::{schematic::Schematic, version},
    schem::{self, Blocks, Pos},
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
fn compared(state: &str) -> String {
    let name = schem::short(state);
    if colour::is_component(name)
        || name.ends_with("_sign")
        || schem::slots(state).is_some()
    {
        schem::unpowered(state)
    } else {
        "block".to_owned()
    }
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
    standardize: bool,
) -> anyhow::Result<Checked> {
    // An adder's or register's inputs are its levers and buttons, coloured
    // by probing before standardizing, as an upload's are.
    let coloured = if standardize {
        import::probed(file, &s.category).await?
    } else {
        file.to_vec()
    };
    let prepared = import::prepare(
        &coloured,
        standardize,
        s.conventional,
        s.keeps_paste,
        false,
    )?;
    let current = version::current_file(db, &s.id).await?.unwrap_or_default();
    let current = Blocks::read(&current)?;
    let comparison = compare(&current, &prepared.blocks);
    let mut refusals = refusals(comparison);
    // Only problems the current version doesn't have count against it, so
    // a build from before a rule can still be fixed. They are the check by
    // function's, as an upload's: inputs are its levers and buttons, not
    // colours.
    if s.conventional {
        // Dust on top of an input is a fault only on a data input, which
        // probing a blessed build checks.
        let current = inspect::inspect(&current);
        let had: Vec<&str> =
            current.faults().map(|problem| problem.rule).collect();
        let found = inspect::inspect(&prepared.blocks);
        let broken: Vec<&str> = found
            .faults()
            .filter(|problem| !had.contains(&problem.rule))
            .map(|problem| problem.message.as_str())
            .collect();
        if let Some(first) = broken.first() {
            refusals.push(format!(
                "It breaks {} building conventions the current version \
                 keeps, such as: {first}",
                broken.len()
            ));
        }
    }
    match version::holder(db, &prepared.sha256).await? {
        Some((other, number)) if other == s.id => {
            refusals.push(format!("It is the same file as version {number}."));
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
