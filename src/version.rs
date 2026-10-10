//! Whether a new file is the same build, fixed, so it may replace the old
//! one as a new version rather than be uploaded as a build of its own.
//!
//! A new version keeps the build's width and depth, though its height may
//! change; keeps its inputs and outputs; and keeps at least 95% of its
//! blocks, or changes at most 10, so a small build can be fixed too. Two
//! different 8-bit carry-cancel adders share at most 14% of their blocks,
//! and a fix such as flipping a lever changes one, so the line sits well
//! between.

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::{
    import::{self, Prepared},
    interface::{self, Port},
    lint,
    models::schematic::{self, Schematic},
    schem::{Blocks, Pos, Size},
};

/// The share of blocks, in percent, a new version keeps at least, unless
/// it changes no more than `MAX_CHANGED`.
pub const MIN_SAME_PERCENT: usize = 95;

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

/// Why `new` cannot replace `old` as a new version, each as a sentence;
/// empty when it can. Sizes are as `import::measure` gives them.
pub fn refusals(
    old: &Blocks,
    old_size: (i64, i64, i64),
    new: &Blocks,
    new_size: &Size,
    comparison: Comparison,
) -> Vec<String> {
    let mut refusals = Vec::new();
    let (x, _, z) = old_size;
    if (i64::from(new_size.x), i64::from(new_size.z)) != (x, z) {
        refusals.push(format!(
            "It is {} wide and {} deep, but the build is {x} wide and {z} \
             deep; a new version may change only the height.",
            new_size.x, new_size.z
        ));
    }
    let (was, now) = (ports(old), ports(new));
    let gone: Vec<&str> = was
        .iter()
        .filter(|port| !now.contains(port))
        .map(String::as_str)
        .collect();
    let added: Vec<&str> = now
        .iter()
        .filter(|port| !was.contains(port))
        .map(String::as_str)
        .collect();
    match (gone.is_empty(), added.is_empty()) {
        (true, true) => {}
        (false, true) => refusals.push(format!(
            "It must keep the same inputs and outputs, but no longer has {}.",
            gone.join("; ")
        )),
        (true, false) => refusals.push(format!(
            "It must keep the same inputs and outputs, but also has {}.",
            added.join("; ")
        )),
        (false, false) => refusals.push(format!(
            "It must keep the same inputs and outputs, but has {} in place \
             of {}.",
            added.join("; "),
            gone.join("; ")
        )),
    }
    if !comparison.close_enough() {
        let permille = comparison.same_permille();
        refusals.push(format!(
            "It changes {} blocks, keeping {}.{}% of them; a new version \
             keeps at least {MIN_SAME_PERCENT}%, or changes at most \
             {MAX_CHANGED}. Upload it as a build of its own instead.",
            comparison.changed,
            permille / 10,
            permille % 10
        ));
    }
    refusals
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
    let keep_paste = schematic::keeping_paste(db).await?.contains(&s.id);
    let prepared =
        import::prepare(file, standardize, s.conventional, keep_paste)?;
    let current = schematic::data(db, &s.id).await?.unwrap_or_default();
    let current = Blocks::read(&current)?;
    let comparison = compare(&current, &prepared.blocks);
    let mut refusals = refusals(
        &current,
        s.size,
        &prepared.blocks,
        &prepared.size,
        comparison,
    );
    // Only conventions the current version keeps count against it, so a
    // build from before a rule can still be fixed.
    let kept: Vec<String> = if s.conventional {
        lint::check(&current)
            .into_iter()
            .map(|v| v.message)
            .collect()
    } else {
        Vec::new()
    };
    let broken: Vec<&str> = prepared
        .violations
        .iter()
        .map(|v| v.message.as_str())
        .filter(|message| !kept.iter().any(|k| k == message))
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

/// Each input and output as words, such as `input A, 8 bits of lime
/// concrete`.
fn ports(blocks: &Blocks) -> Vec<String> {
    let interface = interface::read(blocks);
    let describe = |kind: &str, port: &Port| {
        let bits = port.bits.len();
        let unit = if bits == 1 { "bit" } else { "bits" };
        format!("{kind} {}, {bits} {unit} of {}", port.name, port.block)
    };
    interface
        .inputs
        .iter()
        .map(|port| describe("input", port))
        .chain(
            interface
                .outputs
                .iter()
                .map(|port| describe("output", port)),
        )
        .collect()
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
        let rebuilt = Comparison {
            same: 400,
            changed: 100,
        };
        assert!(!rebuilt.close_enough());
    }
}
