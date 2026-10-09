//! Integration tests for accounts and sessions.

use app::models::{
    session::Session,
    user::{DiscordProfile, Role, User},
};
use app::util::current_time_micros;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

/// Creates an in-memory `SQLite` database with migrations applied.
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

fn profile(id: &str, username: &str) -> DiscordProfile {
    DiscordProfile {
        id: id.to_owned(),
        username: username.to_owned(),
        display_name: Some(format!("{username} shown")),
        avatar: None,
    }
}

async fn sign_in(db: &SqlitePool, id: &str, username: &str) -> User {
    User::sign_in(db, &profile(id, username))
        .await
        .expect("sign in failed")
}

fn create_test_session(user_id: Uuid) -> Session {
    Session {
        id: Uuid::new_v4(),
        user_id,
        expires_at: current_time_micros() + 604_800_000_000, // a week
    }
}

#[tokio::test]
async fn first_login_makes_a_user_account() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "steve").await;
    assert_eq!(user.username, "steve");
    assert_eq!(user.name(), "steve shown");
    assert_eq!(user.role, Role::User);
    assert!(!user.can_edit());

    let fetched = User::get_by_username(&db, "steve").await.unwrap().unwrap();
    assert_eq!(fetched.id, user.id);
}

#[tokio::test]
async fn later_logins_keep_the_account_and_refresh_its_name() {
    let db = setup_test_db().await;
    let first = sign_in(&db, "1", "steve").await;
    User::set_role(&db, first.id, Role::Mod).await.unwrap();

    let again = sign_in(&db, "1", "steve_renamed").await;
    assert_eq!(again.id, first.id);
    assert_eq!(again.username, "steve_renamed");
    assert_eq!(again.role, Role::Mod, "a login keeps the role");
    assert!(User::get_by_username(&db, "steve").await.unwrap().is_none());
}

#[tokio::test]
async fn a_taken_username_moves_to_its_new_owner() {
    let db = setup_test_db().await;
    let old = sign_in(&db, "1", "alex").await;
    // Discord gave the name to someone else after the first account
    // renamed itself there.
    let new = sign_in(&db, "2", "alex").await;
    assert_ne!(old.id, new.id);
    let old = User::get_by_id(&db, old.id).await.unwrap().unwrap();
    assert_eq!(old.username, "1", "the old holder keeps its Discord id");
}

#[tokio::test]
async fn roles_rank_user_mod_admin() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "steve").await;
    for (role, can_edit, admin) in [
        (Role::User, false, false),
        (Role::Mod, true, false),
        (Role::Admin, true, true),
    ] {
        User::set_role(&db, user.id, role).await.unwrap();
        let user = User::get_by_id(&db, user.id).await.unwrap().unwrap();
        assert_eq!(user.can_edit(), can_edit);
        assert_eq!(user.is_admin(), admin);
    }
}

#[tokio::test]
async fn session_insert_and_get_by_id() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "sessionuser").await;

    let session = create_test_session(user.id);
    Session::insert(&db, &session)
        .await
        .expect("session insert failed");

    let fetched = Session::get_by_id(&db, session.id)
        .await
        .expect("get failed")
        .expect("session missing");
    assert_eq!(fetched.user_id, user.id);
}

#[tokio::test]
async fn logging_out_deletes_the_session() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "deleteuser").await;

    let session = create_test_session(user.id);
    Session::insert(&db, &session)
        .await
        .expect("session insert failed");
    Session::delete_by_id(&db, session.id)
        .await
        .expect("delete failed");
    assert!(Session::get_by_id(&db, session.id).await.unwrap().is_none());
}

#[tokio::test]
async fn expired_sessions_are_cleared_out() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "expireuser").await;

    let now = current_time_micros();
    let expired = Session {
        id: Uuid::new_v4(),
        user_id: user.id,
        expires_at: now - 1,
    };
    let valid = create_test_session(user.id);
    Session::insert(&db, &expired).await.unwrap();
    Session::insert(&db, &valid).await.unwrap();

    Session::delete_expired(&db, now).await.unwrap();
    assert!(Session::get_by_id(&db, expired.id).await.unwrap().is_none());
    assert!(Session::get_by_id(&db, valid.id).await.unwrap().is_some());
}

#[tokio::test]
async fn deleting_a_user_deletes_their_sessions() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "cascadeuser").await;

    let session = create_test_session(user.id);
    Session::insert(&db, &session)
        .await
        .expect("session insert failed");

    sqlx::query("DELETE FROM user WHERE id = ?")
        .bind(user.id)
        .execute(&db)
        .await
        .expect("user delete failed");

    assert!(Session::get_by_id(&db, session.id).await.unwrap().is_none());
}

#[tokio::test]
async fn an_account_goes_by_its_minecraft_name() {
    let db = setup_test_db().await;
    let user = sign_in(&db, "1", "steve").await;
    User::set_minecraft_name(&db, user.id, Some("Steve_Builds"), None)
        .await
        .unwrap()
        .unwrap();
    let user = User::get_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(user.name(), "Steve_Builds");
}

#[tokio::test]
async fn a_confirmed_name_respells_its_credits_and_imports() {
    let db = setup_test_db().await;
    sqlx::query(
        "INSERT INTO schematic (id, category, orientation, size_x, size_y,
            size_z, sha256, data, imported_by)
         VALUES ('b', 'adder', 'horz', 1, 1, 1, 'h', x'00', 'PISHLEBACK')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credit (schematic_id, role, name, position)
         VALUES ('b', 'builder', 'Pishleback', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let spelled = || async {
        sqlx::query_as::<_, (String, String)>(
            "SELECT credit.name, imported_by FROM credit
             JOIN schematic ON schematic.id = credit.schematic_id",
        )
        .fetch_one(&db)
        .await
        .unwrap()
    };

    let user = sign_in(&db, "1", "pishleback").await;
    User::set_minecraft_name(&db, user.id, Some("pishleback"), None)
        .await
        .unwrap()
        .unwrap();
    // Unconfirmed, the name respells nothing.
    assert_eq!(spelled().await, ("Pishleback".into(), "PISHLEBACK".into()));

    User::set_minecraft_verified(&db, user.id, true)
        .await
        .unwrap();
    assert_eq!(spelled().await, ("pishleback".into(), "pishleback".into()));

    // Confirmed, the same name in new capitals keeps its confirmation and
    // respells again.
    User::set_minecraft_name(&db, user.id, Some("PishLeback"), None)
        .await
        .unwrap()
        .unwrap();
    let user = User::get_by_id(&db, user.id).await.unwrap().unwrap();
    assert!(user.minecraft_verified);
    assert_eq!(spelled().await, ("PishLeback".into(), "PishLeback".into()));
}

#[tokio::test]
async fn only_an_admin_may_name_a_new_tag() {
    use app::models::schematic::check_new_tags;
    let db = setup_test_db().await;
    let known = vec!["torchless".to_owned(), "comb".to_owned()];
    let new = vec!["torchless".to_owned(), "made_up".to_owned()];
    assert_eq!(check_new_tags(&db, &known, false).await.unwrap(), Ok(()));
    let refused = check_new_tags(&db, &new, false).await.unwrap().unwrap_err();
    assert!(refused.contains("made up"), "{refused}");
    assert!(refused.contains("only an admin"), "{refused}");
    assert_eq!(check_new_tags(&db, &new, true).await.unwrap(), Ok(()));
}
