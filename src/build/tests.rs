use super::*;
use crate::{
    category::Flag,
    models::tag::{self, Tag},
};

fn cla() -> Build {
    Build {
        id: String::new(),
        category: "adder".to_owned(),
        width: Some(8),
        orientation: Orientation::Horizontal,
        encoding: Some(Encoding::Binary),
        latency: Some(8),
        synchronized: Some(false),
        own_logic: None,
        throughput: None,
        at_least: None,
        pulse: None,
        torches: None,
        works_from: None,
        works_from_by: None,
        bytes: None,
        size: (19, 11, 18),
        stacking: Vec::new(),
        widened: Vec::new(),
        stacked_from: None,
        authors: vec!["embizone".to_owned()],
        contributors: Vec::new(),
        built_on: Some("2014-09-06".to_owned()),
        imported_by: None,
        conventional: true,
        keeps_paste: false,
        design: Some("carry_lookahead".to_owned()),
        flags: Vec::new(),
        description: None,
        given_name: None,
        uploader: None,
        uploader_name: None,
        sha256: String::new(),
        added_at: String::new(),
        edited_at: None,
        downloads: 0,
        favorites: 0,
        public: true,
        deleted: false,
        unverified: None,
        status: Status::default(),
        operands: Operands::Split,
        unaligned: false,
        tags: vec![Tag {
            name: "alpha".to_owned(),
            token: None,
            kind: tag::Kind::Restriction,
        }],
    }
}

/// Its design, orientation, and flags are among its tags, after failing
/// and before the rest, marked as found when the site works them out,
/// and as given for a build kept as built.
#[test]
fn a_build_s_design_orientation_and_flags_are_among_its_tags() {
    let mut s = cla();
    s.flags = vec![Flag {
        name: "carry_in",
        label: "Carry in",
    }];
    s.tags
        .insert(0, Tag::derived(tag::FAILING, tag::Kind::Failing));
    let tags = s.every_tag();
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "failing",
            "carry_lookahead",
            "horizontal",
            "carry_in",
            // Its outputs settle apart, as measured.
            "unsynced",
            "alpha"
        ]
    );
    let classes: Vec<&str> = tags.iter().map(Tag::class).collect();
    assert_eq!(
        classes,
        [
            "badge-failing",
            "badge-found",
            "badge-found",
            "badge-found",
            "badge-found",
            "badge-found"
        ]
    );
    s.conventional = false;
    let given: Vec<&str> = s.every_tag()[1..4].iter().map(Tag::class).collect();
    assert_eq!(given, ["badge-other", "badge-other", "badge-other"]);
}

/// A build stands by its current file's stored result whatever reason
/// was stored before it: a pass is passing, a stale reason or not; a
/// broken run is not tested, not failing; and with no result, the
/// reason decides, wrong outputs failing.
#[test]
fn a_build_stands_by_its_stored_result() {
    let stale = Some("Couldn't read its inputs and outputs");
    assert_eq!(Status::of(Some("ran"), false, stale), Status::Passing);
    assert_eq!(Status::of(Some("ran"), true, None), Status::Failing);
    assert_eq!(Status::of(Some("broken"), true, None), Status::NotTested);
    assert_eq!(Status::of(None, false, stale), Status::NotTested);
    assert_eq!(
        Status::of(None, false, Some("Gives wrong outputs for some inputs")),
        Status::Failing
    );
    assert_eq!(Status::of(None, false, None), Status::NoTest);
    // A skip says no more than the reason, as one off the list is only
    // tested to say what keeps it from being recognized.
    assert_eq!(Status::of(Some("skipped"), false, None), Status::NoTest);
}

/// A passing build kept as built, or with a reason stored against it,
/// lists but ranks on no board, saying why.
#[test]
fn a_passing_build_kept_as_built_lists_but_does_not_rank() {
    let mut s = cla();
    s.status = Status::Passing;
    s.conventional = false;
    s.unverified = Some("Couldn't read its inputs and outputs".to_owned());
    assert!(s.verified());
    assert!(!s.ranks());
    assert_eq!(s.unranked().as_deref(), Some("kept as built"));
}

/// An unaligned build is verified, and tagged so, but ranks on no
/// board.
#[test]
fn an_unaligned_build_is_verified_but_does_not_rank() {
    let mut s = cla();
    assert!(s.verified() && s.ranks());
    assert!(!s.every_tag().iter().any(|t| t.name == tag::UNALIGNED));
    s.unaligned = true;
    assert!(s.verified() && !s.ranks());
    assert!(s.every_tag().iter().any(|t| t.name == tag::UNALIGNED));
}

