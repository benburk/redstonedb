//! Private builds, seen only by their uploader and admins, and each
//! account's favourites.

mod common;

use app::{
    details::Details,
    models::{
        schematic::{self, Orientation},
        user::{Role, User},
    },
};
use sqlx::SqlitePool;

/// Stores the fixture `name` as an adder uploaded by `by`, and returns its
/// id.
async fn store(db: &SqlitePool, name: &str, by: &User) -> String {
    let given = Details {
        category: "adder".to_owned(),
        orientation: Orientation::Horizontal,
        ..Details::default()
    };
    common::store(db, name, given, Some(by)).await
}

#[tokio::test]
async fn a_private_build_shows_only_to_its_uploader_and_admins() {
    let db = common::database().await;
    let owner = common::sign_in(&db, "1", "steve").await;
    let other = common::sign_in(&db, "2", "alex").await;
    let admin = common::sign_in(&db, "3", "boss").await;
    User::set_role(&db, admin.id, Role::Admin).await.unwrap();
    let admin = User::get_by_username(&db, "boss").await.unwrap().unwrap();

    let id = store(&db, "alpha-rca", &owner).await;
    let s = schematic::get(&db, &id).await.unwrap().unwrap();
    assert!(s.public, "a build is public unless made private");
    assert!(s.visible_to(None));

    schematic::set_public(&db, &id, false).await.unwrap();
    let seen = |viewer: Option<&User>| {
        let db = db.clone();
        let id = id.clone();
        let viewer = viewer.cloned();
        async move {
            let listed =
                schematic::visible(&db, viewer.as_ref()).await.unwrap();
            let got = schematic::get_visible(&db, &id, viewer.as_ref())
                .await
                .unwrap();
            assert_eq!(listed.iter().any(|s| s.id == id), got.is_some());
            got.is_some()
        }
    };
    assert!(!seen(None).await);
    assert!(!seen(Some(&other)).await);
    assert!(seen(Some(&owner)).await);
    assert!(seen(Some(&admin)).await);

    schematic::set_public(&db, &id, true).await.unwrap();
    assert!(seen(None).await);
}

#[tokio::test]
async fn favourites_are_each_accounts_own() {
    let db = common::database().await;
    let steve = common::sign_in(&db, "1", "steve").await;
    let alex = common::sign_in(&db, "2", "alex").await;
    let id = store(&db, "alpha-rca", &steve).await;

    assert!(!schematic::favorited(&db, alex.id, &id).await.unwrap());
    schematic::set_favorite(&db, alex.id, &id, true)
        .await
        .unwrap();
    // Favouriting twice keeps one.
    schematic::set_favorite(&db, alex.id, &id, true)
        .await
        .unwrap();
    assert!(schematic::favorited(&db, alex.id, &id).await.unwrap());
    assert_eq!(
        schematic::favorites(&db, alex.id).await.unwrap(),
        std::slice::from_ref(&id)
    );
    assert!(
        schematic::favorites(&db, steve.id)
            .await
            .unwrap()
            .is_empty()
    );

    schematic::set_favorite(&db, alex.id, &id, false)
        .await
        .unwrap();
    assert!(schematic::favorites(&db, alex.id).await.unwrap().is_empty());

    // Deleting the build takes it out of everyone's favourites.
    schematic::set_favorite(&db, alex.id, &id, true)
        .await
        .unwrap();
    schematic::delete(&db, &id).await.unwrap();
    assert!(schematic::favorites(&db, alex.id).await.unwrap().is_empty());
}
