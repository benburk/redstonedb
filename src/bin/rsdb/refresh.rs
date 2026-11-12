//! Bringing what is worked out from builds' files up to date: each read
//! again, judged again as its category, its test and timing run again, and
//! what its building blocks must be kept, as after a change to the
//! simulator or the rules; or, changing nothing, where what would be
//! worked out differs from what is stored. Then printing what a simulation
//! finds: whether its cases come out right, how its copies stack, and when
//! its outputs settle.

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, bail, ensure};
use app::{
    models::{schematic, simulation, version},
    simulate::{Action, Report, Stacking, Step, Timing},
};
use sqlx::SqlitePool;

use super::{builds, load, resolve, versions};

/// How many builds `refresh` brings up to date at once.
pub(super) const AT_ONCE: usize = 4;

/// What a refresh of one build does besides what it always does.
#[derive(Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each a step of refreshing, taken or not"
)]
pub(super) struct Refreshing {
    /// Run its test and timing even when the stored result is of the same
    /// file, as after a change to the simulator.
    pub(super) again: bool,
    /// Judge it again as its category, its stored reason for being
    /// unverified, if any, cleared first.
    pub(super) rejudge: bool,
    /// Take out its unused blocks, as a new version.
    pub(super) prune: bool,
    /// Find its lines' roles again by probing it as its category.
    pub(super) probe: bool,
}

/// Brings the builds `named`, or with `all` every build, up to date, as
/// [`refresh_one`] does, taking out unused blocks with `prune` and finding
/// their roles again with `probe`; or with
/// `dry_run` says where what would be worked out differs from what is
/// stored, changing nothing.
pub(super) async fn refresh(
    db: &SqlitePool,
    named: &[String],
    all: bool,
    (prune, probe): (bool, bool),
    dry_run: bool,
) -> Result<()> {
    ensure!(
        all || !named.is_empty(),
        "name the builds to refresh, as `rsdb search --ids` lists them, or \
         pass --all"
    );
    let ids = if all {
        schematic::all(db)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect()
    } else {
        let mut ids = Vec::new();
        for prefix in named {
            ids.push(resolve(db, prefix).await?);
        }
        ids
    };
    if dry_run {
        return compare(db, &ids, all, prune).await;
    }
    let how = Refreshing {
        again: true,
        rejudge: true,
        prune,
        probe,
    };
    let failed = refresh_all(db, ids, !all, how).await?;
    if failed > 0 {
        bail!("{failed} builds fail their tests");
    }
    Ok(())
}

/// Refreshes the builds `ids` as `how` says, `AT_ONCE` at a time, saying
/// of each build with no test so only when there is `one`, and returns how
/// many fail.
async fn refresh_all(
    db: &SqlitePool,
    ids: Vec<String>,
    one: bool,
    how: Refreshing,
) -> Result<usize> {
    let mut failed = 0;
    // Each build simulates on its own thread, `AT_ONCE` at a time, and
    // says how it went as it finishes.
    simulation::run_at_once(AT_ONCE);
    let gate = Arc::new(tokio::sync::Semaphore::new(AT_ONCE));
    let mut running = tokio::task::JoinSet::new();
    for id in ids {
        let (db, gate) = (db.clone(), Arc::clone(&gate));
        running.spawn(async move {
            let _turn = gate.acquire_owned().await?;
            let start = std::time::Instant::now();
            let report = refresh_one(&db, &id, how).await?;
            let name =
                format!("{}  {}", &id[..8], load(&db, &id).await?.filename());
            anyhow::Ok((name, report, start.elapsed()))
        });
    }
    while let Some(done) = running.join_next().await {
        let (name, report, took) = done??;
        match report {
            Some(report) => {
                print_report(&name, &report);
                println!("        in {took:.1?}");
                failed += usize::from(report.failed());
            }
            None if one => {
                println!("{name}: no test for its category and width");
            }
            // Every build is tried; only those with a test say so.
            None => {}
        }
    }
    Ok(failed)
}

