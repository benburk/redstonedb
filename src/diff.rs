//! Two versions of a build side by side, as two files that still work:
//! the older with the blocks the newer takes out in red, the newer with the
//! blocks it adds in green. Only building blocks are coloured, so each
//! side runs as its version does; a redstone component that changed marks
//! the block it stands on or hangs from instead, yellow on both sides, red
//! where it went, green where it came. Every other building block takes
//! the site's own palette, inputs and outputs unmarked, and both sides
//! share one box, so the viewer can swap one for the other in place.
//!
//! A building block counts only as solid or see-through: one recoloured is
//! unchanged, one turned from solid to see-through or back goes and comes,
//! as that changes what powers what.

use std::collections::HashMap;

use anyhow::Result;

use crate::{
    schem::{self, Blocks, Pos, block},
    version,
};

/// What changed between two versions.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Counts {
    /// Building blocks the newer version adds.
    pub added: usize,
    /// Building blocks the newer version takes out.
    pub removed: usize,
    /// Places whose redstone component or sign is added, taken out, or
    /// changed, as a repeater's delay or a torch's place.
    pub components: usize,
}

impl Counts {
    /// Whether nothing changed.
    pub fn is_empty(self) -> bool {
        self == Counts::default()
    }
}

/// The two sides of a diff, each a file, and what changed.
pub struct Diff {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub counts: Counts,
}

/// How a mark colours a building block: a dye, and which wins where two
/// marks meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mark {
    /// A component on it changed.
    Changed,
    /// A component on it went or came.
    Moved(Dye),
    /// The block itself went or came.
    Block(Dye),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Dye {
    Red,
    Lime,
}

impl Mark {
    fn dye(self) -> &'static str {
        match self {
            Mark::Changed => "yellow",
            Mark::Moved(Dye::Red) | Mark::Block(Dye::Red) => "red",
            Mark::Moved(Dye::Lime) | Mark::Block(Dye::Lime) => "lime",
        }
    }
}

/// What a block is to the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    /// A building block, solid or see-through.
    Building { solid: bool },
    /// A redstone component, sign, or container, as compared.
    Component(String),
}

fn part(state: &str) -> Option<Part> {
    let name = schem::short(state);
    if name.ends_with("_sign") {
        return Some(Part::Component(version::compared(state)));
    }
    match block::block_kind(name)? {
        "Solid" | "Transparent" => Some(Part::Building {
            solid: block::solid(state),
        }),
        _ => Some(Part::Component(version::compared(state))),
    }
}

/// The diff of `old` to `new`, two versions' files, lined up as the check
/// of a new version lines them up.
pub fn diff(old: &[u8], new: &[u8]) -> Result<Diff> {
    let (old_blocks, new_blocks) = (Blocks::read(old)?, Blocks::read(new)?);
    let (_, shift) = version::aligned(&old_blocks, &new_blocks);
    let to_new = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] + shift[axis]);
    let to_old = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] - shift[axis]);
    // Each side's blocks, the newer's in the older's positions.
    let before: HashMap<Pos, String> = placed(&old_blocks, |pos| pos);
    let after: HashMap<Pos, String> = placed(&new_blocks, to_old);
    let mut marks_before: HashMap<Pos, Mark> = HashMap::new();
    let mut marks_after: HashMap<Pos, Mark> = HashMap::new();
    let mut counts = Counts::default();
    let mut places: Vec<Pos> =
        before.keys().chain(after.keys()).copied().collect();
    places.sort_unstable();
    places.dedup();
    for pos in places {
        let was = before.get(&pos).and_then(|state| part(state));
        let is = after.get(&pos).and_then(|state| part(state));
        if was == is {
            continue;
        }
        // Not the same, so a building block on either side went or came,
        // as one turned from solid to see-through goes and comes.
        if matches!(was, Some(Part::Building { .. })) {
            counts.removed += 1;
            mark(&mut marks_before, pos, Mark::Block(Dye::Red));
        }
        if matches!(is, Some(Part::Building { .. })) {
            counts.added += 1;
            mark(&mut marks_after, pos, Mark::Block(Dye::Lime));
        }
        let was_component = matches!(was, Some(Part::Component(_)));
        let is_component = matches!(is, Some(Part::Component(_)));
        if !was_component && !is_component {
            continue;
        }
        counts.components += 1;
        // Changed, its block is yellow on both sides; gone, red on the
        // older; come, green on the newer.
        let both = was_component && is_component;
        if was_component && let Some(on) = holder(&old_blocks, pos) {
            let gone = if both {
                Mark::Changed
            } else {
                Mark::Moved(Dye::Red)
            };
            mark(&mut marks_before, on, gone);
        }
        if is_component
            && let Some(on) = holder(&new_blocks, to_new(pos)).map(to_old)
        {
            let came = if both {
                Mark::Changed
            } else {
                Mark::Moved(Dye::Lime)
            };
            mark(&mut marks_after, on, came);
        }
    }
    // Both in one box, the union of the two in the older's positions.
    let (low, size) = union(&old_blocks, &new_blocks, shift);
    let before = side(old, &before, &marks_before, low, size, |pos| pos)?;
    let after = side(new, &after, &marks_after, to_new(low), size, to_new)?;
    Ok(Diff {
        before,
        after,
        counts,
    })
}

