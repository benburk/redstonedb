//! Simulating the fixture adders through every sum.

use std::{collections::HashMap, sync::atomic::AtomicBool};

use app::{
    import, interface, lint,
    models::schematic::{Encoding, Orientation},
    schem::{self, Blocks, Direction},
    simulate::{self, Report, Run, Stacking, Takes, Test},
    stack,
    standardize::{self, Gray},
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

/// The run of an adder's test, which a fixture adder always gets.
fn adder_run(blocks: &Blocks) -> Result<Box<Run>, Report> {
    match Test::Adder.run(blocks) {
        Report::Ran(run) => Ok(run),
        other => Err(other),
    }
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
    let run = adder_run(&blocks("alpha-rca-slice")).unwrap();
    assert_eq!((run.cases, run.wrong), (8, 0));
}

/// The game agrees: mc-tick, a vanilla-accurate engine, lights bit 7 for
/// 95 + 1 too.
#[test]
fn a_carry_lookahead_adder_carries_into_bit_7_wrongly() {
    let run = adder_run(&blocks("alpha-cla")).unwrap();
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
    let run = adder_run(&blocks("vert-3t-cca")).unwrap();
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
    let run = adder_run(&blocks).unwrap();
    assert_eq!((run.cases, run.wrong), (8, 0));
    assert_eq!(run.pattern, "A + B + carry in");
}

/// An 8-bit adder that repeats every 16 blocks up, as `//rstack up 1 16`
/// stacks it, overlapping the copy below. Its comparators read barrels; the
/// first copy alone keeps its carry in, the gray logic under its least
/// significant bit, and the barrel on gray giving that bit's 1ss.
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
    let (stacked, _) =
        stack::stack(&data, &[(step.direction(), 2)], stack::Buses::own())
            .unwrap();
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
    let data = standardize::standardize(&raw, Gray::Whiten).unwrap();
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
fn a_stateful_multiplier_is_skipped() {
    let report = Test::Multiplier.run(&blocks("mult-8-buttons"));
    let Report::Skipped { reason } = report else {
        panic!("tested a sequential multiplier");
    };
    assert!(reason.contains("stateful"), "{reason}");
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
    assert_eq!(incrementer.category, "incrementer");
    // A carry in saved the wrong way round gets every sum wrong, so it is
    // no adder until fixed.
    assert!(simulate::guess(&blocks("vert-3t-cca")).is_none());
}

/// The barrel giving its least significant bit's 1ss stands on gray, so
/// only the first copy keeps it; without the gray block, every copy has
/// one, and adds 1 at its first bit.
#[test]
fn a_barrel_on_gray_belongs_to_the_first_copy() {
    let data = std::fs::read("tests/fixtures/vert-adder-8-stacks.schem")
        .expect("fixture is readable");
    let blocks = Blocks::read(&data).unwrap();
    let gray = [5, 1, 1];
    assert!(
        blocks
            .get(schem::step(gray, Direction::Up))
            .starts_with("minecraft:barrel")
    );
    let grid: HashMap<_, _> = blocks
        .iter()
        .filter(|(pos, state)| *state != schem::AIR && *pos != gray)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let mut nbt = schem::decode(&data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    let unmarked = schem::encode(nbt).unwrap();
    let Some(Stacking::Not { reason, .. }) = Test::Adder.stacking(&unmarked)
    else {
        panic!("it stacks without the gray block");
    };
    assert!(
        reason.contains("A 0 + B 0 + carry in 0 gives 65792"),
        "{reason}"
    );
}

/// A carry cancel adder of 8 hex digits: each input block's dust takes a
/// strength from 0 to 15, so A and B are 32 bits each, too many to try
/// every sum, and it is slower to run than one of levers. Each digit takes
/// its every case, with a carry into it and without, and the patterns.
#[test]
fn a_hex_adder_is_driven_digit_by_digit() {
    let raw = std::fs::read("tests/fixtures/hex-cca-8.schem").unwrap();
    let data = standardize::standardize(&raw, Gray::Whiten).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let interface = interface::read(&blocks);
    let derived = import::derived("adder", &interface);
    assert_eq!(
        (derived.width, derived.orientation, derived.encoding),
        (Some(32), Some(Orientation::Vertical), Some(Encoding::Hex))
    );
    let Report::Ran(run) = Test::Adder.run(&blocks) else {
        panic!("the hex adder was not simulated");
    };
    let widths: Vec<(&str, usize)> = run
        .outputs
        .iter()
        .map(|signal| (signal.name.as_str(), signal.bits))
        .collect();
    assert_eq!(widths, [("output", 32), ("carry out", 1)]);
    assert!(!run.exhaustive && run.by_digit && run.cases < 20_000);
    assert_eq!(run.wrong, 0);
    assert_eq!(simulate::guess(&blocks).unwrap().category, "adder");
}

/// The dust carrying its carry in to its least significant bit stands on
/// gray glass. Whitened, every copy keeps the glass and dust, and a later
/// copy's dust replaces a glass block of the copy before, as `//rstack`
/// replaces it, so it does not add right as saved.
#[test]
fn a_copy_must_not_overwrite_the_copy_before() {
    let data = std::fs::read("tests/fixtures/vert-adder-8-stacks.schem")
        .expect("fixture is readable");
    let blocks = Blocks::read(&data).unwrap();
    assert!(
        blocks
            .get([2, 1, 2])
            .starts_with("minecraft:gray_stained_glass")
    );
    let grid: HashMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| *state != schem::AIR)
        .map(|(pos, state)| {
            let state = if pos == [2, 1, 2] {
                "minecraft:white_stained_glass"
            } else {
                state
            };
            (pos, state.to_owned())
        })
        .collect();
    let mut nbt = schem::decode(&data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    let whitened = schem::encode(nbt).unwrap();
    let Some(Stacking::Not { reason, .. }) = Test::Adder.stacking(&whitened)
    else {
        panic!("it stacks with the glass whitened");
    };
    assert!(reason.contains("sums wrong"), "{reason}");
}

/// A 1-bit ripple carry slice two blocks high whose carry leaves by dust,
/// where the next copy's carry in lands, with no carry out line to say
/// where that copy goes; the shortest step whose copies add right is
/// found instead.
#[test]
fn a_slice_without_a_carry_out_finds_its_step() {
    let data = std::fs::read("tests/fixtures/rca-slice-2-high.schem").unwrap();
    assert!(Test::Adder.run(&Blocks::read(&data).unwrap()).passed());
    let Some(Stacking::Stacks { step, copies, .. }) =
        Test::Adder.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), copies), ("2 up", 9));
}

