//! Reading a build from its file alone.

use std::collections::BTreeMap;

use app::{
    category::Logic,
    ingest::{
        evaluate::evaluate,
        prepare::{self, Purpose},
        publish,
    },
    interface::Orientation,
    schem::{self, Blocks},
    simulate::{Report, Stacking},
    standard::standardize,
};

/// The fixture file at `path`, carrying the roles its colours mark, as the
/// fixtures are saved in the site's old colours; see
/// `legacy::colour_roles`.
fn read_fixture(path: &str) -> std::io::Result<Vec<u8>> {
    let raw = std::fs::read(path)?;
    Ok(app::recognize::legacy::with_colour_roles(&raw)
        .expect("fixture carries its roles"))
}

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let raw = read_fixture(&format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    app::recognize::legacy::with_colour_roles(&tidied)
        .expect("fixture carries its roles")
}

#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_ripple_adder_slice_is_read_whole_from_its_file() {
    let e = evaluate(&stored("rca-slice-capped")).unwrap();
    assert_eq!(e.category, Some("adder"));
    assert!(e.passes());
    assert_eq!(e.width, Some(1));
    assert_eq!(e.flags, ["carry_in", "carry_out"]);
    let latency = e.latency.unwrap();
    let throughput = e.throughput.unwrap();
    assert!((1..=2 * latency.max(1)).contains(&throughput));
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A counter's clock is a lever pulsed for 2 ticks, read as it is or
/// inverted, counting while its lever is off and resting while it is on, and
/// its value changes only on a pulse. The first two counters built to the
/// standard count through every value and load every value of A with their
/// clock, but load A as their write enable turns off alone, which fails
/// them; one, its clock inverted, counts every 2 ticks while its clock is
/// held.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_counters_value_changes_only_on_a_clock_pulse() {
    use std::sync::atomic::AtomicBool;
    for (name, free_running) in [
        ("counter-8-lever-clock", None),
        ("counter-8-inverted-clock", Some(2)),
    ] {
        let raw =
            std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
        let blocks =
            Blocks::read(&publish::with_roles(&raw, "counter").unwrap())
                .unwrap();
        let Report::Ran(run) = app::category::Test::Counter.run(&blocks) else {
            panic!("{name} ran");
        };
        assert!(run.wrong > 0, "{name}");
        assert!(
            run.examples.iter().all(|wrong| wrong
                .op
                .as_deref()
                .is_some_and(|op| op.starts_with("write enable alone"))),
            "{name}: {:?}",
            run.examples
        );
        let timing = app::category::Test::Counter
            .time(&blocks, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(timing.free_running, free_running, "{name}");
    }
}

#[test]
fn a_build_failing_its_test_has_no_timing() {
    let e = evaluate(&stored("cca-8-dust-out")).unwrap();
    assert_eq!(e.category, Some("adder"));
    assert!(!e.passes());
    assert_eq!((e.latency, e.throughput), (None, None));
}

#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_adders_design_comes_from_how_its_carries_are_made() {
    let design = |name| evaluate(&stored(name)).unwrap().design.unwrap();
    let ripple = design("alpha-rca");
    assert_eq!(
        (ripple.name.as_str(), ripple.signature.as_str()),
        ("ripple_carry", "rrrrrrrr")
    );
    assert_eq!(design("diag-3t-cca").name, "carry_cancel");
    assert_eq!(design("rca-slice-capped").name, "ripple_carry");
    // A torch for each transmit and a subtracting comparator for each term,
    // reading a barrel, each carry's terms on its own dust straight into its
    // sum: a cell gives 15 for its term and a little otherwise.
    let everywhere = design("cle-8-subtract-cells");
    assert_eq!(
        (everywhere.name.as_str(), everywhere.terms.as_slice()),
        ("carry_look_everywhere", terms(&[(36, 36)]).as_slice())
    );
    // Torches alone, from before comparators: its carries held only in the
    // halves of its sums look ahead, not cancel.
    let alpha = design("cla-4-alpha-torches");
    assert_eq!(
        (alpha.name.as_str(), alpha.signature.as_str()),
        ("carry_lookahead", "rLLL")
    );
    // Comparators making each carry whole on dust from its generate and
    // the carry below transmitted, read a distance or two away rather than
    // as a strength falling along the dust: a ripple carry, though no
    // component holds a carry and its sums' comparators read them.
    let ripple = design("inverted-adder-8-rca-comparator-ands");
    assert_eq!(
        (ripple.name.as_str(), ripple.signature.as_str()),
        ("ripple_carry", "rrrrrrr-")
    );
    // The same carries made on dust at every other bit, those between
    // looked ahead from the carry two below.
    let alternating = design("adder-8-ripple-even-lookahead-odd");
    assert_eq!(
        (alternating.name.as_str(), alternating.signature.as_str()),
        ("carry_lookahead", "rLrLrLrL")
    );
}

