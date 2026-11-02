//! Each build's simulation result, kept for the file it ran on, and what is
//! derived from the file and the simulation in place of details a person
//! types: an adder's width, and a tested build's latency and whether it is
//! synchronized. Running the tests is `runner`'s, and deciding the details
//! from what they found `derive`'s; this module stores both.

mod derive;
mod runner;

pub use derive::{EVEN, FALLING_EDGE, INVERTED, SIGNED};
pub use runner::{evaluate_ahead, refresh_soon, run, run_at_once, time};

use std::sync::{Arc, atomic::AtomicBool};

use anyhow::{Context, Result};
use sqlx::{SqliteExecutor, SqlitePool};

use crate::{
    import,
    models::{
        schematic::{self, Schematic, StackDirection},
        version,
    },
    recognize::{category, evaluate::evaluate, interface, roles},
    schem::{Blocks, stack},
    simulate::{Report, Test},
};

use derive::Derived;
use runner::{CASES, TIMING, evaluated, taken_ahead};

/// Build `id`'s stored result, when it ran on the file whose hash is
/// `sha256` under `test`; a result of another file or test, none. A change
/// to a test leaves its stored results as they are until a build is
/// simulated again, as `rsdb simulate` does.
pub async fn stored<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    sha256: &str,
    test: Test,
) -> Result<Option<Report>> {
    let name = test.name();
    let report = sqlx::query_scalar!(
        "SELECT report FROM simulation
         WHERE schematic_id = ? AND sha256 = ? AND test = ?",
        id,
        sha256,
        name
    )
    .fetch_optional(db)
    .await?;
    Ok(match report {
        Some(report) => Some(serde_json::from_str(&report)?),
        None => None,
    })
}

/// Keeps `report` as build `id`'s result of `test` on the file whose hash
/// is `sha256`, holding the circuit whose key is `circuit`, in place of any
/// before.
async fn set<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    test: Test,
    (sha256, circuit): (&str, Option<&str>),
    report: &Report,
) -> Result<()> {
    let (name, failed) = (test.name(), report.failed());
    let report = serde_json::to_string(report)?;
    sqlx::query!(
        "INSERT INTO simulation
            (schematic_id, test, sha256, circuit, failed, report)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT (schematic_id) DO UPDATE SET test = excluded.test,
            sha256 = excluded.sha256, circuit = excluded.circuit,
            failed = excluded.failed, report = excluded.report,
            ran_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        id,
        name,
        sha256,
        circuit,
        failed,
        report
    )
    .execute(db)
    .await?;
    Ok(())
}

/// How build `s`'s copies stacked share its buses, as its category and the
/// way its bits run say.
fn buses(s: &Schematic) -> stack::Buses {
    stack::Buses::of(&s.category, s.orientation)
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
    sqlx::query!(
        "UPDATE simulation SET sha256 = ?
         WHERE schematic_id = ? AND circuit = ? AND sha256 <> ?",
        sha256,
        id,
        key,
        sha256
    )
    .execute(db)
    .await?;
    Ok(Some(key))
}

async fn clear<'e, E: SqliteExecutor<'e>>(db: E, id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM simulation WHERE schematic_id = ?", id)
        .execute(db)
        .await?;
    Ok(())
}

/// The listed category an off-list one named for what it seems meant to
/// be, such as `8-bit_register`, or `unrecognized_register` when its width
/// could not be read, stands in for, when the simulation works out that
/// category's builds; such a build goes up off the list while it is not
/// recognized as one. See `details::off_list_name`.
pub fn meant_by(category: &str) -> Option<&str> {
    let meant = category.strip_prefix("unrecognized_").or_else(|| {
        let (bits, meant) = category.split_once("-bit_")?;
        bits.parse::<u32>().ok().map(|_| meant)
    })?;
    category::behaviour(meant).evaluated.then_some(meant)
}

