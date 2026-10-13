//! Reading builds from their files, against the two stored adders.

use app::{
    import, interface, lint, models::schematic::StackDirection, schem::Blocks,
    stack, standardize,
};

fn raw(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable")
}

/// A fixture as the site stores it: markers out, box fitted, and status
/// lines pink, since the fixtures predate that.
fn slice(name: &str) -> Vec<u8> {
    let tidied = standardize::tidy(&raw(name)).expect("fixture tidies");
    standardize::pink_status(&tidied).expect("fixture recolours")
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
    // Each copy's carry in hangs a torch on the previous copy's carry out,
    // which inverts it, so the sums come out right.
    let run = app::simulate::check_adder(&blocks).unwrap();
    assert_eq!((run.cases, run.wrong), (1 << 17, 0));
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
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
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

/// `data` with each block that `change` names replaced, signs and all kept.
fn rebuilt(
    data: &[u8],
    change: impl Fn(app::schem::Pos, &str) -> Option<String>,
) -> Vec<u8> {
    let blocks = Blocks::read(data).unwrap();
    let grid: std::collections::HashMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| *state != "minecraft:air")
        .map(|(pos, state)| {
            (pos, change(pos, state).unwrap_or_else(|| state.to_owned()))
        })
        .collect();
    let mut nbt = app::schem::decode(data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    app::schem::encode(nbt).unwrap()
}

fn multiplier() -> Vec<u8> {
    standardize::standardize(&raw("mult-8-buttons")).unwrap()
}

#[test]
fn a_colour_with_a_sign_is_one_named_operand() {
    // The reset line's block becomes cyan: a third operand, named by its
    // sign, beside A and B.
    let data = multiplier();
    let reset = interface::read(&Blocks::read(&data).unwrap())
        .inputs
        .into_iter()
        .find(|port| port.name == "reset")
        .unwrap()
        .bits[0]
        .pos;
    let data = rebuilt(&data, |pos, _| {
        (pos == reset).then(|| "minecraft:cyan_concrete".to_owned())
    });
    let inputs = interface::read(&Blocks::read(&data).unwrap()).inputs;
    let mut found = names(&inputs);
    found.sort_unstable();
    assert_eq!(found, [("A", 8), ("B", 8), ("calculate", 1), ("reset", 1)]);
    let reset = inputs.iter().find(|port| port.name == "reset").unwrap();
    assert!(!reset.labelled, "a bus, though a sign names it");
    assert_eq!(reset.block, "cyan concrete");
}

#[test]
fn one_colour_takes_one_name() {
    let data = rebuilt(&multiplier(), |_, state| {
        (state == "minecraft:magenta_concrete")
            .then(|| "minecraft:cyan_concrete".to_owned())
    });
    let messages: Vec<String> = lint::check(&Blocks::read(&data).unwrap())
        .iter()
        .map(|v| v.message.clone())
        .collect();
    assert!(
        messages.iter().any(|m| m.starts_with("cyan is named both")),
        "{messages:#?}"
    );
}

#[test]
fn standardizing_keeps_a_levered_colour_as_concrete() {
    // Light blue becomes cyan terracotta; with levers on it, it stays an
    // input, as cyan concrete, and takes the letter after A.
    let data = rebuilt(&raw("mult-8-buttons"), |_, state| {
        (state == "minecraft:light_blue_concrete")
            .then(|| "minecraft:cyan_terracotta".to_owned())
    });
    let standardized = standardize::standardize(&data).unwrap();
    let blocks = Blocks::read(&standardized).unwrap();
    let inputs = interface::read(&blocks).inputs;
    let b = inputs.iter().find(|port| port.name == "B").unwrap();
    assert_eq!((b.block.as_str(), b.bits.len()), ("cyan concrete", 8));
    assert!(
        blocks
            .iter()
            .all(|(_, state)| !state.contains("terracotta"))
    );
}

#[test]
fn each_output_colour_is_a_bus_in_order() {
    // The output's upper four bits become orange: two buses, yellow first.
    let data = multiplier();
    let output = interface::read(&Blocks::read(&data).unwrap())
        .outputs
        .into_iter()
        .find(|port| port.name == "output")
        .unwrap();
    let upper: Vec<_> = output.bits[4..].iter().map(|bit| bit.pos).collect();
    let data = rebuilt(&data, |pos, _| {
        upper
            .contains(&pos)
            .then(|| "minecraft:orange_concrete".to_owned())
    });
    let outputs = interface::read(&Blocks::read(&data).unwrap()).outputs;
    let read: Vec<(&str, usize, &str)> = outputs
        .iter()
        .map(|p| (p.name.as_str(), p.bits.len(), p.block.as_str()))
        .collect();
    assert_eq!(
        read,
        [
            ("output 1", 4, "yellow concrete"),
            ("output 2", 4, "orange concrete"),
        ]
    );
}

#[test]
fn a_status_line_is_pink() {
    let outputs = interface::read(&blocks("alpha-cla")).outputs;
    let carry = outputs.iter().find(|p| p.name == "carry out").unwrap();
    assert!(carry.labelled);
    assert_eq!(carry.block, "pink concrete");
}

#[test]
fn a_yellow_status_line_is_flagged() {
    // Tidied but not recoloured, as an older file arrives.
    let tidied = standardize::tidy(&raw("alpha-cla")).unwrap();
    let messages: Vec<String> = lint::check(&Blocks::read(&tidied).unwrap())
        .iter()
        .map(|v| v.message.clone())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("carry out is a status output")),
        "{messages:#?}"
    );
}

