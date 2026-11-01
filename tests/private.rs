//! Private builds, seen only by their uploader and admins, and each
//! account's favourites.

use app::{
    details::{self, Build, Details},
    import,
    models::{
        schematic::{self, Orientation},
        user::{DiscordProfile, Role, User},
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

async fn sign_in(db: &SqlitePool, id: &str, username: &str) -> User {
    let profile = DiscordProfile {
        id: id.to_owned(),
        username: username.to_owned(),
        display_name: None,
        avatar: None,
    };
    User::sign_in(db, &profile).await.expect("sign in failed")
}

/// Stores the fixture `name`, uploaded by `by`, and returns its id.
async fn store(db: &SqlitePool, name: &str, by: &User) -> String {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let prepared = import::prepare(&raw, false, true, false, false)
        .expect("fixture prepares");
    let given = Details {
        category: "adder".to_owned(),
        orientation: Orientation::Horizontal,
        ..Details::default()
    };
    let ready = details::check(db, given, Build::of(&prepared), false)
        .await
        .expect("the database answers")
        .expect("the details keep the rules");
    import::store(db, &prepared, &ready, Some(by.id), None, None, None)
        .await
        .expect("the database answers")
        .expect("the file is new")
}

#[tokio::test]
async fn a_private_build_shows_only_to_its_uploader_and_admins() {
    let db = setup_test_db().await;
    let owner = sign_in(&db, "1", "steve").await;
    let other = sign_in(&db, "2", "alex").await;
    let admin = sign_in(&db, "3", "boss").await;
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
    let db = setup_test_db().await;
    let steve = sign_in(&db, "1", "steve").await;
    let alex = sign_in(&db, "2", "alex").await;
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
