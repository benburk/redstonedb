//! Which file an upload keeps: standardized when the site recognizes it as
//! the category it is given; as its author made it, unverified, when it
//! does not; and a new version that is recognized verifying a build.

mod common;

use app::{
    category::Category,
    import::{self, Kept, Upload},
    schem::{self, Blocks},
};

/// Each block of `blocks` other than air, by position.
fn placed(blocks: &Blocks) -> Vec<(schem::Pos, String)> {
    blocks
        .iter()
        .filter(|(_, state)| *state != schem::AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect()
}

#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn a_recognized_build_is_kept_standardized() {
    let categories = Category::all();
    let upload = Upload::read(&common::fixture("cca-16-raw")).unwrap();
    let standard = upload.standard.as_ref().unwrap();
    assert_ne!(
        placed(&upload.original.blocks),
        placed(&standard.blocks),
        "the raw build is not in the palette"
    );
    let Ok(Kept::Standard(file)) =
        import::kept(&categories, "adder", &upload).await.unwrap()
    else {
        panic!("the adder is not recognized");
    };
    assert_eq!(placed(&file.blocks), placed(&standard.blocks));
}

#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn an_unrecognized_build_keeps_its_own_blocks_unverified() {
    let categories = Category::all();
    let upload = Upload::read(&common::fixture("cca-16-raw")).unwrap();
    assert!(
        matches!(
            import::kept(&categories, "multiplier", &upload)
                .await
                .unwrap(),
            Ok(Kept::Unverified(_))
        ),
        "an adder passed for a multiplier"
    );
    let failing = Upload::read(&common::fixture("cca-8-dust-out")).unwrap();
    let Ok(Kept::Unverified(why)) =
        import::kept(&categories, "adder", &failing).await.unwrap()
    else {
        panic!("a failing adder was recognized");
    };
    assert!(why.contains("cases wrong: A="), "{why}");
}

/// The site lists RAM but recognizes none yet, so one goes up as built
/// under a category of its own, named for it.
#[tokio::test]
async fn a_build_of_a_category_not_recognized_yet_goes_off_the_list() {
    let categories = Category::all();
    let upload = Upload::read(&common::fixture("cca-16-raw")).unwrap();
    let Ok(Kept::OffList(name)) =
        import::kept(&categories, "ram", &upload).await.unwrap()
    else {
        panic!("a RAM went on the list");
    };
    assert!(name.ends_with(" ram"), "{name}");
    let slug = app::models::tag::name(&name);
    assert!(app::category::check_category(&slug, None).is_ok(), "{slug}");
}

#[tokio::test]
async fn a_category_no_test_recognizes_keeps_the_build_as_built() {
    let categories = Category::all();
    let upload = Upload::read(&common::fixture("cca-16-raw")).unwrap();
    assert!(matches!(
        import::kept(&categories, "piston door", &upload)
            .await
            .unwrap(),
        Ok(Kept::Original)
    ));
}

#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn a_new_version_that_is_recognized_verifies_a_build() {
    let categories = Category::all();
    let category = |name| Category::find(&categories, name).unwrap();
    let file = import::as_built(&common::fixture("cca-16-raw")).unwrap();
    let verified = import::verified_by(category("adder"), &file, false)
        .await
        .unwrap()
        .expect("it is verified");
    assert!(verified.conventional);
    assert!(
        import::verified_by(category("multiplier"), &file, false)
            .await
            .unwrap()
            .is_none()
    );
}

/// A build of wool, its levers on lamps and its outputs magenta wool, as
/// `mop_u` built them, is recognized as it was built: any full block holds
/// a lever and simulates as concrete, and any lone block read from on top
/// is an output, whatever its colour.
#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn a_wool_build_with_levers_on_lamps_is_recognized_as_built() {
    let raw = common::fixture("cca-8-wool-lamps");
    let recognition =
        app::recognize::probe::recognize(&raw).expect("it can be probed");
    assert!(
        recognition.found.as_deref().is_some_and(
            |found| found.starts_with("an adder: all 131072 cases right")
        ),
        "{:?} {:?}",
        recognition.found,
        recognition.problems
    );
    let upload = Upload::read(&recognition.file).expect("it reads");
    let kept = import::kept(&Category::all(), "adder", &upload)
        .await
        .expect("it is checked")
        .expect("it can be simulated");
    assert!(matches!(kept, Kept::Standard(_)));
}

