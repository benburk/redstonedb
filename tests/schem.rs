//! Reading builds from their files, against the two stored adders.

use app::{
    import, interface, lint, models::schematic::StackDirection, schem::Blocks,
    stack, standardize,
};

fn raw(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable")
}

/// A fixture as the site stores it: markers out, box fitted.
fn slice(name: &str) -> Vec<u8> {
    standardize::tidy(&raw(name)).expect("fixture tidies")
}

fn blocks(name: &str) -> Blocks {
    Blocks::read(&slice(name)).expect("fixture parses")
}

fn glass(blocks: &Blocks) -> usize {
    blocks
        .iter()
        .filter(|(_, state)| *state == "minecraft:glass")
        .count()
}

fn names(ports: &[interface::Port]) -> Vec<(&str, usize)> {
    ports
        .iter()
        .map(|port| (port.name.as_str(), port.bits.len()))
        .collect()
}

#[test]
fn size_leaves_out_the_interface_layers() {
    let size = import::measure(&blocks("alpha-cla"), true).unwrap();
    assert_eq!((size.x, size.y, size.z), (19, 11, 18));
    let size = import::measure(&blocks("alpha-rca"), true).unwrap();
    assert_eq!((size.x, size.y, size.z), (16, 6, 9));
}

#[test]
fn tidying_takes_out_the_markers_and_fits_the_box() {
    let before = Blocks::read(&raw("alpha-cla")).unwrap();
    assert_eq!(glass(&before), 32);
    let after = blocks("alpha-cla");
    assert_eq!(glass(&after), 0);
    assert!((0..3).all(|axis| after.size[axis] < before.size[axis]));
    let again = standardize::tidy(&slice("alpha-cla")).unwrap();
    assert_eq!(again, slice("alpha-cla"));
}

#[test]
fn corner_markers_sit_one_block_off_each_corner() {
    let marked = standardize::with_corner_markers(&slice("alpha-cla")).unwrap();
    let blocks = Blocks::read(&marked).unwrap();
    assert_eq!(glass(&blocks), 8);
    let [w, h, l] = blocks.size;
    for x in [0, w - 1] {
        for y in [0, h - 1] {
            for z in [0, l - 1] {
                assert_eq!(blocks.get([x, y, z]), "minecraft:glass");
            }
        }
    }
    assert_eq!(standardize::tidy(&marked).unwrap(), slice("alpha-cla"));
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
    // Seven 2-wide copies, and the first copy's feed for the control lines.
    assert_eq!(size, [15, 12, 12]);
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
fn standardizing_whitens_the_build() {
    let data = standardize::standardize(&raw("threshold-3-raw")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    assert_eq!(glass(&blocks), 0);
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
    let raw = raw("threshold-3-raw");
    let first = standardize::standardize(&raw).unwrap();
    let second = standardize::standardize(&raw).unwrap();
    assert_eq!(first, second);
}

#[test]
fn stacks_are_capped() {
    let plan = [(direction("west=2"), 65)];
    let err = stack::stack(&slice("alpha-rca-slice"), &plan).unwrap_err();
    assert!(err.to_string().contains("at most 64"), "{err}");
}

#[test]
fn a_layer_of_only_inputs_does_not_count_toward_size() {
    // The raw file also has misplaced marker glass, which standardizing
    // drops rather than keeping as part of the build.
    let data = standardize::standardize(&raw("cca-16-raw")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    assert!(lint::check(&blocks).is_empty());
    let size = import::measure(&blocks, true).unwrap();
    assert_eq!((size.x, size.y, size.z), (7, 35, 8));
    let all_layers = blocks.full_size();
    assert!(all_layers.z > size.z, "the input layer is left out");
}

#[test]
fn a_control_line_may_use_a_button() {
    let data = standardize::standardize(&raw("mult-8-buttons")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let messages: Vec<String> = lint::check(&blocks)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(messages.is_empty(), "{messages:#?}");
    let interface::Interface { inputs, outputs } = interface::read(&blocks);
    let mut controls = names(&inputs);
    controls.sort_unstable();
    assert_eq!(
        controls,
        [("A", 8), ("B", 8), ("calculate", 1), ("reset", 1)]
    );
    assert_eq!(names(&outputs), [("output", 8)]);
}

#[test]
fn an_operand_may_not_use_a_button() {
    let data = standardize::standardize(&raw("mult-8-buttons")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let lever = blocks.switches().into_iter().find(|l| !l.button).unwrap();
    let pos = lever.pos;
    let state = blocks
        .get(pos)
        .replace("minecraft:lever", "minecraft:stone_button");
    let mut nbt = app::schem::decode(&data).unwrap();
    let mut grid: std::collections::HashMap<_, _> = blocks
        .iter()
        .filter(|(_, s)| *s != "minecraft:air")
        .map(|(p, s)| (p, s.to_owned()))
        .collect();
    grid.insert(pos, state);
    let entities = nbt.take_entities();
    nbt.write_blocks(blocks.size, &grid, entities).unwrap();
    let changed = Blocks::read(&app::schem::encode(nbt).unwrap()).unwrap();
    let messages: Vec<String> = lint::check(&changed)
        .iter()
        .map(|v| v.message.clone())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("button on an operand bit")),
        "{messages:#?}"
    );
}
