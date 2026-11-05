//! A new database filled from the test fixtures, for a checkout with no
//! copy of the live one.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use app::{
    models::{
        schematic,
        user::{DiscordProfile, Role, User},
    },
    web::publish_suggested,
};
use sqlx::SqlitePool;

use crate::simulate::run_tests;

/// Makes the admin account `debug`, as `/dev/login` does, and publishes
/// every `.schem` file in `dir` as its upload, each with the details its
/// preview suggests, then runs their tests and timing. A file the preview
/// suggests no category for is left out, and a build that fails its test
/// kept as failing, as some fixtures are meant to. Refuses a database that
/// has builds already, such as a pull of the live one.
pub(super) async fn seed(db: &SqlitePool, dir: &Path) -> Result<()> {
    ensure!(
        schematic::all(db).await?.is_empty(),
        "the database has builds already; seed only a new one"
    );
    let profile = DiscordProfile {
        id: "debug".to_owned(),
        username: "debug".to_owned(),
        display_name: Some("Debug".to_owned()),
        avatar: None,
    };
    let user = User::sign_in(db, &profile).await?;
    User::set_role(db, user.id, Role::Admin).await?;
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    files.retain(|path| path.extension().is_some_and(|ext| ext == "schem"));
    files.sort();
    let mut published = 0;
    for path in &files {
        let raw = std::fs::read(path)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        match publish_suggested(db, &user, &raw).await? {
            Ok(id) => {
                println!("{}  {name}", &id[..8]);
                published += 1;
            }
            Err(why) => println!("skipped   {name}: {why}"),
        }
    }
    let ids = schematic::all(db)
        .await?
        .into_iter()
        .map(|s| s.id)
        .collect();
    let failed = run_tests(db, ids, false).await?;
    println!(
        "{published} of {} files published, {failed} failing their tests",
        files.len()
    );
    Ok(())
}