/// Publishing reads again the file its preview carries, the upload as
/// built, and keeps the same file its analysis would, hash and all, so an
/// evaluation the analysis starts is the one the build takes.
#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn publishing_keeps_the_file_its_preview_evaluated() {
    let raw = common::fixture("cca-16-raw");
    let coloured = app::recognize::probe::recognize(&raw).unwrap().file;
    let analyzed = Upload::read(&coloured).unwrap();
    let ahead = import::rewrapped(analyzed.standard.as_ref().unwrap()).unwrap();
    let published = Upload::read(&analyzed.original.data).unwrap();
    let categories = Category::all();
    let Ok(Kept::Standard(file)) =
        import::kept(&categories, "adder", &published)
            .await
            .unwrap()
    else {
        panic!("the adder is not recognized");
    };
    assert_eq!(file.sha256, ahead.sha256);
}

/// A hex adder's inputs, comparators with air behind each taking a hex
/// digit, are probed by their shapes, so it is found a hex adder, rather
/// than its lone carry in faulted as an input out of line, and its digits
/// are coloured in as A and B.
#[test]
fn probing_a_hex_adder_finds_its_digits() {
    let recognition =
        app::recognize::probe::recognize(&common::fixture("hex-cca-8"))
            .unwrap();
    assert_eq!(
        recognition.found.as_deref(),
        Some("a hex adder: all 11928 cases right, A + B"),
        "{:?}",
        recognition.problems
    );
    let lines = app::interface::read(
        &app::schem::Blocks::read(&recognition.file).unwrap(),
    );
    let digits: Vec<_> = lines
        .inputs
        .iter()
        .filter(|port| port.signal_strength)
        .map(|port| (port.role, port.bits.len()))
        .collect();
    assert_eq!(
        digits,
        [
            (Some(app::schem::lines::Role::Operand(0)), 8),
            (Some(app::schem::lines::Role::Operand(1)), 8),
        ]
    );
}

/// A file published with the details its preview suggests, as `rsdb seed`
/// publishes each fixture, goes up as the category it is recognized as,
/// and the same file again is refused.
#[tokio::test]
async fn a_file_published_as_suggested_goes_up_as_what_it_is() {
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "seeder").await;
    let raw = std::fs::read("tests/fixtures/alpha-rca.schem").unwrap();
    let id = app::web::publish_suggested(&db, &user, &raw)
        .await
        .unwrap()
        .expect("it goes up");
    let s = app::models::schematic::get(&db, &id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s.category, "adder");
    let again = app::web::publish_suggested(&db, &user, &raw).await.unwrap();
    assert!(again.is_err());
}

/// A build whose buses bend the conventions, its top bits out of line, its
/// outputs unevenly spaced, or its levers climbing a staircase, is found
/// by matching where probing finds none, kept standardized in its
/// category, and unaligned; a build whose buses keep them is not.
#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn an_unaligned_build_is_kept_in_its_category() {
    let categories = Category::all();
    for (name, category, unaligned) in [
        ("gt-8", "greater_than", true),
        ("adder-8-lsb-top", "adder", true),
        ("negator-8-staircase", "negator", true),
        ("cca-16-raw", "adder", false),
    ] {
        let raw =
            std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
        let recognized = app::recognize::probe::recognize(&raw).unwrap();
        assert!(
            recognized.found.is_some(),
            "{name}: {:?}",
            recognized.problems
        );
        let upload = Upload::read(&recognized.file).unwrap();
        let Ok(Kept::Standard(file)) =
            import::kept(&categories, category, &upload).await.unwrap()
        else {
            panic!("{name} is not kept as a {category}");
        };
        let lines = app::interface::Kept::read(&file.blocks).unwrap();
        assert_eq!(lines.unaligned(), unaligned, "{name}");
    }
}

