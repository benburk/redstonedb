//! Each build's simulation result, kept for the file it ran on, and what is
//! derived from the file and the simulation in place of details a person
//! types: an adder's width, and a tested build's latency and whether it is
//! synchronized. Running the tests is `jobs`'s, and deciding the details
//! from what they found `derive`'s; this module stores both.

use std::sync::{Arc, atomic::AtomicBool};

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use crate::{
    build::Build,
    category::{self, Test},
    ingest::{
        evaluate::evaluate,
        prepare::{self, Purpose},
        publish, read,
    },
    interface,
    recognize::roles,
    schem::Blocks,
    simulate::Report,
    stack::{self, StackDirection},
    store::{builds, results, versions},
};

use super::{
    derive::Derived,
    jobs::{CASES, TIMING, evaluated, run, taken_ahead, time},
};

/// Whether the file `data` carries its lines' roles, as one the site
/// reads, or whose lines its author marked, does.
pub fn carries_roles(data: &[u8]) -> bool {
    crate::schem::Blocks::read(data).is_ok_and(|blocks| {
        blocks.roles.is_some_and(|roles| !roles.0.is_empty())
    })
}

/// Moves build `id`'s stored result to its file `data`, whose hash is
/// `sha256`, when that holds the same circuit, stacked each of `ways` with
/// its buses as `buses` says, as the result ran on, so the file is not
/// simulated again. Returns the circuit's key, `None` when it could not be
/// compiled.
async fn adopt(
    db: &SqlitePool,
    id: &str,
    (data, sha256): (&[u8], &str),
    (ways, buses): (&[StackDirection], stack::Buses),
) -> Result<Option<String>> {
    let Ok(key) = roles::circuit_key(data, ways, buses) else {
        return Ok(None);
    };
    results::adopt(db, id, &key, sha256).await?;
    Ok(Some(key))
}

/// The test build `s` is held to: its category's at its width, or for one
/// not verified as its category, that test whatever its width, to say what
/// keeps it from being recognized as one.
pub fn test_for(s: &Build) -> Option<Test> {
    if s.verified() {
        Test::of(&s.category, s.specs.width)
    } else {
        category::behaviour(&s.category).test
    }
}

/// Build `s`'s stored result, when it is for its file and the test its
/// category and width call for now, as storing it found, see
/// [`complete`]; nothing is run to find out.
pub async fn current(db: &SqlitePool, s: &Build) -> Result<Option<Report>> {
    match test_for(s) {
        Some(test) => results::stored(db, &s.id, &s.file.sha256, test).await,
        None => Ok(None),
    }
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
    Ok(read::derived_width(category, &interface::read(&blocks)))
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
    let s = builds::get(db, id).await?.context("no such schematic")?;
    let data = current_file(db, &s).await?;
    if !again {
        adopt(db, id, (&data, &s.file.sha256), (&s.stacking, s.buses()))
            .await?;
    }
    if s.evaluated() {
        return refresh_evaluated(db, &s, data, again).await;
    }
    if !s.verified() {
        return refresh_unverified(db, &s, &data, again).await;
    }
    let mut derived = Derived::of(&s);
    if s.file.conventional {
        derived.read(&s.category, &Blocks::read(&data)?);
    }
    let test = Test::of(&s.category, derived.specs.width);
    let mut ran = None;
    let report = match test {
        Some(test) => {
            let kept = if again {
                None
            } else {
                results::stored(db, id, &s.file.sha256, test).await?
            };
            if let Some(report) = kept {
                Some(report)
            } else {
                let report = run(test, data.clone()).await?;
                ran = Some((test, report.clone()));
                Some(report)
            }
        }
        None => None,
    };
    if let Some(report) = &report {
        derived.decide(&s, report);
    }
    let (data, sha256) = match (&ran, &report) {
        (Some((_, report)), _) => at_zero(db, &s, data, report).await?,
        (None, Some(report)) => reused_at_zero(db, &s, data, report).await?,
        (None, None) => (data, s.file.sha256.clone()),
    };
    let mut tx = db.begin().await?;
    derived.save(&mut tx, &s).await?;
    match (test, ran) {
        (None, _) => results::clear(&mut *tx, id).await?,
        (Some(_), Some((test, report))) => {
            let key = roles::circuit_key(
                &data,
                &derived.stacking,
                s.buses_running(derived.specs.orientation),
            )
            .ok();
            let file = (sha256.as_str(), key.as_deref());
            results::set(&mut *tx, id, test, file, &report).await?;
        }
        (Some(_), None) => {}
    }
    tx.commit().await?;
    Ok(report)
}

