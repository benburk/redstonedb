//! Integration tests for accounts and sessions.

mod common;

use app::{
    account::{Role, SchematiLink, User},
    store::{
        self,
        sessions::{Session, now_micros},
    },
};
use sqlx::SqlitePool;
use uuid::Uuid;

fn create_test_session(user_id: Uuid) -> Session {
    Session {
        id: Uuid::new_v4(),
        user_id,
        expires_at: now_micros() + 604_800_000_000, // a week
    }
}

#[tokio::test]
async fn first_login_makes_a_user_account() {
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "steve").await;
    assert_eq!(user.username, "steve");
    assert_eq!(user.name(), "steve shown");
    assert_eq!(user.role, Role::User);
    assert!(!user.can_edit());

    let fetched = User::get_by_username(&db, "steve").await.unwrap().unwrap();
    assert_eq!(fetched.id, user.id);
}

#[tokio::test]
async fn later_logins_keep_the_account_and_refresh_its_name() {
    let db = common::database().await;
    let first = common::sign_in(&db, "1", "steve").await;
    User::set_role(&db, first.id, Role::Mod).await.unwrap();

    let again = common::sign_in(&db, "1", "steve_renamed").await;
    assert_eq!(again.id, first.id);
    assert_eq!(again.username, "steve_renamed");
    assert_eq!(again.role, Role::Mod, "a login keeps the role");
    assert!(User::get_by_username(&db, "steve").await.unwrap().is_none());
}

#[tokio::test]
async fn a_taken_username_moves_to_its_new_owner() {
    let db = common::database().await;
    let old = common::sign_in(&db, "1", "alex").await;
    // Discord gave the name to someone else after the first account
    // renamed itself there.
    let new = common::sign_in(&db, "2", "alex").await;
    assert_ne!(old.id, new.id);
    let old = User::get_by_id(&db, old.id).await.unwrap().unwrap();
    assert_eq!(old.username, "1", "the old holder keeps its Discord id");
}

#[tokio::test]
async fn roles_rank_user_mod_admin() {
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "steve").await;
    // A mod edits and deletes any build, and makes new tags; only an
    // admin sets roles.
    for (role, moderates, admin) in [
        (Role::User, false, false),
        (Role::Mod, true, false),
        (Role::Admin, true, true),
    ] {
        User::set_role(&db, user.id, role).await.unwrap();
        let user = User::get_by_id(&db, user.id).await.unwrap().unwrap();
        assert_eq!(user.can_edit(), moderates);
        assert_eq!(user.can_delete(), moderates);
        assert_eq!(user.can_create_tags(), moderates);
        assert_eq!(user.is_admin(), admin);
    }
}

#[tokio::test]
async fn session_insert_and_get_by_id() {
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "sessionuser").await;

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
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "deleteuser").await;

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
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "expireuser").await;

    let now = now_micros();
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
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "cascadeuser").await;

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
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "steve").await;
    User::set_minecraft_name(&db, user.id, Some("Steve_Builds"), None)
        .await
        .unwrap()
        .unwrap();
    let user = User::get_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(user.name(), "Steve_Builds");
}

