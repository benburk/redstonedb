//! Bringing a stored build up to date with its file and its simulation:
//! the details its file and result decide, written over what was typed,
//! and only when they change.

use app::{
    details::{self, Build, Details},
    import,
    models::{
        schematic::{self, Encoding, Orientation},
        simulation, tag,
    },
    simulate::Report,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// An in-memory database with migrations applied, on one connection, so
/// `total_changes()` counts every write.
async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    pool
}

/// Stores the fixture `name` as a conventional build with `details`, and
/// returns its id.
async fn store(db: &SqlitePool, name: &str, details: Details) -> String {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let prepared = import::prepare(&raw, None, true, false, false)
        .expect("fixture prepares");
    let ready = details::check(db, details, Build::of(&prepared), false)
        .await
        .expect("the database answers")
        .expect("the details keep the rules");
    import::store(db, &prepared, &ready, None, None)
        .await
        .expect("the database answers")
        .expect("the file is new")
}

/// How many rows the connection has written since it opened.
async fn changes(db: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn refreshing_sets_what_the_file_says_then_changes_nothing() {
    let db = setup_test_db().await;
    // Typed wrong in every detail the file decides.
    let typed = Details {
        category: "incrementer".to_owned(),
        width: Some(4),
        orientation: Orientation::Horizontal,
        encoding: Some(Encoding::Hex),
        ..Details::default()
    };
    let id = store(&db, "inc-8", typed).await;

    let report = simulation::refresh(&db, &id, false).await.unwrap();
    assert!(matches!(report, Some(Report::Ran(_))), "{report:?}");
    assert!(report.unwrap().passed());
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.width, Some(8));
    assert_eq!(s.orientation, Orientation::Vertical);
    assert_eq!(s.encoding, Some(Encoding::Binary));
    assert!(!s.tags.iter().any(|t| t.name == tag::FAILS_SIMULATION));
    assert!(simulation::current(&db, &s).await.unwrap().is_some());

    // The result is current and the details already match, so refreshing
    // again runs nothing and writes nothing.
    let before = changes(&db).await;
    let again = simulation::refresh(&db, &id, false).await.unwrap();
    assert!(again.is_some_and(|report| report.passed()));
    assert_eq!(changes(&db).await, before);
}

#[tokio::test]
async fn a_build_without_a_test_keeps_no_result() {
    let db = setup_test_db().await;
    let typed = Details {
        category: "multiplier".to_owned(),
        // Wider than any test takes.
        width: Some(64),
        ..Details::default()
    };
    let id = store(&db, "mult-8-buttons", typed).await;
    assert!(
        simulation::refresh(&db, &id, false)
            .await
            .unwrap()
            .is_none()
    );
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.width, Some(64), "a multiplier's width is typed");
}

#[tokio::test]
async fn an_adders_measured_details_come_from_its_file_not_its_typing() {
    let db = setup_test_db().await;
    let typed = Details {
        category: "adder".to_owned(),
        latency: Some(99),
        throughput: Some(99),
        design: Some("carry_lookahead".to_owned()),
        ..Details::default()
    };
    let id = store(&db, "rca-slice-capped", typed).await;
    let report = simulation::refresh(&db, &id, false).await.unwrap();
    assert!(report.is_some_and(|r| r.passed() && r.timing().is_some()));
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.design, "ripple_carry");
    assert_ne!(s.latency, Some(99));
    assert!(s.throughput.is_some_and(|t| (2..99).contains(&t)));
}

#[tokio::test]
async fn an_adder_failing_its_test_can_only_go_up_off_the_list() {
    let db = setup_test_db().await;
    let raw = std::fs::read("tests/fixtures/cca-8-dust-out.schem").unwrap();
    let prepared = import::prepare(&raw, None, true, false, false).unwrap();
    let as_adder = Details {
        category: "adder".to_owned(),
        ..Details::default()
    };
    let refused = details::check(&db, as_adder, Build::of(&prepared), false)
        .await
        .unwrap()
        .err()
        .unwrap();
    assert!(refused.contains("such as 8-bit adder"), "{refused}");
    let as_other = Details {
        category: "8-bit adder".to_owned(),
        ..Details::default()
    };
    let other = details::check(&db, as_other, Build::of(&prepared), false)
        .await
        .unwrap();
    assert!(other.is_ok(), "{:?}", other.err());
}

#[tokio::test]
async fn a_new_file_with_the_same_circuit_keeps_its_result() {
    let db = setup_test_db().await;
    let typed = Details {
        category: "adder".to_owned(),
        ..Details::default()
    };
    let id = store(&db, "rca-slice-capped", typed).await;
    simulation::refresh(&db, &id, false).await.unwrap();
    let ran_at = || async {
        sqlx::query_scalar::<_, String>(
            "SELECT ran_at FROM simulation WHERE schematic_id = ?",
        )
        .bind(&id)
        .fetch_one(&db)
        .await
        .unwrap()
    };
    let first = ran_at().await;

    // A copy with a building block recoloured: another file, the same
    // circuit.
    let old = app::models::version::current_file(&db, &id)
        .await
        .unwrap()
        .unwrap();
    let blocks = app::schem::Blocks::read(&old).unwrap();
    let mut grid: std::collections::HashMap<[i32; 3], String> =
        blocks.iter().map(|(p, s)| (p, s.to_owned())).collect();
    let (&pos, _) = grid
        .iter()
        .find(|(_, s)| s.as_str() == "minecraft:white_concrete")
        .unwrap();
    grid.insert(pos, "minecraft:light_gray_concrete".to_owned());
    let mut nbt = app::schem::decode(&old).unwrap();
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)
        .unwrap();
    let new = app::schem::encode(nbt).unwrap();
    let prepared = import::prepare(&new, None, true, false, false).unwrap();
    let how = app::models::version::Replacing::NextVersion {
        added_by: None,
        note: None,
    };
    app::models::version::replace(&db, &id, &prepared, how)
        .await
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1100));
    let report = simulation::refresh(&db, &id, false).await.unwrap();
    assert!(report.is_some_and(|r| r.passed() && r.timing().is_some()));
    assert_eq!(ran_at().await, first, "it was not simulated again");
}