/// Each block of `blocks` other than air, at the position `at` gives it.
fn placed(blocks: &Blocks, at: impl Fn(Pos) -> Pos) -> HashMap<Pos, String> {
    blocks
        .iter()
        .filter(|&(_, state)| state != schem::AIR)
        .map(|(pos, state)| (at(pos), state.to_owned()))
        .collect()
}

/// Marks `pos` with `mark`, unless a mark that wins over it is there.
fn mark(marks: &mut HashMap<Pos, Mark>, pos: Pos, mark: Mark) {
    let kept = marks.entry(pos).or_insert(mark);
    *kept = (*kept).max(mark);
}

/// The block the component at `pos` in `blocks` stands on or hangs from,
/// or for a whole block, such as a lamp, the block under it.
fn holder(blocks: &Blocks, pos: Pos) -> Option<Pos> {
    let state = blocks.get(pos);
    let name = schem::short(state);
    if name.ends_with("_sign") {
        return blocks.sign_block(pos);
    }
    let id = schem::block::like(name);
    Some(
        schem::block::support(pos, state, id).map_or_else(
            || schem::step(pos, schem::Direction::Down),
            |(on, _)| on,
        ),
    )
}

/// The lowest corner and size, in `old`'s positions, of the box holding
/// both `old` and `new`, whose positions are `shift` on from `old`'s.
fn union(old: &Blocks, new: &Blocks, shift: Pos) -> (Pos, Pos) {
    let low = [0, 1, 2].map(|axis| 0.min(-shift[axis]));
    let high =
        [0, 1, 2].map(|axis| old.size[axis].max(new.size[axis] - shift[axis]));
    (low, [0, 1, 2].map(|axis| high[axis] - low[axis]))
}

/// One side of a diff: the file `data`, its blocks `blocks` in the older's
/// positions, each building block the site's own or as `marks` colour it,
/// in the box of `size` from `low`, in the file's own positions, which
/// `own` gives.
fn side(
    data: &[u8],
    blocks: &HashMap<Pos, String>,
    marks: &HashMap<Pos, Mark>,
    low: Pos,
    size: Pos,
    own: impl Fn(Pos) -> Pos,
) -> Result<Vec<u8>> {
    let mut grid: HashMap<Pos, String> = HashMap::new();
    for (&pos, state) in blocks {
        let shown = match part(state) {
            Some(Part::Building { solid }) => {
                building(state, solid, marks.get(&pos))
            }
            _ => state.clone(),
        };
        grid.insert(own(pos), shown);
    }
    let data = schem::rebox(data, &grid, low, size)?;
    schem::lines::without(&data)
}

/// The building block `state`, solid or not, in the site's own palette,
/// or `mark`'s colour: concrete for a solid block, stained glass for any
/// other. A see-through block shaped otherwise than a whole one, as a slab
/// or stairs, keeps its shape unless marked.
fn building(state: &str, solid: bool, mark: Option<&Mark>) -> String {
    let kind = if solid { "concrete" } else { "stained_glass" };
    match mark {
        Some(mark) => format!("minecraft:{}_{kind}", mark.dye()),
        None if solid || block::whole(state) => {
            format!("minecraft:white_{kind}")
        }
        None => state.to_owned(),
    }
}

