//! A build's original colours: kept with the version its builder's file
//! matches, carried by later versions of the same size, such as one with
//! its unused blocks taken out, and dropped by one of another size.

mod common;

use app::{
    details::{self, Build, Details},
    import,
    models::{
        schematic::Orientation,
        version::{self, Replacing},
    },
    recognize::colouring,
};

#[tokio::test]
async fn colours_carry_to_later_versions_of_the_same_size() {
    let db = common::database().await;
    let first = common::conventional("alpha-rca");
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
    let theirs = app::schem::standardize::edit(&first.data, &wool).unwrap();
    let turn = app::schem::orient::Turn {
        mirrored: false,
        quarters: 2,
    };
    let theirs = app::schem::orient::turned(&theirs, turn).unwrap();
    let colours = colouring::read(&theirs, &first.data).unwrap().unwrap();
    assert_eq!(colours.len(), wool.len());
    version::set_colours(&db, &id, 1, &colours).await.unwrap();

    // A version the site makes of the same size keeps them.
    let mut same = common::conventional("alpha-rca");
    same.data = app::schem::standardize::edit(
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
        restores: None,
    };
    version::replace(&db, &id, &same, how).await.unwrap();
    assert_eq!(version::colours(&db, &id).await.unwrap(), Some(colours));

    // Another size is another arrangement, and has none.
    version::replace(&db, &id, &common::conventional("inverter-1"), how)
        .await
        .unwrap();
    assert!(version::colours(&db, &id).await.unwrap().is_none());
}

/// A new version saved again from the game lying another way, here a half
/// turn, is compared turned back, so it keeps every block.
#[tokio::test]
async fn a_turned_new_version_is_compared_turned_back() {
    let db = common::database().await;
    let first = common::conventional("alpha-rca");
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
    let s = app::models::schematic::get(&db, &id)
        .await
        .unwrap()
        .unwrap();
    let turn = app::schem::orient::Turn {
        mirrored: true,
        quarters: 2,
    };
    let turned = app::schem::orient::turned(&first.data, turn).unwrap();
    let checked = app::version::check(&db, &s, &turned, false).await.unwrap();
    assert_eq!(checked.comparison.changed, 0);
}
