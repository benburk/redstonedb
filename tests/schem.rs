//! Reading builds from their files, against the two stored adders.

use app::{interface, lint, schem::Blocks};

fn blocks(name: &str) -> Blocks {
    let data = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    Blocks::read(&data).expect("fixture parses")
}

fn names(ports: &[interface::Port]) -> Vec<(&str, usize)> {
    ports
        .iter()
        .map(|port| (port.name.as_str(), port.bits.len()))
        .collect()
}

#[test]
fn size_is_inside_the_corner_markers() {
    let size = blocks("alpha-cla").inner_size().unwrap();
    assert_eq!((size.x, size.y, size.z), (19, 11, 18));
    let size = blocks("alpha-rca").inner_size().unwrap();
    assert_eq!((size.x, size.y, size.z), (16, 6, 9));
}

#[test]
fn conventional_build_passes_lint() {
    let violations = lint::check(&blocks("alpha-cla"));
    let messages: Vec<String> =
        violations.iter().map(ToString::to_string).collect();
    assert!(messages.is_empty(), "{messages:#?}");
}

#[test]
fn lint_flags_outputs_that_are_not_lamps() {
    let messages: Vec<String> = lint::check(&blocks("alpha-rca"))
        .iter()
        .map(|v| v.message.clone())
        .collect();
    assert_eq!(
        messages,
        [
            "yellow concrete is not in the block palette (9 blocks)",
            "output is yellow concrete, not redstone lamps",
            "carry out is yellow concrete, not redstone lamps",
        ]
    );
}

#[test]
fn interface_names_operands_and_control_lines() {
    let interface::Interface { inputs, outputs } =
        interface::read(&blocks("alpha-cla"));
    assert_eq!(names(&inputs), [("A", 8), ("B", 8), ("carry in", 1)]);
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
    assert_eq!(inputs[0].bits[0].pos, [15, 6, 17]);
    assert!(inputs.iter().all(|port| port.lsb_known));
    assert!(inputs[2].bits[0].inverted);
    assert!(
        inputs[0]
            .bits
            .iter()
            .all(|bit| bit.entry.as_deref() == Some("south"))
    );
}

#[test]
fn drive_is_hard_when_dust_touches_the_input_block() {
    let cla = interface::read(&blocks("alpha-cla"));
    assert!(cla.inputs[0].bits.iter().all(|bit| bit.hard));
    let rca = interface::read(&blocks("alpha-rca"));
    assert!(rca.inputs[0].bits.iter().all(|bit| !bit.hard));
}
