//! Simulating the fixture adders through every sum.

use std::{collections::HashMap, sync::atomic::AtomicBool};

use app::{
    import, interface, lint,
    schem::{self, Blocks},
    simulate::{self, Report, Stacking, Test},
    stack, standardize,
};

/// A fixture as the site stores it: markers out, box fitted, and status
/// lines pink, since the fixtures predate that.
fn blocks(name: &str) -> Blocks {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    let stored = standardize::pink_status(&tidied).expect("fixture recolours");
    Blocks::read(&stored).expect("fixture parses")
}

#[test]
fn a_ripple_carry_adder_gets_every_sum_right() {
    let report = Test::Adder.run(&blocks("alpha-rca"));
    assert!(report.passed());
    let Report::Ran(run) = report else {
        unreachable!()
    };
    assert!(run.exhaustive);
    assert_eq!(run.cases, 1 << 17);
}

/// Its carry out has a torch beside it, which lights a lamp but does not
/// power the block, so reading a lamp in its place would get this wrong.
#[test]
fn an_output_is_read_from_its_block_not_a_torch_beside_it() {
    let run = simulate::check_adder(&blocks("alpha-rca-slice")).unwrap();
    assert_eq!((run.cases, run.wrong), (8, 0));
}

/// The game agrees: mc-tick, a vanilla-accurate engine, lights bit 7 for
/// 95 + 1 too.
#[test]
fn a_carry_lookahead_adder_carries_into_bit_7_wrongly() {
    let run = simulate::check_adder(&blocks("alpha-cla")).unwrap();
    assert_eq!(run.wrong, 8192);
    let first = &run.examples[0];
    assert_eq!(first.inputs, [95, 1, 0]);
    assert_eq!(
        (&*first.expected, &*first.got),
        (&[96, 0][..], &[224, 0][..])
    );
    let bits: Vec<(&str, u64)> = run
        .wrong_bits
        .iter()
        .map(|(bit, count)| (bit.as_str(), *count))
        .collect();
    assert_eq!(bits, [("output bit 7", 8192), ("carry out", 4096)]);
    assert_eq!(run.hint, None);
}

/// A torch inverts its carry in, which is saved with the lever off, so
/// every sum is one more or less than it should be, and flipping carry in
/// puts every sum right.
#[test]
fn a_carry_in_saved_the_wrong_way_round_is_named_as_the_cause() {
    let run = simulate::check_adder(&blocks("vert-3t-cca")).unwrap();
    assert_eq!(run.wrong, run.cases);
    assert!(run.hint.unwrap().contains("flipped"));
}

#[test]
fn a_build_with_a_hopper_gets_every_sum_right() {
    assert!(Test::Adder.run(&blocks("diag-3t-cca")).passed());
}

/// Its hopper holds a fixed strength, which a barrel gives too; but dust
/// lies on the hopper, which does not conduct, and a barrel does, so the
/// swapped adder breaks. That is why hoppers stay unless asked.
#[test]
fn a_hopper_swaps_for_a_barrel_of_the_same_strength() {
    let raw = std::fs::read("tests/fixtures/diag-3t-cca.schem").unwrap();
    // As the site stores it, with its carry out pink.
    let stored =
        standardize::pink_status(&standardize::tidy(&raw).unwrap()).unwrap();
    let before = Blocks::read(&stored).unwrap();
    let swapped = standardize::barrels(&stored, &["hopper"]).unwrap();
    let after = Blocks::read(&swapped).unwrap();
    assert_eq!(standardize::swappable(&before).hoppers, 1);
    assert_eq!(standardize::swappable(&after).hoppers, 0);
    let strength = |blocks: &Blocks, block: &str| {
        let (pos, _) = blocks
            .iter()
            .find(|(_, state)| state.starts_with(block))
            .unwrap();
        schem::container_strength(
            blocks.containers.get(&pos).map_or(&[], Vec::as_slice),
            schem::slots(blocks.get(pos)).unwrap(),
        )
    };
    assert_eq!(
        strength(&after, "minecraft:barrel"),
        strength(&before, "minecraft:hopper")
    );
    assert!(!Test::Adder.run(&after).passed());
}

