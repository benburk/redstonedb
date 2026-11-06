use super::*;

/// The fixture `name`, standardized as an upload is.
fn fixture(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
    crate::schem::standardize::standardize(&raw).unwrap()
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
];

/// Every fixture probing recognizes as a kind matching knows is matched
/// as that kind, every case right; and every fixture probing recognizes as
/// nothing is matched as nothing with every case right.
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
        assert_eq!(
            found.as_ref().map(|found| found.kind),
            probed,
            "{name}: {}",
            found.as_ref().map_or("none".to_owned(), Match::found)
        );
        compared += usize::from(probed.is_some());
    }
    assert!(compared > 30, "{compared} compared");
}
