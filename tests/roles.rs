//! What a build's blocks are for, read by the rules of redstone.

use std::collections::{BTreeMap, HashMap};

use app::{
    roles::{self, Need, Role},
    schem::{self, Blocks, Pos},
    simulate::{Report, Test},
    standardize,
};

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    standardize::pink_status(&tidied).expect("fixture recolours")
}

/// `data` three blocks taller, with a block and dust on it standing on
/// their own above the build's north-west corner.
fn with_stub(data: &[u8]) -> (Vec<u8>, Pos) {
    let blocks = Blocks::read(data).unwrap();
    let [x, y, z] = blocks.size;
    let mut grid: HashMap<Pos, String> =
        blocks.iter().map(|(p, s)| (p, s.to_owned())).collect();
    let stub = [0, y + 1, 0];
    grid.insert(stub, "minecraft:white_concrete".to_owned());
    grid.insert(
        [0, y + 2, 0],
        "minecraft:redstone_wire[east=side,north=side,power=0,south=side,\
         west=side]"
            .to_owned(),
    );
    let mut nbt = schem::decode(data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], [x, y + 3, z], &grid, entities)
        .unwrap();
    (schem::encode(nbt).unwrap(), stub)
}

/// `data` with the blocks at `gone` taken out.
fn without(data: &[u8], gone: impl IntoIterator<Item = Pos>) -> Vec<u8> {
    let changes: BTreeMap<Pos, String> = gone
        .into_iter()
        .map(|p| (p, schem::AIR.to_owned()))
        .collect();
    standardize::edit(data, &changes).unwrap()
}

fn unused(roles: &BTreeMap<Pos, Role>) -> Vec<Pos> {
    roles
        .iter()
        .filter(|(_, r)| r.need == Need::Unused)
        .map(|(&p, _)| p)
        .collect()
}

#[test]
fn a_block_holding_only_dust_that_leads_nowhere_is_unused() {
    let (data, stub) = with_stub(&stored("rca-slice-capped"));
    let found = unused(&roles::roles(&data, &[]).unwrap());
    assert!(found.contains(&stub));
    assert!(found.contains(&schem::step(stub, schem::Direction::Up)));
}

#[test]
fn torches_keep_their_blocks_and_the_cleaned_adder_still_adds() {
    let data = stored("rca-slice-capped");
    let blocks = Blocks::read(&data).unwrap();
    let roles = roles::roles(&data, &[]).unwrap();
    for (pos, state) in blocks.iter() {
        if let Some(base) = schem::torch_block(pos, state) {
            assert_ne!(roles.get(&base).map(|r| r.need), Some(Need::Unused));
        }
    }
    let cleaned = without(&data, unused(&roles));
    assert!(roles::changed(&data, &cleaned, &[]).unwrap().is_empty());
    let test = Test::of("adder", Some(1)).unwrap();
    let report = test.run(&Blocks::read(&cleaned).unwrap());
    assert!(matches!(report, Report::Ran(run) if run.wrong == 0));
}

#[test]
fn taking_out_a_torchs_block_changes_the_circuit() {
    let data = stored("rca-slice-capped");
    let blocks = Blocks::read(&data).unwrap();
    let base = blocks
        .iter()
        .find_map(|(pos, state)| schem::torch_block(pos, state))
        .unwrap();
    let broken = without(&data, [base]);
    assert!(!roles::changed(&data, &broken, &[]).unwrap().is_empty());
}