/// Build `s`'s current file, carrying its roles: those kept with its
/// version, or, for a version whose roles are not yet found, as a stored
/// build's before the site kept them, those [`publish::with_roles`] finds
/// by probing it as its category, then kept with the version, so every
/// reading of its lines takes them and none reads its colours.
pub async fn current_file(db: &SqlitePool, s: &Build) -> Result<Vec<u8>> {
    let data = versions::current_file(db, &s.id)
        .await?
        .context("schematic vanished")?;
    if Blocks::read(&data)?.roles.is_some() {
        return Ok(data);
    }
    let category = s.category.clone();
    let found = tokio::task::spawn_blocking(move || {
        publish::with_roles(&data, &category)
    })
    .await??;
    versions::set_current_roles(db, &s.id, &found).await?;
    tracing::info!(id = s.id, "found the roles of its current version");
    Ok(found)
}

/// Build `s`'s file `data` with each input at logical 0, once `report`, a
/// fresh result, passed: its levers on for each line found to read
/// inverted and off for every other, whatever way they were saved, and
/// every component at rest with them. Put in its current version's place
/// when that changes it, as the same build. The file and its hash as they
/// then stand.
async fn at_zero(
    db: &SqlitePool,
    s: &Build,
    data: Vec<u8>,
    report: &Report,
) -> Result<(Vec<u8>, String)> {
    let kept = |data| Ok((data, s.file.sha256.clone()));
    if !report.passed() || !s.file.conventional {
        return kept(data);
    }
    let category = s.category.clone();
    let inverted = report.inverted().to_vec();
    let (conventional, keeps_paste) = (s.file.conventional, s.file.keeps_paste);
    let was = data.clone();
    let fixed = tokio::task::spawn_blocking(move || {
        let fixed = crate::standard::standardize::levers_at_zero(
            &was, &category, &inverted,
        )?;
        if fixed == was {
            return Ok(None);
        }
        prepare::prepare_for(
            &fixed,
            Purpose::InPlace {
                standardize: false,
                conventional,
                keep_paste: keeps_paste,
            },
        )
        .map(Some)
    })
    .await?;
    // A file that can't be settled, as one with a clock, is kept as saved.
    // At rest it may be a version before, which the build already holds;
    // it is kept as saved then, as each file is stored once.
    let held = match &fixed {
        Ok(Some(prepared)) => versions::holder(db, &prepared.sha256).await?,
        _ => None,
    };
    if let Some((other, number)) = &held {
        tracing::warn!(
            id = s.id,
            other,
            number,
            "at logical 0 it is a file already stored; kept as saved"
        );
        return kept(data);
    }
    match fixed {
        Ok(Some(prepared)) if prepared.sha256 != s.file.sha256 => {
            versions::replace(
                db,
                &s.id,
                &prepared,
                versions::Replacing::InPlace,
            )
            .await?;
            Ok((prepared.data, prepared.sha256))
        }
        _ => kept(data),
    }
}

/// As [`at_zero`], for build `s`'s file `data` taking a result `report`
/// kept for its circuit rather than run afresh: the result then follows
/// the file to its new hash, when putting its inputs at logical 0 changes
/// it.
async fn reused_at_zero(
    db: &SqlitePool,
    s: &Build,
    data: Vec<u8>,
    report: &Report,
) -> Result<(Vec<u8>, String)> {
    let (data, sha256) = at_zero(db, s, data, report).await?;
    if sha256 != s.file.sha256 {
        adopt(db, &s.id, (&data, &sha256), (&s.stacking, s.buses())).await?;
    }
    Ok((data, sha256))
}

/// Brings build `s`, not verified as its category, up to date with its
/// file `data`: its category's test run on the file standardized, as
/// recognizing it would, to show what keeps it from being one. Nothing
/// else is set from the result, its details being its author's.
async fn refresh_unverified(
    db: &SqlitePool,
    s: &Build,
    data: &[u8],
    again: bool,
) -> Result<Option<Report>> {
    let Some(test) = test_for(s) else {
        return Ok(None);
    };
    if !again && let Some(report) = current(db, s).await? {
        return Ok(Some(report));
    }
    let standard = prepare::prepare_for(
        data,
        Purpose::Standard {
            keep_paste: s.file.keeps_paste,
        },
    );
    let report = match standard {
        Ok(standard) => {
            // Its author's width, when its file shows none.
            let width = derived_width(&s.category, true, &standard.data)?
                .or(s.specs.width);
            match Test::of(&s.category, width) {
                Some(test) => run(test, standard.data).await?,
                None => Report::Skipped {
                    reason: "Couldn't count its bits.".to_owned(),
                },
            }
        }
        Err(err) => Report::Skipped {
            reason: format!("Couldn't standardize it: {err:#}."),
        },
    };
    results::set(db, &s.id, test, (s.file.sha256.as_str(), None), &report)
        .await?;
    Ok(Some(report))
}

