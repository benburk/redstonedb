//! The rules a build's details are held to, the same for an upload, an
//! edit, and `rsdb add`.

use app::{
    details::{self, Build, Details, Ready},
    import,
    models::schematic::{Encoding, Orientation, StackDirection},
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

/// A stored adder's file, as an upload prepares it.
fn adder() -> Vec<u8> {
    let raw = std::fs::read("tests/fixtures/alpha-rca.schem")
        .expect("fixture is readable");
    import::prepare(&raw, None, true, false, false)
        .expect("fixture prepares")
        .data
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
    let data = adder();
    let build = Build {
        data: &data,
        conventional: true,
    };
    details::check(db, details, build, false)
        .await
        .expect("the database answers")
}

#[tokio::test]
async fn only_a_named_category_keeps_a_given_name() {
    let db = setup_test_db().await;
    let bussing = check(&db, given("Bussing")).await.unwrap();
    assert_eq!(bussing.details.given_name.as_deref(), Some("Bus Crossing"));
    let multiplier = check(&db, given("multiplier")).await.unwrap();
    assert_eq!(multiplier.details.given_name, None);
}

#[tokio::test]
async fn only_a_category_ranked_by_a_measure_keeps_it() {
    let db = setup_test_db().await;
    // Typed by its label, in other capitals.
    let ram = check(&db, given("ram")).await.unwrap();
    assert_eq!(ram.details.category, "ram");
    assert_eq!((ram.details.bytes, ram.details.at_least), (Some(32), None));
    let multiplier = check(&db, given("Multiplier")).await.unwrap();
    assert_eq!(multiplier.details.bytes, None);
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
        ..given("multiplier")
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
        ..given("multiplier")
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
        ..given("multiplier")
    };
    let kept = check(&db, dated("2024-02-29")).await.unwrap();
    assert_eq!(kept.details.built_on.as_deref(), Some("2024-02-29"));
    let refused = check(&db, dated("2999-01-01")).await.err().unwrap();
    assert!(refused.contains("still to come"), "{refused}");
}
