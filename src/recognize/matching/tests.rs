use super::*;

/// The fixture `name`, standardized as an upload is.
fn fixture(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
    crate::standard::standardize::standardize(&raw).unwrap()
}

/// The fixture `name` with every sign taken out.
fn unsigned(name: &str) -> Vec<u8> {
    let data = fixture(name);
    let blocks = Blocks::read(&data).unwrap();
    let changes = blocks
        .signs
        .keys()
        .map(|&pos| (pos, crate::schem::AIR.to_owned()))
        .collect();
    crate::schem::edit(&data, &changes).unwrap()
}

/// The fixture `name` matched.
fn matched(name: &str) -> Match {
    recognize(&fixture(name))
        .unwrap()
        .unwrap_or_else(|| panic!("{name}: no category's slots fit"))
}

/// Each build is matched as what it is, every case observed right.
#[test]
fn builds_are_matched_as_what_they_are() {
    for (name, kind, label) in [
        ("xnor-1-wool", Kind::XnorGate, "A xnor B"),
        ("nand-xnor-1", Kind::NandXnorGate, "A nand B, A xnor B"),
        ("cout-8", Kind::CarryOut, "carry of A + B"),
        ("decoder-4-inverted", Kind::Decoder, "bit A off"),
        ("inc-8", Kind::Incrementer, "A + increment"),
        ("negator-8", Kind::Negator, "−A"),
        ("adder-8-date-signs", Kind::Adder, "A + B + carry in"),
    ] {
        let found = matched(name);
        assert_eq!(
            (found.kind, found.wrong),
            (kind, 0),
            "{name}: {}",
            found.found()
        );
        assert_eq!(found.label, label, "{name}");
    }
}

/// A 1-bit XOR gate is a 2-bit odd parity circuit and a 1-bit adder
/// without its carries too; a tie goes to the kind probing tries first,
/// the gate.
#[test]
fn a_tie_goes_to_the_kind_probing_tries_first() {
    let found = matched("xor-1");
    assert_eq!(
        (found.kind, found.wrong),
        (Kind::XorGate, 0),
        "{}",
        found.found()
    );
}

/// An ALU without signs has its operations found from what each setting
/// of its control lines gives, and its carry in, if it has one, from the
/// add it carries into, read inverted as it is.
#[test]
fn an_alus_operations_are_found_by_their_settings() {
    for (name, ops, carry) in [
        (
            "alu-8-and-xor",
            &["add", "and", "or", "xor", "xnor"][..],
            true,
        ),
        (
            "alu-8-nadd-nsub",
            &[
                "add", "sub", "nadd", "nsub", "and", "or", "xor", "nand",
                "nor", "xnor",
            ][..],
            false,
        ),
    ] {
        let found = recognize(&unsigned(name)).unwrap().unwrap();
        assert_eq!(
            (found.kind, found.wrong),
            (Kind::Alu, 0),
            "{name}: {}",
            found.found()
        );
        let named: Vec<&str> = found
            .label
            .split(", ")
            .filter_map(|part| part.split(' ').next())
            .filter(|word| !word.starts_with(|c: char| c.is_ascii_digit()))
            .collect();
        for op in ops {
            assert!(named.contains(op), "{name}: no {op} in {}", found.label);
        }
        let carry_in = found.line("carry in");
        assert_eq!(carry_in.is_some(), carry, "{name}: {}", found.found());
        assert!(carry_in.is_none_or(|line| line.inverted), "{name}");
    }
}

/// An ALU whose control lines' signs name its operations is tested as they
/// name them: one of more lines than matching observes every setting of is
/// found, and one whose signed and and or are wrong in a few cases, or
/// whose signs name operations its lines don't choose, has cases wrong.
#[test]
fn an_alu_is_tested_as_its_signs_name_its_operations() {
    let found = matched("alu-8-ops-raw");
    assert_eq!(
        (found.kind, found.wrong),
        (Kind::Alu, 0),
        "{}",
        found.found()
    );
    for op in ["add", "sub", "and", "shr", "asr"] {
        assert!(found.label.contains(op), "no {op} in {}", found.label);
    }
    for name in ["alu-8-lines", "alu-8-extra-signs"] {
        let found = matched(name);
        assert_eq!(found.kind, Kind::Alu, "{name}: {}", found.found());
        assert!(found.wrong > 0, "{name}: {}", found.found());
    }
    // Its signs taken out, it is more lines than are observed every way.
    let unsigned = recognize(&unsigned("alu-8-ops-raw")).unwrap();
    assert!(unsigned.is_none_or(|found| found.kind != Kind::Alu));
}

