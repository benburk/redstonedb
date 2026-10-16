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
    category, import, interface,
    models::schematic::{
        self, Encoding, Orientation, Schematic, StackDirection,
    },
    models::version,
    schem::Blocks,
    simulate::{Report, Stacking, Test, Timing},
};

/// Build `id`'s stored result, when it ran on the file whose hash is
/// `sha256` under `test` as it is now; a result of another file, test, or
/// version of the test is stale, so none.
pub async fn stored<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    sha256: &str,
    test: Test,
) -> Result<Option<Report>> {
    let (name, version) = (test.name(), test.version());
    let report = sqlx::query_scalar!(
        "SELECT report FROM simulation
         WHERE schematic_id = ? AND sha256 = ? AND test = ? AND version = ?",
        id,
        sha256,
        name,
        version
    )
    .fetch_optional(db)
    .await?;
    Ok(match report {
        Some(report) => Some(serde_json::from_str(&report)?),
        None => None,
    })
}

/// Keeps `report` as build `id`'s result of `test` on the file whose hash
/// is `sha256`, in place of any before.
async fn set<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    test: Test,
    sha256: &str,
    report: &Report,
) -> Result<()> {
    let (name, version, failed) =
        (test.name(), test.version(), report.failed());
    let report = serde_json::to_string(report)?;
    sqlx::query!(
        "INSERT INTO simulation
            (schematic_id, test, version, sha256, failed, report)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT (schematic_id) DO UPDATE SET test = excluded.test,
            version = excluded.version, sha256 = excluded.sha256,
            failed = excluded.failed, report = excluded.report,
            ran_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        id,
        name,
        version,
        sha256,
        failed,
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
    match Test::of(&s.category, s.width) {
        Some(test) => stored(db, &s.id, &s.sha256, test).await,
        None => Ok(None),
    }
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
/// file, runs its test's cases when the stored result is stale, or always
/// when `again`, and drops the result when it has no test; then sets what
/// the result decides, such as how it stacks. Timing is left to
/// `refresh_timing`, being slow. Everything is written together, and only
/// what changed. Returns the result.
pub async fn refresh(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    let s = schematic::get(db, id).await?.context("no such schematic")?;
    let data = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    let mut derived = Derived::of(&s);
    if s.conventional {
        derived.read(&s.category, &Blocks::read(&data)?);
    }
    let test = Test::of(&s.category, derived.width);
    let mut ran = None;
    let report = match test {
        Some(test) => {
            let kept = if again {
                None
            } else {
                stored(db, id, &s.sha256, test).await?
            };
            if let Some(report) = kept {
                Some(report)
            } else {
                let report = run(test, data).await?;
                ran = Some((test, report.clone()));
                Some(report)
            }
        }
        None => None,
    };
    if let Some(report) = &report {
        derived.decide(&s, report);
    }
    let mut tx = db.begin().await?;
    derived.save(&mut tx, &s).await?;
    match (test, ran) {
        (None, _) => clear(&mut *tx, id).await?,
        (Some(_), Some((test, report))) => {
            set(&mut *tx, id, test, &s.sha256, &report).await?;
        }
        (Some(_), None) => {}
    }
    tx.commit().await?;
    Ok(report)
}

/// The details worked out from a build's file and its simulation, over
/// what was typed, as they stand or are to be.
#[derive(PartialEq)]
struct Derived {
    width: Option<i64>,
    orientation: Orientation,
    encoding: Option<Encoding>,
    latency: Option<i64>,
    synchronized: Option<bool>,
    flags: Vec<String>,
    stacking: Vec<StackDirection>,
}

impl Derived {
    /// Build `s`'s, as stored.
    fn of(s: &Schematic) -> Self {
        Derived {
            width: s.width,
            orientation: s.orientation,
            encoding: s.encoding,
            latency: s.latency,
            synchronized: s.synchronized,
            flags: s.flags.iter().map(|flag| flag.name.clone()).collect(),
            stacking: s.stacking.clone(),
        }
    }

    /// Takes what a build of `category` reads from its file, `blocks`.
    fn read(&mut self, category: &str, blocks: &Blocks) {
        let interface = interface::read(blocks);
        let found = import::derived(category, &interface);
        if let Some(width) = found.width {
            self.width = Some(width);
        }
        if let Some(orientation) = found.orientation {
            self.orientation = orientation;
        }
        if let Some(encoding) = found.encoding {
            self.encoding = Some(encoding);
        }
        if let Some(flags) = import::derived_flags(category, &interface) {
            self.flags = flags;
        }
    }

    /// Takes what build `s`'s result `report` decides: the latency and
    /// synchronized of a result that measured its timing, and, for a
    /// conventional build of a category whose stacking the simulation
    /// decides, how it stacks. An adder takes the step its copies added
    /// right at, with no limit on copies, or none. A register takes the
    /// step west its copies stored and read right at, with the most copies
    /// that did, in place of its builder's west step and beside their
    /// others, which the search does not try; it keeps its builder's
    /// stacking when none was found. A result that did not stack copies,
    /// such as one with wrong cases, leaves the stacking as it was.
    fn decide(&mut self, s: &Schematic, report: &Report) {
        if let Some(timing) = report.timing()
            && let Some(latency) = timing.latency()
        {
            self.latency = Some(latency.max(0).into());
            self.synchronized = Some(timing.synced);
        }
        let policy = category::behaviour(&s.category).stacking;
        let Report::Ran(run) = report else {
            return;
        };
        if !s.conventional || policy == category::Stacking::Given {
            return;
        }
        match &run.stacking {
            Some(Stacking::Stacks { step, .. }) => {
                self.stacking = vec![step.direction()];
            }
            Some(
                Stacking::Registers { step, most, .. }
                | Stacking::Wider { step, most, .. },
            ) => {
                let found = StackDirection {
                    max_copies: most.and_then(|n| i64::try_from(n).ok()),
                    ..step.direction()
                };
                self.stacking = s
                    .stacking
                    .iter()
                    .filter(|typed| typed.west == 0)
                    .copied()
                    .chain([found])
                    .collect();
            }
            Some(Stacking::Not { .. })
                if policy == category::Stacking::Inferred =>
            {
                self.stacking = Vec::new();
            }
            Some(Stacking::Not { .. }) | None => {}
        }
        // Copies that add up as a wider adder, or store as a wider register,
        // run the way its bits do, which a 1-bit build's lone input cannot
        // show.
        if let Some(
            Stacking::Stacks { step, .. } | Stacking::Wider { step, .. },
        ) = &run.stacking
            && let Some(orientation) = step.orientation()
        {
            self.orientation = orientation;
        }
    }

    /// Writes over build `s`'s stored details whatever of these differs.
    async fn save(
        &self,
        tx: &mut sqlx::SqliteConnection,
        s: &Schematic,
    ) -> Result<()> {
        let stored = Derived::of(s);
        if *self == stored {
            return Ok(());
        }
        let columns = |d: &Derived| {
            (
                d.width,
                d.orientation,
                d.encoding,
                d.latency,
                d.synchronized,
            )
        };
        if columns(self) != columns(&stored) {
            schematic::set_derived(
                &mut *tx,
                &s.id,
                self.width,
                self.orientation,
                self.encoding,
                self.latency,
                self.synchronized,
            )
            .await?;
        }
        if self.flags != stored.flags {
            schematic::set_flags(tx, &s.id, &self.flags).await?;
        }
        if self.stacking != stored.stacking {
            schematic::set_stacking(tx, &s.id, &self.stacking).await?;
        }
        Ok(())
    }
}

/// Measures build `id`'s timing when its stored result passed and has none
/// yet, or always when `again`, then sets its latency and whether it is
/// synchronized from it, together and only when they changed. Returns the
/// result.
pub async fn refresh_timing(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    let s = schematic::get(db, id).await?.context("no such schematic")?;
    let Some(mut report) = current(db, &s).await? else {
        return Ok(None);
    };
    let mut measured = None;
    if let Report::Ran(run) = &mut report
        && run.wrong == 0
        && (again || run.timing.is_none())
    {
        let test = Test::of(&s.category, s.width).context("no test")?;
        let data = version::current_file(db, id)
            .await?
            .context("schematic vanished")?;
        let cancel = Arc::new(AtomicBool::new(false));
        run.timing = Some(time(test, data, cancel).await?);
        measured = Some(test);
    }
    let mut derived = Derived::of(&s);
    derived.decide(&s, &report);
    let mut tx = db.begin().await?;
    if let Some(test) = measured {
        set(&mut *tx, id, test, &s.sha256, &report).await?;
    }
    derived.save(&mut tx, &s).await?;
    tx.commit().await?;
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
