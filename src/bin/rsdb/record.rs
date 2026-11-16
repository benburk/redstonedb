//! Taking a record of what recognition makes of the fixtures or the
//! stored builds, see [`app::ingest::record`], to compare before and
//! after a change: `diff before.jsonl after.jsonl`. Nothing here writes to
//! the database.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use app::{
    ingest::record::{self, Subject},
    store::{builds, versions},
};
use clap::Args;
use sqlx::SqlitePool;

#[derive(Args)]
pub(super) struct Record {
    /// Where to write the record, one line of JSON a file
    #[arg(long)]
    out: PathBuf,
    /// Record every .schem file in this directory, such as tests/fixtures
    #[arg(long)]
    fixtures: Option<PathBuf>,
    /// Record every stored build of a standard category
    #[arg(long)]
    builds: bool,
}

pub(super) async fn record_command(
    db: &SqlitePool,
    args: &Record,
) -> Result<()> {
    let mut subjects = Vec::new();
    if let Some(dir) = &args.fixtures {
        subjects.extend(record::fixtures(dir)?);
    }
    if args.builds {
        for s in builds::all(db).await? {
            if s.kind().is_none() {
                continue;
            }
            let data = versions::current_file(db, &s.id)
                .await?
                .context("schematic vanished")?;
            subjects.push(Subject {
                name: format!("{}  {}", &s.id[..8], s.filename()),
                data,
                stored: Some((s.category.clone(), s.specs.width)),
            });
        }
    }
    let total = subjects.len();
    let lines =
        tokio::task::spawn_blocking(move || record::record(&subjects)).await?;
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(&args.out, text)
        .with_context(|| format!("writing {}", args.out.display()))?;
    eprintln!("Recorded {total} files in {}", args.out.display());
    Ok(())
}
