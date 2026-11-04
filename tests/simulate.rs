//! Simulating the fixture adders through every sum.

use std::{collections::HashMap, sync::atomic::AtomicBool};

use app::{
    category::Kind,
    import,
    models::schematic::{Encoding, Orientation},
    recognize::interface,
    schem::{self, Blocks, orient, standardize},
    simulate::{
        self, Action, CellControls, Edge, Report, Run, Stacking, Start, Step,
        Takes, Test,
    },
};

/// The fixture file at `path`, carrying the roles its colours mark, as the
/// fixtures are saved in the site's old colours; see
/// `legacy::colour_roles`.
fn read_fixture(path: &str) -> std::io::Result<Vec<u8>> {
    let raw = std::fs::read(path)?;
    Ok(app::recognize::legacy::with_colour_roles(&raw)
        .expect("fixture carries its roles"))
}

/// A fixture as the site stores it: markers out, box fitted, and status
/// lines pink, since the fixtures predate that.
fn blocks(name: &str) -> Blocks {
    let raw = read_fixture(&format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    let stored = app::recognize::legacy::with_colour_roles(&tidied)
        .expect("fixture carries its roles");
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

/// Its carry out as first built is a bare block, powered by dust run into
/// its side: read by the old rule, by its colour, it has nothing standing
/// on it for an output to be read by, so it fails; by the rule now, which
/// reads only a repeater or comparator facing air, it is no output at all,
/// nor is its sum's dust.
#[test]
fn an_output_with_nothing_on_its_block_fails() {
    let data =
        read_fixture("tests/fixtures/old/alpha-rca-slice.schem").unwrap();
    let old = Blocks::read(&data).unwrap();
    let report = app::recognize::legacy::reading(|| Test::Adder.run(&old));
    let Report::Broken { reason } = report else {
        panic!("it did not fail: {report:?}");
    };
    assert!(
        reason.starts_with("Output carry out has a block with no dust"),
        "{reason}"
    );
    assert!(app::recognize::inspect::inspect(&old).outputs.is_empty());
}

/// The game agrees: mc-tick, a vanilla-accurate engine, lights bit 7 for
/// 95 + 1 too.
#[test]
fn a_carry_lookahead_adder_carries_into_bit_7_wrongly() {
    let run = adder_run(&blocks("alpha-cla")).unwrap();
    assert_eq!(run.wrong, 8192);
    let first = &run.examples[0];
    // Its carry in reads inverted: off, it adds one.
    assert_eq!(first.inputs, [95, 0, 0]);
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
    let raw = read_fixture("tests/fixtures/diag-3t-cca.schem").unwrap();
    // As the site stores it, with its carry out pink.
    let stored = app::recognize::legacy::with_colour_roles(
        &standardize::tidy(&raw).unwrap(),
    )
    .unwrap();
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
    let raw = read_fixture("tests/fixtures/cca-8-repeated-out.schem").unwrap();
    let tidied = standardize::tidy(&raw).unwrap();
    let stored = app::recognize::legacy::with_colour_roles(&tidied).unwrap();
    let blocks = Blocks::read(&unsigned(&stored)).unwrap();
    assert!(blocks.signs.is_empty());
    let run = adder_run(&blocks).unwrap();
    assert_eq!((run.cases, run.wrong), (1 << 17, 0));
    // Its carry in adds one with its lever off.
    assert_eq!(run.pattern, "A + B + not carry in");
}

/// An 8-bit adder that repeats every 16 blocks up, its carry in and the
/// gray logic under its least significant bit inside the slice: a stack is
/// the slice's box, unchanged, so every copy would keep them, and it does
/// not stack.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_adder_with_its_carry_in_inside_its_box_does_not_stack() {
    let data = read_fixture("tests/fixtures/vert-adder-8-stacks.schem")
        .expect("fixture is readable");
    assert!(Test::Adder.run(&Blocks::read(&data).unwrap()).passed());
    assert!(matches!(
        Test::Adder.stacking(&data),
        Some(Stacking::Not { .. })
    ));
}

/// A 1-bit ripple carry slice with its carry in's lever on its east face,
/// outside the box the copies stack: each later copy's carry in block
/// lands where the copy before's carry leaves, and nine copies 2 west make
/// a 9-bit adder, every sum right.
#[test]
fn an_adder_stacks_by_its_box() {
    let raw = read_fixture("tests/fixtures/rca-1-carry-in-east.schem").unwrap();
    let data = app::recognize::probe::simulatable(&raw).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let report = app::recognize::inspect::inspect(&blocks);
    let probed = app::recognize::probe::adder(&data, &blocks, &report).unwrap();
    let coloured =
        app::recognize::probe::with_layout(&data, &probed.layout).unwrap();
    let standard = import::prepare_for(
        &coloured,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap();
    let Some(Stacking::Stacks {
        step, copies, bits, ..
    }) = Test::Adder.stacking(&standard.data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), copies, bits), ("2 west", 9, 9));
}

