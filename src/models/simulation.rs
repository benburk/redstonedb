//! Each build's simulation result, kept for the file it ran on, and what is
//! derived from the file and the simulation in place of details a person
//! types: an adder's width, and a tested build's latency and whether it is
//! synchronized.

use std::sync::{Arc, atomic::AtomicBool};

use anyhow::{Context, Result};
use sqlx::{SqliteExecutor, SqlitePool};
use tokio::sync::Semaphore;

use crate::{
    import, interface,
    models::schematic::{self, Schematic},
    schem::Blocks,
    simulate::{Report, Test, Timing},
};

/// A stored result.
pub struct Simulation {
    /// The test's name, such as `adder`.
    pub test: String,
    /// The file's hash when it ran.
    pub sha256: String,
    pub report: Report,
}

pub async fn get<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Option<Simulation>> {
    let Some(row) = sqlx::query!(
        "SELECT test, sha256, report FROM simulation WHERE schematic_id = ?",
        id
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(Simulation {
        test: row.test,
        sha256: row.sha256,
        report: serde_json::from_str(&row.report)
            .context("stored simulation report")?,
    }))
}

async fn set<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    test: &str,
    sha256: &str,
    report: &Report,
) -> Result<()> {
    let report = serde_json::to_string(report)?;
    sqlx::query!(
        "INSERT INTO simulation (schematic_id, test, sha256, report)
        VALUES (?, ?, ?, ?)
        ON CONFLICT (schematic_id) DO UPDATE SET test = excluded.test,
            sha256 = excluded.sha256, report = excluded.report,
            ran_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        id,
        test,
        sha256,
        report
    )
    .execute(db)
    .await?;
    Ok(())
}

async fn clear<'e, E: SqliteExecutor<'e>>(db: E, id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM simulation WHERE schematic_id = ?", id)
        .execute(db)
        .await?;
    Ok(())
}

/// Build `s`'s stored result, when it is for its file and the test its
/// category and width call for now.
pub async fn current(db: &SqlitePool, s: &Schematic) -> Result<Option<Report>> {
    let test = Test::of(&s.category, s.width);
    Ok(get(db, &s.id).await?.and_then(|stored| {
        (test.is_some_and(|test| stored.test == test.name())
            && stored.sha256 == s.sha256)
            .then_some(stored.report)
    }))
}

/// One simulation at a time: timing takes every core for a while, and the
/// server has two.
static RUNNING: Semaphore = Semaphore::const_new(1);

/// Runs `test`'s cases on `data`, a `.schem` file, away from the async
/// threads.
pub async fn run(test: Test, data: Vec<u8>) -> Result<Report> {
    let _turn = RUNNING.acquire().await?;
    tokio::task::spawn_blocking(move || Ok(test.run(&Blocks::read(&data)?)))
        .await?
}

/// Measures when `data`'s outputs settle under `test`, away from the async
/// threads. Setting `cancel` stops it early, as when nobody waits for it
/// any more.
pub async fn time(
    test: Test,
    data: Vec<u8>,
    cancel: Arc<AtomicBool>,
) -> Result<Timing> {
    let _turn = RUNNING.acquire().await?;
    tokio::task::spawn_blocking(move || {
        test.time(&Blocks::read(&data)?, &cancel)
    })
    .await?
}

/// The width `category` reads from a conventional build's file, if it reads
/// one.
pub fn derived_width(
    category: &str,
    conventional: bool,
    data: &[u8],
) -> Result<Option<i64>> {
    if !conventional {
        return Ok(None);
    }
    let blocks = Blocks::read(data)?;
    Ok(import::derived_width(category, &interface::read(&blocks)))
}

/// Brings build `id` up to date with its file, category, and width: sets
/// the width its category reads from the file, runs its test's cases when
/// the stored result is for another file or test, or always when `again`,
/// and drops the result when it has no test. Timing is left to
/// `refresh_timing`, being slow. Returns the result.
pub async fn refresh(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    let mut s = schematic::get(db, id).await?.context("no such schematic")?;
    let data = schematic::data(db, id)
        .await?
        .context("schematic vanished")?;
    if let Some(width) = derived_width(&s.category, s.conventional, &data)?
        && s.width != Some(width)
    {
        schematic::set_width(db, id, width).await?;
        s.width = Some(width);
    }
    let Some(test) = Test::of(&s.category, s.width) else {
        clear(db, id).await?;
        return Ok(None);
    };
    if !again && let Some(report) = current(db, &s).await? {
        apply_timing(db, id, &report).await?;
        return Ok(Some(report));
    }
    let report = run(test, data).await?;
    set(db, id, test.name(), &s.sha256, &report).await?;
    Ok(Some(report))
}

/// Measures build `id`'s timing when its stored result passed and has none
/// yet, or always when `again`, then sets its latency and whether it is
/// synchronized from it. Returns the result.
pub async fn refresh_timing(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    let s = schematic::get(db, id).await?.context("no such schematic")?;
    let Some(mut report) = current(db, &s).await? else {
        return Ok(None);
    };
    if let Report::Ran(run) = &mut report
        && run.wrong == 0
        && (again || run.timing.is_none())
    {
        let test = Test::of(&s.category, s.width).context("no test")?;
        let data = schematic::data(db, id)
            .await?
            .context("schematic vanished")?;
        let cancel = Arc::new(AtomicBool::new(false));
        run.timing = Some(time(test, data, cancel).await?);
        set(db, id, test.name(), &s.sha256, &report).await?;
    }
    apply_timing(db, id, &report).await?;
    Ok(Some(report))
}

/// Refreshes build `id`'s result now and its timing in the background, as
/// the site does once a build is published or edited.
pub async fn refresh_soon(db: &SqlitePool, id: &str) {
    if let Err(err) = refresh(db, id, false).await {
        tracing::error!(id, "simulating a build: {err:#}");
        return;
    }
    let (db, id) = (db.clone(), id.to_owned());
    tokio::spawn(async move {
        if let Err(err) = refresh_timing(&db, &id, false).await {
            tracing::error!(id, "timing a build: {err:#}");
        }
    });
}

/// Sets the latency and synchronized of a build whose result measured its
/// timing, over whatever was typed.
async fn apply_timing(
    db: &SqlitePool,
    id: &str,
    report: &Report,
) -> Result<()> {
    if let Some(timing) = report.timing()
        && let Some(latency) = timing.latency()
    {
        schematic::set_timing(db, id, latency.max(0).into(), timing.synced)
            .await?;
    }
    Ok(())
}