/// The multiplier with B's levers moved to `side` of their blocks, or
/// `None` when something is in the way.
fn b_levers_on(side: &str) -> Option<Vec<u8>> {
    let data = multiplier();
    let blocks = Blocks::read(&data).unwrap();
    let b: Vec<_> = blocks
        .switches()
        .into_iter()
        .filter(|s| blocks.get(s.block) == "minecraft:light_blue_concrete")
        .collect();
    let mut grid: std::collections::HashMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| *state != "minecraft:air")
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    for lever in &b {
        grid.remove(&lever.pos);
    }
    for lever in &b {
        let pos = app::schem::step(lever.block, side);
        if grid.contains_key(&pos) {
            return None;
        }
        let state = match side {
            "up" => "minecraft:lever[face=floor,facing=north,powered=false]"
                .to_owned(),
            _ => format!(
                "minecraft:lever[face=wall,facing={side},powered=false]"
            ),
        };
        grid.insert(pos, state);
    }
    let mut nbt = app::schem::decode(&data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    Some(app::schem::encode(nbt).unwrap())
}

fn lever_complaints(data: &[u8]) -> Vec<String> {
    lint::check(&Blocks::read(data).unwrap())
        .iter()
        .map(|v| v.message.clone())
        .filter(|m| m.contains("lever is on"))
        .collect()
}

#[test]
fn only_a_levers_must_face_south() {
    let side = ["east", "west", "north"]
        .into_iter()
        .find_map(b_levers_on)
        .expect("a free side for B's levers");
    assert!(
        lever_complaints(&side).is_empty(),
        "{:#?}",
        lever_complaints(&side)
    );
    let top = b_levers_on("up").expect("room on top of B");
    let complaints = lever_complaints(&top);
    assert!(
        complaints
            .iter()
            .all(|m| m.starts_with("operand lever is on the top")),
        "{complaints:#?}"
    );
    assert_eq!(complaints.len(), 8);
}

#[test]
fn dust_on_an_input_block_is_a_signal_strength_input() {
    // A hex digit to four bits: one lime block with dust on top, its open
    // end south, feeding comparators. No lever could give its value.
    let data = standardize::standardize(&raw("hex2bin-raw")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let messages: Vec<String> = lint::check(&blocks)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(messages.is_empty(), "{messages:#?}");
    let interface = interface::read(&blocks);
    assert_eq!(names(&interface.inputs), [("A", 1)]);
    let a = &interface.inputs[0];
    assert!(a.signal_strength);
    assert_eq!(a.block, "lime concrete");
    assert_eq!(a.bits[0].entry.as_deref(), Some("south"));
    assert!(!a.bits[0].hard);
    assert_eq!(names(&interface.outputs), [("output", 4)]);
    assert_eq!(import::guess(&interface).0, Some(4));
    let size = import::measure(&blocks, true).unwrap();
    assert_eq!(
        (size.x, size.y, size.z),
        (8, 3, 11),
        "the input layer is left out"
    );
}

#[test]
fn signal_strength_dust_needs_an_open_end() {
    // Dust that turns back into the build, a dot, leaves no side to drive.
    let data = rebuilt(&raw("hex2bin-raw"), |pos, state| {
        (pos == [6, 1, 11] && state.starts_with("minecraft:redstone_wire"))
            .then(|| {
                "minecraft:redstone_wire[east=none,north=none,power=0,south=none,west=none]"
                    .to_owned()
            })
    });
    let messages: Vec<String> = lint::check(&Blocks::read(&data).unwrap())
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("no one open end")),
        "{messages:#?}"
    );
}

