//! Whether a new file may replace a build as a new version.

use std::collections::HashMap;

mod common;

use app::{
    compare::blocks::{compare, warnings},
    schem::{self, Blocks},
    standard::standardize,
};

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let tidied =
        standardize::tidy(&common::fixture(name)).expect("fixture tidies");
    app::recognize::legacy::with_colour_roles(&tidied)
        .expect("fixture carries its roles")
}

/// `data` with the block at `pos` replaced by `state`.
fn edited(data: &[u8], pos: [i32; 3], state: &str) -> Vec<u8> {
    let blocks = Blocks::read(data).unwrap();
    let mut grid: HashMap<[i32; 3], String> =
        blocks.iter().map(|(p, s)| (p, s.to_owned())).collect();
    grid.insert(pos, state.to_owned());
    let mut nbt = schem::decode(data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    schem::encode(nbt).unwrap()
}

/// What replacing `old` with `new`, both as stored, is warned of.
fn warned(old: &[u8], new: &[u8]) -> Vec<String> {
    let (old, new) = (Blocks::read(old).unwrap(), Blocks::read(new).unwrap());
    warnings(compare(&old, &new))
}

#[test]
fn a_one_block_fix_is_a_new_version() {
    let old = stored("alpha-rca");
    let blocks = Blocks::read(&old).unwrap();
    // A block of plain white concrete taken out.
    let (pos, _) = blocks
        .iter()
        .find(|(_, state)| *state == "minecraft:white_concrete")
        .unwrap();
    let new = edited(&old, pos, "minecraft:air");
    let comparison = compare(&blocks, &Blocks::read(&new).unwrap());
    assert_eq!(comparison.changed, 1);
    assert!(warned(&old, &new).is_empty());
}

/// A block recoloured, such as marked or turned gray, or turned from solid
/// to see-through, is no change.
#[test]
fn a_recoloured_block_is_unchanged() {
    let old = stored("alpha-rca");
    let blocks = Blocks::read(&old).unwrap();
    let (pos, _) = blocks
        .iter()
        .find(|(_, state)| *state == "minecraft:white_concrete")
        .unwrap();
    for (state, changed) in [
        ("minecraft:gray_concrete", 0),
        ("minecraft:stone", 0),
        ("minecraft:white_stained_glass", 0),
    ] {
        let new = edited(&old, pos, state);
        let comparison = compare(&blocks, &Blocks::read(&new).unwrap());
        assert_eq!(comparison.changed, changed, "{state}");
    }
}

/// Another adder may replace one, as its uploader may mean it as a fix,
/// but is warned of, changing so much it may be another build.
#[test]
fn a_different_adder_is_warned_of() {
    let warnings = warned(&stored("alpha-rca"), &stored("alpha-cla"));
    assert!(warnings.iter().any(|w| w.contains("keeps at least 90%")));
}

/// A block more to the east makes it wider, which no longer stops it being
/// the same build: its blocks are all it is judged by.
#[test]
fn a_new_version_may_change_its_size() {
    let old = stored("alpha-rca");
    let blocks = Blocks::read(&old).unwrap();
    let mut grid: HashMap<[i32; 3], String> = blocks
        .iter()
        .filter(|(_, state)| *state != schem::AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let [x, y, z] = blocks.size;
    grid.insert([x, 0, 0], "minecraft:white_concrete".to_owned());
    let mut nbt = schem::decode(&old).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], [x + 1, y, z], &grid, entities)
        .unwrap();
    let new = schem::encode(nbt).unwrap();
    assert_eq!(warned(&old, &new), Vec::<String>::new());
}

/// An adder's carry lines need no signs, so taking them off keeps it the
/// same build.
#[test]
fn an_adders_signs_may_come_off() {
    let old = stored("alpha-rca-slice");
    let blocks = Blocks::read(&old).unwrap();
    let grid: HashMap<[i32; 3], String> = blocks
        .iter()
        .filter(|(_, state)| !state.contains("_sign"))
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let mut nbt = schem::decode(&old).unwrap();
    nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, Vec::new())
        .unwrap();
    let new = schem::encode(nbt).unwrap();
    assert_eq!(warned(&old, &new), Vec::<String>::new());
}

#[test]
fn a_resave_with_its_levers_moved_counts_only_the_levers() {
    let old = stored("alpha-rca");
    let blocks = Blocks::read(&old).unwrap();
    let lever = blocks.switches().into_iter().next().unwrap();
    let state = blocks
        .get(lever.pos)
        .replace("powered=false", "powered=true");
    let new = edited(&old, lever.pos, &state);
    assert_eq!(compare(&blocks, &Blocks::read(&new).unwrap()).changed, 1);
}