/// The changed building blocks of each side, by position in its own
/// file, and their new blocks, for tests.
#[cfg(test)]
fn coloured(data: &[u8]) -> std::collections::BTreeMap<Pos, String> {
    let blocks = Blocks::read(data).unwrap();
    blocks
        .iter()
        .filter(|(_, state)| {
            ["red_", "lime_", "yellow_"]
                .iter()
                .any(|dye| schem::short(state).starts_with(dye))
        })
        .map(|(pos, state)| (pos, schem::short(state).to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// A file of `blocks` in a box of `size`, written over a fixture's.
    fn file(size: Pos, blocks: &[(Pos, &str)]) -> Vec<u8> {
        let grid: HashMap<Pos, String> = blocks
            .iter()
            .map(|&(pos, state)| (pos, format!("minecraft:{state}")))
            .collect();
        let any = include_bytes!("../tests/fixtures/monostable-3.schem");
        schem::rebox(any, &grid, [0, 0, 0], size).unwrap()
    }

    #[test]
    fn a_block_taken_out_is_red_before_and_one_added_green_after() {
        let old =
            file([3, 1, 1], &[([0, 0, 0], "stone"), ([1, 0, 0], "stone")]);
        let new =
            file([3, 1, 1], &[([0, 0, 0], "stone"), ([2, 0, 0], "glass")]);
        let diff = diff(&old, &new).unwrap();
        assert_eq!(
            diff.counts,
            Counts {
                added: 1,
                removed: 1,
                components: 0
            }
        );
        assert_eq!(
            coloured(&diff.before),
            BTreeMap::from([([1, 0, 0], "red_concrete".to_owned())])
        );
        assert_eq!(
            coloured(&diff.after),
            BTreeMap::from([([2, 0, 0], "lime_stained_glass".to_owned())])
        );
    }

    #[test]
    fn a_recoloured_block_is_unchanged_and_takes_the_sites_own() {
        let old = file([1, 1, 1], &[([0, 0, 0], "red_wool")]);
        let new = file([1, 1, 1], &[([0, 0, 0], "blue_concrete")]);
        let diff = diff(&old, &new).unwrap();
        assert!(diff.counts.is_empty());
        let after = Blocks::read(&diff.after).unwrap();
        assert_eq!(after.get([0, 0, 0]), "minecraft:white_concrete");
    }

    #[test]
    fn solid_turned_see_through_goes_and_comes() {
        let old = file([1, 1, 1], &[([0, 0, 0], "stone")]);
        let new = file([1, 1, 1], &[([0, 0, 0], "glass")]);
        let diff = diff(&old, &new).unwrap();
        assert_eq!(
            diff.counts,
            Counts {
                added: 1,
                removed: 1,
                components: 0
            }
        );
        assert_eq!(
            coloured(&diff.before),
            BTreeMap::from([([0, 0, 0], "red_concrete".to_owned())])
        );
        assert_eq!(
            coloured(&diff.after),
            BTreeMap::from([([0, 0, 0], "lime_stained_glass".to_owned())])
        );
    }

    #[test]
    fn a_changed_component_marks_its_block_yellow_on_both_sides() {
        let repeater = |delay| format!("repeater[delay={delay},facing=east]");
        let (one, two) = (repeater(1), repeater(2));
        let old = file([1, 2, 1], &[([0, 0, 0], "stone"), ([0, 1, 0], &one)]);
        let new = file([1, 2, 1], &[([0, 0, 0], "stone"), ([0, 1, 0], &two)]);
        let diff = diff(&old, &new).unwrap();
        assert_eq!(
            diff.counts,
            Counts {
                added: 0,
                removed: 0,
                components: 1
            }
        );
        let yellow =
            BTreeMap::from([([0, 0, 0], "yellow_concrete".to_owned())]);
        assert_eq!(coloured(&diff.before), yellow);
        assert_eq!(coloured(&diff.after), yellow);
        // The component itself is as it was on each side.
        let after = Blocks::read(&diff.after).unwrap();
        assert!(after.get([0, 1, 0]).contains("delay=2"));
    }

    #[test]
    fn dust_added_marks_its_block_green_after() {
        let old = file([1, 2, 1], &[([0, 0, 0], "glass")]);
        let new = file(
            [1, 2, 1],
            &[([0, 0, 0], "glass"), ([0, 1, 0], "redstone_wire")],
        );
        let diff = diff(&old, &new).unwrap();
        assert_eq!(
            coloured(&diff.after),
            BTreeMap::from([([0, 0, 0], "lime_stained_glass".to_owned())])
        );
        assert_eq!(coloured(&diff.before), BTreeMap::new());
    }

    #[test]
    fn a_block_taken_out_wins_over_a_change_on_it() {
        let old = file(
            [1, 2, 1],
            &[([0, 0, 0], "stone"), ([0, 1, 0], "redstone_wire")],
        );
        let new = file([1, 2, 1], &[([0, 1, 0], "redstone_wire")]);
        let diff = diff(&old, &new).unwrap();
        assert_eq!(
            coloured(&diff.before),
            BTreeMap::from([([0, 0, 0], "red_concrete".to_owned())])
        );
    }

    #[test]
    fn both_sides_share_one_box_when_a_layer_is_added() {
        // The repeater lines them up, as building blocks count alike.
        let repeater = "repeater[delay=1,facing=east]";
        let old = file(
            [2, 2, 1],
            &[
                ([0, 0, 0], "stone"),
                ([1, 0, 0], "stone"),
                ([1, 1, 0], repeater),
            ],
        );
        let new = file(
            [3, 2, 1],
            &[
                ([0, 0, 0], "glass"),
                ([1, 0, 0], "stone"),
                ([2, 0, 0], "stone"),
                ([2, 1, 0], repeater),
            ],
        );
        let diff = diff(&old, &new).unwrap();
        let (before, after) = (
            Blocks::read(&diff.before).unwrap(),
            Blocks::read(&diff.after).unwrap(),
        );
        assert_eq!(before.size, after.size);
        assert_eq!(
            diff.counts,
            Counts {
                added: 1,
                removed: 0,
                components: 0
            }
        );
        assert_eq!(after.get([0, 0, 0]), "minecraft:lime_stained_glass");
        assert_eq!(before.get([0, 0, 0]), "minecraft:air");
    }
}
