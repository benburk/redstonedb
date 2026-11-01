//! A build's original colours: kept with the version its builder's file
//! matches, carried by later versions of the same size, such as one with
//! its unused blocks taken out, and dropped by one of another size.

use app::{
    colouring,
    details::{self, Build, Details},
    import,
    models::{
        schematic::Orientation,
        version::{self, Replacing},
    },
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

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

fn prepared(name: &str) -> import::Prepared {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    import::prepare(&raw, false, true, false, false).expect("fixture prepares")
}

#[tokio::test]
async fn colours_carry_to_later_versions_of_the_same_size() {
    let db = setup_test_db().await;
    let first = prepared("alpha-rca");
    let given = Details {
        category: "adder".to_owned(),
        orientation: Orientation::Horizontal,
        ..Details::default()
    };
    let ready = details::check(&db, given, Build::of(&first), false)
        .await
        .unwrap()
        .unwrap();
    let id = import::store(&db, &first, &ready, None, None, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(version::colours(&db, &id).await.unwrap().is_none());

    // The builder's copy in blue wool, turned a half turn.
    let blocks = app::schem::Blocks::read(&first.data).unwrap();
    let wool: std::collections::BTreeMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| app::schem::short(state) == "white_concrete")
        .map(|(pos, _)| (pos, "minecraft:blue_wool".to_owned()))
        .collect();
    let theirs = app::standardize::edit(&first.data, &wool).unwrap();
    let turn = app::orient::Turn {
        mirrored: false,
        quarters: 2,
    };
    let theirs = app::orient::turned(&theirs, turn).unwrap();
    let colours = colouring::read(&theirs, &first.data).unwrap().unwrap();
    assert_eq!(colours.len(), wool.len());
    version::set_colours(&db, &id, 1, &colours).await.unwrap();

    // A version the site makes of the same size keeps them.
    let mut same = prepared("alpha-rca");
    same.data = app::standardize::edit(
        &first.data,
        &std::collections::BTreeMap::from([(
            *wool.keys().next().unwrap(),
            "minecraft:air".to_owned(),
        )]),
    )
    .unwrap();
    same.sha256 = import::sha256(&same.data);
    let how = Replacing::NextVersion {
        added_by: None,
        added_from: None,
        note: Some("Unused blocks taken out"),
    };
    version::replace(&db, &id, &same, how).await.unwrap();
    assert_eq!(version::colours(&db, &id).await.unwrap(), Some(colours));

    // Another size is another arrangement, and has none.
    version::replace(&db, &id, &prepared("inverter-1"), how)
        .await
        .unwrap();
    assert!(version::colours(&db, &id).await.unwrap().is_none());
}