#[test]
fn a_paste_lands_south_of_the_south_east_bottom_corner() {
    // The uploader's file put the paste wherever they stood when copying.
    let data = app::schem::paste_from_corner(
        &standardize::tidy(&raw("alpha-cla")).unwrap(),
    )
    .unwrap();
    let [width, _, length] = Blocks::read(&data).unwrap().size;
    let nbt = app::schem::decode(&data).unwrap();
    let int = |value: Option<&fastnbt::Value>| match value {
        Some(fastnbt::Value::Int(n)) => *n,
        other => panic!("not an int: {other:?}"),
    };
    let Some(fastnbt::Value::Compound(metadata)) = nbt.body.get("Metadata")
    else {
        panic!("no metadata");
    };
    let paste =
        ["WEOffsetX", "WEOffsetY", "WEOffsetZ"].map(|k| int(metadata.get(k)));
    // From one block south of the south-east-bottom corner to the
    // north-west-bottom block.
    assert_eq!(paste, [-(width - 1), 0, -length]);
    let Some(fastnbt::Value::IntArray(world)) = nbt.body.get("Offset") else {
        panic!("no offset");
    };
    assert_eq!(world.iter().copied().collect::<Vec<_>>(), [0, 0, 0]);
}

/// The paste offset a version 2 file keeps: from the player to its box's
/// north-west-bottom block.
fn paste_offset(data: &[u8]) -> [i32; 3] {
    let nbt = app::schem::decode(data).unwrap();
    let Some(fastnbt::Value::Compound(metadata)) = nbt.body.get("Metadata")
    else {
        panic!("no metadata");
    };
    ["WEOffsetX", "WEOffsetY", "WEOffsetZ"].map(|key| match metadata.get(key) {
        Some(fastnbt::Value::Int(n)) => *n,
        other => panic!("not an int: {other:?}"),
    })
}

#[test]
fn a_stack_pastes_from_its_own_south_east_bottom_corner() {
    // Copies go west, north, and up from the first, which keeps the
    // south-east-bottom end, so the player stands as they did.
    let slice =
        app::schem::paste_from_corner(&slice("alpha-rca-slice")).unwrap();
    let plan = [(direction("west=2"), 4)];
    let (stacked, _) = stack::stack(&slice, &plan).unwrap();
    let [width, _, length] = Blocks::read(&stacked).unwrap().size;
    assert_eq!(paste_offset(&stacked), [-(width - 1), 0, -length]);
}

#[test]
fn tidying_keeps_where_a_file_pastes_from() {
    // Taking out corner markers moves the box's first block, and the paste
    // offset with it, so //paste still lands the build in the same place.
    let raw = raw("alpha-cla");
    let tidied = standardize::tidy(&raw).unwrap();
    let before = Blocks::read(&raw).unwrap();
    let after = Blocks::read(&tidied).unwrap();
    // A block's place relative to the player is the same in both files.
    let first = |blocks: &Blocks, offset: [i32; 3]| {
        let pos = blocks
            .iter()
            .find(|(_, state)| state.contains("lime_concrete"))
            .map(|(pos, _)| pos)
            .unwrap();
        [0, 1, 2].map(|axis| pos[axis] + offset[axis])
    };
    assert_eq!(
        first(&before, paste_offset(&raw)),
        first(&after, paste_offset(&tidied))
    );
}

