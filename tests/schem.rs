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
fn conventional_ripple_carry_passes_lint() {
    let violations = lint::check(&blocks("alpha-rca"));
    let messages: Vec<String> =
        violations.iter().map(ToString::to_string).collect();
    assert!(messages.is_empty(), "{messages:#?}");
}

#[test]
fn lint_flags_blocks_outside_the_palette() {
    let messages: Vec<String> = lint::check(&blocks("vert-3t-cca"))
        .iter()
        .map(|v| v.message.clone())
        .collect();
    assert!(
        messages.contains(
            &"white stained glass is not in the block palette (58 blocks)"
                .to_owned()
        )
    );
}

#[test]
fn interface_names_operands_and_control_lines() {
    let interface::Interface { inputs, outputs } =
        interface::read(&blocks("alpha-cla"));
    assert_eq!(names(&inputs), [("A", 8), ("B", 8), ("carry in", 1)]);
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
    assert_eq!(inputs[0].bits[0].pos, [15, 6, 17]);
    assert_eq!(outputs[0].bits[0].pos, [18, 6, 0]);
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

#[test]
fn comparators_are_not_signs() {
    let blocks = blocks("vert-3t-cca");
    let mut texts: Vec<&str> = blocks
        .signs
        .values()
        .filter_map(|lines| lines.first().map(String::as_str))
        .collect();
    texts.sort_unstable();
    assert_eq!(texts, ["carry in", "cout"]);
}

#[test]
fn a_lever_on_top_enters_from_above() {
    let interface::Interface { inputs, .. } =
        interface::read(&blocks("vert-3t-cca"));
    assert_eq!(names(&inputs), [("A", 8), ("B", 8), ("carry in", 1)]);
    assert!(
        inputs[0]
            .bits
            .iter()
            .all(|bit| bit.entry.as_deref() == Some("up"))
    );
    // A vertical bus counts up from its bottom bit.
    assert_eq!(inputs[0].bits[0].pos[1], 1);
}

#[test]
fn thumbnail_is_a_png_of_the_set_size() {
    let png = app::thumbnail::render(&blocks("alpha-cla")).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    let expected =
        [app::thumbnail::WIDTH, app::thumbnail::HEIGHT].map(u32::from);
    assert_eq!([width, height], expected);
}
