//! Simulating the fixture adders through every sum.

use std::{collections::HashMap, sync::atomic::AtomicBool};

use app::{
    import, interface, lint,
    models::schematic::{Encoding, Orientation},
    orient,
    schem::{self, Blocks, Direction},
    simulate::{
        self, Action, CellControls, Edge, Report, Run, Stacking, Start, Takes,
        Test,
    },
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

/// Its carry out is a bare block, powered by dust run into its side, with
/// nothing standing on it for an output to be read by, so it fails.
#[test]
fn an_output_with_nothing_on_its_block_fails() {
    let report = Test::Adder.run(&blocks("alpha-rca-slice"));
    let Report::Broken { reason } = report else {
        panic!("it did not fail: {report:?}");
    };
    assert!(
        reason.starts_with("Output carry out has a block with no dust"),
        "{reason}"
    );
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

/// A torch inverts its carry in, which is saved with the lever off: it
/// reads as an adder whose carry in is inverted.
#[test]
fn a_carry_in_a_torch_inverts_reads_inverted() {
    let run = adder_run(&blocks("vert-3t-cca")).unwrap();
    assert_eq!(run.wrong, 0);
    assert!(run.pattern.contains("+ not "), "{}", run.pattern);
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

#[test]
fn an_adders_width_is_its_lime_bit_count() {
    let width =
        |name| import::derived_width("adder", &interface::read(&blocks(name)));
    assert_eq!(width("alpha-rca"), Some(8));
    assert_eq!(width("alpha-rca-slice"), Some(1));
    let memory = interface::read(&blocks("alpha-rca"));
    assert_eq!(import::derived_width("ram", &memory), None);
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
    let raw = std::fs::read("tests/fixtures/cca-8-repeated-out.schem").unwrap();
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
    assert_eq!((run.cases, run.wrong), (1 << 17, 0));
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

/// A sequential multiplier is tested by setting its operands, pressing its
/// reset, then its calculate.
#[test]
fn a_sequential_multiplier_is_pressed_to_calculate() {
    let raw = std::fs::read("tests/fixtures/mult-8-buttons.schem").unwrap();
    let file =
        standardize::standardize(&raw, standardize::Gray::Whiten).unwrap();
    let report = Test::Multiplier.run(&Blocks::read(&file).unwrap());
    assert!(report.passed(), "{report:?}");
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
    // A carry in a torch inverts is still an adder's.
    let inverted = simulate::guess(&blocks("vert-3t-cca")).unwrap();
    assert_eq!((inverted.category, inverted.wrong), ("adder", 0));
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
/// where the next copy's carry in lands, with no carry out line; a slice
/// has both its lines, so it fails.
#[test]
fn a_slice_without_a_carry_out_fails() {
    let data = std::fs::read("tests/fixtures/rca-slice-2-high.schem").unwrap();
    let report = Test::Adder.run(&Blocks::read(&data).unwrap());
    let Report::Broken { reason } = report else {
        panic!("it did not fail: {report:?}");
    };
    assert!(reason.ends_with("this has no carry out."), "{reason}");
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
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
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
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
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
    let data = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
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
    assert_eq!(timing.start, Start::Read);
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

/// A 2-bit latch, embizone's upload with its write lever a button: a
/// register with no read, whose output shows what it holds all the time.
/// The register test finds no read and says it may be a buffer; the buffer
/// test stores every value, keeps it as data in changes, and is timed from
/// the button, data passing through in 2 ticks while it is held. A bit
/// turning on settles a tick before one turning off, so it is not synced.
#[test]
fn a_buffer_shows_what_it_holds_without_a_read() {
    let (_, blocks) = register("buffer-2-button");
    assert!(lint::check_for(&blocks, "buffer").is_empty());
    let mut read = interface::read(&blocks);
    interface::name_lines("buffer", &blocks, &mut read);
    let names: Vec<&str> =
        read.inputs.iter().map(|port| port.name.as_str()).collect();
    assert_eq!(names, ["data in", "write"]);
    let Report::Broken { reason } = Test::Register.run(&blocks) else {
        panic!("the buffer passed as a register");
    };
    assert!(reason.contains("may be a buffer"));
    let Report::Ran(run) = Test::Buffer.run(&blocks) else {
        panic!("the buffer was not simulated");
    };
    assert_eq!(run.pattern, "written, then shown");
    assert_eq!((run.cases, run.wrong), (4, 0));
    let timing = Test::Buffer.time(&blocks, &AtomicBool::new(false)).unwrap();
    assert_eq!(timing.start, Start::Write);
    assert_eq!((timing.write, timing.data), (Some(3), Some(2)));
    assert_eq!((timing.latency(), timing.synced), (Some(2), false));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "buffer");
}

/// Copies of the buffer 4 west share its write line and make more bits.
/// Each keeps its own button, its magenta block landing past the copy
/// before, but only the first copy's is pressed, and from a sixth copy on
/// it no longer reaches the last one along the line, so five make 10 bits.
#[test]
fn a_buffer_stacks_west_into_more_bits() {
    let (data, _) = register("buffer-2-button");
    let Some(Stacking::Wider {
        step,
        copies,
        bits,
        most,
        ..
    }) = Test::Buffer.stacking(&data)
    else {
        panic!("the buffer does not stack into more bits");
    };
    assert_eq!(step.describe(), "4 west");
    assert_eq!((copies, bits, most), (5, 10, Some(5)));
}

/// A register's cells show only while read, so it is no buffer.
#[test]
fn a_register_is_no_buffer() {
    let (_, blocks) = register("register-8-2-cells");
    let Report::Broken { reason } = Test::Buffer.run(&blocks) else {
        panic!("the register passed as a buffer");
    };
    assert!(reason.contains("may be a register"));
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
    let data = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
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
    let data = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
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
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
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

/// A 16-bit double density carry-cancel subtractor: a bit per layer, each
/// layer mirroring the one below, so B and the output zig-zag between two
/// columns as they climb, and A between two beside each other. Each bus
/// counts up the way it climbs, its lone control line is its borrow in,
/// every difference comes out right, and the guess finds a subtractor.
#[test]
fn a_double_density_subtractor_takes_b_from_a() {
    let raw = std::fs::read("tests/fixtures/dd-cca-sub-16.schem").unwrap();
    let blocks = import::prepare(&raw, Some(Gray::Whiten), true, false, true)
        .unwrap()
        .blocks;
    assert!(lint::check_for(&blocks, "subtractor").is_empty());
    let read = interface::read(&blocks);
    for port in read.inputs.iter().chain(&read.outputs) {
        let heights: Vec<i32> =
            port.bits.iter().map(|bit| bit.pos[1]).collect();
        assert!(
            heights.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "{} climbs a layer a bit: {heights:?}",
            port.name
        );
    }
    assert_eq!(
        import::derived_flags("subtractor", &read),
        Some(vec!["borrow_in".to_owned()])
    );
    let Report::Ran(run) = Test::Subtractor.run(&blocks) else {
        panic!("the subtractor was not simulated");
    };
    assert_eq!((run.pattern.as_str(), run.wrong), ("A − B − borrow in", 0));
    // A subtractor is an adder whose B is inverted.
    assert_eq!(simulate::guess(&blocks).unwrap().category, "adder");
}

/// However a recognized build is turned or mirrored, an upload lays it
/// back the standard way: A's levers on the south side, and its least
/// significant bits where its test reads them, so it is known for what it
/// is with every quick case right, and the subtractor gets every case
/// right. It never turns a build upside down, so these are each turned
/// about the vertical and mirrored east for west.
#[test]
fn an_upload_turns_and_mirrors_a_build_to_lie_the_standard_way() {
    let fixtures = [
        ("alpha-rca", "adder"),
        ("dd-cca-sub-16", "adder"),
        ("inc-8", "incrementer"),
        ("alu-8-ops-raw", "alu"),
    ];
    let upload = |data: &[u8]| {
        import::prepare(data, Some(Gray::Whiten), true, false, true).unwrap()
    };
    for (name, category) in fixtures {
        let raw =
            std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
        let standard = upload(&raw);
        assert!(
            standard.turn.is_none(),
            "{name} already lies the standard way"
        );
        for mirrored in [false, true] {
            for quarters in 0..4 {
                let turn = orient::Turn { mirrored, quarters };
                let scrambled = orient::turned(&standard.data, turn).unwrap();
                let blocks = upload(&scrambled).blocks;
                let broken: Vec<String> = lint::check(&blocks)
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                assert!(broken.is_empty(), "{name} {turn:?}: {broken:?}");
                let guess = simulate::guess(&blocks).unwrap();
                assert_eq!(
                    (guess.category, guess.wrong),
                    (category, 0),
                    "{name} {turn:?}"
                );
            }
        }
    }
    // A hex adder's dust may be entered from any side, so nothing turns it.
    let raw = std::fs::read("tests/fixtures/hex-cca-8.schem").unwrap();
    assert!(upload(&raw).turn.is_none());
    // Mirrored, its bits would count from the top of the wrong column.
    let raw = std::fs::read("tests/fixtures/dd-cca-sub-16.schem").unwrap();
    let mirrored = orient::Turn {
        mirrored: true,
        quarters: 0,
    };
    let scrambled = orient::turned(&upload(&raw).data, mirrored).unwrap();
    let laid = upload(&scrambled);
    assert_eq!(laid.turn, mirrored);
    assert!(laid.turn.words().starts_with("Mirrored"));
    assert!(Test::Subtractor.run(&laid.blocks).passed());
}

/// A comparison fixture as an upload standardizes it.
fn comparison(name: &str) -> Blocks {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
    import::prepare(&raw, Some(Gray::Whiten), true, false, true)
        .unwrap()
        .blocks
}

/// An 8-bit vertical greater-than comparator: whether A is greater than B,
/// unsigned, on one unsigned output, right in every one of its 65,536
/// cases. Read in two's complement it would be wrong whenever the top bits
/// differ, so it is taken as unsigned.
#[test]
fn a_greater_than_comparator_compares_unsigned() {
    let blocks = comparison("gt-8");
    assert!(lint::check_for(&blocks, "greater_than").is_empty());
    let Report::Ran(run) = Test::GreaterThan.run(&blocks) else {
        panic!("the comparator was not simulated");
    };
    assert_eq!(run.pattern, "A > B");
    assert_eq!((run.cases, run.wrong), (1 << 16, 0));
    let timing = Test::GreaterThan
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((timing.latency(), timing.synced), (Some(2), true));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "greater_than");
}

/// The unsigned comparator with both its sign bits' levers saved on, so
/// each top bit reaches it inverted: in two's complement, that shifts every
/// value up by 128, keeping their order, so it compares signed, and right.
#[test]
fn a_comparator_with_its_sign_bits_inverted_compares_signed() {
    let blocks = comparison("gt-8-signed");
    let Report::Ran(run) = Test::GreaterThan.run(&blocks) else {
        panic!("the comparator was not simulated");
    };
    assert_eq!(run.pattern, simulate::SIGNED_GREATER_THAN);
    assert_eq!((run.cases, run.wrong), (1 << 16, 0));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "greater_than");
}

/// An 8-bit equality comparator whose bits' differences climb one dust
/// column to its output, a pink line: bit 0's leaves its comparator at
/// 14 and dies 14 dust later, so A and B differing only in bit 0 read as
/// equal, 256 of its 65,536 cases. It is still guessed for what it is.
#[test]
fn an_equality_comparator_whose_lowest_bit_dies_out_fails() {
    let blocks = comparison("eq-8-weak-bit-0");
    assert!(lint::check_for(&blocks, "equality").is_empty());
    let Report::Ran(run) = Test::Equality.run(&blocks) else {
        panic!("the comparator was not simulated");
    };
    assert_eq!(run.pattern, "A = B");
    assert_eq!((run.cases, run.wrong), (1 << 16, 256));
    assert!(
        run.examples
            .iter()
            .all(|case| case.inputs[0] ^ case.inputs[1] == 1)
    );
    assert_eq!(simulate::guess(&blocks).unwrap().category, "equality");
}

/// A 1-wide monostable whose output rests on: A's lever turns one torch
/// off at once and, three torches later, another on, the output on while
/// either is, so it goes off for 3 ticks from tick 1 as A turns on, and
/// stays on as A turns off. Copies 2 west each pulse alone, any number of
/// them.
#[test]
fn an_inverted_monostable_gives_a_3_tick_pulse_and_stacks() {
    // Standardized as a register is, its quartz whitened and its lever
    // turned to face south.
    let (data, blocks) = register("monostable-3-inverted");
    assert!(lint::check_for(&blocks, "monostable").is_empty());
    let Report::Ran(run) = Test::Monostable.run(&blocks) else {
        panic!("the monostable was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (2, 0));
    assert_eq!(run.pattern, "A on gives a 3-tick low pulse, A off none");
    let timing = Test::Monostable
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    let pulse = timing.pulse.unwrap();
    assert_eq!((pulse.ticks, pulse.inverted), (3, true));
    assert_eq!((timing.latency(), timing.synced), (Some(1), true));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "monostable");
    let stacking = Test::Monostable.stacking(&data);
    let Some(Stacking::Wider {
        step, copies, most, ..
    }) = stacking
    else {
        panic!("the monostable does not stack: {stacking:?}");
    };
    assert_eq!(step.describe(), "2 west");
    assert_eq!((copies, most), (16, None));
}

/// The inverted monostable with its output line turned into a torch,
/// which lights a yellow output: it rests off and goes on for 3 ticks from
/// tick 2.
#[test]
fn a_monostable_whose_output_rests_off_gives_a_high_pulse() {
    let (_, blocks) = register("monostable-3");
    assert!(lint::check_for(&blocks, "monostable").is_empty());
    assert!(Test::Monostable.run(&blocks).passed());
    let timing = Test::Monostable
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    let pulse = timing.pulse.unwrap();
    assert_eq!((pulse.ticks, pulse.inverted), (3, false));
    assert_eq!(timing.latency(), Some(2));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "monostable");
}

/// An 8-bit adder has two operands, so it is no monostable.
#[test]
fn an_adder_is_no_monostable() {
    let Report::Broken { reason } = Test::Monostable.run(&blocks("alpha-rca"))
    else {
        panic!("the adder was not refused as a monostable");
    };
    assert!(reason.contains("one input"));
}

/// A splits into a fast path and one 3 ticks slower, and a comparator
/// subtracting the slow from the fast pulses as A turns on, another the
/// fast from the slow as it turns off, both onto one yellow output: a
/// 3-tick high pulse on either edge, so no single-edge monostable.
#[test]
fn a_dual_edge_monostable_pulses_both_ways() {
    let (_, blocks) = register("monostable-3-dual-edge");
    assert!(lint::check_for(&blocks, "dual_edge_monostable").is_empty());
    let Report::Ran(run) = Test::DualEdgeMonostable.run(&blocks) else {
        panic!("the monostable was not simulated");
    };
    assert_eq!(run.wrong, 0);
    assert_eq!(run.pattern, "A on or off gives a 3-tick high pulse");
    let timing = Test::DualEdgeMonostable
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    let pulse = timing.pulse.unwrap();
    assert_eq!((pulse.ticks, pulse.inverted), (3, false));
    assert_eq!(pulse.edge, Edge::Both);
    assert!(Test::Monostable.run(&blocks).failed());
    assert_eq!(
        simulate::guess(&blocks).unwrap().category,
        "dual_edge_monostable"
    );
}

/// The dual-edge monostable without the comparator that pulses as A
/// turns on: a 3-tick high pulse as A turns off alone.
#[test]
fn a_falling_edge_monostable_pulses_as_a_turns_off() {
    let (_, blocks) = register("monostable-3-falling");
    let Report::Ran(run) = Test::Monostable.run(&blocks) else {
        panic!("the monostable was not simulated");
    };
    assert_eq!(run.wrong, 0);
    assert_eq!(run.pattern, "A off gives a 3-tick high pulse, A on none");
    let timing = Test::Monostable
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    let pulse = timing.pulse.unwrap();
    assert_eq!((pulse.ticks, pulse.edge), (3, Edge::Falling));
    assert!(Test::DualEdgeMonostable.run(&blocks).failed());
    assert_eq!(simulate::guess(&blocks).unwrap().category, "monostable");
}

/// A 1-bit inverter: two torches give A and not the line, and not A and
/// the line, onto one yellow output, so its unsigned magenta line inverts
/// A. Copies 2 west share one line, which inverts every bit, up to 8 of
/// them: the line's inverse runs on through every copy's dust, and from a
/// ninth copy on, too far from the torch driving it, it dies out, so that
/// copy inverts with the line off.
#[test]
fn an_inverter_inverts_a_while_its_line_is_on_and_stacks() {
    let (data, blocks) = register("inverter-1");
    assert!(lint::check_for(&blocks, "inverter").is_empty());
    let Report::Ran(run) = Test::Inverter.run(&blocks) else {
        panic!("the inverter was not simulated");
    };
    assert_eq!(run.pattern, "A xor invert");
    assert_eq!((run.cases, run.wrong), (4, 0));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "inverter");
    let stacking = Test::Inverter.stacking(&data);
    let Some(Stacking::Wider {
        step, copies, most, ..
    }) = stacking
    else {
        panic!("the inverter does not stack: {stacking:?}");
    };
    assert_eq!(step.describe(), "2 west");
    assert_eq!((copies, most), (8, Some(8)));
}

/// Two staggered cells of an 8-bit register, packed so close that the west
/// cell's torches face into the east cell's dust: one keeps bits 1, 3, and
/// 5 of the east cell from being written, and another, lit as the west
/// cell writes a 1 into an odd bit, clears the even bit below it in the
/// east cell. Each fault comes with the writes and reads that show it.
#[test]
fn a_register_whose_cells_disturb_each_other_shows_how() {
    let (_, blocks) = register("register-8-2-cells-crosstalk");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert!(run.wrong > 0);
    assert_eq!(
        run.cells,
        [
            CellControls {
                write: [4, 2, 19],
                read: Some([3, 1, 6]),
            },
            CellControls {
                write: [1, 1, 19],
                read: Some([0, 0, 6]),
            },
        ]
    );
    let write = |cell, value| Action::Write { cell, value };
    let read = |cell, expected, got| Action::Read {
        cell,
        bus: 0,
        expected,
        got,
    };
    assert_eq!(
        run.faults,
        [
            vec![write(0, 255), read(0, 255, 0b1101_0101)],
            vec![
                write(0, 255),
                read(0, 255, 0b1101_0101),
                write(1, 0b10),
                read(0, 255, 0b1101_0100),
            ],
        ]
    );
}

/// `data` with every `from` block made `to`, its state otherwise kept.
fn swapped(data: &[u8], from: &str, to: &str) -> Vec<u8> {
    let blocks = Blocks::read(data).unwrap();
    let grid: HashMap<[i32; 3], String> = blocks
        .iter()
        .map(|(pos, state)| (pos, state.replace(from, to)))
        .collect();
    let mut nbt = schem::decode(data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    schem::encode(nbt).unwrap()
}

/// A glass output is never powered, so it is read by the repeater on it,
/// and the adder adds and is timed as it is on concrete.
#[test]
fn a_glass_output_is_read_from_the_repeater_on_it() {
    let raw = std::fs::read("tests/fixtures/cca-8-repeated-out.schem").unwrap();
    let tidied = standardize::tidy(&raw).unwrap();
    let stored = standardize::pink_status(&tidied).unwrap();
    let glazed = swapped(
        &stored,
        "minecraft:yellow_concrete",
        "minecraft:yellow_stained_glass",
    );
    let concrete = Blocks::read(&stored).unwrap();
    let glass = Blocks::read(&glazed).unwrap();
    assert!(
        glass
            .iter()
            .any(|(_, state)| state == "minecraft:yellow_stained_glass")
    );
    let on_concrete = adder_run(&concrete).unwrap();
    let on_glass = adder_run(&glass).unwrap();
    assert_eq!(
        (on_glass.cases, on_glass.wrong),
        (on_concrete.cases, on_concrete.wrong)
    );
    let timing = Test::Adder.time(&glass, &AtomicBool::new(false)).unwrap();
    assert_eq!(timing.latency(), Some(3));
}

/// One bit of A and of B into one output bit, with no carry in or out, is
/// A xor B: an XOR gate, not an adder, which at one bit is a slice of a
/// wider one and has both lines.
#[test]
fn a_one_bit_adder_without_its_lines_is_an_xor_gate() {
    let raw = std::fs::read("tests/fixtures/xor-1.schem").unwrap();
    let prepared =
        import::prepare(&raw, Some(Gray::Keep), true, false, true).unwrap();
    let report = Test::Adder.run(&prepared.blocks);
    let Report::Broken { reason } = report else {
        panic!("it passed for an adder: {report:?}");
    };
    assert!(reason.ends_with("it is an XOR gate."), "{reason}");
    let guess = simulate::guess(&prepared.blocks).unwrap();
    assert_eq!(
        (guess.category, guess.pattern.as_str(), guess.wrong),
        ("xor_xnor_gate", "A xor B", 0)
    );
}

/// A gate has no lines, so copies side by side each add a bit to A, B, and
/// the output; this one's copies fit four blocks apart.
#[test]
fn an_xor_gate_stacks_west_into_wider_ones() {
    let raw = std::fs::read("tests/fixtures/xor-1.schem").unwrap();
    let prepared =
        import::prepare(&raw, Some(Gray::Keep), true, false, true).unwrap();
    let Some(Stacking::Wider { step, copies, .. }) =
        Test::XorXnor.stacking(&prepared.data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), copies), ("4 west", 16));
}

/// A ripple adder holds nothing; a sequential multiplier holds its partial
/// products in loops.
#[test]
fn memory_is_found_in_loops_but_not_in_combinational_logic() {
    let adder = simulate::compiled(&blocks("alpha-rca")).unwrap();
    assert!(adder.memory().is_empty());
    let raw = std::fs::read("tests/fixtures/mult-8-buttons.schem").unwrap();
    let file = standardize::standardize(&raw, Gray::Whiten).unwrap();
    let multiplier = simulate::compiled(&Blocks::read(&file).unwrap()).unwrap();
    assert!(!multiplier.memory().is_empty());
}

/// Each bit's XOR is two comparators in compare mode on one back line,
/// each reading the other from its side: one can put out no more than the
/// other's back, so neither decides the other, and the pair holds nothing.
#[test]
fn compare_mode_comparators_sharing_a_back_hold_nothing() {
    let adder = simulate::compiled(&blocks("xor-pair-adder-8")).unwrap();
    assert!(adder.memory().is_empty());
}

/// A comparator in subtract mode reading a barrel of strength 1 puts out
/// no more than 1, which its own side, a step away, never gets.
#[test]
fn a_loop_that_loses_all_it_could_carry_holds_nothing() {
    let adder = simulate::compiled(&blocks("barrel-leak-adder")).unwrap();
    assert!(adder.kinds.values().any(|k| k == "Constant { ss: 1 }"));
    assert!(adder.memory().is_empty());
}