/// Its paths from A to the sum take from 4 to 6 ticks, depending on the
/// case, so the sum can change twice whenever carry in arrives.
#[test]
fn a_ripple_slice_is_timed_from_a() {
    let timing = Test::Adder
        .time(&blocks("alpha-rca-slice"), &AtomicBool::new(false))
        .unwrap();
    let inputs: Vec<(&str, Option<i32>)> = timing
        .inputs
        .iter()
        .map(|input| (input.name.as_str(), input.tick))
        .collect();
    assert_eq!(
        inputs,
        [("A", Some(0)), ("B", Some(0)), ("carry in", Some(3))]
    );
    let outputs: Vec<(&str, bool, i32)> = timing
        .outputs
        .iter()
        .map(|output| (output.name.as_str(), output.status, output.latency))
        .collect();
    assert_eq!(outputs, [("output", false, 6), ("carry out", true, 5)]);
    assert_eq!(timing.latency(), Some(6));
    assert!(!timing.synced);
}

#[test]
fn an_adders_width_is_its_lime_bit_count() {
    let width =
        |name| import::derived_width("adder", &interface::read(&blocks(name)));
    assert_eq!(width("alpha-rca"), Some(8));
    assert_eq!(width("alpha-rca-slice"), Some(1));
    let multiplier = interface::read(&blocks("alpha-rca"));
    assert_eq!(import::derived_width("multiplier", &multiplier), None);
}

/// `data` without its signs.
fn unsigned(data: &[u8]) -> Vec<u8> {
    let blocks = Blocks::read(data).unwrap();
    let grid: HashMap<[i32; 3], String> = blocks
        .iter()
        .filter(|(_, state)| !state.contains("_sign"))
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let mut nbt = schem::decode(data).unwrap();
    nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, Vec::new())
        .unwrap();
    schem::encode(nbt).unwrap()
}

/// Its carry in and carry out are its only magenta and pink lines, so
/// they need no signs.
#[test]
fn an_adders_lone_lines_need_no_signs() {
    let raw = std::fs::read("tests/fixtures/alpha-rca-slice.schem").unwrap();
    let tidied = standardize::tidy(&raw).unwrap();
    let stored = standardize::pink_status(&tidied).unwrap();
    let blocks = Blocks::read(&unsigned(&stored)).unwrap();
    assert!(blocks.signs.is_empty());
    let unsigned_line = |violations: &[lint::Violation]| {
        violations
            .iter()
            .any(|v| v.message.contains("no sign naming it"))
    };
    assert!(!unsigned_line(&lint::check(&blocks)));
    assert!(!unsigned_line(&lint::check_for(&blocks, "adder")));
    assert!(unsigned_line(&lint::check_for(&blocks, "multiplier")));
    let run = simulate::check_adder(&blocks).unwrap();
    assert_eq!((run.cases, run.wrong), (8, 0));
    assert_eq!(run.pattern, "A + B + carry in");
}

/// An 8-bit adder that repeats every 16 blocks up, as `//rstack up 1 16`
/// stacks it, overlapping the copy below. Its comparators read barrels and
/// a furnace; the first copy alone keeps its carry in, the gray logic under
/// its least significant bit, and the furnace giving that bit's 1ss.
#[test]
fn an_8_bit_adder_stacks_into_a_16_bit_one() {
    let data = std::fs::read("tests/fixtures/vert-adder-8-stacks.schem")
        .expect("fixture is readable");
    assert!(Test::Adder.run(&Blocks::read(&data).unwrap()).passed());
    // The step comes from its bits, 2 up each, and two copies make 16
    // bits, too many to try every sum: each byte's, and the patterns.
    let Some(Stacking::Stacks {
        step,
        copies,
        bits,
        cases,
    }) = Test::Adder.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!(step.describe(), "16 up");
    // Three, so one sits between two others: 24 bits.
    assert_eq!((copies, bits), (3, 24));
    assert!(cases > 1 << 18);
    let (stacked, _) = stack::stack(&data, &[(step.direction(), 2)]).unwrap();
    let blocks = Blocks::read(&stacked).unwrap();
    // Timed over the test patterns, as it is too wide to time every value.
    let timing = Test::Adder.time(&blocks, &AtomicBool::new(false)).unwrap();
    assert!(timing.patterns);
    assert_eq!(timing.latency(), Some(4));
}