/// A 1-bit ripple carry slice two blocks high whose carry out is capped by
/// a block that the copy above replaces, as `//rstack` pastes over it, so
/// its copies chain only when stacked as in-game.
#[test]
fn a_copy_may_cover_the_copy_below() {
    let data = std::fs::read("tests/fixtures/rca-slice-capped.schem").unwrap();
    assert!(Test::Adder.run(&Blocks::read(&data).unwrap()).passed());
    let Some(Stacking::Stacks { step, copies, .. }) =
        Test::Adder.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), copies), ("2 up", 9));
}

/// Nothing powers its yellow output blocks: the repeaters standing on
/// them carry the sum out of the build, a tick later, which its latency
/// leaves out. Its carry out has none.
#[test]
fn an_output_is_timed_where_it_reaches_its_repeaters() {
    let timing = Test::Adder
        .time(&blocks("cca-8-repeated-out"), &AtomicBool::new(false))
        .unwrap();
    let outputs: Vec<(&str, i32, i32)> = timing
        .outputs
        .iter()
        .map(|output| (output.name.as_str(), output.latency, output.carried))
        .collect();
    assert_eq!(outputs, [("output", 3, 1), ("carry out", 5, 0)]);
    assert_eq!(timing.latency(), Some(3));
}

/// Its sum leaves by dust on each yellow block. For 2 + 2, the carry
/// reaches bit 2 at strength 1, which powers the block beside it but
/// leaves the dust on top dark, so the bit is off, as in-game.
#[test]
fn an_output_is_read_from_the_dust_on_its_block() {
    let Report::Ran(run) = Test::Adder.run(&blocks("cca-8-dust-out")) else {
        panic!("the adder was not simulated");
    };
    assert!(run.wrong > 0);
    let wrong = &run.examples[0];
    assert_eq!(
        (wrong.inputs.as_slice(), wrong.got[0], wrong.expected[0]),
        ([2, 2, 0].as_slice(), 0, 4)
    );
}

