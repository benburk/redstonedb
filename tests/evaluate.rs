//! Reading a build from its file alone.

use std::collections::BTreeMap;

use app::{
    evaluate::evaluate,
    import,
    models::schematic::{Logic, Orientation},
    schem::Blocks,
    simulate::{Report, Stacking},
    standardize,
};

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    standardize::pink_status(&tidied).expect("fixture recolours")
}

#[test]
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

#[test]
fn a_build_failing_its_test_has_no_timing() {
    let e = evaluate(&stored("cca-8-dust-out")).unwrap();
    assert_eq!(e.category, Some("adder"));
    assert!(!e.passes());
    assert_eq!((e.latency, e.throughput), (None, None));
}

#[test]
fn an_adders_design_comes_from_how_its_carries_are_made() {
    let design = |name| evaluate(&stored(name)).unwrap().design.unwrap();
    let ripple = design("alpha-rca");
    assert_eq!(
        (ripple.name, ripple.signature.as_str()),
        ("ripple_carry", "rrrrrrrr")
    );
    assert_eq!(design("diag-3t-cca").name, "carry_cancel");
    assert_eq!(design("rca-slice-capped").name, "ripple_carry");
}

#[test]
fn a_carry_signature_names_its_design() {
    use app::simulate::design_of;
    assert_eq!(design_of("~~~~~~~L", false), "carry_cancel");
    assert_eq!(design_of("~~~~~~~-", false), "carry_cancel");
    assert_eq!(design_of("~~~~~~~L~~~~~~~s", false), "carry_cancel");
    assert_eq!(design_of("rLL~~~~L", false), "hybrid");
    assert_eq!(design_of("L~~~~~~L", false), "hybrid");
    assert_eq!(design_of("??????~L", false), "other");
    assert_eq!(design_of("rrrrrrrr", false), "ripple_carry");
    assert_eq!(design_of("rLLLLLLL", true), "carry_look_everywhere");
    assert_eq!(design_of("rLLLLLLL", false), "carry_lookahead");
}

#[test]
fn a_sequential_multiplier_is_pressed_to_calculate_and_never_stacks() {
    // Standardized, as its file is not in the palette.
    let raw = std::fs::read("tests/fixtures/mult-8-buttons.schem").unwrap();
    let file = standardize::standardize(&raw).unwrap();
    let e = evaluate(&file).unwrap();
    assert_eq!(e.category, Some("multiplier"));
    assert!(e.passes(), "{:?}", e.report);
    assert!(e.stacking.is_empty());
    assert!(e.latency.is_some());
    // What it was given is kept from one calculation to the next.
    assert_eq!(e.logic, Some(Logic::Stateful));
    // How often a new calculation can start: its reset and calculate
    // pressed in turn, each at its own tick.
    assert!(e.throughput.is_some());
    let timing = e.report.as_ref().and_then(|r| r.timing()).unwrap();
    for line in ["reset", "calculate"] {
        let timed = timing
            .inputs
            .iter()
            .any(|i| i.name == line && i.tick.is_some());
        assert!(timed, "{line}");
    }
}

