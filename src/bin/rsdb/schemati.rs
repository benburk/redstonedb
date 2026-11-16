//! A player's Schemati folder, read with the session `SCHEMATI_SESSION`
//! holds: its files listed, or one fetched. Nothing here touches the
//! database.

use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use app::external::schemati::{self, Refusal};
use time::OffsetDateTime;

/// The session in `SCHEMATI_SESSION`, from the environment or `.env`.
fn session() -> Result<String> {
    let pasted = dotenvy::var("SCHEMATI_SESSION").context(
        "SCHEMATI_SESSION is not set: put the value of Schemati's sessionId \
         cookie in .env",
    )?;
    schemati::session(&pasted).map_err(|sentence| anyhow!(sentence))
}

/// What Schemati refused, as an error.
fn refused(refusal: Refusal) -> anyhow::Error {
    match refusal {
        Refusal::LoggedOut => anyhow!(
            "Schemati no longer knows the session in SCHEMATI_SESSION: log in \
             to Schemati again and copy the new sessionId cookie into .env"
        ),
        Refusal::Said(sentence) => anyhow!(sentence),
    }
}

/// Prints the player's files, newest first when Schemati says when each
/// was saved, each with when.
pub(super) async fn list() -> Result<()> {
    let folder = schemati::folder(&session()?).await.map_err(refused)?;
    println!("{}'s Schemati folder:", folder.player);
    for file in &folder.files {
        let saved = file
            .modified
            .and_then(|ms| {
                OffsetDateTime::from_unix_timestamp_nanos(
                    i128::from(ms) * 1_000_000,
                )
                .ok()
            })
            .map(|at| {
                format!("{} {:02}:{:02}", at.date(), at.hour(), at.minute())
            });
        match saved {
            Some(saved) => println!("  {saved} UTC  {}", file.name),
            None => println!("  {}", file.name),
        }
    }
    Ok(())
}

/// Fetches the player's file `name`, such as `temp` or `temp.schem`, into
/// `out`, and prints where it went.
pub(super) async fn fetch(name: &str, out: &Path) -> Result<()> {
    let name = schemati::file_name(name);
    let session = session()?;
    let data = schemati::file(&session, &name).await.map_err(refused)?;
    if data.is_empty() {
        bail!("Schemati sent {name} empty");
    }
    fs::create_dir_all(out)
        .with_context(|| format!("creating {}", out.display()))?;
    let path = out.join(&name);
    fs::write(&path, data)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}
