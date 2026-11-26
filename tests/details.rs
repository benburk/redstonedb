//! The rules a build's details are held to, the same for an upload, an
//! edit, and `rsdb set`.

mod common;

use app::{
    build::status::{ADDS, NO_HEX_INPUTS, SUBTRACTS, WITHOUT_LEVERS},
    build::{Details, Ready, Specs},
    ingest::{
        details::{self, Candidate},
        prepare::{self, Purpose},
    },
    interface::{Encoding, Orientation},
    schem::{self, Blocks},
    stack::StackDirection,
};
use sqlx::SqlitePool;

/// A fixture's file, as an upload prepares it, standardized if asked.
fn prepared(name: &str, standardize: bool) -> Vec<u8> {
    let purpose = Purpose::InPlace {
        standardize,
        conventional: true,
        keep_paste: false,
        empty: true,
    };
    prepare::prepare_for(&common::fixture(name), purpose)
        .expect("fixture prepares")
        .data
}

/// A stored adder's file, as an upload prepares it.
fn adder() -> Vec<u8> {
    prepared("alpha-rca", false)
}

/// Details with a category typed as `category`, and the rest as an author
/// might fill them in, whether or not the category has a use for them.
fn given(category: &str) -> Details {
    Details {
        category: category.to_owned(),
        specs: Specs {
            width: Some(8),
            orientation: Orientation::Horizontal,
            encoding: Some(Encoding::Binary),
            at_least: Some(3),
            ..Specs::default()
        },
        ..Details::default()
    }
}

/// Details under Other, the build named `name`, the rest as `given` has
/// them.
fn other(name: &str) -> Details {
    Details {
        name: Some(name.to_owned()),
        ..given("Other")
    }
}

/// `details` checked against the adder's file, as a person who may not
/// name new tags.
async fn check(db: &SqlitePool, details: Details) -> Result<Ready, String> {
    check_file(db, details, &adder(), true).await
}

/// `details` checked against file `data`.
async fn check_file(
    db: &SqlitePool,
    details: Details,
    data: &[u8],
    conventional: bool,
) -> Result<Ready, String> {
    let build = Candidate {
        data,
        conventional,
        passes: false,
    };
    details::check(db, details, build, false)
        .await
        .expect("the database answers")
}

#[tokio::test]
async fn a_build_under_other_has_a_name_but_no_width_or_encoding() {
    let db = common::database().await;
    let diode = check(&db, other(" 8-bit horizontal diode ")).await.unwrap();
    assert_eq!(diode.details.category, "other");
    assert_eq!(
        diode.details.name.as_deref(),
        Some("8-bit horizontal diode")
    );
    assert_eq!(
        (diode.details.specs.width, diode.details.specs.encoding),
        (None, None)
    );
}

#[tokio::test]
async fn a_category_is_one_of_the_list_or_other_and_other_is_named() {
    let db = common::database().await;
    let own = check(&db, given("piston door")).await.err().unwrap();
    assert!(own.contains("Unknown category"), "{own}");
    let unnamed = check(&db, given("other")).await.err().unwrap();
    assert!(unnamed.contains("Missing name"), "{unnamed}");
    // A standard category names the build itself.
    let named = Details {
        name: Some("my adder".to_owned()),
        ..given("adder")
    };
    let adder = check(&db, named).await.unwrap();
    assert_eq!(adder.details.name, None);
}

#[tokio::test]
async fn stacking_that_does_not_fit_is_refused() {
    let db = common::database().await;
    // Two copies this far apart make a stack too large to build.
    let apart = Details {
        stacking: vec!["west=100000".parse::<StackDirection>().unwrap()],
        ..other("8-bit horizontal diode")
    };
    let refused = check(&db, apart.clone()).await.err().unwrap();
    assert!(refused.contains("don't fit"), "{refused}");
    // An adder's stacking is the simulation's, so what is typed is not
    // checked, nor kept.
    let adder = check(
        &db,
        Details {
            category: "adder".to_owned(),
            ..apart
        },
    )
    .await
    .unwrap();
    assert!(adder.stacking_inferred);
}