/// An 8-bit adder stacking 16 up whose carry in and a barrel only the
/// first copy needs sit on its north face: the box rule's box keeps the
/// barrel in every copy, which then all add one, so the box leaving out
/// the two north layers too is the one whose copies add right.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_adder_stacks_by_a_box_found_by_testing() {
    let raw =
        read_fixture("tests/fixtures/vert-adder-8-barrel-north.schem").unwrap();
    let data = app::recognize::probe::simulatable(&raw).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let report = app::recognize::inspect::inspect(&blocks);
    let probed = app::recognize::probe::adder(&data, &blocks, &report).unwrap();
    let coloured =
        app::recognize::probe::with_layout(&data, &probed.layout).unwrap();
    let standard = import::prepare_for(
        &coloured,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap();
    let Some(Stacking::Stacks { step, bits, .. }) =
        Test::Adder.stacking(&standard.data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), bits), ("16 up", 24));
    let faces = step.faces.expect("a box found by testing");
    assert_eq!((faces.low, faces.high), ([0, 2, 2], [0, 0, 0]));
}

/// Its outputs are read through the repeaters standing on them, which lead
/// the product out of the build, since nothing powers the yellow blocks.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_combinational_multiplier_tries_every_product() {
    let raw = read_fixture("tests/fixtures/mult-8-diag-signed.schem")
        .expect("fixture is readable");
    let data = standardize::standardize(&raw).unwrap();
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

/// A sequential multiplier is tested by setting its operands, then
/// pressing its calculate.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_sequential_multiplier_is_pressed_to_calculate() {
    let report = Test::Multiplier.run(&blocks("mult-8-calculate"));
    assert!(report.passed(), "{report:?}");
}

/// Its copies fit, but a carry out of one never reaches the next, which
/// the sums across the seam find before the full test.
#[test]
fn an_adder_whose_carry_stops_at_its_edge_does_not_stack() {
    let raw = read_fixture("tests/fixtures/alpha-rca.schem").unwrap();
    let data = app::recognize::legacy::with_colour_roles(
        &standardize::tidy(&raw).unwrap(),
    )
    .unwrap();
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

/// A carry cancel adder of 8 hex digits: each input block's dust takes a
/// strength from 0 to 15, so A and B are 32 bits each, too many to try
/// every sum, and it is slower to run than one of levers. Each digit takes
/// its every case, with a carry into it and without, and the patterns.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_hex_adder_is_driven_digit_by_digit() {
    let raw = read_fixture("tests/fixtures/hex-cca-8.schem").unwrap();
    let data = standardize::standardize(&raw).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let interface = interface::read(&blocks);
    let derived = import::derived("adder", &interface);
    assert_eq!(
        (derived.width, derived.orientation, derived.encoding),
        (Some(32), Some(Orientation::Vertical), Some(Encoding::Hex))
    );
    // Its inputs taking hex digits, it's taken for a hex adder.
    assert_eq!(simulate::guess(&blocks).unwrap().category, "hex_adder");
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
}

/// A 1-bit ripple carry slice two blocks high whose carry leaves by dust,
/// where the next copy's carry in lands, with no carry out line; a carry
/// out is optional, as a wider adder's is, so it adds.
#[test]
fn a_slice_without_a_carry_out_adds() {
    let data = read_fixture("tests/fixtures/rca-slice-2-high.schem").unwrap();
    let report = Test::Adder.run(&Blocks::read(&data).unwrap());
    let Report::Ran(run) = report else {
        panic!("it did not run: {report:?}");
    };
    assert_eq!(run.wrong, 0);
    assert!(run.outputs.iter().all(|signal| signal.name != "carry out"));
}

/// A 1-bit ripple carry slice two blocks high whose carry out is capped by
/// a block that the copy above replaces, as `//rstack` pastes over it; its
/// carry in shares its bottom layer with its logic, so the box rule keeps
/// it in every copy, and the box found by testing leaves that layer to the
/// first copy.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_copy_may_cover_the_copy_below() {
    let data = read_fixture("tests/fixtures/rca-slice-capped.schem").unwrap();
    assert!(Test::Adder.run(&Blocks::read(&data).unwrap()).passed());
    let Some(Stacking::Stacks { step, .. }) = Test::Adder.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!(step.describe(), "2 up");
    assert_eq!(step.faces.map(|faces| faces.low), Some([0, 1, 0]));
}

