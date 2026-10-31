//! The rules a build's details are held to, the same for an upload, an
//! edit, and `rsdb set`.

use app::{
    details::{self, Build, Details, Ready},
    import,
    models::schematic::{Encoding, Orientation, StackDirection},
    schem::Blocks,
    standardize,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// Creates an in-memory `SQLite` database with migrations applied, which
/// list the standard categories.
async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    pool
}

/// A fixture's file, as an upload prepares it, standardized if asked.
fn prepared(name: &str, standardize: bool) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    import::prepare(&raw, standardize, true, false, false)
        .expect("fixture prepares")
        .data
}

/// A stored adder's file, as an upload prepares it.
fn adder() -> Vec<u8> {
    prepared("alpha-rca", false)
}

/// Details with a category typed as `category`, and the rest as a builder
/// might fill them in, whether or not the category has a use for them.
fn given(category: &str) -> Details {
    Details {
        category: category.to_owned(),
        width: Some(8),
        orientation: Orientation::Horizontal,
        encoding: Some(Encoding::Binary),
        given_name: Some(" Bus Crossing ".to_owned()),
        bytes: Some(32),
        at_least: Some(3),
        ..Details::default()
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
    let build = Build { data, conventional };
    details::check(db, details, build, false)
        .await
        .expect("the database answers")
}

/// Only a blessed category, one probing recognizes, goes on the list; a
/// build given any other listed one is asked for a name of its own.
#[tokio::test]
async fn only_a_blessed_category_goes_on_the_list() {
    let db = setup_test_db().await;
    for category in ["Bussing", "ram", "inc_dec"] {
        let refused = check(&db, given(category)).await.err().unwrap();
        assert!(refused.contains("go on the list so far"), "{refused}");
        assert!(refused.contains("category of its own"), "{refused}");
    }
    assert!(check(&db, given("adder")).await.is_ok());
}

#[tokio::test]
async fn a_category_off_the_list_has_no_width_or_encoding() {
    let db = setup_test_db().await;
    let diode = check(&db, given("8-Bit Horizontal Diode")).await.unwrap();
    assert_eq!(diode.details.category, "8-bit_horizontal_diode");
    assert_eq!((diode.details.width, diode.details.encoding), (None, None));
}

#[tokio::test]
async fn stacking_that_does_not_fit_is_refused() {
    let db = setup_test_db().await;
    // Two copies this far apart make a stack too large to build.
    let apart = Details {
        stacking: vec!["west=100000".parse::<StackDirection>().unwrap()],
        ..given("8-bit ram")
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
async fn only_an_admin_may_name_a_new_tag_with_the_details() {
    let db = setup_test_db().await;
    let tagged = Details {
        tags: vec!["Made Up".to_owned()],
        ..given("piston door")
    };
    let refused = check(&db, tagged.clone()).await.err().unwrap();
    assert!(refused.contains("only an admin"), "{refused}");
    let data = adder();
    let build = Build {
        data: &data,
        conventional: true,
    };
    let ready = details::check(&db, tagged, build, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready.details.tags, ["made_up"]);
}

#[tokio::test]
async fn a_build_date_still_to_come_is_refused() {
    let db = setup_test_db().await;
    let dated = |date: &str| Details {
        built_on: Some(date.to_owned()),
        ..given("piston door")
    };
    let kept = check(&db, dated("2024-02-29")).await.unwrap();
    assert_eq!(kept.details.built_on.as_deref(), Some("2024-02-29"));
    let refused = check(&db, dated("2999-01-01")).await.err().unwrap();
    assert!(refused.contains("still to come"), "{refused}");
}

/// An adder is recognized from its file or goes up under a category of its
/// own: one the simulator can't run, one not standardized, one that fails
/// its test, and one taking a signal strength as an input are refused.
#[tokio::test]
async fn an_unrecognized_build_cannot_go_up_under_a_blessed_category() {
    let db = setup_test_db().await;
    let off_list = |refused: Result<Ready, String>, why: &str| {
        let refused = refused.err().unwrap();
        assert!(refused.contains(why), "{why}: {refused}");
        assert!(refused.contains("category of its own"), "{refused}");
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
    let pushed = standardize::edit(&data, &piston).unwrap();
    off_list(
        check_file(&db, given("adder"), &pushed, true).await,
        "piston, which the simulator does not model",
    );
    off_list(
        check_file(&db, given("adder"), &adder(), false).await,
        "can't read its inputs and outputs",
    );
    off_list(
        check_file(
            &db,
            given("adder"),
            &prepared("cca-8-dust-out", false),
            true,
        )
        .await,
        "does not pass the adder test",
    );
    off_list(
        check_file(
            &db,
            given("adder"),
            &std::fs::read("tests/fixtures/hex-cca-8.schem").unwrap(),
            true,
        )
        .await,
        "signal-strength inputs",
    );
    assert!(check(&db, given("adder")).await.is_ok());
}

/// Nothing measured is typed for a category the simulation works out: a
/// typed width, encoding, and flags give way to what the file shows, and
/// the simulation sets the flags once the build is stored.
#[tokio::test]
async fn an_evaluated_categorys_typed_measures_give_way_to_its_file() {
    let db = setup_test_db().await;
    let typed = Details {
        width: Some(3),
        encoding: Some(Encoding::Hex),
        flags: vec!["carry_in".to_owned()],
        ..given("adder")
    };
    let typed = Details {
        orientation: Orientation::Diagonal,
        ..typed
    };
    let ready = check(&db, typed).await.unwrap();
    assert_eq!(ready.details.width, Some(8), "the ripple adder's own width");
    assert_eq!(
        ready.details.orientation,
        Orientation::Horizontal,
        "as its bits run"
    );
    assert_eq!(ready.details.encoding, Some(Encoding::Binary));
    assert!(ready.details.flags.is_empty());
}
