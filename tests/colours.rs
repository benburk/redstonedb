//! A build's original colours: kept with the version its author's file
//! matches, carried by later versions of the same size, such as one with
//! its unused blocks taken out, and dropped by one of another size.

mod common;

use app::{
    build::Details,
    ingest::{
        details::{self, Candidate},
        publish,
    },
    recognize::colouring,
    store::versions::{self, Replacing},
};

#[tokio::test]
async fn colours_carry_to_later_versions_of_the_same_size() {
    let db = common::database().await;
    let first = common::conventional("alpha-rca");
    let given = Details {
        category: "adder".to_owned(),
        ..Details::default()
    };
    let ready = details::check(&db, given, Candidate::of(&first), false)
        .await
        .unwrap()
        .unwrap();
    let id = publish::store(&db, &first, &ready, None, None, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(versions::colours(&db, &id).await.unwrap().is_none());

    // The author's copy in blue wool, turned a half turn.
    let blocks = app::schem::Blocks::read(&first.data).unwrap();
    let wool: std::collections::BTreeMap<_, _> = blocks
        .iter()
        .filter(|(_, state)| app::schem::short(state) == "white_concrete")
        .map(|(pos, _)| (pos, "minecraft:blue_wool".to_owned()))
        .collect();
    let theirs = app::schem::edit(&first.data, &wool).unwrap();
    let turn = app::schem::turn::Turn {
        mirrored: false,
        quarters: 2,
    };
    let theirs = app::schem::turn::turned(&theirs, turn).unwrap();
    let colours = colouring::read(&theirs, &first.data).unwrap().unwrap();
    assert_eq!(colours.len(), wool.len());
    versions::set_colours(&db, &id, 1, &colours).await.unwrap();

    // A version the site makes of the same size keeps them.
    let mut same = common::conventional("alpha-rca");
    same.data = app::schem::edit(
        &first.data,
        &std::collections::BTreeMap::from([(
            *wool.keys().next().unwrap(),
            "minecraft:air".to_owned(),
        )]),
    )
    .unwrap();
    same.sha256 = app::schem::sha256(&same.data);
    let how = Replacing::NextVersion {
        added_by: None,
        added_from: None,
        note: Some("Unused blocks taken out"),
        restores: None,
    };
    versions::replace(&db, &id, &same, how).await.unwrap();
    assert_eq!(versions::colours(&db, &id).await.unwrap(), Some(colours));

    // Another size is another arrangement, and has none.
    versions::replace(&db, &id, &common::conventional("inverter-1"), how)
        .await
        .unwrap();
    assert!(versions::colours(&db, &id).await.unwrap().is_none());
}

/// A new version saved again from the game lying another way, here a half
/// turn, is compared turned back, so it keeps every block.
#[tokio::test]
async fn a_turned_new_version_is_compared_turned_back() {
    let db = common::database().await;
    let first = common::conventional("alpha-rca");
    let given = Details {
        category: "adder".to_owned(),
        ..Details::default()
    };
    let ready = details::check(&db, given, Candidate::of(&first), false)
        .await
        .unwrap()
        .unwrap();
    let id = publish::store(&db, &first, &ready, None, None, None, None)
        .await
        .unwrap()
        .unwrap();
    let s = app::store::builds::get(&db, &id).await.unwrap().unwrap();
    let turn = app::schem::turn::Turn {
        mirrored: true,
        quarters: 2,
    };
    let turned = app::schem::turn::turned(&first.data, turn).unwrap();
    let checked = app::ingest::version::check(&db, &s, &turned, false)
        .await
        .unwrap();
    assert_eq!(checked.comparison.changed, 0);
}