/// A 10-bit BCD input, from 0 to 255, into eight output bits.
#[test]
fn a_bcd_to_binary_converter_gets_every_decimal_value_right() {
    let report = Test::BcdToBinary.run(&blocks("bcd2bin-10"));
    let Report::Ran(run) = &report else {
        panic!("the converter was not simulated: {report:?}");
    };
    assert_eq!(run.pattern, "A from BCD");
    assert_eq!((run.cases, run.wrong), (256, 0));
    assert!(run.exhaustive);
}

#[test]
fn a_binary_to_bcd_converter_gets_every_value_right() {
    let report = Test::BinaryToBcd.run(&blocks("bin2bcd-8"));
    let Report::Ran(run) = &report else {
        panic!("the converter was not simulated: {report:?}");
    };
    assert_eq!(run.pattern, "A in BCD");
    assert_eq!((run.cases, run.wrong), (256, 0));
}

/// Its timing drives only valid BCD, so the invalid values it was never
/// built for cannot make it look unsynced.
#[test]
fn a_bcd_converter_is_timed_on_valid_values() {
    let cancel = AtomicBool::new(false);
    let timing = Test::BcdToBinary
        .time(&blocks("bcd2bin-10"), &cancel)
        .unwrap();
    assert!(timing.latency().is_some_and(|t| t > 0));
    // A alone from 0 to each of its 255 other values and back, then all
    // inputs together from all off to each of the 256 and back: every 10-bit
    // value would make some four thousand.
    assert_eq!(timing.changes, 510 + 512);
    let timing = Test::BinaryToBcd
        .time(&blocks("bin2bcd-8"), &cancel)
        .unwrap();
    assert!(timing.latency().is_some_and(|t| t > 0));
}

#[test]
fn a_converter_is_known_for_what_it_does() {
    let guess = simulate::guess(&blocks("bin2bcd-8")).unwrap();
    assert_eq!((guess.category, guess.wrong), ("binary_to_bcd", 0));
    let guess = simulate::guess(&blocks("bcd2bin-10")).unwrap();
    assert_eq!((guess.category, guess.wrong), ("bcd_to_binary", 0));
}

/// Saved with bit 2 of A's lever on, which reads as an inverted input, so
/// nearly every case comes out 4 off; driven plainly, every one is right.
#[test]
fn a_lever_saved_on_by_mistake_is_named() {
    let report = Test::BcdToBinary.run(&blocks("bcd2bin-10-lever-on"));
    let Report::Ran(run) = &report else {
        panic!("the converter was not simulated: {report:?}");
    };
    assert_eq!(run.wrong, 232);
    let hint = run.hint.as_deref().unwrap_or_default();
    assert!(hint.contains("bit 2 of A"), "{hint}");
}

/// From Schemati: eleven control lines, seven saved on, each sign naming
/// the operations its line takes part in, its status lines signed carry
/// out, overflow, and lsb shift out. Every result and status comes out
/// right, its carry out the adder's for add and subtract alike.
#[test]
fn an_alu_runs_each_operation_its_signs_name() {
    let raw = std::fs::read("tests/fixtures/alu-8-ops-raw.schem").unwrap();
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false)
        .unwrap()
        .blocks;
    let report = Test::Alu.run(&blocks);
    let Report::Ran(run) = &report else {
        panic!("the ALU was not simulated: {report:?}");
    };
    assert_eq!(
        run.pattern,
        "add, sub, and, or, xor, nand, nor, xnor, shr, asr"
    );
    assert_eq!((run.cases, run.wrong), (10 * 65_536, 0));
    let outputs: Vec<&str> =
        run.outputs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        outputs,
        ["output", "carry out", "overflow", "lsb shift out"]
    );
    // Standardizing took the commas out of its signs, and the signs name
    // its operations as flags.
    assert!(
        blocks
            .sign_texts()
            .values()
            .flat_map(|l| l.iter())
            .all(|l| !l.contains(','))
    );
    let mut flags = import::sign_flags(&interface::read(&blocks));
    flags.sort();
    flags.dedup();
    for flag in ["and", "or", "xor", "nand", "nor", "xnor", "shr", "asr"] {
        assert!(flags.contains(&flag.to_owned()), "{flag} in {flags:?}");
    }
    assert!(!flags.contains(&"shl".to_owned()), "it has no shift left");
    // Its width, orientation, and encoding are read from A, as an adder's.
    let found = import::derived("alu", &interface::read(&blocks));
    assert_eq!(
        (found.width, found.orientation, found.encoding),
        (Some(8), Some(Orientation::Vertical), Some(Encoding::Binary))
    );
}