/// Brings build `s`, of a category whose details all come from
/// `evaluate`, up to date with its file `data`: evaluated afresh, unless
/// its stored result is for this file and complete, and every detail the
/// evaluation gives written over the stored ones. A file its test does not
/// recognize as the category keeps its category's test's result instead.
async fn refresh_evaluated(
    db: &SqlitePool,
    s: &Build,
    data: Vec<u8>,
    again: bool,
) -> Result<Option<Report>> {
    if !again
        && let Some(report) = current(db, s).await?
        && (!report.passed() || report.timing().is_some())
    {
        // A new version of a circuit tested before takes its result, and
        // still comes to lie with its inputs at logical 0.
        reused_at_zero(db, s, data, &report).await?;
        return Ok(Some(report));
    }
    // One evaluated as it was uploaded is taken, or waited on.
    let ahead = match taken_ahead(&s.file.sha256) {
        Some(cell) => cell
            .get_or_init(|| evaluated(data.clone(), &TIMING))
            .await
            .clone(),
        None => None,
    };
    let e = if let Some(e) = ahead {
        tracing::info!(id = s.id, "taking the evaluation made on upload");
        e
    } else {
        let _turn = CASES.acquire().await?;
        let data = data.clone();
        Arc::new(tokio::task::spawn_blocking(move || evaluate(&data)).await??)
    };
    // Its result is the evaluation's when the test recognizes it as its
    // category; else its category's own test's, saying what is wrong.
    let report = match &e.report {
        Some(report) if e.category == Some(s.category.as_str()) => {
            Some(report.clone())
        }
        _ => {
            let _turn = CASES.acquire().await?;
            let (e, category) = (Arc::clone(&e), s.category.clone());
            tokio::task::spawn_blocking(move || {
                let blocks = &e.prepared.blocks;
                let width =
                    read::derived(&category, &interface::read(blocks)).width;
                Test::of(&category, width).map(|test| test.run(blocks))
            })
            .await?
        }
    };
    let derived = Derived::evaluated(s, &e);
    let (data, sha256) = match &report {
        Some(report) => at_zero(db, s, data, report).await?,
        None => (data, s.file.sha256.clone()),
    };
    let mut tx = db.begin().await?;
    derived.save(&mut tx, s).await?;
    match (Test::of(&s.category, derived.specs.width), &report) {
        (Some(test), Some(report)) => {
            let key = roles::circuit_key(
                &data,
                &derived.stacking,
                s.buses_running(derived.specs.orientation),
            )
            .ok();
            let file = (sha256.as_str(), key.as_deref());
            results::set(&mut *tx, &s.id, test, file, report).await?;
        }
        _ => results::clear(&mut *tx, &s.id).await?,
    }
    tx.commit().await?;
    Ok(report)
}

impl Derived {
    /// Writes over build `s`'s stored details whatever of these differs.
    async fn save(
        &self,
        tx: &mut sqlx::SqliteConnection,
        s: &Build,
    ) -> Result<()> {
        let stored = Derived::of(s);
        if *self == stored {
            return Ok(());
        }
        if self.specs != stored.specs {
            builds::set_specs(&mut *tx, &s.id, &self.specs).await?;
        }
        if self.flags != stored.flags {
            builds::set_flags(tx, &s.id, &self.flags).await?;
        }
        if self.stacking != stored.stacking {
            builds::set_stacking(tx, &s.id, &self.stacking).await?;
        }
        if self.widened != stored.widened {
            builds::set_widened(tx, &s.id, &self.widened).await?;
        }
        Ok(())
    }
}

/// Runs build `id`'s test, see [`refresh`], and its timing, see
/// [`refresh_timing`], each where what is stored is stale, or always when
/// `again`. Returns the result; `None` when it has no test.
pub async fn tested(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    match refresh(db, id, again).await? {
        Some(_) => refresh_timing(db, id, again).await,
        None => Ok(None),
    }
}

/// Brings build `id` up to date with its file as it stands: its test and
/// timing, see [`tested`], and what its building blocks must be, see
/// [`keep_needs`]. Nothing of the file changes; see `jobs::complete` for a
/// build newly stored, whose unused blocks go too.
pub async fn measure(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    let report = tested(db, id, again).await?;
    keep_needs(db, id).await?;
    Ok(report)
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
    let report = timed(db, id, again).await?;
    keep_measures(db, id, report.as_ref()).await?;
    Ok(report)
}