/// Each group's lookahead terms, as `found` of `all`.
fn terms(groups: &[(usize, usize)]) -> Vec<app::simulate::Terms> {
    groups
        .iter()
        .map(|&(found, all)| app::simulate::Terms { found, all })
        .collect()
}

#[test]
fn a_carry_signature_and_terms_name_its_design() {
    use app::category::design_of;
    // No terms, carries mostly strengths or looked ahead: a carry cancel,
    // in one group or two, its lowest carries rippling or not.
    for signature in
        ["~~~~~~~L", "~~~~~~~-", "rLL~~~~L", "L~~~~~~L", "rLLLLLLL"]
    {
        assert_eq!(design_of(signature, &terms(&[(0, 36)])), "carry_cancel");
    }
    assert_eq!(
        design_of("~~~~~~~L~~~~~~~s", &terms(&[(0, 36), (0, 28)])),
        "carry_cancel"
    );
    // Every term of every group: a carry look everywhere, in groups too,
    // though its sums' comparators read like strengths.
    assert_eq!(
        design_of("rLLLLLLL", &terms(&[(36, 36)])),
        "carry_look_everywhere"
    );
    assert_eq!(
        design_of("~~~~~~~L~~~~~~~s", &terms(&[(36, 36), (28, 28)])),
        "carry_look_everywhere"
    );
    // A look everywhere group below a carry cancel one.
    assert_eq!(
        design_of("~~~~~~~L~~~~~~~s", &terms(&[(36, 36), (0, 28)])),
        "hybrid"
    );
    assert_eq!(
        design_of("rLLLLLLL", &terms(&[(16, 36)])),
        "carry_lookahead"
    );
    // Strengths throughout, its towers making a few short terms.
    assert_eq!(design_of("~~~~~~~~", &terms(&[(7, 36)])), "carry_cancel");
    // Two-bit groups skipping a carry, with no terms.
    assert_eq!(
        design_of("rrsrsrsr", &terms(&[(0, 4), (0, 1), (0, 1), (0, 0)])),
        "carry_lookahead"
    );
    assert_eq!(design_of("??????~L", &terms(&[(0, 36)])), "other");
    assert_eq!(design_of("rrrrrrrr", &terms(&[(0, 36)])), "ripple_carry");
    // One without a carry out.
    assert_eq!(design_of("rrrrrrr-", &terms(&[(0, 36)])), "ripple_carry");
}

#[test]
fn a_carry_signature_shows_its_groups() {
    use app::category::groups;
    assert_eq!(groups("~~~~~~~L"), [0..=7]);
    assert_eq!(groups("~~~~~~~L~~~~~~~s"), [0..=7, 8..=15]);
    assert_eq!(groups("rLLLLLLL"), [0..=7]);
    assert_eq!(groups("rrsrsrsr"), [0..=2, 3..=4, 5..=6, 7..=7]);
}

#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_sequential_multiplier_is_pressed_to_calculate_and_never_stacks() {
    let file = read_fixture("tests/fixtures/mult-8-calculate.schem").unwrap();
    let e = evaluate(&file).unwrap();
    assert_eq!(e.category, Some("multiplier"));
    assert!(e.passes(), "{:?}", e.report);
    assert!(e.stacking.is_empty());
    assert!(e.latency.is_some());
    // What it was given is kept from one calculation to the next.
    assert_eq!(e.logic, Some(Logic::Stateful));
    // How often a new calculation can start: its calculate pressed in
    // turn, at its own tick.
    assert!(e.throughput.is_some());
    let timing = e.report.as_ref().and_then(|r| r.timing()).unwrap();
    let timed = timing
        .inputs
        .iter()
        .any(|i| i.name == "calculate" && i.tick.is_some());
    assert!(timed);
}

