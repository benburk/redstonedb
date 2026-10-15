//! Each build's simulation result, kept for the file it ran on, and what is
//! derived from the file and the simulation in place of details a person
//! types: an adder's width, and a tested build's latency and whether it is
//! synchronized.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use anyhow::{Context, Result};
use sqlx::{SqliteExecutor, SqlitePool};
use tokio::sync::Semaphore;

use crate::{
    import, interface,
    models::schematic::{self, Schematic, StackDirection},
    schem::Blocks,
    simulate::{
        FINDS_STACKING, INFERS_STACKING, Report, Stacking, Test, Timing,
    },
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
    // A report stored in an older form counts as none, so it runs again.
    let Ok(report) = serde_json::from_str(&row.report) else {
        return Ok(None);
    };
    Ok(Some(Simulation {
        test: row.test,
        sha256: row.sha256,
        report,
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

/// One run of a test's cases at a time, and one timing: the server has two
/// cores, and a timing, which takes a while, should not hold up the cases
/// someone is waiting on.
static CASES: Semaphore = Semaphore::const_new(1);
static TIMING: Semaphore = Semaphore::const_new(1);

/// The results of the latest files run, by the file's hash and the test,
/// so a file previewed and then published, the same bytes, takes what its
/// preview found rather than running again. A restart forgets them.
static RECENT: Mutex<Vec<(String, &'static str, Report)>> =
    Mutex::new(Vec::new());

/// How many results `RECENT` keeps, the latest.
const KEPT: usize = 16;

/// The result `RECENT` holds for `sha256` under `test`, if any.
fn recent(sha256: &str, test: Test) -> Option<Report> {
    let kept = RECENT.lock().expect("the results are whole");
    kept.iter()
        .find(|(sha, name, _)| sha == sha256 && *name == test.name())
        .map(|(_, _, report)| report.clone())
}

/// Keeps `report` for `sha256` under `test`, in place of any before, and
/// forgets the oldest past `KEPT`.
fn remember(sha256: &str, test: Test, report: &Report) {
    let mut kept = RECENT.lock().expect("the results are whole");
    kept.retain(|(sha, name, _)| !(sha == sha256 && *name == test.name()));
    kept.push((sha256.to_owned(), test.name(), report.clone()));
    if kept.len() > KEPT {
        kept.remove(0);
    }
}

/// Runs `test`'s cases on `data`, a `.schem` file, away from the async
/// threads, and, when they all come out right, whether its copies stack;
/// or takes the result of the same file run lately.
pub async fn run(test: Test, data: Vec<u8>) -> Result<Report> {
    let sha256 = import::sha256(&data);
    if let Some(report) = recent(&sha256, test) {
        return Ok(report);
    }
    let _turn = CASES.acquire().await?;
    // Run while this waited for its turn, as by the preview just before.
    if let Some(report) = recent(&sha256, test) {
        return Ok(report);
    }
    let ran = tokio::task::spawn_blocking(move || {
        let mut report = test.run(&Blocks::read(&data)?);
        if let Report::Ran(run) = &mut report
            && run.wrong == 0
        {
            run.stacking = test.stacking(&data);
        }
        anyhow::Ok(report)
    })
    .await;
    // A run that panics is stored as skipped, so asking again does not
    // run it again and again.
    let report = match ran {
        Ok(report) => report?,
        Err(err) if err.is_panic() => Report::Skipped {
            reason: format!(
                "The simulator failed on it: {}.",
                crate::simulate::panic_message(&*err.into_panic())
            ),
        },
        Err(err) => return Err(err.into()),
    };
    remember(&sha256, test, &report);
    Ok(report)
}

/// Measures when `data`'s outputs settle under `test`, away from the async
/// threads, or takes the timing of the same file measured lately. Setting
/// `cancel` stops it early, as when nobody waits for it any more.
pub async fn time(
    test: Test,
    data: Vec<u8>,
    cancel: Arc<AtomicBool>,
) -> Result<Timing> {
    let sha256 = import::sha256(&data);
    let measured = |sha256: &str| {
        recent(sha256, test).and_then(|report| report.timing().cloned())
    };
    if let Some(timing) = measured(&sha256) {
        return Ok(timing);
    }
    let _turn = TIMING.acquire().await?;
    // Measured while this waited for its turn, as by the preview just
    // before.
    if let Some(timing) = measured(&sha256) {
        return Ok(timing);
    }
    let timing = tokio::task::spawn_blocking(move || {
        test.time(&Blocks::read(&data)?, &cancel)
    })
    .await??;
    if let Some(mut report) = recent(&sha256, test) {
        if let Report::Ran(run) = &mut report {
            run.timing = Some(timing.clone());
        }
        remember(&sha256, test, &report);
    }
    Ok(timing)
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
/// the width, orientation, encoding, and flags its category reads from the
/// file, runs its test's cases when
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
    if s.conventional {
        let interface = interface::read(&Blocks::read(&data)?);
        let found = import::derived(&s.category, &interface);
        if let Some(width) = found.width
            && s.width != Some(width)
        {
            schematic::set_width(db, id, width).await?;
            s.width = Some(width);
        }
        if let Some(orientation) = found.orientation
            && s.orientation != orientation
        {
            schematic::set_orientation(db, id, orientation).await?;
            orientation.clone_into(&mut s.orientation);
        }
        if let Some(encoding) = found.encoding
            && s.encoding.as_deref() != Some(encoding)
        {
            schematic::set_encoding(db, id, encoding).await?;
            s.encoding = Some(encoding.to_owned());
        }
        if let Some(flags) = import::derived_flags(&s.category, &interface) {
            let have: Vec<&str> =
                s.flags.iter().map(|flag| flag.name.as_str()).collect();
            if have != flags {
                let mut conn = db.acquire().await?;
                schematic::set_flags(&mut conn, id, &flags).await?;
            }
        }
    }
    let Some(test) = Test::of(&s.category, s.width) else {
        clear(db, id).await?;
        return Ok(None);
    };
    if !again && let Some(report) = current(db, &s).await? {
        apply_timing(db, id, &report).await?;
        apply_stacking(db, &s, &report).await?;
        return Ok(Some(report));
    }
    let report = run(test, data).await?;
    set(db, id, test.name(), &s.sha256, &report).await?;
    apply_stacking(db, &s, &report).await?;
    Ok(Some(report))
}

/// Sets how build `s` stacks from what stacking copies of it found, for a
/// conventional build of a category whose stacking the simulation decides:
/// the step its copies added right at, with no limit on copies, or none.
/// A register takes the step its copies stored and read right at, with
/// the most copies that did, and keeps its builder's stacking when none
/// was found. A result that did not stack copies, such as one with wrong
/// cases, is left as it was.
async fn apply_stacking(
    db: &SqlitePool,
    s: &Schematic,
    report: &Report,
) -> Result<()> {
    let category = s.category.as_str();
    let finds = FINDS_STACKING.contains(&category);
    if !s.conventional || !(INFERS_STACKING.contains(&category) || finds) {
        return Ok(());
    }
    let Report::Ran(run) = report else {
        return Ok(());
    };
    let directions = match &run.stacking {
        Some(Stacking::Stacks { step, .. }) => vec![step.direction()],
        Some(Stacking::Registers { step, most, .. }) => {
            vec![StackDirection {
                max_copies: most.and_then(|n| i64::try_from(n).ok()),
                ..step.direction()
            }]
        }
        Some(Stacking::Not { .. }) if !finds => Vec::new(),
        Some(Stacking::Not { .. }) | None => return Ok(()),
    };
    if s.stacking != directions {
        let mut conn = db.acquire().await?;
        schematic::set_stacking(&mut conn, &s.id, &directions).await?;
    }
    // Copies that add up as a wider adder run the way its bits do, which a
    // 1-bit adder's lone input cannot show.
    if let Some(Stacking::Stacks { step, .. }) = &run.stacking
        && let Some(orientation) = step.orientation()
        && s.orientation != orientation
    {
        schematic::set_orientation(db, &s.id, orientation).await?;
    }
    Ok(())
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

/// The builds being simulated in the background.
static REFRESHING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Brings build `id`'s result and timing up to date in the background, as
/// the site does once a build is published or edited, unless that is
/// already under way: see `refresh` and `refresh_timing`. Asking again is
/// how one lost to a restart starts over.
pub fn refresh_soon(db: &SqlitePool, id: &str) {
    let mut running = REFRESHING.lock().expect("the list is whole");
    if !running.insert(id.to_owned()) {
        return;
    }
    let (db, id) = (db.clone(), id.to_owned());
    tokio::spawn(async move {
        let refreshed = async {
            refresh(&db, &id, false).await?;
            refresh_timing(&db, &id, false).await
        };
        if let Err(err) = refreshed.await {
            tracing::error!(id, "simulating a build: {err:#}");
        }
        REFRESHING.lock().expect("the list is whole").remove(&id);
    });
}

/// Where build `s`'s simulation stands.
pub enum Status {
    /// Its category and width have no test.
    Untested,
    /// Its cases or timing are still to come; asking started them if
    /// nothing was under way.
    Pending(Option<Report>),
    /// Everything there is to know.
    Done(Report),
}

/// Where build `s`'s simulation stands, starting it in the background when
/// something is missing and nothing is under way.
pub async fn status(db: &SqlitePool, s: &Schematic) -> Result<Status> {
    if Test::of(&s.category, s.width).is_none() {
        return Ok(Status::Untested);
    }
    match current(db, s).await? {
        Some(report) if !report.passed() || report.timing().is_some() => {
            Ok(Status::Done(report))
        }
        report => {
            refresh_soon(db, &s.id);
            Ok(Status::Pending(report))
        }
    }
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