#[test]
fn title_reads_width_restrictions_design_then_category() {
    assert_eq!(cla().title(), "8-bit alpha Carry Lookahead Adder");
}

#[test]
fn filename_is_the_name() {
    assert_eq!(cla().filename(), "8_bit_Carry_Lookahead_Adder.schem");
}

#[test]
fn a_name_given_for_a_file_keeps_only_what_schemati_takes() {
    assert_eq!(file_stem("my adder (v2).schem"), "my_adder_v2");
    assert_eq!(file_stem("  __CCA-8__ "), "CCA_8");
    assert_eq!(file_stem("ünïcode"), "n_code");
    assert_eq!(file_stem("../"), "");
}

#[test]
fn a_value_parses_from_its_name_or_label() {
    assert_eq!("horz".parse(), Ok(Orientation::Horizontal));
    assert_eq!(" Vertical ".parse(), Ok(Orientation::Vertical));
    assert_eq!("HEX".parse(), Ok(Encoding::Hex));
    assert_eq!("stateful".parse(), Ok(Logic::Stateful));
    assert!("sideways".parse::<Orientation>().is_err());
}

#[test]
fn only_a_mod_a_confirmed_author_or_the_uploader_may_edit() {
    use crate::models::user::{Role, User};
    let user = |name: Option<&str>, verified, role| User {
        id: uuid::Uuid::nil(),
        discord_id: String::new(),
        username: "someone".to_owned(),
        display_name: None,
        avatar: None,
        role,
        minecraft_name: name.map(str::to_owned),
        minecraft_verified: verified,
        minecraft_uuid: None,
        created_at: String::new(),
    };
    let cla = cla();
    assert!(user(Some("EmbiZone"), true, Role::User).may_edit(&cla));
    assert!(!user(Some("embizone"), false, Role::User).may_edit(&cla));
    assert!(!user(Some("alex_you"), true, Role::User).may_edit(&cla));
    assert!(!user(None, false, Role::User).may_edit(&cla));
    assert!(user(None, false, Role::Mod).may_edit(&cla));
    let mut uploaded = cla;
    uploaded.uploader = Some("someone".to_owned());
    assert!(user(None, false, Role::User).may_edit(&uploaded));
}

#[test]
fn an_adder_that_subtracts_is_named_a_subtractor() {
    let mut subtractor = cla();
    subtractor.flags = vec![Flag {
        name: "subtract",
        label: "Subtract",
    }];
    assert_eq!(subtractor.name(), "8-bit Carry Lookahead Subtractor");
    assert_eq!(cla().name(), "8-bit Carry Lookahead Adder");
}

/// Each gate is a category of its own and is called by its label, as an
/// XNOR gate was once an XOR gate flagged `xnor`, and a flag of that
/// left on a build calls it nothing else.
#[test]
fn a_gate_is_named_by_its_category() {
    let gate = |category: &str, flag: Option<&'static str>| {
        let mut gate = cla();
        gate.category = category.to_owned();
        gate.flags = flag
            .map(|name| Flag { name, label: "" })
            .into_iter()
            .collect();
        gate.what()
    };
    assert_eq!(gate("xor_gate", None), "XOR Gate");
    assert_eq!(gate("xor_gate", Some("xnor")), "XOR Gate");
    assert_eq!(gate("xnor_gate", None), "XNOR Gate");
    assert_eq!(gate("nand_gate", None), "NAND Gate");
}