/// An earlier version of it, whose carry out was the adder's inverted: on
/// for an add that does not carry and off for one that does, as for a
/// subtract that borrows, so every add and subtract is wrong there.
#[test]
fn an_alus_inverted_carry_out_is_wrong() {
    let raw =
        std::fs::read("tests/fixtures/alu-8-ops-inverted-carry.schem").unwrap();
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false)
        .unwrap()
        .blocks;
    let Report::Ran(run) = Test::Alu.run(&blocks) else {
        panic!("the ALU was not simulated");
    };
    assert_eq!(run.wrong, 2 * 65_536);
    assert_eq!(run.wrong_bits, [("carry out".to_owned(), 2 * 65_536)]);
    assert_eq!(run.examples[0].op.as_deref(), Some("add"));
}

/// Its lines name single operations, and it adds with every line off and
/// ORs with AND and XOR both on, which no sign says, so those are found by
/// trying the lines. AND and OR get bit 7 wrong.
#[test]
fn an_alu_is_tried_on_operations_no_sign_names() {
    let report = Test::Alu.run(&blocks("alu-8-lines"));
    let Report::Ran(run) = &report else {
        panic!("the ALU was not simulated: {report:?}");
    };
    assert_eq!(
        run.pattern,
        "add (no lines on), and, or (xor + and on), xor"
    );
    // Add takes its carry in too: twice A and B's cases.
    assert_eq!(run.cases, 2 * 65_536 + 3 * 65_536);
    assert_eq!(run.wrong_bits, [("output bit 7".to_owned(), 16_384)]);
}

/// A register fixture as an upload standardizes it, as its file and its
/// blocks.
fn register(name: &str) -> (Vec<u8>, Blocks) {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
    let data = import::prepare(&raw, Some(Gray::Whiten), true, false)
        .unwrap()
        .data;
    let blocks = Blocks::read(&data).unwrap();
    (data, blocks)
}

/// Two staggered cells of an 8-bit register, each a write button and a
/// read lever on unsigned magenta lines, sharing lime data in and yellow
/// data out. Each cell takes every value, the bus stays empty with every
/// read off, and each read shows its cell alone, 6 ticks after its lever;
/// a write shows 8 ticks after its button, and data passes through a cell
/// whose write is held open in 4, its latency.
#[test]
fn a_register_stores_and_reads_each_cell() {
    let (_, blocks) = register("register-8-2-cells");
    assert!(lint::check(&blocks).is_empty());
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!(run.pattern, "each of 2 cells written, then read");
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert!(run.exhaustive);
    let timing = Test::Register
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((timing.read(), timing.synced), (Some(6), true));
    // A write shows 8 ticks after its button, but its latency is data in
    // to data out, its write held open and read on: 4.
    assert_eq!(timing.write, Some(8));
    assert_eq!((timing.data, timing.latency()), (Some(4), Some(4)));
    assert!(timing.from_read);
    let guess = simulate::guess(&blocks).unwrap();
    assert_eq!(guess.category, "register");
}

/// Copies 4 west share the data buses, driven and read at the first
/// copy's end, through the copies before: four make 8 registers, and a
/// fifth is past where the output bus reaches.
#[test]
fn a_register_file_stacks_west_until_its_bus_runs_too_far() {
    let (data, _) = register("register-8-2-cells");
    let Some(Stacking::Registers {
        step,
        copies,
        registers,
        most,
        ..
    }) = Test::Register.stacking(&data)
    else {
        panic!("the register does not stack");
    };
    assert_eq!(step.describe(), "4 west");
    assert_eq!((copies, registers, most), (4, 8, Some(4)));
}

/// A 1-bit cell whose write line's driver, a torch on a block on a torch
/// on its input, stands on a gray block, which later copies leave out:
/// copies 2 west share its read and write lines and make a wider register,
/// up to 7 bits, past which the write line's dust runs out.
#[test]
fn a_register_cell_stacks_west_into_more_bits() {
    let data =
        std::fs::read("tests/fixtures/register-1-gray-driver.schem").unwrap();
    let Some(Stacking::Wider {
        step,
        copies,
        bits,
        most,
        ..
    }) = Test::Register.stacking(&data)
    else {
        panic!("the register does not stack into more bits");
    };
    assert_eq!(step.describe(), "2 west");
    assert_eq!((copies, bits, most), (7, 7, Some(7)));
}