/// Keeps what build `id`'s current version measured, its result
/// `report`, with the version, so its list can say what each changed.
async fn keep_measures(
    db: &SqlitePool,
    id: &str,
    report: Option<&Report>,
) -> Result<()> {
    let s = builds::get(db, id).await?.context("no such schematic")?;
    let verdict = match report {
        Some(Report::Ran(run)) if run.wrong == 0 => "passes",
        Some(Report::Ran(_)) => "wrong",
        Some(Report::Broken { .. }) => "broken",
        Some(Report::Skipped { .. }) => "skipped",
        None => "untested",
    };
    let measures = versions::Measures {
        verdict: verdict.to_owned(),
        latency: s.specs.latency,
        throughput: s.specs.throughput,
        size: [s.file.size.0, s.file.size.1, s.file.size.2],
        pitch: s.stacking.iter().filter_map(|d| d.pitch).min(),
    };
    versions::set_measures(db, id, &measures).await?;
    Ok(())
}

/// `refresh_timing`'s work, but keeping the measures.
async fn timed(
    db: &SqlitePool,
    id: &str,
    again: bool,
) -> Result<Option<Report>> {
    let s = builds::get(db, id).await?.context("no such schematic")?;
    let Some(mut report) = current(db, &s).await? else {
        return Ok(None);
    };
    // `refresh` timed it already, with everything else.
    if s.evaluated() {
        return Ok(Some(report));
    }
    // Any build its test ran on is timed, passing or not: when its outputs
    // settle and how they glitch on the way needs no right answer, and is
    // most use to one that gets cases wrong.
    let mut measured = None;
    if let Report::Ran(run) = &mut report
        && (again || run.timing.is_none())
    {
        let test = test_for(&s).context("no test")?;
        let data = current_file(db, &s).await?;
        let cancel = Arc::new(AtomicBool::new(false));
        let key = roles::circuit_key(&data, &s.stacking, s.buses()).ok();
        run.timing = Some(time(test, data, cancel).await?);
        measured = Some((test, key));
    }
    let mut derived = Derived::of(&s);
    derived.decide(&s, &report);
    let mut tx = db.begin().await?;
    if let Some((test, key)) = measured {
        let file = (s.file.sha256.as_str(), key.as_deref());
        results::set(&mut *tx, id, test, file, &report).await?;
    }
    derived.save(&mut tx, &s).await?;
    tx.commit().await?;
    Ok(Some(report))
}

/// For a recognized build whose test passes, as once it is uploaded: for
/// a recognized build, see `category::Facts::listed_as`, its unused blocks taken
/// out, as a new version once compiling it shows the circuit unchanged;
/// then its needs kept, as [`keep_needs`] keeps them. Any other build is
/// left as it came.
pub async fn finish(db: &SqlitePool, id: &str) -> Result<()> {
    let s = builds::get(db, id).await?.context("no such schematic")?;
    // Any build the site reads is shown in a theme, passing or not; only
    // one that passes, kept to the conventions, has its unused blocks out.
    if !s.evaluated()
        || !current(db, &s).await?.is_some_and(|report| report.passed())
    {
        return keep_needs(db, id).await;
    }
    let data = current_file(db, &s).await?;
    // When taking them out would change its circuit, which the proof by
    // compiling it shows, the blocks found unused are not, so it keeps
    // them.
    let pruned = if category::listed(&s.category) {
        roles::prune(&data, &s.stacking, s.buses()).unwrap_or_else(|err| {
            tracing::warn!(id, "keeping its blocks: {err:#}");
            None
        })
    } else {
        None
    };
    if let Some(pruned) = pruned {
        let checked =
            crate::ingest::version::check(db, &s, &pruned.file, false).await?;
        // In the version's own place, as its uploader saw it before
        // publishing, rather than as a version after it.
        if checked.refusals.is_empty() {
            let how = versions::Replacing::InPlace;
            versions::replace(db, id, &checked.prepared, how).await?;
            // The same circuit, so its result carries over unsimulated.
            refresh(db, id, false).await?;
        }
    }
    keep_needs(db, id).await
}

/// For a build whose current file carries its lines' roles, as one the
/// site reads does, passing or not, what each of its building blocks must
/// be, solid, transparent, or either, and which carry hard or soft power,
/// kept with its version, for block themes, with nothing taken out;
/// whatever made the version, so a fix by hand is shown in a theme too. A
/// build the site can't read is left as it came.
pub async fn keep_needs(db: &SqlitePool, id: &str) -> Result<()> {
    let s = builds::get(db, id).await?.context("no such schematic")?;
    let data = current_file(db, &s).await?;
    if !carries_roles(&data) {
        return Ok(());
    }
    let needs =
        crate::recognize::palette::needs_of(&data, &s.stacking, s.buses())?;
    versions::set_needs(db, id, &needs).await?;
    Ok(())
}