/// The multiplier's furnaces each give a comparator 1ss; taking the file
/// in swaps each for a barrel giving the same.
#[test]
fn furnaces_are_swapped_for_barrels_of_the_same_strength() {
    let data = standardize::tidy(&raw("mult-8-diag-signed")).unwrap();
    let before = Blocks::read(&data).unwrap();
    let (furnace, _) = before
        .iter()
        .find(|(_, state)| state.starts_with("minecraft:furnace"))
        .unwrap();
    let prepared = import::prepare(&data, false, true, false).unwrap();
    assert_eq!(prepared.swapped, [("furnace".to_owned(), 36)]);
    let after = &prepared.blocks;
    assert!(after.iter().all(|(_, state)| !state.contains("furnace")));
    let strength = |blocks: &Blocks, pos| {
        app::schem::container_strength(
            &blocks.containers[&pos],
            app::schem::slots(blocks.get(pos)).unwrap(),
        )
    };
    assert_eq!(strength(&before, furnace), 1);
    // The box is fitted the same, so the barrel sits where the furnace did.
    assert_eq!(strength(after, furnace), 1);
}

/// Cauldrons and cakes give a comparator their signal strength, which a
/// barrel gives instead.
#[test]
fn cauldrons_and_cakes_break_the_conventions_until_swapped() {
    let data = raw("alpha-rca-slice");
    let blocks = Blocks::read(&data).unwrap();
    let mut grid: std::collections::HashMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| *state != app::schem::AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let [w, h, l] = blocks.size;
    // Each stand-in, at a spot beyond the build, and the strength it gives.
    let stand_ins = [
        ("minecraft:water_cauldron[level=2]", [w, h, l], 2),
        ("minecraft:cake[bites=2]", [w, h, l + 2], 10),
        ("minecraft:white_candle_cake[lit=false]", [w, h, l + 4], 14),
    ];
    for (state, pos, _) in stand_ins {
        grid.insert(pos, state.to_owned());
    }
    let mut nbt = app::schem::decode(&data).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], [w + 1, h + 1, l + 5], &grid, entities)
        .unwrap();
    let with_stand_ins = app::schem::encode(nbt).unwrap();
    let before = Blocks::read(&with_stand_ins).unwrap();
    let messages: Vec<String> = lint::check(&before)
        .iter()
        .map(ToString::to_string)
        .collect();
    for name in ["water cauldron", "cake", "white candle cake"] {
        assert!(
            messages
                .iter()
                .any(|m| m.contains(&format!(": {name} is not"))
                    && m.contains("barrel")),
            "{name}: {messages:#?}"
        );
    }
    assert_eq!(standardize::swappable(&before).stand_ins.len(), 3);
    let swapped =
        Blocks::read(&standardize::barrels(&with_stand_ins, &[]).unwrap())
            .unwrap();
    for (_, pos, strength) in stand_ins {
        assert!(swapped.get(pos).starts_with("minecraft:barrel"));
        assert_eq!(
            app::schem::container_strength(&swapped.containers[&pos], 27),
            strength
        );
    }
}

/// A multiplier whose buses run east as they climb, so every bit is as near
/// the south-east-bottom corner as any other; `lsb` and `msb` signs on
/// their ends say which way they count, and name nothing.
#[test]
fn lsb_and_msb_signs_say_which_way_a_bus_counts() {
    let blocks = blocks("mult-8-diag-signed");
    let interface::Interface { inputs, .. } = interface::read(&blocks);
    assert_eq!(names(&inputs), [("A", 8), ("B", 8)]);
    for port in &inputs {
        let first = port.bits.first().unwrap().pos;
        let last = port.bits.last().unwrap().pos;
        assert_ne!(interface::order_mark(&blocks, last), Some("lsb"));
        assert_ne!(interface::order_mark(&blocks, first), Some("msb"));
        // Each bit one step further from the least significant.
        let steps: Vec<i32> = port
            .bits
            .windows(2)
            .map(|pair| (pair[1].pos[1] - pair[0].pos[1]).abs())
            .collect();
        assert!(steps.iter().all(|&step| step == 2), "{steps:?}");
    }
    assert_eq!(
        interface::order_mark(&blocks, inputs[0].bits[0].pos),
        Some("lsb")
    );
    assert_eq!(
        interface::order_mark(&blocks, inputs[1].bits[7].pos),
        Some("msb")
    );
    let messages: Vec<String> = lint::check(&blocks)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        !messages.iter().any(|m| m.contains("sb sign")),
        "{messages:#?}"
    );
}