#[test]
fn a_gate_is_read_whole_from_its_file_its_stacking_by_its_copies() {
    // Prepared as an upload is, turned the standard way.
    let raw = std::fs::read("tests/fixtures/xor-1.schem").unwrap();
    let file = import::prepare(&raw, true, true, false, true).unwrap().data;
    let e = evaluate(&file).unwrap();
    assert_eq!(e.category, Some("xor_xnor_gate"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(1));
    // A 1-bit gate's lone input can't show its orientation; its copies
    // stacking west into more bits do.
    assert_eq!(e.stacking.len(), 1, "{:?}", e.stacking);
    assert_eq!(e.stacking[0].west, 4);
    assert_eq!(e.orientation, Some(Orientation::Horizontal));
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
    assert!(e.flags.is_empty(), "an XOR, not an XNOR: {:?}", e.flags);
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
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
    import::prepare(&raw, true, true, false, true).unwrap().data
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
        &std::fs::read("tests/fixtures/register-8-write-disturbs.schem")
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
    let e = evaluate(&standardize::edit(&data, &off).unwrap()).unwrap();
    assert_eq!(e.category, Some("alu"));
    assert!(e.passes(), "{:?}", e.report);
}

/// An ALU is read whole from its file: its flags are the operations its
/// test checks, but add and subtract, and the lines whose meaning is
/// plain; it never stacks, so its stacking is never searched; and its
/// throughput is measured on a stream of operations.
#[test]
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
fn an_alus_inverted_add_and_subtract_are_found_by_its_lines() {
    let e = evaluate(
        &std::fs::read("tests/fixtures/alu-8-nadd-nsub.schem").unwrap(),
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
/// stack 2 west by a box found by testing, which leaves the east layer to
/// the first copy, as many as the line reaches; and its throughput is
/// measured.
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
        ("2 west per copy, up to 8 copies", Some([1, 0, 0]))
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

/// Inverting a line does not change what a build is: a subtractor is an
/// adder whose B is inverted, its carries borrows, flagged so; a carry in
/// a torch inverts is an inverted carry in; and a register that reads its
/// cells inverted says so.
#[test]
fn inverted_lines_are_flags_not_categories() {
    let subtractor = evaluate(&uploaded("dd-cca-sub-16")).unwrap();
    assert_eq!(subtractor.category, Some("adder"));
    assert!(subtractor.passes(), "{:?}", subtractor.report);
    assert!(
        subtractor.flags.iter().any(|f| f == "subtract"),
        "{:?}",
        subtractor.flags
    );
    assert!(subtractor.design.is_some());

    let raw = std::fs::read("tests/fixtures/vert-3t-cca.schem").unwrap();
    let file = import::prepare(&raw, false, true, false, false)
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
/// output bit on, or, as this one, off, the rest on.
#[test]
fn a_decoder_is_read_whole_from_its_file() {
    let e = evaluate(&stored("decoder-4-inverted")).unwrap();
    assert_eq!(e.category, Some("decoder"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(4));
    assert_eq!(e.flags, ["inverted"]);
    assert!(e.latency.is_some() && e.throughput.is_some());
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A NAND+XNOR gate gives both on outputs of their own, and stacks west
/// into more bits as an XOR gate does.
#[test]
fn a_nand_xnor_gate_is_read_whole_from_its_file() {
    let e = evaluate(&stored("nand-xnor-1")).unwrap();
    assert_eq!(e.category, Some("nand_xnor_gate"));
    assert!(e.passes(), "{:?}", e.report);
    assert_eq!(e.width, Some(1));
    assert_eq!(e.stacking.len(), 1, "{:?}", e.stacking);
    assert_eq!(e.orientation, Some(Orientation::Horizontal));
    assert_eq!(e.logic, Some(Logic::Stateless));
}

/// A barrel shifter's shifts are found by trying its lines at every
/// amount on every value of A: this one shifts left, and a combination of
/// its lines that shifts only some values is no shift of it.
#[test]
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

/// Copies of a decoder stacked side by side are more decoders, each
/// decoding its own bits of A onto its own outputs, however their bits lie
/// between each other's; they show nothing of how its bits run.
#[test]
fn a_decoders_copies_stack_as_more_decoders() {
    let e = evaluate(&stored("decoder-4-inverted")).unwrap();
    let Some(Report::Ran(run)) = &e.report else {
        panic!("it ran");
    };
    assert!(
        matches!(run.stacking, Some(Stacking::Separate { .. })),
        "{:?}",
        run.stacking
    );
    // Both ways: copies side by side west, and up on top of each other.
    assert_eq!(e.stacking.len(), 2, "{:?}", e.stacking);
    assert!(e.stacking.iter().any(|d| d.west > 0 && d.up == 0));
    assert!(e.stacking.iter().any(|d| d.up > 0 && d.west == 0));
    assert_eq!(
        e.orientation,
        Some(Orientation::Horizontal),
        "as its own bits run"
    );
}
