//! A new database filled from the test fixtures, for a checkout with no
//! copy of the live one.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use app::{
    models::{
        simulation,
        user::{DiscordProfile, Role, User},
    },
    store::builds,
    web::publish_suggested,
};
use sqlx::SqlitePool;

use crate::refresh::AT_ONCE;

/// Makes the admin account `debug`, as `/dev/login` does, and publishes
/// every `.schem` file in `dir` as its upload, each with the details its
/// preview suggests, simulated and stored whole as it goes in, `AT_ONCE`
/// at a time. A file the preview suggests no category for is left out,
/// and a build that fails its test kept as failing, as some fixtures are
/// meant to. Refuses a database that has builds already, such as a pull of
/// the live one.
pub(super) async fn seed(db: &SqlitePool, dir: &Path) -> Result<()> {
    ensure!(
        builds::all(db).await?.is_empty(),
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
    simulation::run_at_once(AT_ONCE);
    let gate = Arc::new(tokio::sync::Semaphore::new(AT_ONCE));
    let mut running = tokio::task::JoinSet::new();
    for path in files.clone() {
        let (db, user, gate) = (db.clone(), user.clone(), Arc::clone(&gate));
        running.spawn(async move {
            let _turn = gate.acquire_owned().await?;
            let raw = std::fs::read(&path)?;
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let said = match publish_suggested(&db, &user, &raw).await? {
                Ok(id) => {
                    println!("{}  {name}", &id[..8]);
                    true
                }
                Err(why) => {
                    println!("skipped   {name}: {why}");
                    false
                }
            };
            anyhow::Ok(said)
        });
    }
    let mut published = 0;
    while let Some(done) = running.join_next().await {
        published += usize::from(done??);
    }
    println!("{published} of {} files published", files.len());
    Ok(())
}
