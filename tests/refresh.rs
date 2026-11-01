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
    let prepared = import::prepare(&raw, false, true, false, false)
        .expect("fixture prepares");
    let ready = details::check(db, details, Build::of(&prepared), false)
        .await
        .expect("the database answers")
        .expect("the details keep the rules");
    import::store(db, &prepared, &ready, None, None, None, None)
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
        category: "adder".to_owned(),
        width: Some(4),
        orientation: Orientation::Vertical,
        encoding: Some(Encoding::Hex),
        ..Details::default()
    };
    let id = store(&db, "alpha-rca", typed).await;

    let report = simulation::refresh(&db, &id, false).await.unwrap();
    assert!(matches!(report, Some(Report::Ran(_))), "{report:?}");
    assert!(report.unwrap().passed());
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.width, Some(8));
    assert_eq!(s.orientation, Orientation::Horizontal);
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
        category: "piston door".to_owned(),
        latency: Some(64),
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
    assert_eq!(s.latency, Some(64), "its builder's latency stands");
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
    let prepared = import::prepare(&raw, false, true, false, false).unwrap();
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

/// Off the list, it is held to the adder test all the same, so its page
/// shows what keeps it from being one, though nothing else is set from it.
#[tokio::test]
async fn an_off_list_build_is_tested_as_what_it_stands_in_for() {
    let db = setup_test_db().await;
    let typed = Details {
        category: "8-bit adder".to_owned(),
        latency: Some(99),
        ..Details::default()
    };
    let id = store(&db, "cca-8-dust-out", typed).await;
    let report = simulation::refresh(&db, &id, false)
        .await
        .unwrap()
        .expect("it is tested");
    assert!(report.failed(), "the fixture fails the adder test");
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.category, "8-bit_adder");
    assert_eq!(s.latency, Some(99), "its typed details stay");
    assert!(matches!(
        simulation::status(&db, &s).await.unwrap(),
        simulation::Status::Done(_)
    ));
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
    let prepared = import::prepare(&new, false, true, false, false).unwrap();
    let how = app::models::version::Replacing::NextVersion {
        added_by: None,
        added_from: None,
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

/// Editing an adder's details, as renaming its builder, keeps what the
/// simulation measured: its result on the same file stands, so nothing
/// runs again to set them back.
#[tokio::test]
async fn editing_an_adders_details_keeps_what_was_measured() {
    let db = setup_test_db().await;
    let id = store(
        &db,
        "rca-slice-capped",
        Details {
            category: "adder".to_owned(),
            ..Details::default()
        },
    )
    .await;
    simulation::refresh(&db, &id, false).await.unwrap();
    let measured = schematic::get(&db, &id).await.unwrap().unwrap();
    assert!(measured.latency.is_some() && measured.throughput.is_some());

    let mut edited = Details::of(&measured);
    edited.builders = vec!["Someone_Else".to_owned()];
    edited.description = Some("A slice.".to_owned());
    let data = app::models::version::current_file(&db, &id)
        .await
        .unwrap()
        .unwrap();
    let build = Build {
        data: &data,
        conventional: true,
        passes: false,
    };
    let ready = details::check(&db, edited, build, false)
        .await
        .unwrap()
        .unwrap();
    schematic::set_details(&db, &id, &ready).await.unwrap();
    simulation::refresh(&db, &id, false).await.unwrap();

    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    // Respelled as the player spells it, whatever that is.
    assert!(
        matches!(s.builders.as_slice(), [name] if name.eq_ignore_ascii_case("someone_else"))
    );
    assert_eq!(s.description.as_deref(), Some("A slice."));
    let fields = |s: &schematic::Schematic| {
        (
            s.width,
            s.orientation,
            s.latency,
            s.synchronized,
            s.throughput,
            s.design.clone(),
            s.logic,
            s.stacking.clone(),
        )
    };
    assert_eq!(fields(&s), fields(&measured));
}

/// A new version of a circuit tested before takes its result, and still
/// comes to lie with its inputs at logical 0, however its levers were
/// saved.
#[tokio::test]
async fn a_new_version_of_a_tested_circuit_lies_at_logical_zero() {
    let db = setup_test_db().await;
    let typed = Details {
        category: "adder".to_owned(),
        orientation: Orientation::Horizontal,
        ..Details::default()
    };
    let id = store(&db, "alpha-rca", typed).await;
    assert!(
        simulation::refresh(&db, &id, false)
            .await
            .unwrap()
            .is_some()
    );
    let on = |data: &[u8]| {
        app::schem::Blocks::read(data)
            .unwrap()
            .iter()
            .filter(|(_, state)| {
                state.contains("lever") && state.contains("powered=true")
            })
            .count()
    };
    let current = app::models::version::current_file(&db, &id)
        .await
        .unwrap()
        .unwrap();
    let levers_on = on(&current);

    // The same circuit, saved with one more lever on and a block touching
    // nothing of it added, so it is a new file.
    let blocks = app::schem::Blocks::read(&current).unwrap();
    let [w, h, l] = blocks.size;
    let stray = (0..w)
        .flat_map(|x| (0..h).flat_map(move |y| (0..l).map(move |z| [x, y, z])))
        .find(|&pos| {
            let near = |[x, y, z]: [i32; 3]| {
                [
                    [1, 0, 0],
                    [-1, 0, 0],
                    [0, 1, 0],
                    [0, -1, 0],
                    [0, 0, 1],
                    [0, 0, -1],
                ]
                .iter()
                .all(|[dx, dy, dz]| {
                    blocks.get([x + dx, y + dy, z + dz]) == app::schem::AIR
                })
            };
            blocks.get(pos) == app::schem::AIR && near(pos)
        })
        .expect("an empty place touching nothing");
    let (pos, lever) = blocks
        .iter()
        .find(|(_, state)| {
            state.contains("lever") && state.contains("powered=false")
        })
        .unwrap();
    let thrown = std::collections::BTreeMap::from([
        (stray, "minecraft:white_concrete".to_owned()),
        (pos, lever.replace("powered=false", "powered=true")),
    ]);
    let data = app::standardize::edit(&current, &thrown).unwrap();
    eprintln!(
        "DEBUG same {} lever {lever} on now {}",
        data == current,
        on(&data)
    );
    let mut prepared =
        import::prepare(&data, false, true, false, false).unwrap();
    prepared.data = data;
    prepared.sha256 = import::sha256(&prepared.data);
    let how = app::models::version::Replacing::NextVersion {
        added_by: None,
        added_from: None,
        note: None,
    };
    app::models::version::replace(&db, &id, &prepared, how)
        .await
        .unwrap();

    let report = simulation::refresh(&db, &id, false).await.unwrap().unwrap();
    assert!(report.passed());
    let now = app::models::version::current_file(&db, &id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(on(&now), levers_on);
}
