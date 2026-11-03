//! Running builds' tests and timing again, and printing what a simulation
//! finds: whether its cases come out right, how its copies stack, and when
//! its outputs settle.

use std::sync::Arc;

use anyhow::{Result, bail, ensure};
use app::{
    models::{schematic, simulation},
    simulate::{Action, Report, Stacking, Step, Timing},
};
use sqlx::SqlitePool;

use super::{load, resolve};

/// How many builds `simulate` runs at once.
const AT_ONCE: usize = 4;

/// Runs the tests of the builds `named`, or with `all` of every build with
/// one, again, and stores what they find.
pub(super) async fn simulate(
    db: &SqlitePool,
    named: &[String],
    all: bool,
) -> Result<()> {
    ensure!(
        all || !named.is_empty(),
        "name the builds to simulate, as `rsdb search --ids` lists them, or \
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
    let one = !all;
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
            let report = match simulation::refresh(&db, &id, true).await? {
                Some(_) => simulation::refresh_timing(&db, &id, true).await?,
                None => None,
            };
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
    if failed > 0 {
        bail!("{failed} builds fail their tests");
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
/// any `controls` lines, each way with its pitch class where it has one.
fn print_stacking(stacking: &Stacking, controls: bool) {
    let pitch = |step: Step, bits: usize| {
        step.pitch(bits)
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
        } => println!(
            "        stacks: {copies} copies {} apart make {bits} bits{}, {}",
            step.describe(),
            pitch(*step, bits / copies),
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
        } => {
            let each = bits / copies;
            for way in also {
                println!(
                    "        stacks: {} copies {} apart make {} bits too{}, {}{}",
                    way.copies,
                    way.step.describe(),
                    each * way.copies,
                    pitch(way.step, each),
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
                pitch(*step, each),
                if controls {
                    ", sharing their control lines"
                } else {
                    ""
                },
                right(*cases),
                limit(*most)
            );
        }
        Stacking::Separate { ways } => {
            for way in ways {
                println!(
                    "        stacks: {} copies {} apart each work on their own, {}{}",
                    way.copies,
                    way.step.describe(),
                    right(way.cases),
                    limit(way.most)
                );
            }
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