/// Brings build `id` up to date with its file, as after a change to the
/// rules or the simulator, a new version, or new details: reads its file's
/// facts again, judges it again as its category when `how` says to, runs
/// its test and timing, keeps what its building blocks must be, for
/// themes, and takes out its unused blocks when `how` says to. Returns its
/// test's result; `None` when it has no test.
pub(super) async fn refresh_one(
    db: &SqlitePool,
    id: &str,
    how: Refreshing,
) -> Result<Option<Report>> {
    versions::reread(db, id).await?;
    if how.probe {
        reprobe(db, id).await?;
    }
    if how.rejudge {
        builds::rejudge(db, id).await?;
    }
    let report = match simulation::refresh(db, id, how.again).await? {
        Some(_) => simulation::refresh_timing(db, id, how.again).await?,
        None => None,
    };
    simulation::keep_needs(db, id).await?;
    if how.prune {
        versions::prune(db, id, false).await?;
    }
    Ok(report)
}

/// Finds build `id`'s lines' roles again, probing its current file as its
/// category, as an upload's are found, and keeps them as
/// [`app::recognize::probe::reprobe`] chooses; says when it wasn't found
/// as its category, and which roles it kept.
async fn reprobe(db: &SqlitePool, id: &str) -> Result<()> {
    let s = load(db, id).await?;
    let data = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    let category = s.category.clone();
    let reprobed = tokio::task::spawn_blocking(move || {
        app::recognize::probe::reprobe(&data, &category)
    })
    .await??;
    let kept = match reprobed {
        Some((file, found)) => {
            version::set_current_roles(db, id, &file).await?;
            if found {
                return Ok(());
            }
            "its nearest reading's roles kept"
        }
        None => "its roles kept",
    };
    println!(
        "{}  {}: not found as its category, {kept}",
        &id[..8],
        s.filename()
    );
    Ok(())
}

/// Works out each of the builds `ids` from its file alone, and prints each
/// field where that differs from what is stored, then how many builds
/// differ in each; with `prune`, first what would be taken out of each.
/// For `all`, only builds of a category whose every measured detail is
/// worked out so.
async fn compare(
    db: &SqlitePool,
    ids: &[String],
    all: bool,
    prune: bool,
) -> Result<()> {
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    let mut compared = 0;
    for id in ids {
        let s = load(db, id).await?;
        if all && !app::category::behaviour(&s.category).evaluated {
            continue;
        }
        compared += 1;
        if prune {
            versions::prune(db, id, true).await?;
        }
        let name = format!("{}  {}", &s.id[..8], s.filename());
        let file = version::current_file(db, &s.id)
            .await?
            .context("schematic vanished")?;
        // Its test and timing spread over every core already.
        let e = match app::recognize::evaluate::evaluate(&file) {
            Ok(e) => e,
            Err(err) => {
                println!("{name}: could not evaluate: {err}");
                *tally.entry("could not evaluate").or_default() += 1;
                continue;
            }
        };
        let show = |v: &dyn std::fmt::Debug| format!("{v:?}");
        let mut stored_flags: Vec<String> =
            s.flags.iter().map(|f| f.name.to_owned()).collect();
        stored_flags.sort();
        let mut flags = e.flags.clone();
        flags.sort();
        let show_design =
            |design: Option<&str>| design.unwrap_or("none").to_owned();
        let design = e.design.as_ref().and_then(|d| d.given());
        let fields: [(&str, String, String); 12] = [
            (
                "category",
                s.category.clone(),
                e.category.unwrap_or("none").to_owned(),
            ),
            ("passes", "true".to_owned(), e.passes().to_string()),
            ("width", show(&s.specs.width), show(&e.width)),
            (
                "orientation",
                show(&Some(s.specs.orientation)),
                show(&e.orientation),
            ),
            ("encoding", show(&s.specs.encoding), show(&e.encoding)),
            ("flags", show(&stored_flags), show(&flags)),
            ("stacking", show(&s.stacking), show(&e.stacking)),
            ("latency", show(&s.specs.latency), show(&e.latency)),
            (
                "synchronized",
                show(&s.specs.synchronized),
                show(&e.synchronized),
            ),
            ("throughput", show(&s.specs.throughput), show(&e.throughput)),
            (
                "design",
                show_design(s.specs.design.as_deref()),
                show_design(design),
            ),
            ("logic", show(&s.logic()), show(&e.logic)),
        ];
        let differ: Vec<String> = fields
            .iter()
            .filter(|(_, stored, derived)| stored != derived)
            .map(|(field, stored, derived)| {
                *tally.entry(field).or_default() += 1;
                format!("{field} {stored} -> {derived}")
            })
            .collect();
        if !differ.is_empty() {
            let signature = e.design.as_ref().map_or(String::new(), |d| {
                format!(" [{} {} terms]", d.signature, d.products)
            });
            println!("{name}{signature}: {}", differ.join("; "));
        }
    }
    println!("{compared} builds worked out");
    for (field, n) in tally {
        println!("  {n} differ in {field}");
    }
    Ok(())
}