#[tokio::test]
async fn a_confirmed_name_respells_its_credits_and_imports() {
    let db = common::database().await;
    sqlx::query(
        "INSERT INTO build (id, category, orientation, size_x, size_y,
            size_z, imported_by)
         VALUES ('b', 'adder', 'horz', 1, 1, 1, 'PISHLEBACK')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credit (build_id, role, name, position)
         VALUES ('b', 'author', 'Pishleback', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let spelled = || async {
        sqlx::query_as::<_, (String, String)>(
            "SELECT credit.name, imported_by FROM credit
             JOIN build ON build.id = credit.build_id",
        )
        .fetch_one(&db)
        .await
        .unwrap()
    };

    let user = common::sign_in(&db, "1", "pishleback").await;
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
async fn only_a_mod_or_admin_may_name_a_new_tag() {
    use app::{
        build::tag::Kind,
        store::tags::{check_new, define},
    };
    let db = common::database().await;
    define(&db, "instant", None, Kind::Other).await.unwrap();
    let known = vec!["instant".to_owned()];
    let new = vec!["instant".to_owned(), "made_up".to_owned()];
    assert_eq!(check_new(&db, &known, false).await.unwrap(), Ok(()));
    let refused = check_new(&db, &new, false).await.unwrap().unwrap_err();
    assert!(refused.contains("made up"), "{refused}");
    assert!(refused.contains("only a mod or admin"), "{refused}");
    assert_eq!(check_new(&db, &new, true).await.unwrap(), Ok(()));
}

/// What the account's Schemati session is known to be, as Settings reads
/// it.
async fn schemati_link(db: &SqlitePool, user: &User) -> Option<SchematiLink> {
    User::schemati_link(db, user.id).await.unwrap()
}

#[tokio::test]
async fn a_saved_schemati_session_keeps_its_player_until_disconnected() {
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "steve").await;
    assert_eq!(schemati_link(&db, &user).await, None);

    User::connect_schemati(&db, user.id, "aaaa", Some("embizone"))
        .await
        .unwrap();
    let link = schemati_link(&db, &user).await.unwrap();
    assert_eq!(link.player.as_deref(), Some("embizone"));
    assert!(link.checked.is_some());
    assert!(!link.expired);
    let session = User::schemati_session(&db, user.id).await.unwrap();
    assert_eq!(session.as_deref(), Some("aaaa"));

    User::schemati_forgot(&db, user.id, "aaaa").await.unwrap();
    User::disconnect_schemati(&db, user.id).await.unwrap();
    assert_eq!(schemati_link(&db, &user).await, None);
    assert_eq!(User::schemati_session(&db, user.id).await.unwrap(), None);
    // Nothing of the old session is left for a new one to show.
    User::schemati_answered(&db, user.id, "aaaa", Some("embizone"))
        .await
        .unwrap();
    assert_eq!(schemati_link(&db, &user).await, None);
}

/// What Schemati says of a session is kept only while it is the one the
/// account gave, so an answer to an older one changes nothing.
#[tokio::test]
async fn schemati_answers_mark_the_session_expired_or_working() {
    let db = common::database().await;
    let user = common::sign_in(&db, "1", "steve").await;
    User::connect_schemati(&db, user.id, "aaaa", None)
        .await
        .unwrap();

    User::schemati_forgot(&db, user.id, "bbbb").await.unwrap();
    assert!(!schemati_link(&db, &user).await.unwrap().expired);
    User::schemati_forgot(&db, user.id, "aaaa").await.unwrap();
    assert!(schemati_link(&db, &user).await.unwrap().expired);

    // Answering again, it works after all, and names its player.
    User::schemati_answered(&db, user.id, "aaaa", Some("embizone"))
        .await
        .unwrap();
    let link = schemati_link(&db, &user).await.unwrap();
    assert!(!link.expired);
    assert_eq!(link.player.as_deref(), Some("embizone"));
    // An answer naming no one keeps the player known.
    User::schemati_answered(&db, user.id, "aaaa", None)
        .await
        .unwrap();
    let link = schemati_link(&db, &user).await.unwrap();
    assert_eq!(link.player.as_deref(), Some("embizone"));

    // A new session starts over, whatever the old one was.
    User::schemati_forgot(&db, user.id, "aaaa").await.unwrap();
    User::connect_schemati(&db, user.id, "cccc", Some("Other"))
        .await
        .unwrap();
    let link = schemati_link(&db, &user).await.unwrap();
    assert!(!link.expired);
    assert_eq!(link.player.as_deref(), Some("Other"));
}

/// A player looked up lately answers from what was kept, not Mojang: a
/// name no player has, kept as one that some player has, comes back as
/// kept, which Mojang would not say, beside one kept as no player's.
#[tokio::test]
async fn a_player_looked_up_lately_is_not_asked_about_again() {
    use app::external::mojang::Profile;
    let db = common::database().await;
    let kept = Profile {
        uuid: "0123456789abcdef0123456789abcdef".to_owned(),
        name: "Not_A_Real_Player_Q".to_owned(),
    };
    store::players::remember(&db, "not_a_real_player_q", Some(&kept))
        .await
        .unwrap();
    store::players::remember(&db, "nobody_here_q", None)
        .await
        .unwrap();
    let names = ["NOT_A_REAL_PLAYER_Q".to_owned(), "Nobody_Here_Q".to_owned()];
    let players = app::players::look_up(&db, &names).await.unwrap();
    assert_eq!(players["not_a_real_player_q"], Some(kept));
    assert_eq!(players["nobody_here_q"], None);
}
