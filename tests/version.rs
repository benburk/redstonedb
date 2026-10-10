//! Whether a new file may replace a build as a new version.

use std::collections::HashMap;

use app::{
    import,
    schem::{self, Blocks},
    standardize,
    version::{compare, refusals},
};

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    standardize::pink_status(&tidied).expect("fixture recolours")
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

/// Why `new` cannot replace `old`, both as stored.
fn refused(old: &[u8], new: &[u8]) -> Vec<String> {
    let (old, new) = (Blocks::read(old).unwrap(), Blocks::read(new).unwrap());
    let size = |blocks: &Blocks| import::measure(blocks, true).unwrap();
    let old_size = size(&old);
    let old_size = (old_size.x.into(), old_size.y.into(), old_size.z.into());
    refusals(&old, old_size, &new, &size(&new), compare(&old, &new))
}

#[test]
fn a_one_block_fix_is_a_new_version() {
    let old = stored("alpha-rca");
    let blocks = Blocks::read(&old).unwrap();
    // A block of plain white concrete turned to white glass.
    let (pos, _) = blocks
        .iter()
        .find(|(_, state)| *state == "minecraft:white_concrete")
        .unwrap();
    let new = edited(&old, pos, "minecraft:white_stained_glass");
    let comparison = compare(&blocks, &Blocks::read(&new).unwrap());
    assert_eq!(comparison.changed, 1);
    assert!(refused(&old, &new).is_empty());
}

#[test]
fn a_different_adder_is_not_a_new_version() {
    let refusals = refused(&stored("alpha-rca"), &stored("alpha-cla"));
    assert!(refusals.iter().any(|r| r.contains("keeps at least 95%")));
    assert!(
        refusals
            .iter()
            .any(|r| r.contains("may change only the height"))
    );
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
