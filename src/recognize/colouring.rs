//! A build's original colours: the block its author put in each of its
//! building blocks' places, kept beside the stored file, which the site
//! standardizes to its own blocks. Like a palette, a colouring only
//! chooses blocks for places the build already has: a palette by what
//! each place must be, a colouring place by place.
//!
//! An author's file is read against the stored one under each way of
//! turning and mirroring it, and every place must hold the same kind of
//! block in both: the same redstone component, or a building block of the
//! same kind, solid, see-through, or one a comparator reads, as a furnace
//! is and the barrel standing in for it.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use crate::{
    interface::colour,
    schem::{
        self, Blocks, Pos, block,
        turn::{self, Turn},
    },
};

/// The block for each building block's place that differs from the
/// stored file's, in the stored file's positions.
pub type Colouring = Vec<(Pos, String)>;

/// What kind of block `state` is, for matching an author's file with the
/// stored one: air, a component by its name, or a building block by kind.
fn kind(state: &str) -> &str {
    let name = schem::short(state);
    if name == "air" || name.ends_with("sign") {
        // A sign passes redstone by, and standardizing takes most out.
        "air"
    } else if name == "barrel"
        || name == "hopper"
        || colour::barrel_stands_in(name)
    {
        "container"
    } else if block::see_through(name) {
        "see-through"
    } else if name == "redstone_lamp" {
        // A lamp an input's lever sat on was made that input's block.
        "solid"
    } else if block::is_component(name) {
        name
    } else {
        "solid"
    }
}

/// Whether `state` is plain glass, which an old corner marker is, and
/// standardizing takes out wherever it touches nothing.
fn marker(state: &str) -> bool {
    schem::short(state) == "glass"
}

/// Whether `state` is a block of the build that the stored file must have
/// a place for: neither air nor a corner marker.
fn counted(state: &str) -> bool {
    kind(state) != "air" && !marker(state)
}

/// Whether `kind` is a building block's, which a colouring may choose.
fn building(kind: &str) -> bool {
    matches!(kind, "solid" | "see-through" | "container")
}

/// The colouring the author's file `original` gives the stored file
/// `stored`: each building block it puts in another block's place. The
/// `Err` says, for the way it comes closest, which places hold another
/// kind of block, when no way of turning it matches.
pub fn read(
    original: &[u8],
    stored: &[u8],
) -> Result<std::result::Result<Colouring, Vec<String>>> {
    let stored = Blocks::read(stored)?;
    let mut closest: Option<(usize, Vec<String>)> = None;
    for mirrored in [false, true] {
        for quarters in 0..4 {
            let turned = turn::turned(original, Turn { mirrored, quarters })?;
            let turned = Blocks::read(&turned)?;
            let room = [0, 1, 2].map(|i| turned.size[i] - stored.size[i]);
            if room.iter().any(|&r| r < 0) {
                continue;
            }
            for dx in 0..=room[0] {
                for dy in 0..=room[1] {
                    for dz in 0..=room[2] {
                        let at = |[x, y, z]: Pos| [x + dx, y + dy, z + dz];
                        let faults = faults(&turned, &stored, &at);
                        if faults.is_empty() {
                            return Ok(Ok(colouring(&turned, &stored, &at)));
                        }
                        if closest
                            .as_ref()
                            .is_none_or(|(n, _)| faults.len() < *n)
                        {
                            closest = Some((faults.len(), faults));
                        }
                    }
                }
            }
        }
    }
    Ok(Err(closest.map_or_else(
        || vec!["Smaller than the stored build every way it turns".to_owned()],
        |(_, faults)| faults,
    )))
}

/// Where the author's `turned` file, its place for each stored position
/// `at`, holds another kind of block than `stored`, as lines saying so,
/// with any block it has outside the stored build's box.
fn faults(
    turned: &Blocks,
    stored: &Blocks,
    at: &impl Fn(Pos) -> Pos,
) -> Vec<String> {
    let mut faults: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut inside = 0;
    for (pos, state) in stored.iter() {
        let theirs = turned.get(at(pos));
        if counted(theirs) {
            inside += 1;
        }
        if kind(theirs) != kind(state)
            && !(marker(theirs) && kind(state) == "air")
        {
            let names = (
                schem::short(theirs).to_owned(),
                schem::short(state).to_owned(),
            );
            *faults.entry(names).or_default() += 1;
        }
    }
    let total = turned.iter().filter(|(_, state)| counted(state)).count();
    let mut lines: Vec<String> = faults
        .into_iter()
        .map(|((theirs, ours), n)| {
            format!("{theirs} where the stored build has {ours}: {n}")
        })
        .collect();
    if total > inside {
        lines.push(format!(
            "Blocks outside the stored build's box: {}",
            total - inside
        ));
    }
    lines
}

