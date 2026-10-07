//! Reading builds from their files, against the two stored adders.

use app::{
    interface, lint, models::schematic::StackDirection, schem::Blocks, stack,
};

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
    let messages: Vec<String> = lint::check(&blocks("quartz-rca-slice"))
        .iter()
        .map(|v| v.message.clone())
        .collect();
    assert!(
        messages.contains(
            &"quartz block is not in the block palette (20 blocks)".to_owned()
        ),
        "{messages:#?}"
    );
}

#[test]
fn a_named_output_leaves_the_bus() {
    let interface::Interface { outputs, .. } =
        interface::read(&blocks("diag-3t-cca"));
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
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

fn slice(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap()
}

fn direction(text: &str) -> StackDirection {
    text.parse().unwrap()
}

#[test]
fn stacked_slices_form_one_wider_build() {
    let plan = [(direction("west=2"), 8)];
    let (data, size) = stack::stack(&slice("alpha-rca-slice"), &plan).unwrap();
    assert_eq!(size, [16, 6, 9]);
    let blocks = Blocks::read(&data).unwrap();
    let messages: Vec<String> = lint::check(&blocks)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(messages.is_empty(), "{messages:#?}");
    let interface::Interface { inputs, outputs } = interface::read(&blocks);
    assert_eq!(names(&inputs), [("A", 8), ("B", 8), ("carry in", 1)]);
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
}

#[test]
fn a_register_stacks_two_ways() {
    let plan = [
        (direction("west=2,max=7"), 7),
        (direction("north=4,max=3"), 3),
    ];
    let (data, size) = stack::stack(&slice("alpha-register"), &plan).unwrap();
    assert_eq!(size, [14, 12, 12]);
    let interface::Interface { inputs, .. } =
        interface::read(&Blocks::read(&data).unwrap());
    // Each of the three registers keeps its own read and write lines.
    let controls = inputs
        .iter()
        .filter(|p| p.name == "read" || p.name == "write");
    assert_eq!(controls.count(), 6);
}

#[test]
fn stacking_respects_the_limit() {
    let west = direction("west=2,max=4");
    assert!(stack::stack(&slice("alpha-rca-slice"), &[(west, 4)]).is_ok());
    assert!(stack::stack(&slice("alpha-rca-slice"), &[(west, 5)]).is_err());
}

#[test]
fn a_wrong_step_collides() {
    let plan = [(direction("west=1"), 2)];
    let err = stack::stack(&slice("alpha-rca-slice"), &plan).unwrap_err();
    assert!(err.to_string().contains("collide"), "{err}");
}

#[test]
fn shell_holds_only_the_interface() {
    let messages: Vec<String> = lint::check(&blocks("alpha-register"))
        .iter()
        .map(ToString::to_string)
        .collect();
    // The wall torches on the read and write blocks are allowed; the white
    // block beside them, and the torch standing on it, are not.
    assert_eq!(
        messages,
        [
            "(2, 8, 2): white concrete in the shell; only levers, signs, \
             input blocks, and torches on input blocks belong there",
            "(2, 9, 2): torch in the shell is not on an input block",
        ]
    );
}

#[test]
fn standardizing_whitens_the_build_and_adds_markers() {
    let data =
        app::standardize::standardize(&slice("threshold-3-raw")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let messages: Vec<String> = lint::check(&blocks)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(messages.is_empty(), "{messages:#?}");
    assert!(blocks.iter().all(|(_, state)| !state.contains("quartz")));
    let interface::Interface { inputs, outputs } = interface::read(&blocks);
    assert_eq!(names(&inputs), [("A", 8)]);
    assert_eq!(names(&outputs), [("output", 1)]);
    assert_eq!(outputs[0].block, "yellow stained glass");
}

#[test]
fn standardizing_twice_gives_the_same_bytes() {
    let raw = slice("threshold-3-raw");
    let first = app::standardize::standardize(&raw).unwrap();
    let second = app::standardize::standardize(&raw).unwrap();
    assert_eq!(first, second);
}

#[test]
fn stacks_are_capped() {
    let plan = [(direction("west=2"), 65)];
    let err = stack::stack(&slice("alpha-rca-slice"), &plan).unwrap_err();
    assert!(err.to_string().contains("at most 64"), "{err}");
}