/// Nothing powers its yellow output blocks: the repeaters standing on
/// them carry the sum out of the build, a tick later, which its latency
/// leaves out, as the repeater tapping its carry out does.
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
    assert_eq!(outputs, [("output", 3, 1), ("carry out", 5, 1)]);
    assert_eq!(timing.latency(), Some(3));
}

/// An adder times the same whichever operand is named A: its latency, and
/// the tick each line may switch by, are from t = 0, as the inputs each
/// output waits longest on switch, not from A's switching. The port of an
/// 8-bit carry-cancel adder adds through its x = 0 bus a tick slower than
/// through its x = 3 one, so it read 6 ticks named one way and 5 the
/// other; an 8-bit adder whose carry in is a tick slower than both its
/// operands read a tick fast, from A.
#[test]
fn an_adders_timing_does_not_depend_on_which_operand_is_a() {
    use app::schem::lines::Role;
    for (name, latency) in [("cca-8-b-slower", 6), ("xor-pair-adder-8", 5)] {
        // Its roles as probing finds them.
        let probed = || {
            let raw =
                std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
            Blocks::read(&import::with_roles(&raw, "adder").unwrap()).unwrap()
        };
        let (named, mut swapped) = (probed(), probed());
        let roles = swapped.roles.as_mut().expect("its roles");
        for role in roles.0.values_mut() {
            *role = match *role {
                Role::Operand(0) => Role::Operand(1),
                Role::Operand(1) => Role::Operand(0),
                other => other,
            };
        }
        // Its latency and each line's tick, an operand by where its first
        // bit is rather than its name.
        let timed = |blocks: &Blocks| {
            let timing =
                Test::Adder.time(blocks, &AtomicBool::new(false)).unwrap();
            let interface = interface::read(blocks);
            let mut ticks: Vec<(String, Option<i32>)> = timing
                .inputs
                .iter()
                .map(|input| {
                    let at = interface
                        .inputs
                        .iter()
                        .find(|port| port.name == input.name)
                        .map_or(input.name.clone(), |port| {
                            format!("{:?}", port.bits[0].pos)
                        });
                    (at, input.tick)
                })
                .collect();
            ticks.sort();
            (timing.latency(), ticks)
        };
        let found = timed(&named);
        assert_eq!(found.0, Some(latency), "{name}");
        assert_eq!(timed(&swapped), found, "{name}");
    }
}

/// Its sum leaves by dust on each yellow block. For 2 + 2 + 1, its carry in
/// reading inverted, the carry reaches bit 2 at strength 1, which powers
/// the block beside it but leaves the dust on top dark, so the bit is off,
/// as in-game.
#[test]
fn an_output_is_read_from_the_dust_on_its_block() {
    let Report::Ran(run) = Test::Adder.run(&blocks("cca-8-dust-out")) else {
        panic!("the adder was not simulated");
    };
    assert!(run.wrong > 0);
    let wrong = &run.examples[0];
    assert_eq!(
        (wrong.inputs.as_slice(), wrong.got[0], wrong.expected[0]),
        ([2, 2, 0].as_slice(), 1, 5)
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

/// Saved with bit 2 of A's lever on by mistake: how a lever was saved says
/// nothing, so every case is right.
#[test]
fn a_lever_saved_on_by_mistake_changes_nothing() {
    let report = Test::BcdToBinary.run(&blocks("bcd2bin-10-lever-on"));
    let Report::Ran(run) = &report else {
        panic!("the converter was not simulated: {report:?}");
    };
    assert_eq!(run.wrong, 0);
    assert!(run.inverted.is_empty(), "{:?}", run.inverted);
}

/// From Schemati: eleven control lines, seven saved on, each sign naming
/// the operations its line takes part in, its status lines signed carry
/// out, overflow, and lsb shift out. Every result and status comes out
/// right, its carry out the adder's for add and subtract alike.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_alu_runs_each_operation_its_signs_name() {
    let raw = read_fixture("tests/fixtures/alu-8-ops-raw.schem").unwrap();
    let blocks = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
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
#[ignore = "measures whole builds: just test-full"]
fn an_alus_inverted_carry_out_is_wrong() {
    let raw =
        read_fixture("tests/fixtures/alu-8-ops-inverted-carry.schem").unwrap();
    let blocks = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
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
    let raw = read_fixture(&format!("tests/fixtures/{name}.schem")).unwrap();
    let data = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap()
    .data;
    let blocks = Blocks::read(&data).unwrap();
    (data, blocks)
}

/// Two staggered cells of an 8-bit register, each a write button and a
/// read lever on unsigned magenta lines, sharing lime data in and yellow
/// data out. Each cell takes every value, the bus stays empty with every
/// read off, and each read shows its cell alone; a write shows 8 ticks
/// after its button. Timed from its data changing, its write pressed and
/// read turned on where they let the data pass soonest, data out settles
/// at 4, its latency, and each write has a window to be pressed in.
#[test]
fn a_register_stores_and_reads_each_cell() {
    let (_, blocks) = register("register-8-2-cells");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!(run.pattern, "each of 2 cells written, then read");
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert!(run.exhaustive);
    let timing = Test::Register
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((timing.read(), timing.synced), (Some(4), true));
    // A write shows 8 ticks after its button, but its latency is data in
    // to data out, its controls switched in time: 4.
    assert_eq!(timing.write, Some(8));
    assert_eq!((timing.data, timing.latency()), (Some(4), Some(4)));
    assert_eq!(timing.start, Start::Input);
    let names: Vec<&str> = timing
        .inputs
        .iter()
        .map(|input| input.name.as_str())
        .collect();
    assert_eq!(names, ["data in", "write 1", "read 1", "write 2", "read 2"]);
    assert_eq!(timing.inputs[0].tick, Some(0));
    for write in timing.inputs.iter().filter(|i| i.name.starts_with("write")) {
        let (Some(earliest), Some(latest)) = (write.earliest, write.tick)
        else {
            panic!("{} has no window", write.name);
        };
        assert!(earliest < latest, "{}: {earliest} to {latest}", write.name);
    }
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
/// on its input, stands inside the box the box rule finds, so every such
/// copy would keep it; the box found by testing leaves the driver's layer,
/// on the face with its write lever, to the first copy, and copies 2 west
/// make a wider register, as many as its lines reach.
#[test]
fn a_register_cell_with_its_driver_inside_its_box_stacks_without_it() {
    let data =
        read_fixture("tests/fixtures/register-1-gray-driver.schem").unwrap();
    let Some(Stacking::Wider { step, most, .. }) =
        Test::Register.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), most), ("2 west", Some(7)));
    assert_eq!(step.faces.map(|faces| faces.high), Some([1, 0, 0]));
}

