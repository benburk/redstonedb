//! The part of the check by function that takes the compiler: whether a
//! bus could drive each input through a repeater in its lever's place,
//! proved by compiling the build both ways; see
//! [`crate::interface::inspect`] for the rules read off the blocks alone.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;

use crate::{
    interface::inspect::{Report, Rule, free_behind, inspect, problem, short},
    schem::{self, Blocks, Direction, Pos, Switch},
    simulate,
};

/// The check of the file in `data`, as [`inspect`] makes it.
pub fn inspect_file(data: &[u8]) -> Result<Report> {
    let blocks = Blocks::read(data)?;
    let mut report = inspect(&blocks);
    if !report
        .errors
        .iter()
        .any(|p| p.rule == Rule::NoRoomForRepeater)
    {
        for lever in not_bus_drivable(data)? {
            problem(
                &mut report,
                Rule::NotBusDrivable,
                vec![lever],
                "Lever at {}: not bus-drivable",
                &[lever],
            );
        }
    }
    Ok(report)
}

/// The levers whose place a repeater can't take: the build in `data`
/// compiled with every lever swapped for a repeater on a block, pointed
/// into its input block and driven by a lever behind it, against the
/// build as it is. Where the two differ but for those spots, a bus could
/// not drive the input through a repeater as the lever does. A lever with
/// a block behind it, which only a control line's may have, is left as it
/// is. Read by the rules of redstone, with no simulation.
pub fn not_bus_drivable(data: &[u8]) -> Result<Vec<Pos>> {
    let blocks = Blocks::read(data)?;
    let levers: Vec<Switch> = blocks
        .switches()
        .into_iter()
        .filter(|s| {
            !s.button
                && !matches!(s.side, Direction::Up | Direction::Down)
                && free_behind(&blocks, s)
        })
        .collect();
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    let mut spots: HashSet<Pos> = HashSet::new();
    for lever in &levers {
        let behind = schem::step(lever.pos, lever.side);
        changes.insert(
            lever.pos,
            format!(
                "minecraft:repeater[delay=1,facing={},locked=false,powered=false]",
                lever.side.name()
            ),
        );
        for spot in [lever.pos, behind] {
            let under = schem::step(spot, Direction::Down);
            if short(&blocks, under) == "air" && !changes.contains_key(&under) {
                changes.insert(under, "minecraft:white_concrete".to_owned());
            }
            spots.insert(spot);
        }
        changes.insert(
            behind,
            "minecraft:lever[face=floor,facing=north,powered=false]".to_owned(),
        );
    }
    let (swapped, moved) = schem::edit_grown(data, &changes)?;
    let back = |p: Pos| [0, 1, 2].map(|axis| p[axis] - moved[axis]);
    let before = simulate::compiled(&blocks)?;
    let after = simulate::compiled(&Blocks::read(&swapped)?)?;
    let kinds = |kinds: BTreeMap<Pos, String>, map: &dyn Fn(Pos) -> Pos| {
        kinds
            .into_iter()
            .map(|(p, kind)| (map(p), kind))
            .filter(|(p, _)| !spots.contains(p))
            .collect::<BTreeMap<_, _>>()
    };
    let links = |links: BTreeMap<simulate::Link, u8>,
                 map: &dyn Fn(Pos) -> Pos| {
        links
            .into_iter()
            .map(|((from, to, side), ss)| ((map(from), map(to), side), ss))
            // Links out of each lever's spot are compared, the repeater's
            // against the lever's; those of the levers behind are not.
            .filter(|((from, to, _), _)| {
                let swapped = levers.iter().any(|l| l.pos == *from);
                !spots.contains(to) && (swapped || !spots.contains(from))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let (old_kinds, new_kinds) =
        (kinds(before.kinds, &|p| p), kinds(after.kinds, &back));
    let (old_links, new_links) =
        (links(before.links, &|p| p), links(after.links, &back));
    let mut wrong: BTreeSet<Pos> = BTreeSet::new();
    let changed_ends: Vec<Pos> = old_links
        .keys()
        .chain(new_links.keys())
        .filter(|k| old_links.get(*k) != new_links.get(*k))
        .flat_map(|(from, to, _)| [*from, *to])
        .chain(
            old_kinds
                .keys()
                .chain(new_kinds.keys())
                .filter(|p| old_kinds.get(*p) != new_kinds.get(*p))
                .copied(),
        )
        .collect();
    // Each difference is put down to the lever nearest it.
    for end in changed_ends {
        if let Some(lever) =
            levers.iter().min_by_key(|l| schem::distance(l.pos, end))
        {
            wrong.insert(lever.pos);
        }
    }
    Ok(wrong.into_iter().collect())
}
