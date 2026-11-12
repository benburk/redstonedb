//! What the integration tests share: a fresh database, the fixture files,
//! and accounts and builds to work with.

// Each test file is a crate of its own, using only some of these.
#![allow(dead_code)]

use app::{
    account::User,
    details::{self, Candidate, Details},
    external::discord::DiscordProfile,
    import::{self, Prepared, Purpose},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// An in-memory database with the migrations applied.
pub async fn database() -> SqlitePool {
    migrated(SqlitePoolOptions::new()).await
}

/// An in-memory database opened with `options`, with the migrations
/// applied.
pub async fn migrated(options: SqlitePoolOptions) -> SqlitePool {
    let pool = options
        .connect("sqlite::memory:")
        .await
        .expect("an in-memory database opens");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("the migrations run");
    pool
}

/// The file of the fixture `name`, under `tests/fixtures`, carrying the
/// roles its colours mark, as the fixtures are saved in the site's old
/// colours; see `legacy::colour_roles`.
pub fn fixture(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    app::recognize::legacy::with_colour_roles(&raw)
        .expect("fixture carries its roles")
}

/// A conventional build's file kept where it lies: neither standardized
/// nor turned.
pub const IN_PLACE: Purpose = Purpose::InPlace {
    standardize: false,
    conventional: true,
    keep_paste: false,
};

/// The fixture `name` as a conventional build's file is kept where it
/// lies; see [`IN_PLACE`].
pub fn conventional(name: &str) -> Prepared {
    import::prepare_for(&fixture(name), IN_PLACE).expect("fixture prepares")
}

/// Signs in the Discord account `id` as `username`, shown as
/// `{username} shown`, making its account the first time.
pub async fn sign_in(db: &SqlitePool, id: &str, username: &str) -> User {
    let profile = DiscordProfile {
        id: id.to_owned(),
        username: username.to_owned(),
        display_name: Some(format!("{username} shown")),
        avatar: None,
    };
    User::sign_in(db, &profile).await.expect("sign in failed")
}

/// Stores the fixture `name` as a conventional build with `details`,
/// uploaded by `by` if anyone, and returns its id.
pub async fn store(
    db: &SqlitePool,
    name: &str,
    details: Details,
    by: Option<&User>,
) -> String {
    let prepared = conventional(name);
    let ready = details::check(db, details, Candidate::of(&prepared), false)
        .await
        .expect("the database answers")
        .expect("the details keep the rules");
    import::store(db, &prepared, &ready, by.map(|u| u.id), None, None, None)
        .await
        .expect("the database answers")
        .expect("the file is new")
}
