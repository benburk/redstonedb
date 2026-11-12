//! Reading builds from their files, against the two stored adders.

use std::collections::{BTreeMap, BTreeSet};

use app::{
    category::Kind,
    import,
    ingest::standardize,
    interface::{self, Orientation},
    schem::{self, Blocks, Direction, lines::Role},
    simulate,
    stack::{self, StackDirection},
};

fn raw(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable")
}

/// A fixture as the site stores it: markers out, box fitted, and status
/// lines pink, since the fixtures predate that.
fn slice(name: &str) -> Vec<u8> {
    let tidied = standardize::tidy(&raw(name)).expect("fixture tidies");
    app::recognize::legacy::with_colour_roles(&tidied)
        .expect("fixture carries its roles")
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

/// Corner markers of stained glass are markers too: a hex adder's two of
/// light blue glass go, its box shrinking to the build, and the glass behind
/// one of its comparators, touching it, stays.
#[test]
fn tidying_takes_out_markers_of_any_glass() {
    let raw = std::fs::read(
        "tests/fixtures/hex-adder-4-operands-in-one-column.schem",
    )
    .unwrap();
    let stained = |blocks: &Blocks| {
        blocks
            .iter()
            .filter(|(_, state)| state.ends_with("stained_glass"))
            .count()
    };
    let before = Blocks::read(&raw).unwrap();
    let after = Blocks::read(&standardize::tidy(&raw).unwrap()).unwrap();
    assert_eq!((stained(&before), stained(&after)), (3, 1));
    assert!((0..3).all(|axis| after.size[axis] <= before.size[axis]));
    assert!(after.size != before.size);
}

/// A 1-bit adder stacking 2 west, its carry in on its east face, which
/// only the first copy keeps: the glass goes at two opposite corners of
/// the rest, where nothing was, and nothing else changes.
#[test]
fn stacking_markers_sit_at_opposite_corners_of_the_box() {
    let data = slice("rca-1-carry-in-east");
    let west = StackDirection {
        west: 2,
        up: 0,
        max_copies: None,
        faces: None,
        pitch: None,
    };
    let marked = stack::with_markers(&data, west).unwrap().unwrap();
    let (before, after) =
        (Blocks::read(&data).unwrap(), Blocks::read(&marked).unwrap());
    assert_eq!(after.size, before.size);
    let placed: Vec<_> = after
        .iter()
        .filter(|&(pos, state)| state != before.get(pos))
        .collect();
    assert!(placed.iter().all(|&(pos, state)| state == "minecraft:glass"
        && before.get(pos) == schem::AIR));
    let [(near, _), (far, _)] = placed.as_slice() else {
        panic!("{placed:?}");
    };
    assert!((0..3).all(|axis| near[axis] != far[axis]));
    // The east layer, with the carry in, is left out of the box.
    let span =
        |axis: usize| [near[axis].min(far[axis]), near[axis].max(far[axis])];
    let size = before.size;
    assert_eq!(span(0), [0, size[0] - 2]);
    assert_eq!(span(1), [0, size[1] - 1]);
    assert_eq!(span(2), [0, size[2] - 1]);
}

#[test]
fn a_named_output_leaves_the_bus() {
    let interface::Interface { outputs, .. } =
        interface::read(&blocks("diag-3t-cca"));
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
}

#[test]
fn interface_names_operands_and_control_lines() {
    let interface::Interface {
        inputs, outputs, ..
    } = interface::read(&blocks("alpha-cla"));
    assert_eq!(names(&inputs), [("A", 8), ("B", 8), ("carry in", 1)]);
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
    assert_eq!(inputs[0].bits[0].pos, [16, 6, 18]);
    assert_eq!(outputs[0].bits[0].pos, [19, 6, 0]);
    assert!(inputs[2].bits[0].inverted);
    assert!(
        inputs[0]
            .bits
            .iter()
            .all(|bit| bit.entry == Some(Direction::South))
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
            .all(|bit| bit.entry == Some(Direction::Up))
    );
    // A vertical bus counts up from its bottom bit.
    assert_eq!(inputs[0].bits[0].pos[1], 1);
}

#[test]
fn thumbnail_is_a_webp_of_the_set_size() {
    // Without the game's files, every block is a cube of one colour.
    struct Nothing;
    impl blockrender::Assets for Nothing {
        fn json(&self, _: &str) -> Option<serde_json::Value> {
            None
        }
        fn texture(&self, _: &str) -> Option<blockrender::Pixmap> {
            None
        }
    }
    let renderer = blockrender::Renderer::new(Nothing);
    let webp =
        app::thumbnail::render_with(&blocks("alpha-cla"), &renderer).unwrap();
    assert_eq!((&webp[0..4], &webp[8..12]), (&b"RIFF"[..], &b"WEBP"[..]));
}

fn direction(text: &str) -> StackDirection {
    text.parse().unwrap()
}

#[test]
fn stacked_slices_form_one_wider_build() {
    let plan = [(direction("west=2"), 8)];
    let (data, size) =
        stack::stack(&slice("alpha-rca-slice"), &plan, stack::Buses::own())
            .unwrap();
    assert_eq!(size, [16, 6, 9]);
    let blocks = Blocks::read(&data).unwrap();
    let interface::Interface {
        inputs, outputs, ..
    } = interface::read(&blocks);
    assert_eq!(names(&inputs), [("A", 8), ("B", 8), ("carry in", 1)]);
    assert_eq!(names(&outputs), [("output", 8), ("carry out", 1)]);
    // Each copy's carry in hangs a torch on the previous copy's carry out
    // block, and its sign covers the repeater reading that block, so only
    // the last copy's carry out is read, and the stack adds.
    let report = simulate::Test::Adder.run(&blocks);
    assert!(report.passed(), "{report:?}");
}

#[test]
fn a_register_stacks_two_ways() {
    let plan = [
        (direction("west=2,max=7"), 7),
        (direction("up=12,max=3"), 3),
    ];
    let (data, size) = stack::stack(
        &slice("alpha-register"),
        &plan,
        stack::Buses::of(Kind::Register.copies(), Orientation::Horizontal),
    )
    .unwrap();
    // Seven 2-wide copies, and the first copy's feed for the control lines,
    // three rows of them, each 12 high.
    assert_eq!(size, [15, 36, 4]);
    let interface::Interface { inputs, .. } =
        interface::read(&Blocks::read(&data).unwrap());
    // Its read and write lines sit inside the box, so every copy keeps
    // its own: its copies west are no wider register, and each of the 21
    // has its read and write.
    let controls = inputs
        .iter()
        .filter(|p| p.name == "read" || p.name == "write");
    assert_eq!(controls.count(), 42);
}

#[test]
fn stacking_respects_the_limit() {
    let west = direction("west=2,max=4");
    assert!(
        stack::stack(
            &slice("alpha-rca-slice"),
            &[(west, 4)],
            stack::Buses::own()
        )
        .is_ok()
    );
    assert!(
        stack::stack(
            &slice("alpha-rca-slice"),
            &[(west, 5)],
            stack::Buses::own()
        )
        .is_err()
    );
}

/// A later copy covers the copy before, as `//rstack` pastes it, even
/// where they disagree, as a wrong step's do: 1 up here stacks, and its
/// review lists the spots where the later copy puts another block over the
/// copy before's, as its inputs and outputs over the next bit's.
#[test]
fn a_wrong_step_shows_where_copies_disagree() {
    let plan = [(direction("up=1"), 2)];
    assert!(
        stack::stack(&slice("alpha-rca-slice"), &plan, stack::Buses::own())
            .is_ok()
    );
    let review = stack::review(
        &slice("alpha-rca-slice"),
        direction("up=1"),
        stack::Buses::own(),
    )
    .unwrap();
    assert!(!review.disagreements.is_empty());
}

#[test]
fn standardizing_keeps_the_authors_blocks() {
    let raw = raw("threshold-3-raw");
    let data = standardize::standardize(&raw).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let quartz = |blocks: &Blocks| {
        blocks
            .iter()
            .filter(|(_, state)| state.contains("quartz"))
            .count()
    };
    assert!(quartz(&blocks) > 0);
    assert_eq!(quartz(&blocks), quartz(&Blocks::read(&raw).unwrap()));
    let interface::Interface {
        inputs, outputs, ..
    } = interface::read(&blocks);
    assert_eq!(names(&inputs), [("A", 8)]);
    assert_eq!(names(&outputs), [("output", 1)]);
    // Its output is read by the repeater tapping it, its role an output's
    // by its shape, as no probe has read it.
    assert_eq!(outputs[0].role, Some(Role::Output(0)));
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
    let err =
        stack::stack(&slice("alpha-rca-slice"), &plan, stack::Buses::own())
            .unwrap_err();
    assert!(err.to_string().contains("at most 64"), "{err}");
}

#[test]
fn a_layer_of_only_inputs_does_not_count_toward_size() {
    // The raw file also has misplaced marker glass, which standardizing
    // drops rather than keeping as part of the build.
    let data = standardize::standardize(&raw("cca-16-raw")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let size = import::measure(&blocks, true).unwrap();
    assert_eq!((size.x, size.y, size.z), (7, 35, 8));
    let all_layers = blocks.full_size();
    assert!(all_layers.z > size.z, "the input layer is left out");
}

#[test]
fn a_control_line_may_use_a_button() {
    let data = standardize::standardize(&raw("mult-8-buttons")).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let interface::Interface {
        inputs, outputs, ..
    } = interface::read(&blocks);
    let mut controls = names(&inputs);
    controls.sort_unstable();
    assert_eq!(
        controls,
        [("A", 8), ("B", 8), ("calculate", 1), ("reset", 1)]
    );
    assert_eq!(names(&outputs), [("output", 8)]);
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

/// The multiplier fixture standardized, carrying the roles its colours
/// mark.
fn multiplier() -> Vec<u8> {
    let standard = standardize::standardize(&raw("mult-8-buttons")).unwrap();
    app::recognize::legacy::with_colour_roles(&standard).unwrap()
}

/// The file `data` with the blocks of `at` given `role`.
fn given(data: &[u8], at: &[app::schem::Pos], role: Role) -> Vec<u8> {
    let mut roles = Blocks::read(data).unwrap().roles.unwrap_or_default();
    roles.mark(role, at);
    app::schem::lines::with(data, Some(&roles)).unwrap()
}

#[test]
fn a_role_with_a_sign_is_one_named_operand() {
    // The reset line's block becomes a third operand, named by its sign,
    // beside A and B.
    let data = multiplier();
    let reset = interface::read(&Blocks::read(&data).unwrap())
        .inputs
        .into_iter()
        .find(|port| port.name == "reset")
        .unwrap()
        .bits[0]
        .pos;
    let data = given(&data, &[reset], Role::Operand(2));
    let inputs = interface::read(&Blocks::read(&data).unwrap()).inputs;
    let mut found = names(&inputs);
    found.sort_unstable();
    assert_eq!(found, [("A", 8), ("B", 8), ("calculate", 1), ("reset", 1)]);
    let reset = inputs.iter().find(|port| port.name == "reset").unwrap();
    assert!(!reset.labelled, "a bus, though a sign names it");
    assert_eq!(reset.role, Some(Role::Operand(2)));
}

#[test]
fn standardizing_keeps_the_authors_colours() {
    // Light blue becomes cyan terracotta, which stays cyan terracotta, its
    // levers an input still.
    let data = rebuilt(&raw("mult-8-buttons"), |_, state| {
        (state == "minecraft:light_blue_concrete")
            .then(|| "minecraft:cyan_terracotta".to_owned())
    });
    let standardized = standardize::standardize(&data).unwrap();
    let blocks = Blocks::read(&standardized).unwrap();
    let terracotta = |blocks: &Blocks| {
        blocks
            .iter()
            .filter(|(_, state)| *state == "minecraft:cyan_terracotta")
            .count()
    };
    let before = terracotta(&Blocks::read(&data).unwrap());
    assert!(before >= 8);
    assert_eq!(terracotta(&blocks), before);
    let inputs = interface::read(&blocks).inputs;
    assert!(
        inputs
            .iter()
            .any(|port| port.bits.len() == 8 && !port.labelled)
    );
}

#[test]
fn each_output_role_is_a_bus_in_order() {
    // The output's upper four bits become a second output bus: two buses,
    // the first first.
    let data = multiplier();
    let output = interface::read(&Blocks::read(&data).unwrap())
        .outputs
        .into_iter()
        .find(|port| port.name == "output")
        .unwrap();
    let upper: Vec<_> = output.bits[4..].iter().map(|bit| bit.pos).collect();
    let data = given(&data, &upper, Role::Output(1));
    let outputs = interface::read(&Blocks::read(&data).unwrap()).outputs;
    let read: Vec<(&str, usize, Option<Role>)> = outputs
        .iter()
        .map(|p| (p.name.as_str(), p.bits.len(), p.role))
        .collect();
    assert_eq!(
        read,
        [
            ("output 1", 4, Some(Role::Output(0))),
            ("output 2", 4, Some(Role::Output(1))),
        ]
    );
}

#[test]
fn a_status_line_is_its_own_line() {
    let outputs = interface::read(&blocks("alpha-cla")).outputs;
    let carry = outputs.iter().find(|p| p.name == "carry out").unwrap();
    assert!(carry.labelled);
    assert_eq!(carry.role, Some(Role::Status));
}

#[test]
fn a_comparator_with_air_behind_is_a_signal_strength_input() {
    // A hex digit to four bits: one comparator with air behind it to the
    // south, on lime concrete, feeding dust into comparators. No lever
    // could give its value.
    let data = standardize::standardize(&raw("hex2bin-raw")).unwrap();
    let data = app::recognize::legacy::with_colour_roles(&data).unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let interface = interface::read(&blocks);
    assert_eq!(names(&interface.inputs), [("A", 1)]);
    let a = &interface.inputs[0];
    assert!(a.signal_strength);
    assert_eq!(a.role, Some(Role::Operand(0)));
    assert_eq!(a.bits[0].entry, Some(Direction::South));
    assert!(!a.bits[0].hard);
    assert_eq!(names(&interface.outputs), [("output", 4)]);
    assert_eq!(import::guess(&interface).0, Some(4));
    // The comparator's layer is left out; the dust it feeds, on the block
    // that was the input before the build took a comparator, is the
    // build's.
    let size = import::measure(&blocks, true).unwrap();
    assert_eq!(
        (size.x, size.y, size.z),
        (8, 3, 12),
        "the input layer is left out"
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
    let (stacked, _) =
        stack::stack(&slice, &plan, stack::Buses::own()).unwrap();
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
    let prepared = import::prepare_for(
        &data,
        import::Purpose::InPlace {
            standardize: false,
            conventional: true,
            keep_paste: false,
        },
    )
    .unwrap();
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
fn cauldrons_and_cakes_are_swapped_for_barrels() {
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
}

/// Birch signs noting the day it was built stand on its target blocks.
/// They label nothing, so standardizing takes them out, as redstone passes
/// a sign by as it does air, rather than turning them into concrete.
#[test]
fn standardizing_takes_out_signs_that_label_nothing() {
    let raw = std::fs::read("tests/fixtures/adder-8-date-signs.schem").unwrap();
    let blocks =
        Blocks::read(&standardize::standardize(&raw).unwrap()).unwrap();
    assert!(blocks.signs.is_empty());
}

/// Its sum's repeater, on yellow concrete, sticks out a layer to the east,
/// and its carry out's a layer to the north, beside the block two layers
/// up whose dust it reads, as its inputs stick out to the south; the
/// layers of outputs and inputs don't count toward its size, but the dust
/// its carry out reads, and the block under it, do. Stacking still goes by
/// the core, which keeps the outputs.
#[test]
fn an_output_sticking_out_is_not_measured() {
    let data =
        std::fs::read("tests/fixtures/rca-1-vert-output-out.schem").unwrap();
    let blocks = app::schem::Blocks::read(&data).unwrap();
    let size = app::import::measure(&blocks, true).unwrap();
    assert_eq!((size.y, size.x, size.z), (6, 4, 6));
    let (low, high) = app::interface::core(&blocks);
    assert_eq!(high[0] - low[0] + 1, 5, "the core keeps the output");
}

/// Tidying faces every barrel up and every hopper down, where their facing
/// does nothing, so files that differ only in that compare alike; a hopper
/// pointing into a container would fill it, so keeps its facing.
#[test]
fn tidying_faces_barrels_up_and_hoppers_down() {
    let tidied = standardize::tidy(&raw("register-8-hoppers")).unwrap();
    let blocks = Blocks::read(&tidied).unwrap();
    let facings: Vec<(&str, Option<&str>)> = blocks
        .iter()
        .filter_map(|(_, state)| {
            let id = app::schem::id(state).trim_start_matches("minecraft:");
            matches!(id, "barrel" | "hopper")
                .then(|| (id, app::schem::property(state, "facing")))
        })
        .collect();
    assert!(!facings.is_empty());
    assert!(facings.iter().all(|&(id, facing)| match id {
        "barrel" => facing == Some("up"),
        _ => facing == Some("down"),
    }));
}

/// The 3D viewer's copy of a file turns its wall signs facing east or west
/// the other way, which the viewer turns back, and keeps every other block.
#[test]
fn the_viewers_copy_turns_east_and_west_wall_signs() {
    let mut turned = 0;
    for entry in std::fs::read_dir("tests/fixtures").unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            continue;
        }
        let data = std::fs::read(path).unwrap();
        let before = Blocks::read(&data).unwrap();
        let after = Blocks::read(&schem::for_viewer(&data).unwrap()).unwrap();
        for (pos, state) in before.iter() {
            let wall_sign = schem::id(state).ends_with("_wall_sign");
            let expected = match schem::property(state, "facing") {
                Some("east") if wall_sign => {
                    turned += 1;
                    state.replace("facing=east", "facing=west")
                }
                Some("west") if wall_sign => {
                    turned += 1;
                    state.replace("facing=west", "facing=east")
                }
                _ => state.to_owned(),
            };
            assert_eq!(after.get(pos), expected, "{pos:?}");
        }
        assert_eq!(after.signs.len(), before.signs.len());
    }
    assert!(turned > 0, "no fixture has an east or west wall sign");
}

/// An adder's lone unsigned magenta and pink lines show as its carry in
/// and carry out, as the simulator takes them; another category keeps
/// them unnamed.
#[test]
fn an_adders_unsigned_carry_lines_are_named() {
    let mut renamed = 0;
    for name in ["alpha-rca-slice", "rca-slice-capped", "cout-8"] {
        let blocks = Blocks::read(&raw(name)).unwrap();
        let read = interface::read(&blocks);
        let unnamed = |ports: &[interface::Port]| {
            ports
                .iter()
                .filter(|p| p.name.starts_with("unnamed"))
                .count()
        };
        let before = unnamed(&read.inputs) + unnamed(&read.outputs);
        let mut adder = interface::read(&blocks);
        interface::name_lines(Kind::Adder.lines(), &blocks, &mut adder);
        let mut other = interface::read(&blocks);
        interface::name_lines(interface::Lines::Signed, &blocks, &mut other);
        assert_eq!(
            unnamed(&adder.inputs) + unnamed(&adder.outputs),
            0,
            "{name}"
        );
        assert_eq!(unnamed(&other.inputs) + unnamed(&other.outputs), before);
        if before > 0 {
            renamed += 1;
            let names: Vec<&str> = adder
                .inputs
                .iter()
                .chain(&adder.outputs)
                .map(|p| p.name.as_str())
                .collect();
            assert!(
                names.iter().any(|n| *n == "carry in" || *n == "carry out"),
                "{name}: {names:?}"
            );
        }
    }
    assert!(renamed > 0, "no fixture has an unsigned carry line");
}

/// A register's data buses show as data in and data out, and its unsigned
/// magenta lines as writes, each on a button, and reads, each on a lever,
/// as its test takes them.
#[test]
fn a_registers_lines_are_named_by_their_roles() {
    let blocks = Blocks::read(&raw("register-8-2-cells")).unwrap();
    let mut read = interface::read(&blocks);
    interface::name_lines(Kind::Register.lines(), &blocks, &mut read);
    let inputs: Vec<&str> =
        read.inputs.iter().map(|p| p.name.as_str()).collect();
    let outputs: Vec<&str> =
        read.outputs.iter().map(|p| p.name.as_str()).collect();
    println!("{inputs:?} {outputs:?}");
    assert!(inputs.contains(&"data in"), "{inputs:?}");
    assert_eq!(outputs, ["data out"]);
    assert_eq!(
        inputs.iter().filter(|n| n.starts_with("write")).count(),
        2,
        "{inputs:?}"
    );
    assert_eq!(
        inputs.iter().filter(|n| n.starts_with("read")).count(),
        2,
        "{inputs:?}"
    );
}

/// `name` with its signs taken off: their blocks turned to air and their
/// text dropped.
fn unsigned(name: &str) -> Vec<u8> {
    let data = raw(name);
    let blocks = Blocks::read(&data).unwrap();
    let grid: std::collections::HashMap<schem::Pos, String> = blocks
        .iter()
        .filter(|(_, state)| !schem::id(state).ends_with("sign"))
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let mut nbt = schem::decode(&data).unwrap();
    nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, Vec::new())
        .unwrap();
    schem::encode(nbt).unwrap()
}

/// An incrementer's lone control line needs no sign: the conventions let
/// it pass, the page names it increment, and the simulation takes it so.
#[test]
fn an_incrementers_unsigned_line_is_its_increment() {
    let data = unsigned("inc-8");
    let blocks = Blocks::read(&data).unwrap();
    let read = interface::read(&blocks);
    assert!(read.inputs.iter().any(|p| p.name == "unnamed control"));
    let mut named = interface::read(&blocks);
    interface::name_lines(Kind::Incrementer.lines(), &blocks, &mut named);
    assert!(named.inputs.iter().any(|p| p.name == "increment"));
    let simulate::Report::Ran(run) = simulate::Test::Incrementer.run(&blocks)
    else {
        panic!("the incrementer was not simulated");
    };
    assert_eq!((run.pattern.as_str(), run.wrong), ("A + increment", 0));
}

/// Signs whose words sit in text components' `extra` parts name their
/// lines, and the file the site writes keeps each line as plain text.
#[test]
fn signs_written_as_text_components_are_read_and_written_plain() {
    let raw = raw("alu-8-extra-signs");
    for standardized in [false, true] {
        let prepared = import::prepare_for(
            &raw,
            if standardized {
                import::Purpose::Standard { keep_paste: false }
            } else {
                import::Purpose::InPlace {
                    standardize: false,
                    conventional: true,
                    keep_paste: false,
                }
            },
        )
        .unwrap();
        let read = interface::read(&prepared.blocks);
        let names: Vec<&str> =
            read.inputs.iter().map(|p| p.name.as_str()).collect();
        assert!(
            names.contains(&"shr")
                && !names.iter().any(|n| n.starts_with("unnamed")),
            "{names:?}"
        );
        let mut nbt = schem::decode(&prepared.data).unwrap();
        let signs: Vec<String> = nbt
            .take_entities()
            .iter()
            .map(|e| format!("{e:?}"))
            .filter(|e| e.contains("sign"))
            .collect();
        assert!(!signs.is_empty());
        for sign in signs {
            assert!(!sign.contains("extra"), "{sign}");
        }
    }
}

/// Blocks are counted by kind: a torch on a wall or the floor as one,
/// dust whatever its shape, every see-through block as one and every
/// other solid block as one; air and signs not at all.
#[test]
fn blocks_are_counted_by_kind() {
    for (name, kind) in [
        ("redstone_wall_torch", Some("Torch")),
        ("redstone_torch", Some("Torch")),
        ("redstone_wire", Some("Dust")),
        ("white_stained_glass", Some("Transparent")),
        ("smooth_stone_slab", Some("Transparent")),
        ("lime_concrete", Some("Solid")),
        ("stone", Some("Solid")),
        ("stone_button", Some("Button")),
        ("oak_wall_sign", None),
        ("air", None),
    ] {
        assert_eq!(schem::block::block_kind(name), kind, "{name}");
    }
    let counts = schem::block::block_counts(&blocks("alpha-rca"));
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    assert!(total > 0);
    assert!(counts.windows(2).all(|w| w[0].1 >= w[1].1), "most first");
}

/// The register fixture standardized, as the simulator takes it.
fn register() -> Vec<u8> {
    standardize::standardize(&raw("register-8-2-cells")).unwrap()
}

/// The file of the standardized register `raw` with its levers on
/// `colours` of concrete switched on and every component at rest with
/// them.
fn with_levers_on(raw: &[u8], colours: &[&str]) -> Vec<u8> {
    let blocks = Blocks::read(raw).unwrap();
    let on: BTreeSet<schem::Pos> = blocks
        .switches()
        .into_iter()
        .filter(|switch| {
            !switch.button
                && colours.iter().any(|colour| {
                    blocks.get(switch.block)
                        == format!("minecraft:{colour}_concrete")
                })
        })
        .map(|switch| switch.pos)
        .collect();
    assert!(!on.is_empty(), "it has levers on {colours:?}");
    standardize::with_levers(raw, &on).unwrap()
}

/// Whether each lever of the inputs of the file `data` is on, the data
/// inputs' and the control lines' apart.
fn input_levers(data: &[u8]) -> (Vec<bool>, Vec<bool>) {
    let blocks = Blocks::read(data).unwrap();
    let mut levers = (Vec::new(), Vec::new());
    for port in interface::read(&blocks).inputs {
        let control = port.control();
        for bit in &port.bits {
            let state = blocks.get(schem::step(bit.pos, bit.entry.unwrap()));
            if schem::short(state) != "lever" {
                continue;
            }
            let on = state.contains("powered=true");
            if control {
                levers.1.push(on);
            } else {
                levers.0.push(on);
            }
        }
    }
    levers
}

fn standardized(data: &[u8]) -> Vec<u8> {
    import::prepare_for(data, import::Purpose::Standard { keep_paste: false })
        .unwrap()
        .data
}

/// Standardizing a file twice gives the same bytes, its barrels' contents
/// included, so one build always comes out as one file.
#[test]
fn standardizing_is_the_same_each_time() {
    let raw = raw("register-8-2-cells");
    assert_eq!(standardized(&raw), standardized(&raw));
}

/// Saved with its data levers on, an uploaded build is kept with them off
/// and its circuit at rest with them: the same file as one saved with them
/// off, as a data input never reads inverted.
#[test]
fn an_uploads_data_levers_are_switched_off() {
    let saved_off = register();
    let saved_on = with_levers_on(&saved_off, &["lime"]);
    let as_saved = Blocks::read(&saved_on).unwrap();
    let off = Blocks::read(&saved_off).unwrap();
    assert!(
        as_saved.iter().any(|(pos, state)| {
            schem::short(state) == "redstone_wire" && off.get(pos) != state
        }),
        "its dust is powered with them"
    );
    let kept = standardized(&saved_on);
    let (data, control) = input_levers(&kept);
    assert!(!data.is_empty() && !control.is_empty());
    assert!(data.iter().all(|on| !on), "{data:?}");
    assert_eq!(kept, standardized(&saved_off));
}

/// A control line's levers are left as saved, as which read inverted is
/// for its test to find, while its data levers are switched off.
#[test]
fn an_uploads_control_levers_are_left_as_saved() {
    let register = register();
    let saved_on = with_levers_on(&register, &["lime", "magenta"]);
    let (data, control) = input_levers(&standardized(&saved_on));
    assert!(data.iter().all(|on| !on), "{data:?}");
    assert!(!control.is_empty() && control.iter().all(|on| *on));
    let control_on = with_levers_on(&register, &["magenta"]);
    assert_eq!(standardized(&saved_on), standardized(&control_on));
}

/// A build that never comes to rest, here for a torch clock set beside it,
/// keeps its data levers as saved.
#[test]
fn an_upload_that_cannot_settle_keeps_its_levers() {
    let saved_on = with_levers_on(&register(), &["lime"]);
    let size = Blocks::read(&saved_on).unwrap().size;
    // A torch on the side of a block, powering dust that runs round to a
    // repeater back into that block, all on a floor a block above the
    // build.
    let (floor, y) = (size[1] + 1, size[1] + 2);
    let mut clock: BTreeMap<schem::Pos, String> = BTreeMap::new();
    for x in 0..3 {
        for z in 0..3 {
            clock.insert([x, floor, z], "minecraft:stone".to_owned());
        }
    }
    let dust = "minecraft:redstone_wire[east=side,north=side,power=0,\
                south=side,west=side]";
    clock.insert([0, y, 0], "minecraft:stone".to_owned());
    clock.insert(
        [1, y, 0],
        "minecraft:redstone_wall_torch[facing=east,lit=true]".to_owned(),
    );
    for pos in [[2, y, 0], [2, y, 1], [2, y, 2], [1, y, 2], [0, y, 2]] {
        clock.insert(pos, dust.to_owned());
    }
    clock.insert(
        [0, y, 1],
        "minecraft:repeater[delay=1,facing=south,locked=false,\
         powered=false]"
            .to_owned(),
    );
    let (ticking, _) = schem::edit_grown(&saved_on, &clock).unwrap();
    assert!(
        standardize::with_levers(&ticking, &BTreeSet::new()).is_err(),
        "the clock never comes to rest"
    );
    let (data, _) = input_levers(&standardized(&ticking));
    assert!(!data.is_empty() && data.iter().all(|on| *on), "{data:?}");
}