/// Its outputs are read through the repeaters standing on them, which lead
/// the product out of the build, since nothing powers the yellow blocks.
#[test]
fn a_combinational_multiplier_tries_every_product() {
    let raw = std::fs::read("tests/fixtures/mult-8-diag-signed.schem")
        .expect("fixture is readable");
    let data = standardize::barrels(
        &standardize::standardize(&raw).unwrap(),
        &["furnace"],
    )
    .unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let Report::Ran(run) = Test::Multiplier.run(&blocks) else {
        panic!("the multiplier was not simulated");
    };
    assert_eq!(run.pattern, "A × B");
    assert!(run.exhaustive);
    assert_eq!(run.cases, 1 << 16);
    let outputs: Vec<(&str, usize)> = run
        .outputs
        .iter()
        .map(|s| (s.name.as_str(), s.bits))
        .collect();
    assert_eq!(outputs, [("output", 16)]);
    // The low bits of 3 × 5 come out through the repeaters.
    let mut circuit =
        simulate::Circuit::new(&blocks, &interface::read(&blocks)).unwrap();
    circuit.set(0, 3);
    circuit.set(1, 5);
    circuit.settle().unwrap();
    assert_eq!(circuit.get(0) & 0xF, 15);
}

/// A multiplier with a clock, reset, or calculate line is sequential.
#[test]
fn a_sequential_multiplier_is_skipped() {
    let report = Test::Multiplier.run(&blocks("mult-8-buttons"));
    let Report::Skipped { reason } = report else {
        panic!("tested a sequential multiplier");
    };
    assert!(reason.contains("not combinational"), "{reason}");
}

/// A 1-bit slice steps from its carry in to its carry out, which the next
/// copy's carry in lands on.
#[test]
fn a_ripple_carry_slice_stacks_any_number_of_times() {
    let raw = std::fs::read("tests/fixtures/alpha-rca-slice.schem").unwrap();
    let data =
        standardize::pink_status(&standardize::tidy(&raw).unwrap()).unwrap();
    let Some(Stacking::Stacks { step, copies, .. }) =
        Test::Adder.stacking(&data)
    else {
        panic!("it does not stack");
    };
    // Nine reach a copy past the 15 blocks dust carries.
    assert_eq!((step.describe().as_str(), copies), ("2 west", 9));
}

/// Its copies fit, but a carry out of one never reaches the next, which
/// the sums across the seam find before the full test.
#[test]
fn an_adder_whose_carry_stops_at_its_edge_does_not_stack() {
    let raw = std::fs::read("tests/fixtures/alpha-rca.schem").unwrap();
    let data =
        standardize::pink_status(&standardize::tidy(&raw).unwrap()).unwrap();
    let Some(Stacking::Not { reason, .. }) = Test::Adder.stacking(&data) else {
        panic!("it stacks");
    };
    assert!(reason.contains("carry does not pass"), "{reason}");
}

/// Its control line, `increment`, adds 1 when on.
#[test]
fn an_incrementer_gets_every_value_right() {
    let report = Test::Incrementer.run(&blocks("inc-8"));
    let Report::Ran(run) = &report else {
        panic!("the incrementer was not simulated");
    };
    assert_eq!(run.pattern, "A + increment");
    assert_eq!((run.cases, run.wrong), (512, 0));
}

/// A quick run of the tests a file fits tells what it is.
#[test]
fn a_build_is_known_for_what_it_does() {
    let adder = simulate::guess(&blocks("alpha-rca")).unwrap();
    assert_eq!((adder.category, adder.wrong), ("adder", 0));
    let incrementer = simulate::guess(&blocks("inc-8")).unwrap();
    assert_eq!(
        (incrementer.category, incrementer.design),
        ("incrementer", Some("increment"))
    );
    // A carry in saved the wrong way round gets every sum wrong, so it is
    // no adder until fixed.
    assert!(simulate::guess(&blocks("vert-3t-cca")).is_none());
}
