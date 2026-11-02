//! Checking a file or stored build by function, and working builds out
//! from their files alone to compare with what is stored.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result};
use app::models::{catalogue, schematic, version};
use clap::Args;
use sqlx::SqlitePool;

use super::{load, resolve};

#[derive(Args)]
pub(super) struct Inspect {
    file_or_id: Option<String>,
    /// Only the builds of this category, probed as it, such as adder
    #[arg(long)]
    category: Option<String>,
}

/// Prints what `inspect` finds of one file or build, or of every build on
/// the list, with a count of each problem and of the builds it stops.
pub(super) async fn inspect_command(
    db: &SqlitePool,
    args: &Inspect,
) -> Result<()> {
    let (file_or_id, category) =
        (args.file_or_id.as_deref(), args.category.as_deref());
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
            let listed: Vec<String> = catalogue::categories(db)
                .await?
                .into_iter()
                .map(|c| c.name)
                .collect();
            for s in schematic::all(db).await? {
                if !listed.contains(&s.category)
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
        let report = app::recognize::inspect::inspect_file(data, adder)?;
        let buses = |runs: &[app::recognize::inspect::Run]| {
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
    }
    Ok(())
}

/// Evaluates build `id`, or every build of `category`, from its file, and
/// prints each field where that differs from what is stored, then how
/// many builds differ in each.
pub(super) async fn evaluate(
    db: &SqlitePool,
    id: Option<&str>,
    category: &str,
) -> Result<()> {
    let builds: Vec<schematic::Schematic> = match id {
        Some(prefix) => vec![load(db, &resolve(db, prefix).await?).await?],
        None => schematic::all(db)
            .await?
            .into_iter()
            .filter(|s| s.category == category)
            .collect(),
    };
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &builds {
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
            s.flags.iter().map(|f| f.name.clone()).collect();
        stored_flags.sort();
        let mut flags = e.flags.clone();
        flags.sort();
        let design = e.design.as_ref().map_or("other", |d| d.name.as_str());
        let fields: [(&str, String, String); 12] = [
            (
                "category",
                s.category.clone(),
                e.category.unwrap_or("none").to_owned(),
            ),
            ("passes", "true".to_owned(), e.passes().to_string()),
            ("width", show(&s.width), show(&e.width)),
            (
                "orientation",
                show(&Some(s.orientation)),
                show(&e.orientation),
            ),
            ("encoding", show(&s.encoding), show(&e.encoding)),
            ("flags", show(&stored_flags), show(&flags)),
            ("stacking", show(&s.stacking), show(&e.stacking)),
            ("latency", show(&s.latency), show(&e.latency)),
            ("synchronized", show(&s.synchronized), show(&e.synchronized)),
            ("throughput", show(&s.throughput), show(&e.throughput)),
            ("design", s.design.clone(), design.to_owned()),
            ("logic", show(&s.logic), show(&e.logic)),
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
    println!("{} builds evaluated", builds.len());
    for (field, n) in tally {
        println!("  {n} differ in {field}");
    }
    Ok(())
}