#[test]
fn a_multiplier_is_named_by_its_operands_and_product() {
    let flag = |name: &'static str| Flag { name, label: "" };
    let mut multiplier = Build {
        category: "multiplier".to_owned(),
        design: None,
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(multiplier.name(), "8x8=8 Multiplier");
    multiplier.flags = vec![flag("upper_product")];
    assert_eq!(multiplier.name(), "8x8=16 Multiplier");
    assert_eq!(multiplier.filename(), "8x8_16_Multiplier.schem");
    multiplier.flags.push(flag("fma"));
    assert_eq!(multiplier.name(), "8x8+8=16 Multiplier");
}

#[test]
fn a_memory_names_its_capacity_and_stacks_multiply_it() {
    let ram = Build {
        category: "ram".to_owned(),
        design: None,
        bytes: Some(32),
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(ram.name(), "8-bit 32-byte RAM");
    let up = StackDirection {
        west: 0,
        up: 3,
        max_copies: None,
        faces: None,
        pitch: None,
    };
    let stacked = ram.stacked(&[(up, 4)], [19, 11, 72]);
    assert_eq!(stacked.bytes, Some(128));
    assert_eq!(stacked.width, Some(8), "copies up add words, not bits");
}

#[test]
fn a_category_label_keeps_its_symbols_out_of_the_filename() {
    let s = Build {
        category: "inc_dec".to_owned(),
        design: None,
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(s.name(), "8-bit Inc/Dec");
    assert_eq!(s.filename(), "8_bit_Inc_Dec.schem");
}

#[test]
fn a_design_keeps_its_label() {
    let s = Build {
        category: "rng".to_owned(),
        design: Some("lfsr".to_owned()),
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(s.name(), "8-bit LFSR RNG");
}

#[test]
fn a_threshold_names_its_k_after_its_width() {
    let s = Build {
        category: "threshold".to_owned(),
        design: None,
        at_least: Some(3),
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(s.name(), "8-bit At Least 3 Threshold");
    assert_eq!(s.filename(), "8_bit_At_Least_3_Threshold.schem");
}

#[test]
fn a_serial_build_leads_with_its_throughput_and_others_do_not() {
    let serial = Build {
        category: "serial_adder".to_owned(),
        design: None,
        width: None,
        throughput: Some(2),
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(serial.name(), "2-tick Serial Adder");
    let adder = Build {
        throughput: Some(3),
        ..cla()
    };
    assert_eq!(adder.name(), "8-bit Carry Lookahead Adder");
}

#[test]
fn a_synchronized_stateless_build_faster_than_its_latency_is_pipelined() {
    let piped = Build {
        synchronized: Some(true),
        throughput: Some(3),
        ..cla()
    };
    assert!(piped.pipelined());
    let unsynced = Build {
        synchronized: Some(false),
        ..piped.clone()
    };
    assert!(!unsynced.pipelined(), "pipelining needs synced outputs");
    let stateful = Build {
        own_logic: Some(Logic::Stateful),
        ..piped.clone()
    };
    assert!(!stateful.pipelined());
    let as_slow = Build {
        throughput: Some(8),
        ..piped
    };
    assert!(!as_slow.pipelined(), "throughput must beat latency");
}

#[test]
fn a_stack_grows_by_its_step_however_much_copies_overlap() {
    let way = |west, up, _| StackDirection {
        west,
        up,
        max_copies: None,
        faces: None,
        pitch: None,
    };
    // Copies 2 up a box 6 high overlap by 4, so each adds 2 high.
    let vertical = Build {
        size: (5, 6, 6),
        stacking: vec![way(0, 2, 0)],
        ..cla()
    };
    assert_eq!(vertical.per_copy(), Some((5, 2, 6)));
    let horizontal = Build {
        size: (7, 2, 6),
        stacking: vec![way(5, 0, 0)],
        ..cla()
    };
    assert_eq!(horizontal.per_copy(), Some((5, 2, 6)));
    let diagonal = Build {
        stacking: vec![way(1, 1, 0)],
        ..cla()
    };
    assert_eq!(diagonal.per_copy(), None);
    assert_eq!(cla().per_copy(), None);
    // Compared by what a copy adds, but its box is still all of it.
    assert_eq!((vertical.volume(), vertical.box_volume()), (60, 180));
    assert_eq!(cla().volume(), cla().box_volume());
}

#[test]
fn a_category_keeps_its_label() {
    let s = Build {
        category: "ram".to_owned(),
        design: None,
        ..cla()
    };
    assert_eq!(s.name(), "8-bit RAM");
}

#[test]
fn a_category_off_the_list_is_capitalized() {
    let s = Build {
        category: "maze_screen".to_owned(),
        width: None,
        design: None,
        ..cla()
    };
    assert_eq!(s.name(), "Maze Screen");
}

#[test]
fn unknown_parts_are_left_out() {
    let s = Build {
        width: None,
        design: None,
        latency: None,
        synchronized: None,
        throughput: None,
        bytes: None,
        authors: Vec::new(),
        built_on: None,
        tags: Vec::new(),
        ..cla()
    };
    assert_eq!(s.title(), "Adder");
    assert_eq!(s.filename(), "Adder.schem");
}