/// The category whose test build `s` is held to only to say what keeps it
/// from being recognized as one: its own, when it is unverified as one, or
/// the one an off-list name stands in for, as [`meant_by`] reads it.
/// `None` for any other build, held to its category's test as it is.
pub fn stands_in(s: &Schematic) -> Option<&str> {
    if s.unverified.is_some() {
        return Some(&s.category);
    }
    meant_by(&s.category)
}

/// The test build `s` is held to: its category's at its width, or for one
/// it [`stands_in`] for, that category's test, to say what keeps it from
/// being recognized as one.
pub fn test_for(s: &Schematic) -> Option<Test> {
    match stands_in(s) {
        Some(meant) => category::behaviour(meant).test,
        None => Test::of(&s.category, s.width),
    }
}

/// Build `s`'s stored result, when it is for its file and the test its
/// category and width call for now.
pub async fn current(db: &SqlitePool, s: &Schematic) -> Result<Option<Report>> {
    match test_for(s) {
        Some(test) => stored(db, &s.id, &s.sha256, test).await,
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
    if !again {
        adopt(db, id, (&data, &s.sha256), (&s.stacking, buses(&s))).await?;
    }
    if s.evaluated() {
        return refresh_evaluated(db, &s, data, again).await;
    }
    if let Some(meant) = stands_in(&s) {
        return refresh_off_list(db, &s, &data, meant, again).await;
    }
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
        (None, None) => (data, s.sha256.clone()),
    };
    let mut tx = db.begin().await?;
    derived.save(&mut tx, &s).await?;
    match (test, ran) {
        (None, _) => clear(&mut *tx, id).await?,
        (Some(_), Some((test, report))) => {
            let key = roles::circuit_key(
                &data,
                &derived.stacking,
                stack::Buses::of(&s.category, derived.orientation),
            )
            .ok();
            let file = (sha256.as_str(), key.as_deref());
            set(&mut *tx, id, test, file, &report).await?;
        }
        (Some(_), None) => {}
    }
    tx.commit().await?;
    Ok(report)
}

