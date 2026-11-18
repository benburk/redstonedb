//! What a build's blocks are for, read by the rules of redstone.

use std::collections::{BTreeMap, HashMap};

use app::{
    category::{Kind, Test},
    recognize::roles::{self, Need, Role},
    schem::{self, Blocks, Pos},
    simulate::Report,
    stack,
    standard::standardize,
};

/// The fixture file at `path`, carrying the roles its colours mark, as the
/// fixtures are saved in the site's old colours; see
/// `colours::colour_roles`.
fn read_fixture(path: &str) -> std::io::Result<Vec<u8>> {
    let raw = std::fs::read(path)?;
    Ok(app::recognize::colours::with_colour_roles(&raw)
        .expect("fixture carries its roles"))
}

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let raw = read_fixture(&format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    app::recognize::colours::with_colour_roles(&tidied)
        .expect("fixture carries its roles")
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

/// `data` one block wider to the west, with a block standing on its own
/// in the new column, holding a tripwire hook on its east face, as builds
/// hang hooks to turn dust.
fn with_hook(data: &[u8]) -> (Vec<u8>, Pos) {
    let blocks = Blocks::read(data).unwrap();
    let [x, y, z] = blocks.size;
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .map(|([px, py, pz], s)| ([px + 1, py, pz], s.to_owned()))
        .collect();
    let base = [0, 0, 0];
    grid.insert(base, "minecraft:white_concrete".to_owned());
    grid.insert(
        [1, 0, 0],
        "minecraft:tripwire_hook[attached=false,facing=east,powered=false]"
            .to_owned(),
    );
    let mut nbt = schem::decode(data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], [x + 1, y, z], &grid, entities)
        .unwrap();
    (schem::encode(nbt).unwrap(), base)
}

/// `data` with the blocks at `gone` taken out.
fn without(data: &[u8], gone: impl IntoIterator<Item = Pos>) -> Vec<u8> {
    let changes: BTreeMap<Pos, String> = gone
        .into_iter()
        .map(|p| (p, schem::AIR.to_owned()))
        .collect();
    schem::edit(data, &changes).unwrap()
}

fn unused(roles: &BTreeMap<Pos, Role>) -> Vec<Pos> {
    roles
        .iter()
        .filter(|(_, r)| r.need == Need::Unused)
        .map(|(&p, _)| p)
        .collect()
}

/// Dust that leads nowhere still shapes the dust it joins, so neither it
/// nor the block under it is ever taken out.
#[test]
fn a_block_holding_dust_that_leads_nowhere_stays() {
    let (data, stub) = with_stub(&stored("rca-slice-capped"));
    let found = unused(&roles::roles(&data, &[], stack::Buses::own()).unwrap());
    assert!(!found.contains(&stub));
    assert!(!found.contains(&schem::step(stub, schem::Direction::Up)));
}

/// The game drops a tripwire hook whose block is gone, though the
/// simulator runs it floating, so its block stays, and the hook too.
#[test]
fn a_block_holding_a_tripwire_hook_stays() {
    let (data, base) = with_hook(&stored("rca-slice-capped"));
    let roles = roles::roles(&data, &[], stack::Buses::own()).unwrap();
    assert!(!unused(&roles).contains(&base));
    // Nor is the hook a building block, to be taken out itself.
    assert!(!roles.contains_key(&schem::step(base, schem::Direction::East)));
}

#[test]
fn torches_keep_their_blocks_and_the_cleaned_adder_still_adds() {
    let data = stored("rca-slice-capped");
    let blocks = Blocks::read(&data).unwrap();
    let roles = roles::roles(&data, &[], stack::Buses::own()).unwrap();
    for (pos, state) in blocks.iter() {
        if let Some(base) = schem::torch_block(pos, state) {
            assert_ne!(roles.get(&base).map(|r| r.need), Some(Need::Unused));
        }
    }
    let cleaned = without(&data, unused(&roles));
    assert!(
        roles::changed(&data, &cleaned, &[], stack::Buses::own())
            .unwrap()
            .is_empty()
    );
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
    assert!(
        !roles::changed(&data, &broken, &[], stack::Buses::own())
            .unwrap()
            .is_empty()
    );
}

/// A register file's copies share its data bus, joined by dust up its back
/// that a lone copy does not use; dust is never unused, so it and its
/// blocks stay, read alone or as the site stacks it.
#[test]
fn the_dust_joining_a_register_files_bus_stays() {
    use app::{interface::Orientation, stack::StackDirection};
    let data = read_fixture("tests/fixtures/register-8-shared-bus-stack.schem")
        .unwrap();
    let way = StackDirection {
        west: 4,
        up: 0,
        max_copies: Some(3),
        faces: None,
        pitch: None,
    };
    let unused = |buses| {
        unused(&roles::roles(&data, &[way], buses).unwrap())
            .into_iter()
            .filter(|&[x, _, z]| (x, z) == (1, 8))
            .count()
    };
    assert_eq!(unused(stack::Buses::own()), 0);
    assert_eq!(
        unused(stack::Buses::of(
            Kind::Register.copies(),
            Orientation::Vertical
        )),
        0
    );
}

/// Torchless Hex Max, 4d7912fa, built with cakes giving its constants: the
/// cakes are barrels once uploaded, which need nothing under them, so the
/// two blocks that held up its top cakes are taken out, and nothing else.
#[test]
fn the_blocks_under_a_hex_maxs_barrels_are_taken_out() {
    let data = read_fixture("tests/fixtures/hex-max-8.schem").unwrap();
    let found = app::recognize::probe::recognize(&data).unwrap();
    let blocks = Blocks::read(&found.file).unwrap();
    let pruned = roles::prune(&found.file, &[], stack::Buses::own())
        .unwrap()
        .expect("it has blocks to take out");
    assert_eq!(pruned.gone.len(), 2);
    for pos in pruned.gone {
        let above = schem::step(pos, schem::Direction::Up);
        assert_eq!(schem::short(blocks.get(above)), "barrel", "{pos:?}");
    }
}

/// zPippo_'s Incrementer/Decrementer, fd6f7e47, with a lamp lit by each
/// output bit: the lamps power nothing, so they are taken out, and nothing
/// else; the lamps its levers hang on stay.
#[test]
fn an_inc_decs_output_lamps_are_taken_out() {
    let data =
        read_fixture("tests/fixtures/inc-dec-8-indicator-lamps.schem").unwrap();
    let found = app::recognize::probe::recognize(&data).unwrap();
    let blocks = Blocks::read(&found.file).unwrap();
    let lamps = blocks
        .iter()
        .filter(|(_, state)| schem::short(state) == "redstone_lamp")
        .count();
    let pruned = roles::prune(&found.file, &[], stack::Buses::own())
        .unwrap()
        .expect("it has blocks to take out");
    assert_eq!(pruned.gone.len(), 8);
    for pos in &pruned.gone {
        assert_eq!(schem::short(blocks.get(*pos)), "redstone_lamp", "{pos:?}");
    }
    assert_eq!(lamps, 16);
}