/// The same cell's timing: its bus is empty while its write lever is on,
/// and it takes its data as the lever turns off, showing it 3 ticks later.
/// Data changing as the lever turns off shows 4 ticks later, its latency.
#[test]
fn a_register_cell_that_takes_its_data_as_its_write_closes_is_timed_from_it() {
    let data =
        read_fixture("tests/fixtures/register-1-gray-driver.schem").unwrap();
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
    let data = read_fixture("tests/fixtures/register-1-button.schem").unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let timing = Test::Register
        .time(&blocks, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(timing.takes, Takes::Closing);
    assert_eq!((timing.write, timing.data), (Some(3), Some(4)));
    assert_eq!(timing.latency(), Some(4));
}

/// The same cell standardized, its driver's block whitened, stacks the
/// same way: gray marks nothing, the box found by testing does.
#[test]
fn a_register_cell_whose_driver_is_white_stacks_the_same() {
    let (data, _) = register("register-1-gray-driver");
    let Some(Stacking::Wider { step, .. }) = Test::Register.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!(step.describe(), "2 west");
    assert!(step.faces.is_some());
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
    let mut read = interface::read(&blocks);
    interface::name_lines(Kind::Buffer, &blocks, &mut read);
    let names: Vec<&str> =
        read.inputs.iter().map(|port| port.name.as_str()).collect();
    assert_eq!(names, ["data in", "write"]);
    let Report::Broken { reason } = Test::Register.run(&blocks) else {
        panic!("the buffer passed as a register");
    };
    // Its lines are said: a write button, and no lever to read.
    assert!(reason.contains("found 1 button, no levers"), "{reason}");
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
/// Its button, alone on its east face, is the first copy's, and from a
/// fourth copy on the write no longer reaches the last one along the line,
/// so three make 6 bits.
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
    assert_eq!((copies, bits, most), (3, 6, Some(3)));
}

/// A register's cells show only while read, so it is no buffer.
#[test]
fn a_register_is_no_buffer() {
    let (_, blocks) = register("register-8-2-cells");
    let Report::Broken { reason } = Test::Buffer.run(&blocks) else {
        panic!("the register passed as a buffer");
    };
    assert!(reason.contains("expected only writes"), "{reason}");
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

/// Its data levers saved on: how a lever was saved says nothing, so each
/// cell takes what the test writes and its reads show it, plainly.
#[test]
fn a_register_whose_data_levers_are_saved_on_reads_plainly() {
    let blocks = with_levers_on("lime");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert_eq!(run.pattern, "each of 2 cells written, then read");
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
/// whose data levers share a column with its logic, so they sit inside the
/// box every copy keeps, landing on the copy before's data dust: copies 4
/// west make no register file.
#[test]
fn a_register_whose_data_levers_are_inside_its_box_does_not_stack() {
    let (data, blocks) = register("register-8-hoppers");
    let Report::Ran(run) = Test::Register.run(&blocks) else {
        panic!("the register was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (3 * 256, 0));
    assert!(matches!(
        Test::Register.stacking(&data),
        Some(Stacking::Not { .. })
    ));
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
    let raw = read_fixture("tests/fixtures/abs-8.schem").unwrap();
    let data = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
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

/// An 8-bit parity circuit: on when an odd number of A's bits are, in
/// every one of its 256 cases, timed by its one output.
#[test]
fn a_parity_circuit_gives_whether_an_odd_number_of_bits_are_on() {
    let raw = read_fixture("tests/fixtures/parity-8.schem").unwrap();
    let data = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap()
    .data;
    let blocks = Blocks::read(&data).unwrap();
    let Report::Ran(run) = Test::Parity.run(&blocks) else {
        panic!("the parity circuit was not simulated");
    };
    assert_eq!((run.cases, run.wrong), (256, 0), "{:?}", run.examples);
    assert_eq!(run.pattern, "xor of A's bits");
    assert!(run.inverted.is_empty());
    assert_eq!(simulate::guess(&blocks).unwrap().category, "parity");
    let timing = Test::Parity.time(&blocks, &AtomicBool::new(false)).unwrap();
    assert!(timing.latency().is_some());
}

/// An 8-bit carry out: of A + B, only the carry, on one unsigned status
/// line, in every one of its 65,536 cases, timed by that line, as it has no
/// other output.
#[test]
fn a_carry_out_gives_the_carry_of_every_sum() {
    let raw = read_fixture("tests/fixtures/cout-8.schem").unwrap();
    let data = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap()
    .data;
    let blocks = Blocks::read(&data).unwrap();
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
    let raw = read_fixture("tests/fixtures/adder-8-lsb-top.schem").unwrap();
    let blocks = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
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
    let raw = read_fixture("tests/fixtures/dd-cca-sub-16.schem").unwrap();
    let blocks = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap()
    .blocks;
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
        import::derived_flags("adder", &read),
        Some(vec!["carry_in".to_owned()])
    );
    // A subtractor is an adder whose B is inverted.
    let Report::Ran(run) = Test::Adder.run(&blocks) else {
        panic!("the subtractor was not simulated");
    };
    assert_eq!((run.pattern.as_str(), run.wrong), ("A − B − borrow in", 0));
    // An adder whose B is subtracted is taken for a subtractor.
    assert_eq!(simulate::guess(&blocks).unwrap().category, "subtractor");
}

/// However a recognized build is turned or mirrored, an upload lays it
/// back the standard way: A's levers on the south side, and its least
/// significant bits where its test reads them, so it is known for what it
/// is with every quick case right, and the subtractor gets every case
/// right. It never turns a build upside down, so these are each turned
/// about the vertical and mirrored east for west.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_upload_turns_and_mirrors_a_build_to_lie_the_standard_way() {
    let fixtures = [
        ("alpha-rca", "adder"),
        ("dd-cca-sub-16", "subtractor"),
        ("inc-8", "incrementer"),
        ("alu-8-ops-raw", "alu"),
    ];
    let upload = |data: &[u8]| {
        import::prepare_for(
            data,
            import::Purpose::Standard { keep_paste: false },
        )
        .unwrap()
    };
    for (name, category) in fixtures {
        let raw =
            read_fixture(&format!("tests/fixtures/{name}.schem")).unwrap();
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
    let raw = read_fixture("tests/fixtures/hex-cca-8.schem").unwrap();
    assert!(upload(&raw).turn.is_none());
    // Mirrored, its bits would count from the top of the wrong column.
    let raw = read_fixture("tests/fixtures/dd-cca-sub-16.schem").unwrap();
    let mirrored = orient::Turn {
        mirrored: true,
        quarters: 0,
    };
    let scrambled = orient::turned(&upload(&raw).data, mirrored).unwrap();
    let laid = upload(&scrambled);
    assert_eq!(laid.turn, mirrored);
    assert!(laid.turn.words().starts_with("Mirrored"));
    assert!(Test::Adder.run(&laid.blocks).passed());
}

/// A comparison fixture as an upload standardizes it.
fn comparison(name: &str) -> Blocks {
    let raw = read_fixture(&format!("tests/fixtures/{name}.schem")).unwrap();
    import::prepare_for(&raw, import::Purpose::Standard { keep_paste: false })
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

/// The unsigned comparator with both its sign bits' levers saved on. How a
/// lever was saved says nothing, so it compares unsigned: signed, its sign
/// bits would need inverting in the circuit.
#[test]
fn a_comparator_signed_only_by_its_levers_compares_unsigned() {
    let blocks = comparison("gt-8-signed");
    let Report::Ran(run) = Test::GreaterThan.run(&blocks) else {
        panic!("the comparator was not simulated");
    };
    assert_eq!(run.pattern, "A > B");
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
/// stays on as A turns off. Copies 1 west interfere, but 2 west, a block
/// of air between them, cannot meet, so they stack untested, pitch 2.
#[test]
fn an_inverted_monostable_gives_a_3_tick_pulse_and_stacks() {
    // Standardized as a register is, its quartz whitened and its lever
    // turned to face south.
    let (data, blocks) = register("monostable-3-inverted");
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
    assert_eq!((copies, most), (9, None));
}

/// The inverted monostable with its output line turned into a torch,
/// which lights a yellow output: it rests off and goes on for 3 ticks from
/// tick 2.
#[test]
fn a_monostable_whose_output_rests_off_gives_a_high_pulse() {
    let (_, blocks) = register("monostable-3");
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
/// A. Its line sits inside the box the box rule finds, so those copies 2
/// west would each have their own; a box leaving the east layer, with the
/// line, to the first copy makes a wider inverter.
#[test]
fn an_inverter_inverts_a_while_its_line_is_on_and_stacks() {
    let (data, blocks) = register("inverter-1");
    let Report::Ran(run) = Test::Inverter.run(&blocks) else {
        panic!("the inverter was not simulated");
    };
    assert_eq!(run.pattern, "A xor invert");
    assert_eq!((run.cases, run.wrong), (4, 0));
    assert_eq!(simulate::guess(&blocks).unwrap().category, "inverter");
    let Some(Stacking::Wider { step, .. }) = Test::Inverter.stacking(&data)
    else {
        panic!("it does not stack");
    };
    assert_eq!(step.describe(), "2 west");
    assert_eq!(step.faces.map(|faces| faces.high), Some([1, 0, 0]));
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
                write: [4, 2, 20],
                read: Some([3, 1, 7]),
            },
            CellControls {
                write: [1, 1, 20],
                read: Some([0, 0, 7]),
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
#[ignore = "measures whole builds: just test-full"]
fn a_glass_output_is_read_from_the_repeater_on_it() {
    let raw = read_fixture("tests/fixtures/cca-8-repeated-out.schem").unwrap();
    let tidied = standardize::tidy(&raw).unwrap();
    let stored = app::recognize::legacy::with_colour_roles(&tidied).unwrap();
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
    let raw = read_fixture("tests/fixtures/xor-1.schem").unwrap();
    let prepared = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap();
    let report = Test::Adder.run(&prepared.blocks);
    let Report::Broken { reason } = report else {
        panic!("it passed for an adder: {report:?}");
    };
    assert!(reason.ends_with("it is an XOR gate."), "{reason}");
    let guess = simulate::guess(&prepared.blocks).unwrap();
    assert_eq!(
        (guess.category, guess.pattern.as_str(), guess.wrong),
        ("xor_gate", "A xor B", 0)
    );
}

/// Each gate's test takes its own function alone: an XOR gate gets every
/// case right as one and every case wrong as an XNOR gate, its inverse.
#[test]
fn an_xor_gate_is_no_xnor_gate() {
    let raw = read_fixture("tests/fixtures/xor-1.schem").unwrap();
    let prepared = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap();
    let wrong = |test: Test| match test.run(&prepared.blocks) {
        Report::Ran(run) => (run.pattern, run.wrong, run.cases),
        report => panic!("it did not run: {report:?}"),
    };
    assert_eq!(wrong(Test::Xor), ("A xor B".to_owned(), 0, 4));
    assert_eq!(wrong(Test::Xnor), ("A xnor B".to_owned(), 4, 4));
}

/// A gate has no lines, so copies side by side each add a bit to A, B, and
/// the output; this one, 3 wide, its output read by a repeater standing on
/// its own block, stacks 4 up, pitch 4, 5 copies tested, and 4 west,
/// pitch 4, untested, as copies with air between them cannot meet, where
/// copies 1 or 2 west overlap wrongly.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_xor_gate_stacks_up_into_wider_ones() {
    let raw = read_fixture("tests/fixtures/xor-1.schem").unwrap();
    let prepared = import::prepare_for(
        &raw,
        import::Purpose::Standard { keep_paste: false },
    )
    .unwrap();
    let Some(Stacking::Wider {
        step, cases, also, ..
    }) = Test::Xor.stacking(&prepared.data)
    else {
        panic!("it does not stack");
    };
    assert_eq!((step.describe().as_str(), cases), ("4 west", 0));
    let [up] = also.as_slice() else {
        panic!("it stacks one other way too: {also:?}");
    };
    assert_eq!((up.step.describe().as_str(), up.copies), ("4 up", 5));
    assert!(up.cases > 0);
}

/// Cells that touch are tested: a 1-bit XOR gate 2 wide stacks 2 west,
/// pitch 2, every case of 3 copies right, each a gate of its own beside
/// the next; copies 4 west, a gap of air between them, cannot meet, so
/// they are taken to stack with no cases run, and, of pitch 2, it counts
/// as 4-wide stackable anyway.
#[test]
fn a_2_wide_gate_is_tested_at_2_and_not_at_4() {
    let raw = std::fs::read("tests/fixtures/xor-1-2-wide.schem").unwrap();
    let data = import::with_roles(&raw, "xor_gate").unwrap();
    let at = |west| Test::Xor.stacks_at(&data, Step::new(west, 0), 3);
    let Some(Ok((step, cases))) = at(2) else {
        panic!("it does not stack 2 west: {:?}", at(2));
    };
    assert_eq!((step.pitch(1), cases), (Some(2), 64));
    let Some(Ok((step, cases))) = at(4) else {
        panic!("it does not stack 4 west: {:?}", at(4));
    };
    assert_eq!((step.pitch(1), cases), (Some(4), 0));
    assert!(app::models::schematic::classes(2).any(|class| class == 4));
}

/// Cells narrower than a step stack there untested, as copies with air
/// between them cannot meet: the 1-wide monostable's copies 2 west, a
/// block of air between each, and 4 west, three, are taken to work with
/// no cases run, while copies 1 west, touching, are tested.
#[test]
fn cells_with_air_between_stack_untested() {
    let (data, _) = register("monostable-3-inverted");
    let at = |west| Test::Monostable.stacks_at(&data, Step::new(west, 0), 9);
    for west in [2, 4] {
        let Some(Ok((step, cases))) = at(west) else {
            panic!("it does not stack {west} west: {:?}", at(west));
        };
        assert_eq!((step.pitch(1), cases), (Some(west), 0));
    }
    let touching = at(1);
    assert!(
        !matches!(touching, Some(Ok((_, 0)))),
        "copies 1 west were not tested: {touching:?}"
    );
}

/// A 1-bit ripple adder, the port of adder 54926989, its core 6 wide and
/// its carry out dust on concrete at its west end, beside its outputs'
/// repeater: no pitch class works, its copies 1, 2, and 4 west
/// overlapping it, so its copies are tried end to end, from the narrowest
/// the box rule's box cuts its core to, 5, its east layer repeating the
/// copy before's west one, to its whole file's width, 9; 5 west, the box
/// rule's box, they chain, pitch 5, 5-wide stackable and of no other
/// class. Copies 10 west, air between them, are no stack: a chain's copies
/// must touch.
#[test]
fn an_adder_stacks_end_to_end_at_its_own_width() {
    let raw = std::fs::read("tests/fixtures/rca-1-5-wide.schem").unwrap();
    let data = import::with_roles(&raw, "adder").unwrap();
    let stacking = Test::Adder.stacking(&data).expect("it is tried");
    let Stacking::Stacks { step, .. } = &stacking else {
        panic!("it does not stack: {stacking:?}");
    };
    assert_eq!(step.describe(), "5 west");
    assert_eq!(step.faces, None, "not the box rule's box: {step:?}");
    let directions = stacking.directions();
    assert_eq!(directions[0].pitch, Some(5), "{directions:?}");
    let classes: Vec<i64> = app::models::schematic::classes(5).collect();
    assert_eq!(classes, [5]);
    let apart = Test::Adder.stacks_at(&data, Step::new(10, 0), 3);
    assert!(
        matches!(&apart, Some(Err(reason)) if reason.contains("air")),
        "{apart:?}"
    );
}

/// The 2-wide gate's own stacking: copies tested enough to reach past what
/// dust carries, 9 of them 2 west apart, every case of the 9-bit gate
/// right, pitch 2, tighter than any way up.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_2_wide_gates_stacking_is_pitch_2_west() {
    let raw = std::fs::read("tests/fixtures/xor-1-2-wide.schem").unwrap();
    let data = import::with_roles(&raw, "xor_gate").unwrap();
    let stacking = Test::Xor.stacking(&data).expect("it is tried");
    let Stacking::Wider {
        step, copies, bits, ..
    } = &stacking
    else {
        panic!("it does not stack: {stacking:?}");
    };
    assert_eq!((step.describe().as_str(), *copies, *bits), ("2 west", 9, 9));
    let directions = stacking.directions();
    assert_eq!(directions[0].pitch, Some(2), "{directions:?}");
}

/// A ripple adder holds nothing; a sequential multiplier holds its partial
/// products in loops.
#[test]
fn memory_is_found_in_loops_but_not_in_combinational_logic() {
    let adder = simulate::compiled(&blocks("alpha-rca")).unwrap();
    assert!(adder.memory().is_empty());
    let raw = read_fixture("tests/fixtures/mult-8-buttons.schem").unwrap();
    let file = standardize::standardize(&raw).unwrap();
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

/// A build is stored with each input at logical 0, its levers flicked to
/// it whatever way they were saved, and the circuit at rest with them.
/// This adder reads its carry in inverted, its lever saved on: saved off,
/// it is flicked back on, its torch and dust settling as they were, and
/// flicking it again changes nothing.
#[test]
fn a_build_is_stored_with_its_inputs_at_logical_0() {
    let raw = read_fixture("tests/fixtures/cca-8-repeated-out.schem").unwrap();
    let saved = app::recognize::legacy::with_colour_roles(
        &standardize::tidy(&raw).unwrap(),
    )
    .unwrap();
    let blocks = Blocks::read(&saved).unwrap();
    let lever = blocks
        .iter()
        .find(|(_, state)| {
            state.starts_with("minecraft:lever")
                && state.contains("powered=true")
        })
        .map(|(pos, _)| pos)
        .expect("its carry in's lever is saved on");
    let off: std::collections::BTreeMap<_, _> = [(
        lever,
        blocks.get(lever).replace("powered=true", "powered=false"),
    )]
    .into();
    let saved_off = standardize::edit(&saved, &off).unwrap();
    let inverted = ["carry in".to_owned()];
    let fixed =
        standardize::levers_at_zero(&saved_off, "adder", &inverted).unwrap();
    let at_zero = Blocks::read(&fixed).unwrap();
    assert!(at_zero.get(lever).contains("powered=true"));
    let settled =
        standardize::levers_at_zero(&saved, "adder", &inverted).unwrap();
    let as_saved = Blocks::read(&settled).unwrap();
    for (pos, state) in as_saved.iter() {
        assert_eq!(at_zero.get(pos), state, "{pos:?}");
    }
    let again =
        standardize::levers_at_zero(&fixed, "adder", &inverted).unwrap();
    assert_eq!(again, fixed);
}

/// Driven weakly, as a comparator into each block would, every data input
/// at one strength: this carry-cancel adder's operands reach its
/// comparators through dust, so it needs 5; the ripple carry slice needs
/// 8. Each output leaves through a repeater, which gives 15, but its
/// strength is what arrives there: the carry-cancel adder's comparators
/// leave its sum at 1, the slice's dust at 14.
#[test]
fn an_adders_data_inputs_are_driven_as_weakly_as_they_work() {
    let strength = |name: &str| {
        let Report::Ran(run) = Test::Adder.run(&blocks(name)) else {
            panic!("{name} was not simulated");
        };
        run.strength.expect("a passing adder is measured")
    };
    let cca = strength("cca-8-repeated-out");
    assert_eq!(cca.input, 5);
    assert_eq!(cca.outputs, [("output".to_owned(), vec![1; 8])]);
    let rca = strength("rca-slice-capped");
    assert_eq!(rca.input, 8);
    assert_eq!(rca.outputs, [("output".to_owned(), vec![14])]);
}

/// A hex adder of 16 digits, 64 bits a side, gives a sum and a carry out
/// of 65 bits, which its outputs are read into as one value: it is taken
/// for a hex adder and tested as one, each digit's every case and the
/// patterns, where its outputs once wrapped round, or panicked in a debug
/// build, and then were too wide to simulate. The build itself, e76edf,
/// drops some carries into its odd digits, as 0x100 + 0xF00 with a carry
/// in, which gives 0x001 for 0x1001.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_64_bit_hex_adder_is_tested_digit_by_digit() {
    let blocks = blocks("hex-adder-64");
    assert_eq!(simulate::guess(&blocks).unwrap().category, "hex_adder");
    let Report::Ran(run) = Test::Adder.run(&blocks) else {
        panic!("a 64-bit hex adder was not simulated");
    };
    let widths: Vec<(&str, usize)> = run
        .outputs
        .iter()
        .map(|signal| (signal.name.as_str(), signal.bits))
        .collect();
    assert_eq!(widths, [("output", 64), ("carry out", 1)]);
    assert!(!run.exhaustive && run.by_digit && run.cases < 30_000);
    assert_eq!(run.cases, 24_631);
    assert_eq!(run.examples[0].inputs, [0x100, 0xF00, 1]);
    assert_eq!(run.examples[0].expected, [0x1001, 0]);
    assert_eq!(run.examples[0].got, [0x001, 0]);
}