/// The same cell's timing: its bus is empty while its write lever is on,
/// and it takes its data as the lever turns off, showing it 3 ticks later.
/// Data changing as the lever turns off shows 4 ticks later, its latency.
#[test]
fn a_register_cell_that_takes_its_data_as_its_write_closes_is_timed_from_it() {
    let data =
        std::fs::read("tests/fixtures/register-1-gray-driver.schem").unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let timing = Test::Register
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(timing.takes, Takes::Closing);
    assert_eq!(
        (timing.read(), timing.write, timing.data),
        (Some(2), Some(3), Some(4))
    );
    assert_eq!(timing.latency(), Some(4));
}

/// A cell like it written by a button, which holds its write open for ten
/// ticks: it takes its data as the button springs back, so it is timed
/// from then, as the lever's is from turning off, not from the press.
#[test]
fn a_register_cell_written_by_a_button_is_timed_from_its_release() {
    let data = std::fs::read("tests/fixtures/register-1-button.schem").unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let timing = Test::Register
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(timing.takes, Takes::Closing);
    assert_eq!((timing.write, timing.data), (Some(3), Some(4)));
    assert_eq!(timing.latency(), Some(4));
}

/// The same cell with its driver's block whitened: every copy keeps a
/// driver, whose torch lands on the copy before's write line, so no step
/// stacks.
#[test]
fn a_register_cell_whose_driver_is_white_does_not_stack() {
    let (data, _) = register("register-1-gray-driver");
    assert!(matches!(
        Test::Register.stacking(&data),
        Some(Stacking::Not { .. })
    ));
}

/// The same register with barrels of 1 signal strength where its read
/// comparators subtract 15: nothing keeps a cell off the bus, so it shows
/// with every read off.
#[test]
fn a_register_whose_reads_are_not_gated_fails() {
    let (_, blocks) = register("register-8-weak-barrels");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert!(run.wrong > 0);
    assert!(run.hint.unwrap().contains("every read lever off"));
}

/// The register fixture with the levers on `colour` concrete saved on.
fn with_levers_on(colour: &str) -> Blocks {
    let (data, blocks) = register("register-8-2-cells");
    let on: Vec<_> = blocks
        .switches()
        .into_iter()
        .filter(|switch| {
            !switch.button
                && blocks.get(switch.block)
                    == format!("minecraft:{colour}_concrete")
        })
        .map(|switch| switch.pos)
        .collect();
    let grid: HashMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| *state != schem::AIR)
        .map(|(pos, state)| {
            let state = if on.contains(&pos) {
                state.replace("powered=false", "powered=true")
            } else {
                state.to_owned()
            };
            (pos, state)
        })
        .collect();
    let mut nbt = schem::decode(&data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    Blocks::read(&schem::encode(nbt).unwrap()).unwrap()
}

/// Its read levers saved on, each cell reads as saved and stops when its
/// lever is flipped: read inverted, which the test finds and holds each
/// lever flipped while not reading.
#[test]
fn a_register_whose_reads_are_saved_reading_passes() {
    let blocks = with_levers_on("magenta");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert_eq!(run.pattern, "each of 2 cells written, then read");
}

/// Its data levers saved on, so each cell takes the complement of what the
/// test writes, and its reads show the complement: an inverted output.
#[test]
fn a_register_whose_output_is_inverted_passes() {
    let blocks = with_levers_on("lime");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert_eq!(run.pattern, "each of 2 cells written, then read inverted");
}

/// A register whose reads show each cell inverted, on a bus resting at all
/// ones. Its read lines land on air in the copy before, 4 west, so each
/// copy keeps its own, and three copies make 6 registers.
#[test]
fn an_inverted_register_reads_and_stacks() {
    let (data, blocks) = register("register-8-inverted");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert_eq!(run.pattern, "each of 2 cells written, then read inverted");
    let Some(Stacking::Registers {
        step,
        registers,
        most,
        ..
    }) = Test::Register.stacking(&data)
    else {
        panic!("the register does not stack");
    };
    assert_eq!(
        (step.describe(), registers, most),
        ("4 west".to_owned(), 6, Some(3))
    );
}

/// A register whose comparators read hoppers, which carry no power, and
/// whose data levers share a column with its logic. Stacked 4 west, each
/// later copy's data blocks land on the copy before's data dust and are
/// left out, the bus running on through it; three copies make 6 registers.
#[test]
fn a_register_whose_data_levers_land_on_the_copy_before_stacks() {
    let (data, blocks) = register("register-8-hoppers");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    let Some(Stacking::Registers {
        step,
        registers,
        most,
        ..
    }) = Test::Register.stacking(&data)
    else {
        panic!("the register does not stack");
    };
    assert_eq!(
        (step.describe(), registers, most),
        ("4 west".to_owned(), 6, Some(3))
    );
}