/// 4ff460f9 as first uploaded: its calculate a wooden button, held down 15
/// ticks, as its reworked stone button with a pulse extender is.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_multiplier_calculated_on_a_wooden_button_holds() {
    let file = read_fixture("tests/fixtures/mult-8-wooden.schem").unwrap();
    let e = evaluate(&file).unwrap();
    assert_eq!(e.category, Some("multiplier"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!((e.latency, e.throughput), (Some(25), Some(32)));
}

#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_gate_is_read_whole_from_its_file_its_stacking_by_its_copies() {
    // Prepared as an upload is, turned the standard way.
    let raw = read_fixture("tests/fixtures/xor-1.schem").unwrap();
    let file =
        prepare::prepare_for(&raw, Purpose::Standard { keep_paste: false })
            .unwrap()
            .data;
    let e = evaluate(&file).unwrap();
    assert_eq!(e.category, Some("xor_gate"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(1));
    // A 1-bit gate's lone input can't show its orientation; its copies
    // stacking into more bits do, at the tightest pitch that works, west
    // first on a tie: its output's repeater standing on a block of its
    // own, they fit 4 up, pitch 4, and 4 west, pitch 4, as copies of a
    // gate 3 wide with air between them cannot meet.
    let steps: Vec<(i64, i64, Option<i64>)> =
        e.stacking.iter().map(|d| (d.west, d.up, d.pitch)).collect();
    assert_eq!(
        steps,
        [(4, 0, Some(4)), (0, 4, Some(4))],
        "{:?}",
        e.stacking
    );
    assert_eq!(e.orientation, Some(Orientation::Horizontal));
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
    assert!(e.flags.is_empty(), "{:?}", e.flags);
}

#[test]
fn an_incrementer_with_no_carry_out_does_not_stack() {
    let e = evaluate(&stored("inc-8")).unwrap();
    assert_eq!(e.category, Some("incrementer"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(8));
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(
        matches!(run.stacking, Some(Stacking::Not { .. })),
        "{:?}",
        run.stacking
    );
    assert!(e.stacking.is_empty());
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A fixture prepared as an upload is, whitened and turned the standard
/// way.
fn uploaded(name: &str) -> Vec<u8> {
    let raw = read_fixture(&format!("tests/fixtures/{name}.schem")).unwrap();
    prepare::prepare_for(&raw, Purpose::Standard { keep_paste: false })
        .unwrap()
        .data
}

/// A comparator or absolute value is read whole from its file, a
/// comparator's sign from its cases; copies of either make no wider one,
/// so neither is searched for stacking. A comparator signed only by its
/// sign bits' levers saved on is unsigned, as how a lever was saved says
/// nothing.
#[test]
fn a_comparison_and_an_absolute_value_are_read_whole_from_their_files() {
    for (name, category, flags) in [
        ("gt-8", "greater_than", &[] as &[&str]),
        ("gt-8-signed", "greater_than", &[][..]),
        ("abs-8", "absolute_value", &[][..]),
    ] {
        let e = evaluate(&uploaded(name)).unwrap();
        assert_eq!(e.category, Some(category), "{name}");
        assert!(e.passes(), "{name}: {:?}", e.report);
        assert_eq!(e.width, Some(8), "{name}");
        assert_eq!(e.flags, flags, "{name}");
        assert!(e.stacking.is_empty(), "{name}");
        let Some(Report::Ran(run)) = &e.report else {
            panic!("{name} ran");
        };
        assert!(run.stacking.is_none(), "{name} is never searched");
        assert!(e.latency.is_some() && e.throughput.is_some(), "{name}");
        assert_eq!(e.logic, Some(Logic::Stateless), "{name}");
    }
    // An equality comparator whose bit 0 is too weak to count is one, but
    // fails, so it has no timing.
    let weak = evaluate(&uploaded("eq-8-weak-bit-0")).unwrap();
    assert_eq!(weak.category, Some("equality"));
    assert!(!weak.passes());
    assert_eq!((weak.latency, weak.throughput), (None, None));
}

/// A parity circuit built on lamps, standardized and coloured by probing
/// as an upload is, is read whole from its file, its width A's and its
/// orientation the way A runs, up; copies of one make no wider one, so it
/// is never searched for stacking.
#[test]
fn a_parity_circuit_is_read_whole_from_its_file() {
    let recognition =
        app::recognize::probe::recognize(&uploaded("parity-8")).unwrap();
    assert_eq!(
        recognition.found.as_deref(),
        Some("a parity circuit: all 256 cases right, xor of A's bits"),
        "{:?}",
        recognition.problems
    );
    let e = evaluate(&recognition.file).unwrap();
    assert_eq!(e.category, Some("parity"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(8));
    assert_eq!(e.orientation, Some(Orientation::Vertical));
    assert!(e.flags.is_empty(), "{:?}", e.flags);
    assert!(e.stacking.is_empty());
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert_eq!((run.cases, run.wrong), (256, 0));
    assert!(run.stacking.is_none(), "it is never searched");
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A negator, standardized and coloured by probing as an upload is, is
/// read whole from its file, its width A's and its orientation the way A
/// runs, up; copies of one make no wider one, so it is never searched for
/// stacking.
#[test]
fn a_negator_is_read_whole_from_its_file() {
    let recognition =
        app::recognize::probe::recognize(&uploaded("negator-8")).unwrap();
    assert_eq!(
        recognition.found.as_deref(),
        Some("a negator: all 256 cases right, −A"),
        "{:?}",
        recognition.problems
    );
    let e = evaluate(&recognition.file).unwrap();
    assert_eq!(e.category, Some("negator"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(8));
    assert_eq!(e.orientation, Some(Orientation::Vertical));
    assert!(e.flags.is_empty(), "{:?}", e.flags);
    assert!(e.stacking.is_empty());
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(run.stacking.is_none(), "it is never searched");
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A parity circuit is on for an odd number of A's bits on: the owner's
/// with its output inverted by a torch, on for an even number, is not
/// one.
#[test]
fn an_even_parity_circuit_is_not_a_parity_circuit() {
    // The dust its two last comparators power, which its output's
    // repeater read, powers a block in the repeater's place, and a torch
    // on that block feeds the output's repeater, moved north of the torch.
    let standard = uploaded("parity-8");
    let changes: BTreeMap<[i32; 3], String> = [
        ([1, 0, 0], "white_concrete"),
        ([1, 1, 0], "white_concrete"),
        ([1, 1, -1], "redstone_wall_torch[facing=north,lit=true]"),
        ([1, 0, -2], "yellow_concrete"),
        (
            [1, 1, -2],
            "repeater[delay=1,facing=south,locked=false,powered=true]",
        ),
    ]
    .into_iter()
    .map(|(pos, state)| (pos, format!("minecraft:{state}")))
    .collect();
    let (even, _) = schem::edit_grown(&standard, &changes).unwrap();
    let recognition = app::recognize::probe::recognize(&even).unwrap();
    assert!(
        !recognition
            .found
            .as_deref()
            .is_some_and(|found| found.starts_with("a parity circuit")),
        "{:?}",
        recognition.found
    );
}

/// A register is read whole from its file: holding what it was written is
/// its test, so passing it is holding state; its throughput is its burnout
/// floor once synchronized, and its copies stack west into a register file.
#[test]
fn a_register_is_read_whole_from_its_file() {
    let e = evaluate(&uploaded("register-8-2-cells")).unwrap();
    assert_eq!(e.category, Some("register"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(8));
    assert_eq!(e.logic, Some(Logic::Stateful));
    assert_eq!(e.synchronized, Some(true));
    // Measured on a stream of accesses, held to its burnout floor.
    let torches = e.prepared.facts.torches > 0;
    let throughput = e.throughput.expect("its accesses stream");
    assert!(throughput >= if torches { 3 } else { 2 });
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    if let Some(Stacking::Registers { step, .. }) = &run.stacking {
        assert_eq!(e.stacking.len(), 1);
        assert_eq!(e.stacking[0].west, step.west);
    }
}

/// Writing one cell of this register changes another, which a round that
/// writes its cells in one order hides, as the later write puts it right:
/// each round starts its writes a cell on, so it fails.
#[test]
fn a_register_whose_write_disturbs_another_cell_fails() {
    let e = evaluate(
        &read_fixture("tests/fixtures/register-8-write-disturbs.schem")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(e.category, Some("register"));
    assert!(!e.passes(), "{:?}", e.report);
}

/// An ALU's control lines read inverted where they are, whatever way their
/// levers were saved: with every lever saved off, they are found from what
/// its lever settings give.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_alus_inverted_lines_are_found_with_its_levers_saved_off() {
    let data = uploaded("alu-8-ops-raw");
    let blocks = Blocks::read(&data).unwrap();
    let off: BTreeMap<[i32; 3], String> = blocks
        .iter()
        .filter(|(_, state)| {
            state.contains("lever") && state.contains("powered=true")
        })
        .map(|(pos, state)| {
            (pos, state.replace("powered=true", "powered=false"))
        })
        .collect();
    assert!(!off.is_empty());
    let e = evaluate(&schem::edit(&data, &off).unwrap()).unwrap();
    assert_eq!(e.category, Some("alu"));
    assert!(e.passes(), "{:?}", e.report);
}

/// An ALU is read whole from its file: its flags are the operations its
/// test checks, but add and subtract, and the lines whose meaning is
/// plain; it never stacks, so its stacking is never searched; and its
/// throughput is measured on a stream of operations.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_alu_is_read_whole_from_its_file() {
    let e = evaluate(&uploaded("alu-8-ops-raw")).unwrap();
    assert_eq!(e.category, Some("alu"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(8));
    for op in ["and", "or", "xor", "nand", "nor", "xnor", "shr", "asr"] {
        assert!(e.flags.iter().any(|f| f == op), "{op}: {:?}", e.flags);
    }
    assert!(!e.flags.iter().any(|f| f == "add" || f == "sub"));
    assert!(e.stacking.is_empty());
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(run.stacking.is_none(), "an ALU is never searched");
    let torches = e.prepared.facts.torches > 0;
    let throughput = e.throughput.expect("its operations stream");
    assert!(throughput >= if torches { 3 } else { 2 });
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// Nadd and nsub, add and subtract with the result inverted, are checked
/// as any operation is, and are flags. This ALU signs two lines both `nadd
/// nsub`, which cannot tell them apart, so trying its lines finds each.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn an_alus_inverted_add_and_subtract_are_found_by_its_lines() {
    let e = evaluate(
        &read_fixture("tests/fixtures/alu-8-nadd-nsub.schem").unwrap(),
    )
    .unwrap();
    assert_eq!(e.category, Some("alu"));
    assert!(e.passes(), "{:?}", e.report);
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(run.pattern.contains("nadd ("), "{}", run.pattern);
    assert!(run.pattern.contains("nsub ("), "{}", run.pattern);
    for flag in ["nadd", "nsub"] {
        assert!(e.flags.iter().any(|f| f == flag), "{flag}: {:?}", e.flags);
    }
}

/// An inverter is read whole from its file: its control line sits on its
/// east face beside logic the box rule keeps in every copy, so its copies
/// stack 2 west, pitch 2, by a box found by testing, which leaves the east
/// layer to the first copy, as many as the line reaches; and its
/// throughput is measured.
#[test]
fn an_inverter_is_read_whole_from_its_file() {
    let e = evaluate(&uploaded("inverter-1")).unwrap();
    assert_eq!(e.category, Some("inverter"));
    assert!(e.passes(), "{:?}", e.report);
    let [direction] = e.stacking.as_slice() else {
        panic!("{:?}", e.stacking);
    };
    assert_eq!(
        (
            direction.describe().as_str(),
            direction.faces.map(|f| f.high)
        ),
        ("2 west per copy, pitch 2, up to 8 copies", Some([1, 0, 0]))
    );
    assert!(e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A monostable is read whole from its file: its pulse, whether it rests
/// on, and the edge it pulses on come from its test; a pulse generator
/// holds no state; and its throughput is how often it can be triggered,
/// each trigger giving its whole pulse.
#[test]
fn a_monostable_is_read_whole_from_its_file() {
    for (name, category, flags) in [
        ("monostable-3", "monostable", &[][..]),
        ("monostable-3-inverted", "monostable", &["inverted"][..]),
        ("monostable-3-falling", "monostable", &["falling_edge"][..]),
        ("monostable-3-dual-edge", "dual_edge_monostable", &[][..]),
    ] {
        let e = evaluate(&uploaded(name)).unwrap();
        assert_eq!(e.category, Some(category), "{name}");
        assert!(e.passes(), "{name}: {:?}", e.report);
        assert_eq!(e.pulse, Some(3), "{name}");
        assert_eq!(e.flags, flags, "{name}");
        assert_eq!(e.logic, Some(Logic::Stateless), "{name}");
        // A pulse of 3 ticks can't be triggered again before it ends.
        let throughput =
            e.throughput.expect("it can be triggered over and over");
        assert!(throughput > 3, "{name}: {throughput}");
    }
}

/// A buffer holds what it is written, so it holds state, and its
/// throughput is how often a cell can take a new value.
#[test]
fn a_buffer_is_read_whole_from_its_file() {
    let e = evaluate(&uploaded("buffer-2-button")).unwrap();
    assert_eq!(e.category, Some("buffer"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.logic, Some(Logic::Stateful));
    let torches = e.prepared.facts.torches > 0;
    let throughput = e.throughput.expect("its writes stream");
    assert!(throughput >= if torches { 3 } else { 2 });
}

/// Inverting a line does not change what a build is: a carry in a torch
/// inverts is an inverted carry in, and a register that reads its cells
/// inverted says so. A subtractor, an adder whose B is subtracted, its
/// carries borrows, is a category of its own.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn inverted_lines_are_flags_not_categories() {
    let subtractor = evaluate(&uploaded("dd-cca-sub-16")).unwrap();
    assert_eq!(subtractor.category, Some("subtractor"));
    assert!(subtractor.passes(), "{:?}", subtractor.report);
    assert!(
        subtractor.flags.iter().any(|f| f == "subtract"),
        "{:?}",
        subtractor.flags
    );
    assert!(subtractor.design.is_some());

    let raw = read_fixture("tests/fixtures/vert-3t-cca.schem").unwrap();
    let file = prepare::prepare_for(
        &raw,
        Purpose::InPlace {
            standardize: false,
            conventional: true,
            keep_paste: false,
        },
    )
    .unwrap()
    .data;
    let inverted = evaluate(&file).unwrap();
    assert_eq!(inverted.category, Some("adder"));
    assert!(inverted.passes(), "{:?}", inverted.report);
    assert!(
        inverted.flags.iter().any(|f| f == "inverted_carry_in"),
        "{:?}",
        inverted.flags
    );
    assert!(!inverted.flags.iter().any(|f| f == "subtract"));

    let plain = evaluate(&stored("alpha-rca")).unwrap();
    assert!(
        !plain
            .flags
            .iter()
            .any(|f| f.starts_with("inverted") || f == "subtract"),
        "{:?}",
        plain.flags
    );

    let register = evaluate(&uploaded("register-8-inverted")).unwrap();
    assert_eq!(register.category, Some("register"));
    assert!(
        register.flags.iter().any(|f| f == "inverted"),
        "{:?}",
        register.flags
    );
}

/// A decoder is read whole from its file: each value of A turns its own
/// output bit off, the rest on, as every decoder's are, so no flag says
/// so.
#[test]
fn a_decoder_is_read_whole_from_its_file() {
    let e = evaluate(&stored("decoder-4-inverted")).unwrap();
    assert_eq!(e.category, Some("decoder"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(4));
    assert!(e.flags.is_empty(), "{:?}", e.flags);
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A NAND+XNOR gate gives both on outputs of their own, and stacks west
/// into more bits as an XOR gate does.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_nand_xnor_gate_is_read_whole_from_its_file() {
    let e = evaluate(&stored("nand-xnor-1")).unwrap();
    assert_eq!(e.category, Some("nand_xnor_gate"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(1));
    assert!(e.stacking[0].west > 0, "{:?}", e.stacking);
    assert_eq!(e.orientation, Some(Orientation::Horizontal));
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A barrel shifter's shifts are found by trying its lines at every
/// amount on every value of A: this one shifts left, and a combination of
/// its lines that shifts only some values is no shift of it.
#[test]
#[ignore = "measures whole builds: just test-full"]
fn a_barrel_shifters_shifts_are_found_by_its_lines() {
    let e = evaluate(&stored("barrel-shifter-8")).unwrap();
    assert_eq!(e.category, Some("barrel_shifter"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(8));
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(
        run.pattern.contains("A << n (shift left on)"),
        "{}",
        run.pattern
    );
    assert!(e.stacking.is_empty());
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// Copies of a decoder side by side are separate decoders, not a wider
/// one, so a decoder never stacks: its stacking is neither searched nor
/// kept, and its orientation is its own bits'.
#[test]
fn a_decoder_never_stacks() {
    let e = evaluate(&stored("decoder-4-inverted")).unwrap();
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(e.passes(), "{:?}", e.report);
    assert!(run.stacking.is_none(), "{:?}", run.stacking);
    assert!(e.stacking.is_empty(), "{:?}", e.stacking);
    assert_eq!(
        e.orientation,
        Some(Orientation::Horizontal),
        "as its own bits run"
    );
}