/// An adder whose carry out is normally on is an adder, its carry out
/// read so, and no ALU that only adds, as its carry in chooses no second
/// operation.
#[test]
fn an_adder_with_a_normally_on_carry_out_is_no_alu() {
    let found = matched("vert-adder-8-stacks");
    assert_eq!(
        (found.kind, found.wrong),
        (Kind::Adder, 0),
        "{}",
        found.found()
    );
    assert!(found.line("carry out").is_some_and(|line| line.inverted));
}

/// A 10-bit carry-cancel adder whose carry out of its low three bits never
/// reaches bit 3 is matched as the adder it is closest to, its lines read
/// whole, its sum holding its carry, and its cases wrong, so it is not
/// recognized; its full test finds bit 3 wrong, and the bits above only
/// through it.
#[test]
fn a_broken_adder_is_matched_as_an_adder_with_cases_wrong() {
    let found = matched("cca-10-bit-3-carry-lost");
    assert_eq!(found.kind, Kind::Adder, "{}", found.found());
    assert!(found.wrong > 0, "{}", found.found());
    let width = |name: &str| found.line(name).map_or(0, |line| line.bits.len());
    assert_eq!(
        (width("A"), width("B"), width("carry in"), width("output")),
        (10, 10, 1, 11)
    );
    let simulated = probe::simulatable(
        &lines::without(&fixture("cca-10-bit-3-carry-lost")).unwrap(),
    )
    .unwrap();
    let blocks = Blocks::read(&simulated).unwrap();
    let run = crate::category::Test::Adder
        .check_with(&blocks, found.interface(&blocks), false)
        .unwrap();
    assert_eq!(
        run.wrong_bits.first().map(|(bit, _)| bit.as_str()),
        Some("output bit 3")
    );
}

/// A greater-than comparator whose operands' top bits stand out of line,
/// side by side, read as a run of their own: no conventional reading fits
/// its lines, so each top bit joins the run beside it, and it is found, its
/// I/O non-standard.
#[test]
fn stray_top_bits_join_their_runs() {
    let found = matched("gt-8");
    assert_eq!(
        (found.kind, found.wrong),
        (Kind::GreaterThan, 0),
        "{}",
        found.found()
    );
    assert!(!found.conventional);
    let a = &found.line("A").unwrap().bits;
    assert_eq!(a.len(), 8);
    assert_eq!(a[7][1] - a[6][1], 2, "the stray is A's top bit: {a:?}");
    assert!(matched("eq-8").conventional);
}

/// A negator whose levers climb a staircase, two to a step, is read as
/// one A, in order up the stairs, its I/O non-standard.
#[test]
fn a_staircase_of_levers_is_one_operand() {
    let found = matched("negator-8-staircase");
    assert_eq!(
        (found.kind, found.wrong),
        (Kind::Negator, 0),
        "{}",
        found.found()
    );
    assert!(!found.conventional);
    let a = &found.line("A").unwrap().bits;
    assert!(a.windows(2).all(|w| w[0][1] < w[1][1]), "{a:?}");
    assert!(matched("negator-8").conventional);
}

/// A status line may be normally on: a 2-bit adder whose carry out is on
/// unless it carries is read so, the line named inverted, every case right.
#[test]
fn a_status_line_may_be_normally_on() {
    let blocks = Blocks::read(&fixture("xor-1")).unwrap();
    let line = |x: i32, bits: i32| -> Vec<Pos> {
        (0..bits).map(|y| [x, 2 * y, 0]).collect()
    };
    let mut cases = Vec::new();
    for a in 0..4 {
        for b in 0..4 {
            let sum: Value = a + b;
            cases.push((vec![a, b], vec![sum & 3, !(sum >> 2) & 1]));
        }
    }
    let observed = Observation {
        inputs: vec![line(0, 2), line(1, 2)],
        outputs: vec![line(2, 2), vec![[3, 0, 0]]],
        cases,
    };
    let (_, found) = judge(
        &categories::Adder,
        None,
        &blocks,
        &observed,
        false,
        Order::default(),
    )
    .unwrap();
    assert_eq!(found.wrong, 0, "{}", found.found());
    assert!(found.line("carry out").is_some_and(|line| line.inverted));
}