/// One build's simulation result, on a line or a few.
pub(super) fn print_report(name: &str, report: &Report) {
    match report {
        Report::Ran(run) if run.wrong == 0 => println!(
            "ok      {name}: {} {} {}, all right",
            match (run.exhaustive, run.by_digit) {
                (true, _) => "every",
                (false, true) => "each digit's and the patterns'",
                (false, false) => "each byte's and the patterns'",
            },
            run.cases,
            run.pattern
        ),
        Report::Ran(run) => {
            println!(
                "WRONG   {name}: {} of {} {} wrong",
                run.wrong, run.cases, run.pattern
            );
            if let Some(hint) = &run.hint {
                println!("        {hint}");
            }
            for (i, cell) in run.cells.iter().enumerate() {
                let at = |[x, y, z]: [i32; 3]| format!("({x}, {y}, {z})");
                let read = cell.read.map(|pos| format!(", read {}", at(pos)));
                println!(
                    "        cell {}: write {}{}",
                    i + 1,
                    at(cell.write),
                    read.unwrap_or_default()
                );
            }
            let width = run.inputs.first().map_or(1, |signal| signal.bits);
            for steps in &run.faults {
                let steps: Vec<String> = steps
                    .iter()
                    .map(|step| match step {
                        Action::Write { cell, value } => {
                            format!("w{} {value:0width$b}", cell + 1)
                        }
                        Action::Read {
                            cell,
                            expected,
                            got,
                            ..
                        } if got == expected => {
                            format!("r{} {got:0width$b}", cell + 1)
                        }
                        Action::Read {
                            cell,
                            expected,
                            got,
                            ..
                        } => format!(
                            "r{} got {got:0width$b} expected {expected:0width$b}",
                            cell + 1
                        ),
                    })
                    .collect();
                println!("        fault: {}", steps.join("; "));
            }
            let bits: Vec<String> = run
                .wrong_bits
                .iter()
                .map(|(bit, count)| format!("{bit} ({count})"))
                .collect();
            println!("        wrong bits: {}", bits.join(", "));
            for wrong in &run.examples {
                println!("        {}", run.case(wrong));
            }
        }
        Report::Skipped { reason } => println!("skipped {name}: {reason}"),
        Report::Broken { reason } => println!("BROKEN  {name}: {reason}"),
    }
    if let Report::Ran(run) = report
        && let Some(stacking) = &run.stacking
    {
        // A build with only input A, as a monostable, has no control
        // lines for its copies to share.
        print_stacking(stacking, run.inputs.len() > 1);
    }
    if let Some(timing) = report.timing() {
        print_timing(timing);
    }
}

/// The cases copies stacked ran, all right; none for cells 1 wide, 2
/// apart, taken to stack untested.
fn right(cases: u64) -> String {
    if cases == 0 {
        "taken to work untested, 1 wide with air between".to_owned()
    } else {
        format!("all {cases} cases right")
    }
}

