//! Running builds' tests and timings, and keeping what they found lately,
//! off the async threads and a few at a time, as the server's cores allow.
//! The queues, the evaluations started ahead of an upload being published,
//! the results kept for files previewed, and the builds being completed in
//! the background are each one per process, so they are statics: every
//! caller, the site and `rsdb` alike, shares them.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use anyhow::Result;
use sqlx::SqlitePool;
use tokio::sync::{OnceCell, Semaphore};

use crate::{
    category::Test,
    recognize::evaluate::{Evaluation, evaluate},
    schem::{self, Blocks},
    simulate::{Report, Timing},
};

/// One run of a test's cases at a time, and one timing: the server has two
/// cores, and a timing, which takes a while, should not hold up the cases
/// someone is waiting on.
pub(super) static CASES: Semaphore = Semaphore::const_new(1);
pub(super) static TIMING: Semaphore = Semaphore::const_new(1);

/// An evaluation of a file, once it is done; `None` when it failed.
type Ahead = Arc<OnceCell<Option<Arc<Evaluation>>>>;

/// Evaluations of files not yet stored, by their hash, as an upload's
/// while its uploader fills in its details, so storing one takes that
/// evaluation, or waits on it, rather than starting over. The latest
/// [`AHEAD_KEPT`] only, as most previews are never published.
static AHEAD: Mutex<Vec<(String, Ahead)>> = Mutex::new(Vec::new());
const AHEAD_KEPT: usize = 8;

/// Starts evaluating `data`, a file as the site keeps it whose hash is
/// `sha256`, in the background, for the build it becomes once stored to
/// take; see [`AHEAD`].
pub fn evaluate_ahead(data: Vec<u8>, sha256: String) {
    let cell = {
        let mut ahead = AHEAD.lock().expect("the list is whole");
        if ahead.iter().any(|(hash, _)| *hash == sha256) {
            return;
        }
        let cell = Ahead::default();
        ahead.push((sha256, Arc::clone(&cell)));
        if ahead.len() > AHEAD_KEPT {
            ahead.remove(0);
        }
        cell
    };
    // Behind timings, as it takes a while, so it holds up no cases
    // someone is waiting on, as publishing it runs.
    tokio::spawn(async move {
        cell.get_or_init(|| evaluated(data, &TIMING)).await;
    });
}

/// The evaluation started ahead of the file whose hash is `sha256`, taken
/// off the list; `None` when none was.
pub(super) fn taken_ahead(sha256: &str) -> Option<Ahead> {
    let mut ahead = AHEAD.lock().expect("the list is whole");
    let at = ahead.iter().position(|(hash, _)| hash == sha256)?;
    Some(ahead.remove(at).1)
}

/// `data` evaluated, one at a time with others `turns` lets go; `None`
/// when it failed.
pub(super) async fn evaluated(
    data: Vec<u8>,
    turns: &Semaphore,
) -> Option<Arc<Evaluation>> {
    let _turn = turns.acquire().await.ok()?;
    tokio::task::spawn_blocking(move || evaluate(&data).ok().map(Arc::new))
        .await
        .ok()
        .flatten()
}

/// Lets `at_once` runs of cases, and as many timings, go at a time, as on
/// a machine with the cores for them; call once, before any runs.
pub fn run_at_once(at_once: usize) {
    CASES.add_permits(at_once.saturating_sub(1));
    TIMING.add_permits(at_once.saturating_sub(1));
}

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
    let sha256 = schem::sha256(&data);
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
    let sha256 = schem::sha256(&data);
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

/// Brings newly stored build `id` wholly up to date, as storing a build,
/// a new version of one, or an edit starts it doing: its result, see
/// `refresh`, its timing, see `refresh_timing`, and, once it passes, its
/// unused blocks taken out, see `finish`. Nothing runs again later: a
/// build's page shows what was stored here.
pub async fn complete(db: &SqlitePool, id: &str) -> Result<()> {
    super::refresh(db, id, false).await?;
    super::refresh_timing(db, id, false).await?;
    super::finish(db, id).await
}

/// The builds being completed in the background.
static COMPLETING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Completes newly stored build `id` in the background, see [`complete`],
/// so storing it is done at once, its page showing it simulating until
/// this is; nothing, when it is under way already.
pub fn complete_soon(db: &SqlitePool, id: &str) {
    if !COMPLETING
        .lock()
        .expect("the list is whole")
        .insert(id.to_owned())
    {
        return;
    }
    let (db, id) = (db.clone(), id.to_owned());
    tokio::spawn(async move {
        if let Err(err) = complete(&db, &id).await {
            tracing::error!(id, "simulating a build: {err:#}");
        }
        COMPLETING.lock().expect("the list is whole").remove(&id);
    });
}

/// Whether build `id` is being completed in the background, see
/// [`complete_soon`].
pub fn completing(id: &str) -> bool {
    COMPLETING.lock().expect("the list is whole").contains(id)
}