/// A reading that leaves a build's digit inputs out is not what it is: a
/// one-hexit adder whose carry in is broken was read as a one-bit
/// popcount of its carry lever, and passed; now it is no build probing
/// knows, nearest a hex adder with cases wrong.
#[test]
fn a_reading_that_leaves_out_digit_inputs_is_none() {
    let raw = std::fs::read("tests/fixtures/hex-adder-4-carry-in-broken.schem")
        .unwrap();
    let bare = app::schem::lines::with(&raw, None).unwrap();
    let recognition = app::recognize::probe::recognize(&bare).unwrap();
    assert_eq!(recognition.found, None);
    let problem = recognition.problems.last().unwrap();
    assert!(problem.contains("cases wrong"), "{problem}");
}

/// One hex digit is four bits, a whole adder without a carry in, as one
/// bit is not: a one-hexit adder with none is found as a hex adder, where
/// it was refused as a one-bit adder lacking its carry in.
#[test]
fn a_one_hexit_adder_needs_no_carry_in() {
    let raw =
        std::fs::read("tests/fixtures/hex-adder-4-no-carry-in.schem").unwrap();
    let bare = app::schem::lines::with(&raw, None).unwrap();
    let recognition = app::recognize::probe::recognize(&bare).unwrap();
    assert_eq!(
        recognition.found.as_deref(),
        Some("a hex adder: all 256 cases right, A + B"),
        "{:?}",
        recognition.problems
    );
}

/// An adder whose sum reads inverted is an inverted adder, a category of
/// its own: zPippo's 3-tick carry-cancel adder, every case its sum the
/// inverse of A + B + carry in.
#[test]
fn an_adder_whose_sum_reads_inverted_is_an_inverted_adder() {
    let raw =
        std::fs::read("tests/fixtures/inverted-adder-8-cca.schem").unwrap();
    let bare = app::schem::lines::with(&raw, None).unwrap();
    let recognition = app::recognize::probe::recognize(&bare).unwrap();
    assert_eq!(
        recognition.found.as_deref(),
        Some(
            "an inverted adder: all 131072 cases right, not (A + B + carry \
             in)"
        ),
        "{:?}",
        recognition.problems
    );
}

/// A hex adder's two digit inputs one above the other, one run, are A and
/// B, a digit each, where the run was read as A alone, of two digits.
#[test]
fn a_hex_adders_one_run_of_digits_is_both_operands() {
    let raw = std::fs::read(
        "tests/fixtures/hex-adder-4-operands-in-one-column.schem",
    )
    .unwrap();
    let bare = app::schem::lines::with(&raw, None).unwrap();
    let recognition = app::recognize::probe::recognize(&bare).unwrap();
    assert_eq!(
        recognition.found.as_deref(),
        Some("a hex adder: all 256 cases right, A + B"),
        "{:?}",
        recognition.problems
    );
}

/// A sum is as wide as its operands, and an output bit beyond them is the
/// carry out: zPippo's 10-bit carry-cancel adder, its eleven outputs one
/// bus, gives a 10-bit sum and a carry out line, where its carry was read
/// as an eleventh bit of the sum.
#[test]
fn an_adders_output_bit_past_its_operands_is_its_carry_out() {
    use app::schem::lines::Role;
    let raw =
        std::fs::read("tests/fixtures/cca-10-carry-on-top.schem").unwrap();
    let bare = app::schem::lines::with(&raw, None).unwrap();
    let recognition = app::recognize::probe::recognize(&bare).unwrap();
    assert!(recognition.found.is_some(), "{:?}", recognition.problems);
    let blocks = app::schem::Blocks::read(&recognition.file).unwrap();
    let outputs: Vec<(Option<Role>, usize)> = app::interface::read(&blocks)
        .outputs
        .iter()
        .map(|port| (port.role, port.bits.len()))
        .collect();
    assert_eq!(
        outputs,
        [(Some(Role::Output(0)), 10), (Some(Role::Status), 1)]
    );
}