/// What stacking copies of a simulated build found, whose copies share
/// any `controls` lines, each way with its pitch class, in blocks a place,
/// a bit or a digit, where it has one.
fn print_stacking(stacking: &Stacking, controls: bool) {
    let pitch = |step: Step, places: usize| {
        step.pitch(places)
            .map(|pitch| format!(", pitch {pitch}"))
            .unwrap_or_default()
    };
    let limit = |most: Option<usize>| {
        if most.is_some() {
            "; one more copy fails"
        } else {
            ""
        }
    };
    match stacking {
        Stacking::Stacks {
            step,
            copies,
            bits,
            cases,
            place,
        } => println!(
            "        stacks: {copies} copies {} apart make {bits} bits{}, {}",
            step.describe(),
            pitch(*step, bits / copies / place),
            right(*cases)
        ),
        Stacking::Registers {
            step,
            copies,
            registers,
            cases,
            most,
        } => println!(
            "        stacks: {copies} copies {} apart make {registers} registers, {}{}",
            step.describe(),
            right(*cases),
            limit(*most)
        ),
        Stacking::Wider {
            step,
            copies,
            bits,
            cases,
            most,
            also,
            words,
            place,
        } => {
            let each = bits / copies;
            let places = each / place;
            for way in also {
                println!(
                    "        stacks: {} copies {} apart make {} bits too{}, {}{}",
                    way.copies,
                    way.step.describe(),
                    each * way.copies,
                    pitch(way.step, places),
                    right(way.cases),
                    limit(way.most)
                );
            }
            if let Some(words) = words {
                println!(
                    "        stacks: {} copies {} apart make {} words, {}{}",
                    words.copies,
                    words.step.describe(),
                    words.copies,
                    right(words.cases),
                    limit(words.most)
                );
            }
            println!(
                "        stacks: {copies} copies {} apart make {bits} bits{}{}, {}{}",
                step.describe(),
                pitch(*step, places),
                if controls {
                    ", sharing their control lines"
                } else {
                    ""
                },
                right(*cases),
                limit(*most)
            );
        }
        Stacking::Not { step, reason } => println!(
            "        doesn't stack{}: {reason}",
            step.map(|s| format!(" {} apart", s.describe()))
                .unwrap_or_default()
        ),
    }
}

/// When a build's inputs may change and its outputs settle, from t = 0, as
/// the slowest inputs change, and each bit's timing for an output that is not synced.
fn print_timing(timing: &Timing) {
    let tick = |tick: i32| match tick {
        0 => "0".to_owned(),
        t => format!("{t:+}"),
    };
    let inputs: Vec<String> = timing
        .inputs
        .iter()
        .map(|input| {
            let at = input.tick.map_or_else(|| "unused".to_owned(), tick);
            format!("{} at {at}", input.name)
        })
        .collect();
    let outputs: Vec<String> = timing
        .outputs
        .iter()
        .map(|output| match output.carried {
            0 => format!("{} by {}t", output.name, output.latency),
            carried => format!(
                "{} by {}t, carried out {carried}t later",
                output.name, output.latency
            ),
        })
        .collect();
    println!(
        "        timing: {}; {}; {}",
        inputs.join(", "),
        outputs.join(", "),
        if timing.synced {
            "synced"
        } else {
            "not synced"
        }
    );
    println!(
        "        half settle by {}t, 99% by {}t, slowest {:?} by {}t, over {} changes",
        timing.median,
        timing.p99,
        timing.slowest_case,
        timing.slowest,
        timing.changes
    );
    if let Some(pulse) = timing.pulse {
        println!(
            "        pulse: {} ticks, {}, as {}",
            pulse.ticks,
            if pulse.inverted {
                "low, inverted"
            } else {
                "high"
            },
            pulse.edge.words()
        );
    }
    for output in timing
        .outputs
        .iter()
        .filter(|o| o.glitches > 0 || o.staggered > 0)
    {
        println!(
            "        {} glitches in {} changes and staggers in {}; by bit, first change, settled, glitches:",
            output.name, output.glitches, output.staggered
        );
        if let Some(glitch) = &output.glitch {
            println!(
                "          e.g. from all off to {:?}: from {} to {:?}",
                glitch.case, glitch.before, glitch.steps
            );
        }
        for (bit, b) in output.bits.iter().enumerate() {
            println!(
                "          {bit}: {}, {}, {}",
                tick(b.first_change),
                tick(b.latency),
                b.glitches
            );
        }
    }
}