#[tokio::test]
async fn only_a_mod_or_admin_may_name_a_new_tag_with_the_details() {
    let db = common::database().await;
    let tagged = Details {
        tags: vec!["Made Up".to_owned()],
        ..other("piston door")
    };
    let refused = check(&db, tagged.clone()).await.err().unwrap();
    assert!(refused.contains("only a mod or admin"), "{refused}");
    let data = adder();
    let build = Candidate {
        data: &data,
        conventional: true,
        passes: false,
    };
    let ready = details::check(&db, tagged, build, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready.details.tags, ["made_up"]);
}

#[tokio::test]
async fn a_build_date_still_to_come_is_refused() {
    let db = common::database().await;
    let dated = |date: &str| Details {
        built_on: Some(date.to_owned()),
        ..other("piston door")
    };
    let kept = check(&db, dated("2024-02-29")).await.unwrap();
    assert_eq!(kept.details.built_on.as_deref(), Some("2024-02-29"));
    let refused = check(&db, dated("2999-01-01")).await.err().unwrap();
    assert!(refused.contains("date in the future"), "{refused}");
}

/// An adder is recognized from its file or goes up unverified, saying
/// why, with the specs typed for it: one the simulator can't run, one not
/// standardized, one that fails its test, and one taking a signal strength
/// as an input.
#[tokio::test]
async fn an_unrecognized_build_goes_up_unverified_under_a_blessed_category() {
    let db = common::database().await;
    let typed = || {
        let mut typed = given("adder");
        typed.specs.width = Some(8);
        typed.specs.latency = Some(5);
        typed
    };
    let unverified = |ready: Result<Ready, String>, why: &str| {
        let ready = ready.unwrap();
        let said = ready.details.unverified.as_deref().unwrap_or_default();
        assert!(said.contains(why), "{why}: {said}");
        assert_eq!(ready.details.category, "adder");
        assert_eq!(ready.details.specs.latency, Some(5), "{why}");
        assert_eq!(ready.details.specs.width, Some(8), "{why}");
        assert!(!ready.evaluated, "{why}");
    };
    // A piston in its first empty space, which the simulator does not
    // model.
    let data = adder();
    let blocks = Blocks::read(&data).unwrap();
    let (empty, _) =
        blocks.iter().find(|(_, s)| *s == "minecraft:air").unwrap();
    let piston = [(
        empty,
        "minecraft:piston[extended=false,facing=up]".to_owned(),
    )]
    .into();
    let pushed = schem::edit(&data, &piston).unwrap();
    unverified(
        check_file(&db, typed(), &pushed, true).await,
        "Can't simulate piston",
    );
    unverified(
        check_file(&db, typed(), &adder(), false).await,
        "Inputs and outputs can't be read",
    );
    unverified(
        check_file(&db, typed(), &prepared("cca-8-dust-out", false), true)
            .await,
        "cases wrong: A=",
    );
    unverified(
        check_file(&db, typed(), &common::fixture("hex-cca-8"), true).await,
        "Expected levers, found signal-strength inputs",
    );
    let recognized = check(&db, given("adder")).await.unwrap();
    assert_eq!(recognized.details.unverified, None);
}

/// Unverified, a build says the problem that kept it so: kept as built,
/// what probing it as its category finds, each kind once with its places
/// counted; and a test that couldn't run, why, untested rather than
/// failing, as nothing it gave was wrong.
#[tokio::test]
async fn an_unverified_build_says_its_own_problem() {
    let db = common::database().await;
    let reason = |ready: Result<Ready, String>| {
        ready.unwrap().details.unverified.expect("unverified")
    };
    let levers_on_top = reason(
        check_file(&db, given("adder"), &common::fixture("vert-3t-cca"), false)
            .await,
    );
    assert!(
        levers_on_top.contains("on top of block") && !levers_on_top.wrong,
        "{levers_on_top:?}"
    );
    let incrementer = reason(
        check_file(&db, given("adder"), &prepared("inc-8", false), true).await,
    );
    assert!(
        incrementer.why.ends_with("found \"increment\""),
        "{incrementer:?}"
    );
    assert!(!incrementer.wrong);
}

