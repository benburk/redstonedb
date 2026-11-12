//! Private builds, listed only for their uploader and admins but open to
//! anyone with the link, and each account's favourites.

mod common;

use app::{
    account::{Role, User},
    build::Details,
    store::builds,
};
use sqlx::SqlitePool;

/// Stores the fixture `name` as an adder uploaded by `by`, and returns its
/// id.
async fn store(db: &SqlitePool, name: &str, by: &User) -> String {
    let given = Details {
        category: "adder".to_owned(),
        ..Details::default()
    };
    common::store(db, name, given, Some(by)).await
}

#[tokio::test]
async fn a_private_build_is_listed_only_for_its_uploader_and_admins() {
    let db = common::database().await;
    let owner = common::sign_in(&db, "1", "steve").await;
    let other = common::sign_in(&db, "2", "alex").await;
    let admin = common::sign_in(&db, "3", "boss").await;
    let moderator = common::sign_in(&db, "4", "mod").await;
    User::set_role(&db, admin.id, Role::Admin).await.unwrap();
    User::set_role(&db, moderator.id, Role::Mod).await.unwrap();
    let admin = User::get_by_username(&db, "boss").await.unwrap().unwrap();
    let moderator = User::get_by_username(&db, "mod").await.unwrap().unwrap();

    let id = store(&db, "alpha-rca", &owner).await;
    let s = builds::get(&db, &id).await.unwrap().unwrap();
    assert!(s.entry.public, "a build is public unless made private");
    assert!(app::account::lists(None, &s));

    builds::set_public(&db, &id, false).await.unwrap();
    let listed = |viewer: Option<&User>| {
        let db = db.clone();
        let id = id.clone();
        let viewer = viewer.cloned();
        async move {
            let all = builds::visible(&db, viewer.as_ref()).await.unwrap();
            all.iter().any(|s| s.id == id)
        }
    };
    assert!(!listed(None).await);
    assert!(!listed(Some(&other)).await);
    assert!(!listed(Some(&moderator)).await);
    assert!(listed(Some(&owner)).await);
    assert!(listed(Some(&admin)).await);
    // Its link still opens it, for anyone.
    assert!(builds::get(&db, &id).await.unwrap().is_some());

    builds::set_public(&db, &id, true).await.unwrap();
    assert!(listed(None).await);
}

#[tokio::test]
async fn favourites_are_each_accounts_own() {
    let db = common::database().await;
    let steve = common::sign_in(&db, "1", "steve").await;
    let alex = common::sign_in(&db, "2", "alex").await;
    let id = store(&db, "alpha-rca", &steve).await;

    assert!(!builds::favorited(&db, alex.id, &id).await.unwrap());
    builds::set_favorite(&db, alex.id, &id, true).await.unwrap();
    // Favouriting twice keeps one.
    builds::set_favorite(&db, alex.id, &id, true).await.unwrap();
    assert!(builds::favorited(&db, alex.id, &id).await.unwrap());
    // Its cards and page count each account that favourited it once.
    assert_eq!(builds::favorite_count(&db, &id).await.unwrap(), 1);
    let s = builds::get(&db, &id).await.unwrap().unwrap();
    assert_eq!(s.entry.favorites, 1);
    assert_eq!(
        builds::favorites(&db, alex.id).await.unwrap(),
        std::slice::from_ref(&id)
    );
    assert!(builds::favorites(&db, steve.id).await.unwrap().is_empty());

    builds::set_favorite(&db, alex.id, &id, false)
        .await
        .unwrap();
    assert!(builds::favorites(&db, alex.id).await.unwrap().is_empty());

    // Deleting the build takes it out of everyone's favourites, and
    // restoring it puts it back.
    builds::set_favorite(&db, alex.id, &id, true).await.unwrap();
    assert!(builds::delete(&db, &id, steve.id).await.unwrap());
    assert!(builds::favorites(&db, alex.id).await.unwrap().is_empty());
    assert!(builds::undelete(&db, &id, steve.id).await.unwrap());
    assert_eq!(
        builds::favorites(&db, alex.id).await.unwrap(),
        std::slice::from_ref(&id)
    );
}
