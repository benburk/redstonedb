//! The rules a build's details are held to, the same for an upload, an
//! edit, and `rsdb add`.

use app::{
    details::{self, Build, Details, Ready},
    import,
    models::schematic::{Encoding, Orientation, StackDirection},
    schem::Blocks,
    standardize::{self, Gray},
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

/// A fixture's file, as an upload prepares it, its gray blocks whitened
/// for one not in the palette.
fn prepared(name: &str, gray: Option<Gray>) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    import::prepare(&raw, gray, true, false, false)
        .expect("fixture prepares")
        .data
}

/// A stored adder's file, as an upload prepares it.
fn adder() -> Vec<u8> {
    prepared("alpha-rca", None)
}

/// A stored sequential multiplier's file.
fn multiplier() -> Vec<u8> {
    prepared("mult-8-buttons", Some(Gray::Whiten))
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

#[tokio::test]
async fn only_a_named_category_keeps_a_given_name() {
    let db = setup_test_db().await;
    let bussing = check(&db, given("Bussing")).await.unwrap();
    assert_eq!(bussing.details.given_name.as_deref(), Some("Bus Crossing"));
    let subtractor = check(&db, given("subtractor")).await.unwrap();
    assert_eq!(subtractor.details.given_name, None);
}

#[tokio::test]
async fn only_a_category_ranked_by_a_measure_keeps_it() {
    let db = setup_test_db().await;
    // Typed by its label, in other capitals.
    let ram = check(&db, given("ram")).await.unwrap();
    assert_eq!(ram.details.category, "ram");
    assert_eq!((ram.details.bytes, ram.details.at_least), (Some(32), None));
    let subtractor = check(&db, given("Subtractor")).await.unwrap();
    assert_eq!(subtractor.details.bytes, None);
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
        ..given("ram")
    };
    let refused = check(&db, apart.clone()).await.err().unwrap();
    assert!(refused.contains("don't fit"), "{refused}");
    // A multiplier never stacks, so what is typed is dropped.
    let multiplier = Details {
        category: "multiplier".to_owned(),
        ..apart.clone()
    };
    let multiplier = check_file(&db, multiplier, &self::multiplier(), true)
        .await
        .unwrap();
    assert!(multiplier.details.stacking.is_empty());
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
        ..given("subtractor")
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
        ..given("subtractor")
    };
    let kept = check(&db, dated("2024-02-29")).await.unwrap();
    assert_eq!(kept.details.built_on.as_deref(), Some("2024-02-29"));
    let refused = check(&db, dated("2999-01-01")).await.err().unwrap();
    assert!(refused.contains("still to come"), "{refused}");
}

/// An adder or multiplier is recognized from its file or goes up under a
/// category of its own: one the simulator can't run, one that breaks the
/// conventions its test reads, and one that fails its test are refused.
#[tokio::test]
async fn an_unrecognized_build_cannot_go_up_under_an_evaluated_category() {
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
        "building conventions",
    );
    off_list(
        check_file(
            &db,
            given("adder"),
            &prepared("cca-8-dust-out", None),
            true,
        )
        .await,
        "does not pass the adder test",
    );
    assert!(check(&db, given("adder")).await.is_ok());
}