/// A stored build whose test has passed is not tested again when only its
/// details are saved, which keeps a save quick: its file goes unchecked,
/// so even one the test would find unverified stays verified.
#[tokio::test]
async fn a_build_that_passed_is_not_tested_again() {
    let db = common::database().await;
    let data = prepared("cca-8-dust-out", false);
    let tested = |passes| Candidate {
        data: &data,
        conventional: true,
        passes,
    };
    let checked =
        |passes| details::check(&db, given("adder"), tested(passes), false);
    let unverified = |ready: Ready| ready.details.unverified;
    assert!(unverified(checked(false).await.unwrap().unwrap()).is_some());
    assert!(unverified(checked(true).await.unwrap().unwrap()).is_none());
}

/// Nothing measured is typed for a category the simulation works out: a
/// typed width, encoding, and flags give way to what the file shows, and
/// the simulation sets the flags once the build is stored.
#[tokio::test]
async fn an_evaluated_categorys_typed_measures_give_way_to_its_file() {
    let db = common::database().await;
    let mut typed = given("adder");
    typed.specs.width = Some(3);
    typed.specs.encoding = Some(Encoding::Hex);
    typed.specs.orientation = Orientation::Diagonal;
    typed.flags = vec!["carry_in".to_owned()];
    let ready = check(&db, typed).await.unwrap();
    assert_eq!(
        ready.details.specs.width,
        Some(8),
        "the ripple adder's own width"
    );
    assert_eq!(
        ready.details.specs.orientation,
        Orientation::Horizontal,
        "as its bits run"
    );
    assert_eq!(ready.details.specs.encoding, Some(Encoding::Binary));
    assert!(ready.details.flags.is_empty());
}

/// A hex adder, its inputs taking a hex digit per block through dust, is
/// verified as a Hex Adder and not as an Adder; an adder driven by levers
/// is verified as an Adder and not as a Hex Adder.
#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn hex_inputs_make_a_hex_adder_and_levers_an_adder() {
    let db = common::database().await;
    // Standardized, as an upload is before it is checked.
    let hex = prepared("hex-cca-8", true);
    let why = |ready: Result<Ready, String>| ready.unwrap().details.unverified;
    assert_eq!(
        why(check_file(&db, given("hex adder"), &hex, true).await),
        None
    );
    assert_eq!(
        why(check_file(&db, given("adder"), &hex, true).await).as_deref(),
        Some(WITHOUT_LEVERS)
    );
    assert_eq!(why(check(&db, given("adder")).await), None);
    assert_eq!(
        why(check(&db, given("hex adder")).await).as_deref(),
        Some(NO_HEX_INPUTS)
    );
}

/// A hex-to-binary converter, its digit a comparator with air behind, is
/// verified as Hex to Binary, as a hex adder's digits are as a Hex Adder,
/// and not as an Adder; its maker's simplified fix of cfc02dcb.
#[tokio::test]
async fn a_hex_to_binary_converters_digit_input_is_its_own() {
    let db = common::database().await;
    let data = prepared("hex-bin-4-simplified", true);
    let why = |ready: Result<Ready, String>| ready.unwrap().details.unverified;
    assert_eq!(
        why(check_file(&db, given("hex to binary"), &data, true).await),
        None
    );
    assert_eq!(
        why(check_file(&db, given("adder"), &data, true).await).as_deref(),
        Some(WITHOUT_LEVERS)
    );
}

/// An adder is verified as an Adder and not as a Subtractor, and a
/// subtractor, its B subtracted, as a Subtractor and not as an Adder.
#[tokio::test]
#[ignore = "measures whole builds: just test-full"]
async fn adders_and_subtractors_are_each_their_own_category() {
    let db = common::database().await;
    let why = |ready: Result<Ready, String>| ready.unwrap().details.unverified;
    // Coloured by probing, then standardized, as an upload is.
    let raw = common::fixture("sub-16-torchless");
    let coloured = app::recognize::probe::recognize(&raw).unwrap().file;
    let standardized = Purpose::InPlace {
        standardize: true,
        conventional: true,
        keep_paste: false,
        empty: true,
    };
    let subtractor =
        prepare::prepare_for(&coloured, standardized).unwrap().data;
    assert_eq!(
        why(check_file(&db, given("subtractor"), &subtractor, true).await),
        None
    );
    assert_eq!(
        why(check_file(&db, given("adder"), &subtractor, true).await)
            .as_deref(),
        Some(SUBTRACTS)
    );
    assert_eq!(
        why(check(&db, given("subtractor")).await).as_deref(),
        Some(ADDS)
    );
}
