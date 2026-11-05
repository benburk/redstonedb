//! Which file an upload keeps: standardized when the site recognizes it as
//! the category it is given, with its gray blocks kept only when its copies
//! stack with them left out; as its author made it, under a category off
//! the list, when it does not; and a new version that is recognized moving
//! a build off the list up.

mod common;

use app::{
    category::Category,
    details,
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
    let Ok(Kept::Standard { file, category }) =
        import::kept(&categories, "adder", &upload).await.unwrap()
    else {
        panic!("the adder is not recognized");
    };
    assert_eq!(category, "adder");
    assert_eq!(placed(&file.blocks), placed(&standard.blocks));
    // Named for it off the list, it goes up as one all the same.
    let Ok(Kept::Standard { category, .. }) =
        import::kept(&categories, "16-bit adder", &upload)
            .await
            .unwrap()
    else {
        panic!("the adder is not recognized by its off-list name");
    };
    assert_eq!(category, "adder");
}

#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn an_unrecognized_build_keeps_its_own_blocks_off_the_list() {
    let categories = Category::all();
    let upload = Upload::read(&common::fixture("cca-16-raw")).unwrap();
    let Ok(Kept::Original(Some(off))) =
        import::kept(&categories, "multiplier", &upload)
            .await
            .unwrap()
    else {
        panic!("an adder passed for a multiplier");
    };
    assert!(off.name.ends_with("multiplier"), "{}", off.name);
    let failing = Upload::read(&common::fixture("cca-8-dust-out")).unwrap();
    let Ok(Kept::Original(Some(off))) =
        import::kept(&categories, "adder", &failing).await.unwrap()
    else {
        panic!("a failing adder was recognized");
    };
    assert_eq!(off.name, "8-bit adder");
    assert!(off.why.contains("cases wrong: A="), "{}", off.why);
}

#[tokio::test]
async fn a_category_no_test_recognizes_keeps_the_build_as_built() {
    let categories = Category::all();
    let upload = Upload::read(&common::fixture("cca-16-raw")).unwrap();
    assert!(matches!(
        import::kept(&categories, "piston door", &upload)
            .await
            .unwrap(),
        Ok(Kept::Original(None))
    ));
}

#[tokio::test]
async fn an_off_list_name_means_the_category_it_stands_in_for() {
    let categories = Category::all();
    let meant =
        |name| details::meant(&categories, name).map(|c| c.name.clone());
    assert_eq!(meant("8-bit adder").as_deref(), Some("adder"));
    assert_eq!(meant("32-bit_adder").as_deref(), Some("adder"));
    assert_eq!(meant("Adder").as_deref(), Some("adder"));
    assert_eq!(meant("piston door"), None);
    assert_eq!(meant("eight-bit adder"), None);
}

#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn a_new_version_that_is_recognized_moves_a_build_up() {
    let categories = Category::all();
    let file = import::as_built(&common::fixture("cca-16-raw")).unwrap();
    let (moved, to) =
        import::moved_up(&categories, "16-bit_adder", &file, false)
            .await
            .unwrap()
            .expect("it moves up");
    assert_eq!(to.name, "adder");
    assert!(moved.conventional);
    assert!(
        import::moved_up(&categories, "16-bit_multiplier", &file, false)
            .await
            .unwrap()
            .is_none()
    );
    // An adder unverified as one moves up the same way.
    let (_, to) = import::moved_up(&categories, "adder", &file, false)
        .await
        .unwrap()
        .expect("an unverified adder moves up");
    assert_eq!(to.name, "adder");
}

/// Every category the site recognizes builds of names its off-list stand-in
/// so it reads back, whatever its label holds, as NAND+XNOR Gate's plus.
#[tokio::test]
async fn an_off_list_name_reads_back_as_its_category() {
    let categories = Category::all();
    let evaluated = categories
        .iter()
        .filter(|c| app::category::behaviour(&c.name).evaluated);
    for category in evaluated {
        for width in [Some(8), None] {
            let name = details::off_list_name(category, width);
            let slug = app::models::tag::name(&name);
            assert!(app::models::tag::valid_name(&slug), "{slug}");
            assert_eq!(
                app::models::simulation::meant_by(&slug),
                Some(category.name.as_str()),
                "{slug}"
            );
            assert!(Category::find(&categories, &name).is_none(), "{name}");
        }
    }
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
    assert!(matches!(kept, Kept::Standard { .. }));
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
    let Ok(Kept::Standard { file, .. }) =
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
    let lines = app::recognize::interface::read(
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
