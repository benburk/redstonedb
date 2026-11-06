//! Checking a file or stored build by function: its levers, buttons, and
//! comparators with air behind in, and its repeaters and comparators
//! facing air out, what probing finds with them, and how its copies stack.
//! Nothing here writes to the database.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, bail};
use app::{
    models::{schematic, version},
    recognize::inspect,
};
use clap::Args;
use sqlx::SqlitePool;

use super::{
    load,
    outputs::{print_lines, source},
    resolve,
    stack_check::{StackCheck, stack_check_command},
};

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
    /// Stack copies of the build as the site's stacking test does, the
    /// first whole and the rest its box, each way given, of west, east, and
    /// up, settle them, read them by the rule for outputs, and test them
    /// as the site tests its kind's copies; with no ways given, a stored
    /// build's own and their opposites, else all three
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    stack: Option<Vec<String>>,
    /// Copies stacked in all, the original among them
    #[arg(long, default_value_t = 3, requires = "stack")]
    copies: usize,
    /// Blocks between stacked copies; a stored build's own along that axis
    /// by default, else each of 1 to 4
    #[arg(long, requires = "stack")]
    spacing: Option<i64>,
    /// A stored build whose category and stacking to stack a file as, for
    /// a file made from it
    #[arg(long, requires = "stack")]
    like: Option<String>,
    /// Stack the one way and spacing given here, in this process, exiting
    /// 0 when it passes and 3 when not, as each stack is run apart
    #[arg(long, hide = true, requires = "stack")]
    one: bool,
}

/// Prints what `inspect` finds of one file or build: its problems, its
/// inputs and outputs, and what probing finds with them, or how its copies
/// stack; or of every build on the list, with a count of each problem and
/// of the builds it stops.
pub(super) async fn inspect_command(
    db: &SqlitePool,
    args: &Inspect,
) -> Result<()> {
    let (file_or_id, category) =
        (args.file_or_id.as_deref(), args.category.as_deref());
    if let Some(dirs) = &args.stack {
        let Some(file_or_id) = file_or_id else {
            bail!("name the build or file to stack");
        };
        let check = StackCheck {
            file_or_id,
            copies: args.copies,
            dirs,
            spacing: args.spacing,
            category,
            like: args.like.as_deref(),
            one: args.one,
        };
        return stack_check_command(db, &check).await;
    }
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
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
        }
        None => {
            for s in schematic::all(db).await? {
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
        let adder = category == Some("adder");
        let report = inspect::inspect_file(data, adder)?;
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
    if let Some(file_or_id) = file_or_id {
        print_lines(&source(db, file_or_id).await?, category, args.no_probe)?;
    }
    Ok(())
}
