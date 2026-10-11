//! Simulating the fixture adders through every sum.

use std::{collections::HashMap, sync::atomic::AtomicBool};

use app::{
    import, interface, lint,
    schem::{self, Blocks},
    simulate::{self, Report, Test},
    standardize,
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
fn a_build_with_a_hopper_is_skipped_and_says_why() {
    let report = Test::Adder.run(&blocks("diag-3t-cca"));
    let Report::Skipped { reason } = report else {
        panic!("simulated a build with a hopper");
    };
    assert_eq!(
        reason,
        "It has a hopper, which the simulator does not model."
    );
}

#[test]
fn only_8_bit_adders_have_a_test_for_now() {
    assert_eq!(Test::of("adder", Some(8)), Some(Test::Adder));
    assert_eq!(Test::of("adder", Some(16)), None);
    assert_eq!(Test::of("multiplier", Some(8)), None);
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
