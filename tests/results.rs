//! Results carried between copies of the database: taken while a build's
//! file and details are as they were, refused once either changes.

use app::{
    details::{self, Build, Details},
    import,
    models::{
        results::{self, Taken},
        schematic, simulation, version,
    },
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

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

#[tokio::test]
async fn results_are_taken_only_while_the_build_is_as_it_was() {
    let db = setup_test_db().await;
    let raw = std::fs::read("tests/fixtures/rca-slice-capped.schem").unwrap();
    let prepared = import::prepare(&raw, false, true, false, false).unwrap();
    let typed = Details {
        category: "adder".to_owned(),
        ..Details::default()
    };
    let ready = details::check(&db, typed, Build::of(&prepared), false)
        .await
        .unwrap()
        .unwrap();
    let id = import::store(&db, &prepared, &ready, None, None, None, None)
        .await
        .unwrap()
        .unwrap();
    simulation::refresh(&db, &id, false).await.unwrap();

    // As if worked out elsewhere: a latency the build does not have.
    let mut sent = results::export(&db, std::slice::from_ref(&id))
        .await
        .unwrap();
    assert!(sent[0].simulation.is_some());
    sent[0].latency = Some(99);
    let taken = results::import(&db, &sent).await.unwrap();
    assert_eq!(taken, [(id.clone(), Taken::Taken)]);
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.latency, Some(99));

    // Once its details are edited, results worked out before are refused.
    let mut edited = Details::of(&s);
    edited.description = Some("Edited since.".to_owned());
    let data = version::current_file(&db, &id).await.unwrap().unwrap();
    let build = Build {
        data: &data,
        conventional: true,
    };
    let ready = details::check(&db, edited, build, false)
        .await
        .unwrap()
        .unwrap();
    schematic::set_details(&db, &id, &ready).await.unwrap();
    sent[0].latency = Some(7);
    let taken = results::import(&db, &sent).await.unwrap();
    assert!(matches!(taken[0].1, Taken::Stale(_)), "{taken:?}");
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.latency, Some(99), "the stale results changed nothing");
}