/// A threshold's count shows only in values with that many bits on, so a
/// build of few levers is driven every way.
#[test]
fn a_threshold_is_matched_at_its_count() {
    let found = matched("threshold-3-raw");
    assert_eq!(found.cases, 256);
    assert_eq!(found.label, "at least 3 of A");
}

/// A carry in that reads inverted is read so, and named, rather than the
/// sum read wrong.
#[test]
fn a_carry_in_may_read_inverted() {
    let found = matched("adder-8-date-signs");
    assert!(found.line("carry in").is_some_and(|line| line.inverted));
    assert!(found.line("A").is_some_and(|line| !line.inverted));
    assert!(found.found().ends_with("; inverted: carry in"));
}

/// Every shape a category's slots may take that adds up: 17 levers and 9
/// outputs are an adder of 8 bits with a carry in, its sum holding its
/// carry or a carry out beside it; 18 levers are never a sum of 3 bits.
#[test]
fn shapes_add_up() {
    let slots = categories::Adder.slots();
    let found = shapes(slots, 17, 9);
    assert!(found.iter().all(|shape| shape.n == 8 && shape.has(2)));
    assert_eq!(found.len(), 2);
    assert!(shapes(slots, 18, 3).is_empty());
}

/// The kind probing found a build to be, from what it says it found, such
/// as `an adder: …`.
fn probed_kind(found: &str) -> Option<Kind> {
    Kind::all().find(|kind| found.starts_with(&format!("{}:", kind.a())))
}

/// The fixtures probing and matching disagree on, each for a reason.
const DISAGREEING: &[&str] = &[
    // Its carry out is wrong, so matching takes it for no adder; probing
    // takes it for a parity circuit, reading the carry out as a status
    // line, which a parity's test doesn't check.
    "rca-1-repeater-c",
    // Its carry out is normally on, which matching reads, so it finds an
    // adder; probing, reading no output inverted, takes it for an ALU that
    // only adds, checking only the setting with its carry in off.
    "vert-adder-8-stacks",
];

/// Every fixture probing recognizes as a kind matching knows is matched
/// as that kind, every case right; and every fixture probing recognizes as
/// nothing is matched as nothing with every case right, or as a kind whose
/// test, run by probing, finds cases wrong that matching's patterns miss.
#[test]
#[ignore = "probes every fixture"]
fn matching_agrees_with_probing() {
    let known: Vec<Kind> = CATEGORIES
        .iter()
        .map(|category| category.kind())
        .chain([Kind::Subtractor, Kind::Decrementer, Kind::IncDec])
        .collect();
    let mut names: Vec<String> = std::fs::read_dir("tests/fixtures")
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".schem").map(str::to_owned)
        })
        .filter(|name| !DISAGREEING.contains(&name.as_str()))
        .collect();
    names.sort();
    let mut compared = 0;
    for name in names {
        let data = fixture(&name);
        let probed = probe::recognize(&data)
            .unwrap()
            .found
            .and_then(|found| probed_kind(&found));
        // A kind matching doesn't know yet, as an ALU, can't be compared.
        if probed.is_some_and(|kind| !known.contains(&kind)) {
            continue;
        }
        let found = recognize(&data).unwrap().filter(|found| found.wrong == 0);
        let matched = found.as_ref().map(|found| found.kind);
        // Matching only identifies a build, on a few patterns; a bug they
        // miss is its kind's full test's to find, as probing's test, run
        // as that kind, finds it.
        let buggy = |kind: Kind| {
            probe::recognize_as(&data, Some(kind.name()))
                .unwrap()
                .problems
                .iter()
                .any(|problem| problem.contains(" cases wrong"))
        };
        let agrees =
            matched == probed || probed.is_none() && matched.is_some_and(buggy);
        assert!(
            agrees,
            "{name}: matched {}, probed {probed:?}",
            found.as_ref().map_or("none".to_owned(), Match::found)
        );
        compared += usize::from(probed.is_some());
    }
    assert!(compared > 30, "{compared} compared");
}
