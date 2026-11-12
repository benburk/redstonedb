use super::*;
use crate::{
    category::Flag,
    models::tag::{self, Tag},
};

fn cla() -> Build {
    Build {
        id: String::new(),
        category: "adder".to_owned(),
        specs: Specs {
            width: Some(8),
            orientation: Orientation::Horizontal,
            encoding: Some(Encoding::Binary),
            latency: Some(8),
            synchronized: Some(false),
            design: Some("carry_lookahead".to_owned()),
            ..Specs::default()
        },
        torches: None,
        works_from: None,
        works_from_by: None,
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

/// A build of `category` with `specs`, no tags, and the rest as [`cla`]'s.
fn of(category: &str, specs: Specs) -> Build {
    Build {
        category: category.to_owned(),
        specs,
        tags: Vec::new(),
        ..cla()
    }
}

/// [`cla`]'s specs with no design.
fn plain() -> Specs {
    Specs {
        design: None,
        ..cla().specs
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
    let mut multiplier = of("multiplier", plain());
    assert_eq!(multiplier.name(), "8x8=8 Multiplier");
    multiplier.flags = vec![flag("upper_product")];
    assert_eq!(multiplier.name(), "8x8=16 Multiplier");
    assert_eq!(multiplier.filename(), "8x8_16_Multiplier.schem");
    multiplier.flags.push(flag("fma"));
    assert_eq!(multiplier.name(), "8x8+8=16 Multiplier");
}

#[test]
fn a_memory_names_its_capacity_and_stacks_multiply_it() {
    let specs = Specs {
        bytes: Some(32),
        ..plain()
    };
    let ram = of("ram", specs);
    assert_eq!(ram.name(), "8-bit 32-byte RAM");
    let up = StackDirection {
        west: 0,
        up: 3,
        max_copies: None,
        faces: None,
        pitch: None,
    };
    let stacked = ram.stacked(&[(up, 4)], [19, 11, 72]);
    assert_eq!(stacked.specs.bytes, Some(128));
    assert_eq!(
        stacked.specs.width,
        Some(8),
        "copies up add words, not bits"
    );
}

#[test]
fn a_category_label_keeps_its_symbols_out_of_the_filename() {
    let s = of("inc_dec", plain());
    assert_eq!(s.name(), "8-bit Inc/Dec");
    assert_eq!(s.filename(), "8_bit_Inc_Dec.schem");
}

#[test]
fn a_design_keeps_its_label() {
    let specs = Specs {
        design: Some("lfsr".to_owned()),
        ..cla().specs
    };
    let s = of("rng", specs);
    assert_eq!(s.name(), "8-bit LFSR RNG");
}

#[test]
fn a_threshold_names_its_k_after_its_width() {
    let specs = Specs {
        at_least: Some(3),
        ..plain()
    };
    let s = of("threshold", specs);
    assert_eq!(s.name(), "8-bit At Least 3 Threshold");
    assert_eq!(s.filename(), "8_bit_At_Least_3_Threshold.schem");
}

#[test]
fn a_serial_build_leads_with_its_throughput_and_others_do_not() {
    let specs = Specs {
        width: None,
        throughput: Some(2),
        ..plain()
    };
    let serial = of("serial_adder", specs);
    assert_eq!(serial.name(), "2-tick Serial Adder");
    let mut adder = cla();
    adder.specs.throughput = Some(3);
    assert_eq!(adder.name(), "8-bit Carry Lookahead Adder");
}

#[test]
fn a_synchronized_stateless_build_faster_than_its_latency_is_pipelined() {
    let mut piped = cla();
    piped.specs.synchronized = Some(true);
    piped.specs.throughput = Some(3);
    assert!(piped.pipelined());
    let mut unsynced = piped.clone();
    unsynced.specs.synchronized = Some(false);
    assert!(!unsynced.pipelined(), "pipelining needs synced outputs");
    let mut stateful = piped.clone();
    stateful.specs.logic = Some(Logic::Stateful);
    assert!(!stateful.pipelined());
    let mut as_slow = piped;
    as_slow.specs.throughput = Some(8);
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
    let s = of("ram", plain());
    assert_eq!(s.name(), "8-bit RAM");
}

#[test]
fn a_category_off_the_list_is_capitalized() {
    let specs = Specs {
        width: None,
        ..plain()
    };
    let s = of("maze_screen", specs);
    assert_eq!(s.name(), "Maze Screen");
}

#[test]
fn unknown_parts_are_left_out() {
    let s = Build {
        authors: Vec::new(),
        built_on: None,
        ..of("adder", Specs::default())
    };
    assert_eq!(s.title(), "Adder");
    assert_eq!(s.filename(), "Adder.schem");
}