/// Build `s`'s file `data` with each input at logical 0, once `report`, a
/// fresh result, passed: its levers on for each line found to read
/// inverted and off for every other, whatever way they were saved, and
/// every component at rest with them. Put in its current version's place
/// when that changes it, as the same build. The file and its hash as they
/// then stand.
async fn at_zero(
    db: &SqlitePool,
    s: &Schematic,
    data: Vec<u8>,
    report: &Report,
) -> Result<(Vec<u8>, String)> {
    let kept = |data| Ok((data, s.sha256.clone()));
    if !report.passed() || !s.conventional {
        return kept(data);
    }
    let category = stands_in(s).unwrap_or(&s.category).to_owned();
    let inverted = report.inverted().to_vec();
    let (conventional, keeps_paste) = (s.conventional, s.keeps_paste);
    let was = data.clone();
    let fixed = tokio::task::spawn_blocking(move || {
        let fixed = crate::schem::standardize::levers_at_zero(
            &was, &category, &inverted,
        )?;
        if fixed == was {
            return Ok(None);
        }
        import::prepare_for(
            &fixed,
            import::Purpose::InPlace {
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
        Ok(Some(prepared)) => version::holder(db, &prepared.sha256).await?,
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
        Ok(Some(prepared)) if prepared.sha256 != s.sha256 => {
            version::replace(db, &s.id, &prepared, version::Replacing::InPlace)
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
    s: &Schematic,
    data: Vec<u8>,
    report: &Report,
) -> Result<(Vec<u8>, String)> {
    let (data, sha256) = at_zero(db, s, data, report).await?;
    if sha256 != s.sha256 {
        adopt(db, &s.id, (&data, &sha256), (&s.stacking, buses(s))).await?;
    }
    Ok((data, sha256))
}

/// Brings build `s`, unverified or off the list while it is not
/// recognized as the category `meant` it stands in for, up to date with
/// its file `data`:
/// that category's test run on the file standardized, as recognizing it
/// would, to show what keeps it from being one. Nothing else is set from
/// the result, its details being its builder's.
async fn refresh_off_list(
    db: &SqlitePool,
    s: &Schematic,
    data: &[u8],
    meant: &str,
    again: bool,
) -> Result<Option<Report>> {
    let Some(test) = test_for(s) else {
        return Ok(None);
    };
    if !again && let Some(report) = current(db, s).await? {
        return Ok(Some(report));
    }
    let standard = import::prepare_for(
        data,
        import::Purpose::Standard {
            keep_paste: s.keeps_paste,
        },
    );
    let report = match standard {
        Ok(standard) => {
            // Its builder's width, when its file shows none.
            let width = derived_width(meant, true, &standard.data)?.or(s.width);
            match Test::of(meant, width) {
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
    set(db, &s.id, test, (s.sha256.as_str(), None), &report).await?;
    Ok(Some(report))
}

/// Brings build `s`, of a category whose details all come from
/// `evaluate`, up to date with its file `data`: evaluated afresh, unless
/// its stored result is for this file and complete, and every detail the
/// evaluation gives written over the stored ones. A file its test does not
/// recognize as the category keeps its category's test's result instead.
async fn refresh_evaluated(
    db: &SqlitePool,
    s: &Schematic,
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
    let ahead = match taken_ahead(&s.sha256) {
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
                    import::derived(&category, &interface::read(blocks)).width;
                Test::of(&category, width).map(|test| test.run(blocks))
            })
            .await?
        }
    };
    let derived = Derived::evaluated(s, &e);
    let (data, sha256) = match &report {
        Some(report) => at_zero(db, s, data, report).await?,
        None => (data, s.sha256.clone()),
    };
    let mut tx = db.begin().await?;
    derived.save(&mut tx, s).await?;
    match (Test::of(&s.category, derived.width), &report) {
        (Some(test), Some(report)) => {
            let key = roles::circuit_key(
                &data,
                &derived.stacking,
                stack::Buses::of(&s.category, derived.orientation),
            )
            .ok();
            let file = (sha256.as_str(), key.as_deref());
            set(&mut *tx, &s.id, test, file, report).await?;
        }
        _ => clear(&mut *tx, &s.id).await?,
    }
    tx.commit().await?;
    Ok(report)
}

impl Derived {
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
        if self.pulse != stored.pulse {
            schematic::set_pulse(&mut *tx, &s.id, self.pulse).await?;
        }
        if self.at_least != stored.at_least {
            schematic::set_at_least(&mut *tx, &s.id, self.at_least).await?;
        }
        if self.flags != stored.flags {
            schematic::set_flags(tx, &s.id, &self.flags).await?;
        }
        if self.stacking != stored.stacking {
            schematic::set_stacking(tx, &s.id, &self.stacking).await?;
        }
        if self.widened != stored.widened {
            schematic::set_widened(tx, &s.id, &self.widened).await?;
        }
        if self.logic != stored.logic {
            schematic::set_logic(&mut *tx, &s.id, self.logic).await?;
        }
        if (self.throughput, &self.design)
            != (stored.throughput, &stored.design)
        {
            schematic::set_evaluated(
                &mut *tx,
                &s.id,
                self.throughput,
                &self.design,
            )
            .await?;
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
    // `refresh` timed it already, with everything else; an off-list build
    // is only tested, to show what is wrong with it.
    if s.evaluated() || stands_in(&s).is_some() {
        return Ok(Some(report));
    }
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
        let key = roles::circuit_key(&data, &s.stacking, buses(&s)).ok();
        run.timing = Some(time(test, data, cancel).await?);
        measured = Some((test, key));
    }
    let mut derived = Derived::of(&s);
    derived.decide(&s, &report);
    let mut tx = db.begin().await?;
    if let Some((test, key)) = measured {
        let file = (s.sha256.as_str(), key.as_deref());
        set(&mut *tx, id, test, file, &report).await?;
    }
    derived.save(&mut tx, &s).await?;
    tx.commit().await?;
    Ok(Some(report))
}

/// For a recognized build whose test passes, as once it is uploaded: for
/// a blessed build, see `category::BLESSED`, its unused blocks taken
/// out, as a new version once compiling it shows the circuit unchanged;
/// then its needs kept, as [`keep_needs`] keeps them. Any other build is
/// left as it came.
pub async fn finish(db: &SqlitePool, id: &str) -> Result<()> {
    let s = schematic::get(db, id).await?.context("no such schematic")?;
    if !s.evaluated() {
        return Ok(());
    }
    if !current(db, &s).await?.is_some_and(|report| report.passed()) {
        return Ok(());
    }
    let data = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    // When taking them out would change its circuit, which the proof by
    // compiling it shows, the blocks found unused are not, so it keeps
    // them.
    let pruned = if category::BLESSED.contains(&s.category.as_str()) {
        roles::prune(&data, &s.stacking, buses(&s)).unwrap_or_else(|err| {
            tracing::warn!(id, "keeping its blocks: {err:#}");
            None
        })
    } else {
        None
    };
    if let Some(pruned) = pruned {
        let checked =
            crate::version::check(db, &s, &pruned.file, false).await?;
        if checked.refusals.is_empty() {
            let how = version::Replacing::NextVersion {
                added_by: None,
                added_from: None,
                note: Some("Unused blocks taken out"),
            };
            version::replace(db, id, &checked.prepared, how).await?;
            // The same circuit, so its result carries over unsimulated.
            refresh(db, id, false).await?;
        }
    }
    keep_needs(db, id).await
}

/// For a recognized build whose test passes, what each of its current
/// file's building blocks must be, solid, transparent, or either, and
/// which carry hard or soft power, kept with its version, for block
/// themes, with nothing taken out; whatever made the version, so a fix by
/// hand is shown in a theme too. Any other build is left as it came.
pub async fn keep_needs(db: &SqlitePool, id: &str) -> Result<()> {
    let s = schematic::get(db, id).await?.context("no such schematic")?;
    if !s.evaluated() {
        return Ok(());
    }
    if !current(db, &s).await?.is_some_and(|report| report.passed()) {
        return Ok(());
    }
    let data = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    let mut needs: std::collections::BTreeMap<&str, Vec<[i32; 3]>> =
        std::collections::BTreeMap::new();
    for (pos, role) in roles::roles(&data, &s.stacking, buses(&s))? {
        if role.need != roles::Need::Unused {
            needs.entry(role.need.name()).or_default().push(pos);
        }
        // Which solid blocks carry power, hard or soft, so a block theme can
        // show them apart; a block that would carry it were it solid but
        // must not is transparent, and carries none.
        if role.need == roles::Need::Solid {
            if role.uses.contains(&roles::Use::HardPower) {
                needs.entry("hard_power").or_default().push(pos);
            } else if role.uses.contains(&roles::Use::SoftPower) {
                needs.entry("soft_power").or_default().push(pos);
            }
        }
    }
    let needs = serde_json::to_string(&needs)?;
    sqlx::query!(
        "UPDATE schematic_version SET needs = ?
         WHERE schematic_id = ? AND version = (
             SELECT max(version) FROM schematic_version WHERE schematic_id = ?
         )",
        needs,
        id,
        id
    )
    .execute(db)
    .await?;
    Ok(())
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
    if test_for(s).is_none() {
        return Ok(Status::Untested);
    }
    match current(db, s).await? {
        // An off-list build is not timed.
        Some(report) if stands_in(s).is_some() => Ok(Status::Done(report)),
        Some(report) if !report.passed() || report.timing().is_some() => {
            Ok(Status::Done(report))
        }
        report => {
            refresh_soon(db, &s.id);
            Ok(Status::Pending(report))
        }
    }
}
