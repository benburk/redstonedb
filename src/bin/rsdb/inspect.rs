//! Checking a file or stored build by function: its levers, buttons, and
//! comparators with air behind in, and its repeaters and comparators
//! facing air out, and what the site recognizes it as with them. Nothing
//! here writes to the database.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result};
use app::{
    interface::{
        inspect::{self, Run},
        repeater_outputs::{self, Output},
    },
    models::version,
    recognize::probe,
    schem::{self, Blocks, Pos},
    store::builds,
};
use clap::Args;
use sqlx::SqlitePool;

use super::{load, resolve};

#[derive(Args)]
pub(super) struct Inspect {
    /// A .schem file, or a stored build's id or the start of one; with
    /// neither, every build on the list
    file_or_id: Option<String>,
    /// Probe as this category, such as adder, or with no build named, only
    /// the builds of it; a stored build's own by default
    #[arg(long)]
    category: Option<String>,
    /// Check the build's problems and its inputs and outputs only, probing
    /// nothing
    #[arg(long)]
    no_probe: bool,
}

/// Prints what `inspect` finds of one file or build: its problems, its
/// inputs and outputs, and what the site recognizes it as with them; or of
/// every build on the list, with a count of each problem and of the builds
/// it stops.
pub(super) async fn inspect_command(
    db: &SqlitePool,
    args: &Inspect,
) -> Result<()> {
    let (file_or_id, category) =
        (args.file_or_id.as_deref(), args.category.as_deref());
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    // A stored build named alone is probed as its own category.
    let mut stored = None;
    match file_or_id {
        Some(path) if fs::exists(path)? => {
            files.push((path.to_owned(), fs::read(path)?));
        }
        Some(prefix) => {
            let id = resolve(db, prefix).await?;
            let s = load(db, &id).await?;
            let data = version::current_file(db, &id)
                .await?
                .context("schematic vanished")?;
            files.push((format!("{}  {}", &id[..8], s.filename()), data));
            stored = Some(s.category);
        }
        None => {
            for s in builds::all(db).await? {
                if s.kind().is_none()
                    || category.is_some_and(|c| c != s.category)
                {
                    continue;
                }
                let data = version::current_file(db, &s.id)
                    .await?
                    .context("schematic vanished")?;
                files.push((format!("{}  {}", &s.id[..8], s.filename()), data));
            }
        }
    }
    let one = files.len() == 1;
    let mut problems: BTreeMap<&str, usize> = BTreeMap::new();
    let mut stopped: BTreeMap<&str, usize> = BTreeMap::new();
    let mut clean = 0;
    for (name, data) in &files {
        let report = app::recognize::bus_drivable::inspect_file(data)?;
        let buses = |runs: &[inspect::Run]| {
            runs.iter()
                .map(|run| run.bits.len().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "{name}: {} errors; inputs {}; outputs {}",
            report.faults().count(),
            buses(&report.inputs),
            buses(&report.outputs)
        );
        if one {
            for fix in &report.fixes {
                println!("  fix: {fix}");
            }
            for note in &report.notes {
                println!("  note: {note}");
            }
            // A fault only on a data input, which probing tells apart.
            for problem in report.data_only() {
                println!("  note: {}", problem.message);
            }
        }
        let mut rules = BTreeSet::new();
        for problem in report.faults() {
            println!("  {}", problem.message);
            *problems.entry(problem.rule).or_default() += 1;
            rules.insert(problem.rule);
        }
        for rule in &rules {
            *stopped.entry(rule).or_default() += 1;
        }
        clean += usize::from(report.faults().next().is_none());
    }
    if !one {
        println!("\n{clean} of {} builds have no errors", files.len());
        for (rule, builds) in &stopped {
            println!("{builds:4} builds  {:5} times  {rule}", problems[rule]);
        }
        return Ok(());
    }
    if let [(_, data)] = files.as_slice() {
        print_lines(data, category.or(stored.as_deref()), args.no_probe)?;
    }
    Ok(())
}
/// An output in words, such as `repeater at (1, 2, 3) facing west, giving
/// out east, on yellow concrete`.
fn describe(blocks: &Blocks, output: Output) -> String {
    let under = schem::short(blocks.get(output.block())).replace('_', " ");
    format!(
        "{} at {} facing {}, giving out {}, on {under}",
        output.kind(),
        inspect::place(output.at),
        output.facing,
        output.points()
    )
}

/// A run's bits in words: its length and step, or the lone line.
fn run_words(run: &Run) -> String {
    if run.bits.len() == 1 {
        "line".to_owned()
    } else {
        format!(
            "bus of {}, step {}",
            run.bits.len(),
            inspect::place(run.step).replace(['(', ')'], "")
        )
    }
}

/// Prints the inputs and outputs of the file in `data` as the rules for
/// them read them, and unless `no_probe` what the site recognizes it as,
/// probing it as `category`, or with none as each in turn.
fn print_lines(
    data: &[u8],
    category: Option<&str>,
    no_probe: bool,
) -> Result<()> {
    let blocks = Blocks::read(data)?;
    let report = inspect::inspect(&blocks);
    println!("inputs: {}", report.inputs.len());
    let switches: BTreeMap<Pos, schem::Switch> = blocks
        .switches()
        .into_iter()
        .map(|switch| (switch.block, switch))
        .collect();
    for run in &report.inputs {
        let bits: Vec<String> = run
            .bits
            .iter()
            .map(|block| {
                let switch = &switches[block];
                format!(
                    "{} on {} {}",
                    schem::short(blocks.get(switch.pos)).replace('_', " "),
                    schem::short(blocks.get(*block)).replace('_', " "),
                    inspect::place(*block)
                )
            })
            .collect();
        println!("  {}: {}", run_words(run), bits.join("; "));
    }
    print_strength_inputs(&blocks);
    let found = repeater_outputs::find(&blocks);
    let by_at: BTreeMap<Pos, Output> =
        found.iter().map(|&o| (o.at, o)).collect();
    println!("outputs: {}", found.len());
    for run in repeater_outputs::lines(&found) {
        println!("  {}:", run_words(&run));
        for at in &run.bits {
            println!("    {}", describe(&blocks, by_at[at]));
        }
    }
    if no_probe {
        return Ok(());
    }
    let recognition = probe::recognize_as(data, category)?;
    match &recognition.found {
        Some(found) => println!("recognized: {found}"),
        None => println!("recognized: nothing holds"),
    }
    for problem in &recognition.problems {
        println!("  {problem}");
    }
    Ok(())
}

/// Prints the signal-strength inputs the rule for them finds in `blocks`,
/// comparators with air behind, in buses.
fn print_strength_inputs(blocks: &Blocks) {
    let found = repeater_outputs::strength_inputs(blocks);
    println!("signal-strength inputs: {}", found.len());
    let by_at: BTreeMap<Pos, repeater_outputs::StrengthInput> =
        found.iter().map(|&i| (i.at, i)).collect();
    for run in repeater_outputs::strength_lines(&found) {
        let bits: Vec<String> = run
            .bits
            .iter()
            .map(|at| {
                let input = by_at[at];
                format!(
                    "{} facing {} into {} {}",
                    inspect::place(input.at),
                    input.facing,
                    schem::short(blocks.get(input.front())).replace('_', " "),
                    inspect::place(input.front())
                )
            })
            .collect();
        println!("  {}: {}", run_words(&run), bits.join("; "));
    }
}