/// The author's block in each of `stored`'s building blocks' places
/// where it differs, `at` giving its place in `turned`.
fn colouring(
    turned: &Blocks,
    stored: &Blocks,
    at: &impl Fn(Pos) -> Pos,
) -> Colouring {
    stored
        .iter()
        .filter(|(_, state)| building(kind(state)))
        .filter_map(|(pos, state)| {
            let theirs = turned.get(at(pos));
            (theirs != state).then(|| (pos, theirs.to_owned()))
        })
        .collect()
}

/// The file in `data` in the colours of `colouring`: each of its places
/// still holding a building block of the kind the colouring's block is,
/// so a place a later version emptied, or a block of another kind, is
/// left as it is.
pub fn apply(data: &[u8], colouring: &Colouring) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let inside = |[x, y, z]: Pos| {
        let [w, h, l] = blocks.size;
        (0..w).contains(&x) && (0..h).contains(&y) && (0..l).contains(&z)
    };
    let mut changes: BTreeMap<Pos, String> = colouring
        .iter()
        .filter(|(pos, block)| {
            inside(*pos)
                && building(kind(block))
                && kind(blocks.get(*pos)) == kind(block)
        })
        .cloned()
        .collect();
    if changes.is_empty() {
        return Ok(data.to_vec());
    }
    lamps_at_rest(data, &blocks, &mut changes)?;
    schem::edit(data, &changes)
}

/// Each lamp among `changes`, a colouring of the file in `data` read as
/// `blocks`, lit as the build at rest lights it, as `//update` would, not
/// as its author's file was saved: the stored file settled with those
/// lamps in it and its levers as they are. Its other building blocks carry
/// power as the author's do, which the simulator may not model, such as
/// wool.
fn lamps_at_rest(
    data: &[u8],
    blocks: &Blocks,
    changes: &mut BTreeMap<Pos, String>,
) -> Result<()> {
    let off = |state: &str| schem::with_property(state, "lit", "false");
    let lamps: BTreeMap<Pos, String> = changes
        .iter()
        .filter(|(_, block)| schem::short(block) == "redstone_lamp")
        .map(|(&pos, block)| (pos, off(block)))
        .collect();
    if lamps.is_empty() {
        return Ok(());
    }
    let with_lamps = Blocks::read(&schem::edit(data, &lamps)?)?;
    let on: BTreeSet<Pos> = blocks
        .iter()
        .filter(|(_, state)| {
            schem::short(state) == "lever"
                && schem::property(state, "powered") == Some("true")
        })
        .map(|(pos, _)| pos)
        .collect();
    let (settled, _) = crate::simulate::settled(&with_lamps, &on)?;
    for (pos, lamp) in lamps {
        let lit = settled
            .get(&pos)
            .and_then(|state| schem::property(state, "lit"))
            .unwrap_or("false");
        changes.insert(pos, schem::with_property(&lamp, "lit", lit));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap()
    }

    /// The stored file's first few solid places, made blue wool in a copy
    /// turned a half turn and mirrored, come back as blue wool in the
    /// stored file's places, and applying them gives the stored file in
    /// those colours, its components as they were.
    #[test]
    fn a_turned_copy_gives_its_colours_in_the_stored_places() {
        let stored = fixture("alpha-rca");
        let blocks = Blocks::read(&stored).unwrap();
        let solid: Vec<Pos> = blocks
            .iter()
            .filter(|(_, state)| kind(state) == "solid")
            .map(|(pos, _)| pos)
            .take(5)
            .collect();
        let wool: BTreeMap<Pos, String> = solid
            .iter()
            .map(|&pos| (pos, "minecraft:blue_wool".to_owned()))
            .collect();
        let authors = schem::edit(&stored, &wool).unwrap();
        let turn = Turn {
            mirrored: true,
            quarters: 2,
        };
        let authors = turn::turned(&authors, turn).unwrap();

        let colouring = read(&authors, &stored).unwrap().unwrap();
        let expected: Colouring = solid
            .iter()
            .map(|&pos| (pos, "minecraft:blue_wool".to_owned()))
            .collect();
        assert_eq!(colouring, expected);
        let coloured =
            Blocks::read(&apply(&stored, &colouring).unwrap()).unwrap();
        for (pos, state) in blocks.iter() {
            let want = if solid.contains(&pos) {
                "minecraft:blue_wool"
            } else {
                state
            };
            assert_eq!(coloured.get(pos), want, "at {pos:?}");
        }
    }

    /// Another build says where it differs, and a colouring's block of
    /// another kind than the place's is left out.
    #[test]
    fn another_build_says_where_it_differs() {
        let faults = read(&fixture("inverter-1"), &fixture("alpha-rca"))
            .unwrap()
            .unwrap_err();
        assert!(!faults.is_empty());
        let stored = fixture("alpha-rca");
        let blocks = Blocks::read(&stored).unwrap();
        let (pos, _) = blocks.iter().find(|(_, s)| kind(s) == "solid").unwrap();
        let glass = vec![(pos, "minecraft:glass".to_owned())];
        assert_eq!(apply(&stored, &glass).unwrap(), stored);
    }
}