/// An inverted register built on target blocks, whose data levers sit 4
/// east of the block and dust its data runs through: stacked 4 west, each
/// later copy's lime block lands on the copy before's data block, and its
/// lever on that block's dust, so both are left out and the bus runs on
/// through. Three copies make 6 registers.
#[test]
fn a_register_whose_data_levers_land_on_its_data_line_stacks() {
    let (data, blocks) = register("register-8-targets");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    let Some(Stacking::Registers {
        step,
        registers,
        most,
        ..
    }) = Test::Register.stacking(&data)
    else {
        panic!("the register does not stack");
    };
    assert_eq!(
        (step.describe(), registers, most),
        ("4 west".to_owned(), 6, Some(3))
    );
}

/// A register whose two cells are each 3 wide. Copies 1 west apart give a
/// line more inputs than Redpiler compiles, which counts as not working
/// rather than stopping the search; 6 west, each copy keeping its own
/// write buttons, two copies make 4 registers, and a third is past where
/// the bus reaches.
#[test]
fn a_register_with_wide_cells_stacks_at_its_width() {
    let (data, _) = register("register-8-wide-cells");
    let Some(Stacking::Registers {
        step,
        registers,
        most,
        ..
    }) = Test::Register.stacking(&data)
    else {
        panic!("the register does not stack");
    };
    assert_eq!(
        (step.describe(), registers, most),
        ("6 west".to_owned(), 4, Some(2))
    );
}

/// An 8-bit absolute value: A's magnitude, A taken as a signed number, in
/// every one of its 256 cases, the most negative giving itself.
#[test]
fn an_absolute_value_gives_each_inputs_magnitude() {
    let raw = std::fs::read("tests/fixtures/abs-8.schem").unwrap();
    let data = import::prepare(&raw, Some(Gray::Whiten), true, false)
        .unwrap()
        .data;
    let blocks = Blocks::read(&data).unwrap();
    let Report::Ran(run) = Test::AbsoluteValue.run(&blocks) else {
        panic!("the absolute value was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (256, 0));
    assert_eq!(run.pattern, "|A|");
    assert_eq!(simulate::guess(&blocks).unwrap().category, "absolute_value");
}

/// An 8-bit carry out: of A + B, only the carry, on one unsigned status
/// line, in every one of its 65,536 cases, timed by that line, as it has no
/// other output.
#[test]
fn a_carry_out_gives_the_carry_of_every_sum() {
    let raw = std::fs::read("tests/fixtures/cout-8.schem").unwrap();
    let data = import::prepare(&raw, Some(Gray::Whiten), true, false)
        .unwrap()
        .data;
    let blocks = Blocks::read(&data).unwrap();
    assert!(lint::check_for(&blocks, "carry_out").is_empty());
    let Report::Ran(run) = Test::CarryOut.run(&blocks) else {
        panic!("the carry out was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (1 << 16, 0));
    assert_eq!(run.pattern, "carry of A + B");
    assert_eq!(simulate::guess(&blocks).unwrap().category, "carry_out");
    let timing = Test::CarryOut
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert!(timing.latency().is_some());
}

/// An 8-bit vertical adder with its least significant bits at the top and
/// no carry out, whose B alone is signed `lsb` at its top: the sign orders
/// A and the output too, as they run alongside, so every case comes out
/// right and the guess finds an adder.
#[test]
fn one_lsb_sign_orders_the_buses_alongside() {
    // As an upload standardizes it, its blocks then all ones the
    // simulator models.
    let raw = std::fs::read("tests/fixtures/adder-8-lsb-top.schem").unwrap();
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false)
        .unwrap()
        .blocks;
    let read = interface::read(&blocks);
    for port in read.inputs.iter().chain(&read.outputs) {
        let (first, last) =
            (port.bits[0].pos, port.bits[port.bits.len() - 1].pos);
        assert!(
            first[1] > last[1],
            "{} counts up from the bottom",
            port.name
        );
    }
    let Report::Ran(run) = Test::Adder.run(&blocks) else {
        panic!("the adder was not simulated");
    };
    assert_eq!((run.pattern.as_str(), run.wrong), ("A + B", 0));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "adder");
}
