//! Measuring the rule proposed for outputs, a repeater or comparator
//! powering nothing, see [`repeater_outputs`], before the site reads
//! builds by it: what it finds in a build beside what today's rule finds,
//! and a build reworked to it by rule; `stack_check` stacks copies of one.
//! Nothing here writes to the database.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::Write as _,
    fs,
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use app::{
    category::Kind,
    models::{schematic, simulation, version},
    recognize::{
        inspect::{self, Run},
        interface::{self, Interface},
        probe,
        repeater_outputs::{self, Output, Reworked},
    },
    schem::{self, Blocks, Direction, Pos, standardize},
    simulate::{self, Circuit, Test},
};
use clap::Args;
use sqlx::SqlitePool;

use super::{load, resolve};

#[derive(Args)]
pub(super) struct Outputs {
    /// A .schem file, or a stored build's id or the start of one
    file_or_id: String,
    /// Probe only as this category, such as adder; a stored build's own
    /// by default
    #[arg(long)]
    category: Option<String>,
    /// List the outputs only, probing nothing
    #[arg(long)]
    no_probe: bool,
}

#[derive(Args)]
pub(super) struct Convert {
    /// A .schem file, or a stored build's id or the start of one
    file_or_id: String,
    /// Where to write the reworked build; nothing is written without it
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// The build's category, for its test; a stored build's own by default
    #[arg(long)]
    category: Option<String>,
    /// Read today's outputs as an upload's, any lone block read on top,
    /// rather than in the site's output colours
    #[arg(long)]
    upload: bool,
}

/// A build to measure: its name, its file, and its row when stored.
pub(super) struct Source {
    pub(super) name: String,
    pub(super) data: Vec<u8>,
    pub(super) stored: Option<schematic::Schematic>,
}

/// The file at `file_or_id`, or the current file of the stored build it
/// names.
pub(super) async fn source(
    db: &SqlitePool,
    file_or_id: &str,
) -> Result<Source> {
    if fs::exists(file_or_id)? {
        return Ok(Source {
            name: file_or_id.to_owned(),
            data: fs::read(file_or_id)?,
            stored: None,
        });
    }
    let id = resolve(db, file_or_id).await?;
    let stored = load(db, &id).await?;
    let data = version::current_file(db, &id)
        .await?
        .context("schematic vanished")?;
    Ok(Source {
        name: format!("{}  {}", &id[..8], stored.filename()),
        data,
        stored: Some(stored),
    })
}

/// The kind named `category`, or a stored build's.
pub(super) fn kind_of(
    category: Option<&str>,
    stored: Option<&schematic::Schematic>,
) -> Option<Kind> {
    category
        .map(str::to_owned)
        .or_else(|| stored.map(|s| s.category.clone()))
        .and_then(|name| Kind::named(&name))
}

pub(super) fn place(pos: Pos) -> String {
    inspect::place(pos)
}

/// An output in words, such as `repeater at (1, 2, 3) facing west, giving
/// out east, on yellow concrete`.
pub(super) fn describe(blocks: &Blocks, output: Output) -> String {
    let under = schem::short(blocks.get(output.block())).replace('_', " ");
    let hint = repeater_outputs::colour_under(blocks, output)
        .map_or(String::new(), |c| format!(", a {c} role colour"));
    format!(
        "{} at {} facing {}, giving out {}, on {under}{hint}",
        output.kind(),
        place(output.at),
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
            place(run.step).replace(['(', ')'], "")
        )
    }
}

/// Prints the inputs and outputs of a build as the proposed rule reads
/// them, beside today's outputs, and what probing finds with them.
pub(super) async fn outputs_command(
    db: &SqlitePool,
    args: &Outputs,
) -> Result<()> {
    let source = source(db, &args.file_or_id).await?;
    let blocks = Blocks::read(&source.data)?;
    println!("{}", source.name);
    let report = repeater_outputs::report(&blocks);
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
                    place(*block)
                )
            })
            .collect();
        println!("  {}: {}", run_words(run), bits.join("; "));
    }
    let found = repeater_outputs::find(&blocks);
    let by_at: BTreeMap<Pos, Output> =
        found.iter().map(|&o| (o.at, o)).collect();
    println!("outputs by the proposed rule: {}", found.len());
    for run in repeater_outputs::lines(&found) {
        println!("  {}:", run_words(&run));
        for at in &run.bits {
            println!("    {}", describe(&blocks, by_at[at]));
        }
    }
    for problem in report.faults() {
        println!("  problem: {}", problem.message);
    }
    let proposed: BTreeSet<Pos> = found.iter().map(|o| o.block()).collect();
    print_today(&blocks, &proposed);
    if args.no_probe {
        return Ok(());
    }
    let meant = kind_of(args.category.as_deref(), source.stored.as_ref());
    match repeater_outputs::probe(&source.data, meant)? {
        Ok(probed) => {
            println!("probed with these outputs: {}", probed.found.2);
            if let Some(layout) = &probed.layout {
                println!("  {}", layout.describe());
            }
            let (table_blocks, interface) =
                probed_interface(&source.data, &probed)?;
            match table(&table_blocks, &interface) {
                Ok(table) => print_min_ss(&interface, &table),
                Err(err) => println!("  min SS not measured: {err}"),
            }
        }
        Err(faults) => {
            println!("probed with these outputs: nothing holds");
            for (kind, fault) in faults.iter().take(6) {
                println!("  {}: {fault}", kind.name());
            }
        }
    }
    Ok(())
}

/// Prints the outputs today's rule finds in `blocks`, read in the site's
/// colours and as an upload, and where they differ from `proposed`, the
/// blocks under the proposed rule's outputs.
fn print_today(blocks: &Blocks, proposed: &BTreeSet<Pos>) {
    for (label, marks) in [
        ("site colours", inspect::SITE),
        ("as an upload", inspect::UPLOAD),
    ] {
        let today = inspect::inspect_marked(blocks, marks);
        let bits: BTreeSet<Pos> =
            today.outputs.iter().flat_map(|r| r.bits.clone()).collect();
        let runs: Vec<String> = today
            .outputs
            .iter()
            .map(|r| r.bits.len().to_string())
            .collect();
        println!(
            "today's rule, {label}: {} outputs, runs {}",
            bits.len(),
            if runs.is_empty() {
                "none".to_owned()
            } else {
                runs.join(" ")
            }
        );
        let words = |set: Vec<&Pos>| {
            set.iter()
                .map(|&&pos| {
                    let top = schem::step(pos, Direction::Up);
                    format!(
                        "{} {} under {}",
                        place(pos),
                        schem::short(blocks.get(pos)).replace('_', " "),
                        schem::short(blocks.get(top)).replace('_', " ")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        let only_today: Vec<&Pos> = bits.difference(proposed).collect();
        let only_proposed: Vec<&Pos> = proposed.difference(&bits).collect();
        if !only_today.is_empty() {
            println!("  only today's: {}", words(only_today));
        }
        if !only_proposed.is_empty() {
            println!("  only the proposed rule's: {}", words(only_proposed));
        }
    }
}

/// The blocks and interface a probed build's cases run on: an adder's
/// layout over the build as it is, or else the roles probing coloured in.
fn probed_interface(
    data: &[u8],
    probed: &repeater_outputs::Probed,
) -> Result<(Blocks, Interface)> {
    if let Some(layout) = &probed.layout {
        let blocks = Blocks::read(&probe::simulatable(data)?)?;
        let interface = layout.interface(&blocks);
        return Ok((blocks, interface));
    }
    let blocks = Blocks::read(&probed.found.1)?;
    let interface = interface::read(&blocks);
    Ok((blocks, interface))
}

/// Each case's inputs and outputs, and for each output bit the weakest
/// strength while on: arriving at the repeater or comparator on its block,
/// and shown by what stands on it, as the site measures today.
struct Table {
    cases: Vec<(u64, Vec<u64>)>,
    /// Each case's strength arriving at each output bit's repeater or
    /// comparator, the outputs' bits one after another.
    arrivals: Vec<Vec<u8>>,
    arriving: Vec<Vec<u8>>,
    shown: Vec<Vec<u8>>,
    exhaustive: bool,
}

/// The most input bits every combination of which is run; more are
/// sampled.
const EXHAUSTIVE_BITS: usize = 16;
const SAMPLES: usize = 4096;

/// Runs `blocks` through `interface` case by case, every combination of
/// its inputs, or a fixed sample of them when there are many. A build
/// with a button or a signal-strength input isn't run so.
fn table(blocks: &Blocks, interface: &Interface) -> Result<Table> {
    if interface.inputs.is_empty() || interface.outputs.is_empty() {
        bail!("no inputs or no outputs to run");
    }
    if interface.inputs.iter().any(|port| port.signal_strength) {
        bail!("a signal-strength input");
    }
    let mut circuit = Circuit::new(blocks, interface)?;
    let widths: Vec<usize> = interface
        .inputs
        .iter()
        .map(|port| port.bits.len())
        .collect();
    for (port, &width) in widths.iter().enumerate() {
        if (0..width).any(|bit| circuit.is_button(port, bit)) {
            bail!("a button, so it holds state");
        }
    }
    let bits: usize = widths.iter().sum();
    if bits > 64 {
        bail!("{bits} input bits");
    }
    let exhaustive = bits <= EXHAUSTIVE_BITS;
    let inputs: Vec<u64> = if exhaustive {
        (0..1u64 << bits).collect()
    } else {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1 << bits) - 1
        };
        (0..SAMPLES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state & mask
            })
            .collect()
    };
    let mut arriving: Vec<Vec<u8>> = interface
        .outputs
        .iter()
        .map(|port| vec![0; port.bits.len()])
        .collect();
    let mut shown = arriving.clone();
    let mut cases = Vec::with_capacity(inputs.len());
    let mut arrivals = Vec::with_capacity(inputs.len());
    let weaker = |kept: &mut u8, now: u8| {
        if now > 0 && (*kept == 0 || now < *kept) {
            *kept = now;
        }
    };
    for value in inputs {
        let mut shift = 0;
        for (port, &width) in widths.iter().enumerate() {
            let mask = if width == 64 {
                u64::MAX
            } else {
                (1 << width) - 1
            };
            circuit.set(port, value >> shift & mask);
            shift += width;
        }
        circuit.settle()?;
        let got: Vec<u64> = (0..interface.outputs.len())
            .map(|p| circuit.get(p))
            .collect();
        let mut arrived = Vec::new();
        for (port, out) in interface.outputs.iter().enumerate() {
            for (bit, strength) in
                circuit.strengths(port).into_iter().enumerate()
            {
                weaker(&mut shown[port][bit], strength);
            }
            for (bit, b) in out.bits.iter().enumerate() {
                let top = schem::step(b.pos, Direction::Up);
                let strength = circuit.arriving(top);
                weaker(&mut arriving[port][bit], strength);
                arrived.push(strength);
            }
        }
        arrivals.push(arrived);
        cases.push((value, got));
    }
    Ok(Table {
        cases,
        arrivals,
        arriving,
        shown,
        exhaustive,
    })
}

fn print_min_ss(interface: &Interface, table: &Table) {
    for (port, out) in interface.outputs.iter().enumerate() {
        let each = |strengths: &[u8]| {
            strengths
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "  min SS of {}, least significant first, over {} cases{}: \
             arriving {}; shown on top {}",
            out.name,
            table.cases.len(),
            if table.exhaustive { "" } else { " sampled" },
            each(&table.arriving[port]),
            each(&table.shown[port])
        );
    }
}

/// What a test run says, in a line.
fn verdict(report: &simulate::Report) -> String {
    match report {
        simulate::Report::Ran(run) if run.wrong == 0 => {
            format!("passes, all {} cases right, {}", run.cases, run.pattern)
        }
        simulate::Report::Ran(run) => run.wrong_cases(),
        simulate::Report::Broken { reason } => format!("broken: {reason}"),
        simulate::Report::Skipped { reason } => format!("skipped: {reason}"),
    }
}

/// Reworks a build's outputs, as today's rule reads them, to the proposed
/// rule, says what came of each, and checks the reworked build computes
/// what the original does.
pub(super) async fn convert_command(
    db: &SqlitePool,
    args: &Convert,
) -> Result<()> {
    let source = source(db, &args.file_or_id).await?;
    println!("{}", source.name);
    let conversion = convert(
        &source.data,
        args.upload,
        args.category
            .as_deref()
            .or(source.stored.as_ref().map(|s| s.category.as_str())),
        source.stored.as_ref().and_then(|s| s.width),
    )?;
    print!("{}", conversion.said);
    if let (Some(out), Some(file)) = (&args.out, &conversion.file) {
        fs::write(out, file)?;
        println!("wrote {}", out.display());
    }
    Ok(())
}

/// What `convert` made of a build: its reworked file, when every output
/// was reworked or already one, and what it found, in lines.
struct Conversion {
    file: Option<Vec<u8>>,
    said: String,
}

/// The build in `data` reworked to the proposed rule, its outputs read as
/// an upload's with `upload`, else in the site's colours, then checked
/// against the original: by its category's test, as `category` and
/// `width` give it, and case by case through one interface.
fn convert(
    data: &[u8],
    upload: bool,
    category: Option<&str>,
    width: Option<i64>,
) -> Result<Conversion> {
    let mut said = String::new();
    let simulated = probe::simulatable(data)?;
    let blocks = Blocks::read(&simulated)?;
    let marks = if upload {
        inspect::UPLOAD
    } else {
        inspect::SITE
    };
    let today = inspect::inspect_marked(&blocks, marks);
    let outputs: Vec<Pos> =
        today.outputs.iter().flat_map(|r| r.bits.clone()).collect();
    if outputs.is_empty() {
        writeln!(said, "  no outputs by today's rule")?;
        return Ok(Conversion { file: None, said });
    }
    let lines = interface::read(&blocks);
    let width = match (category, width) {
        (Some(category), None) => {
            simulation::derived_width(category, true, data)?
        }
        _ => width,
    };
    let test = category.and_then(|category| Test::of(category, width));
    let before = test.map(|test| test.run(&blocks));
    let digits = digit_outputs(&lines, before.as_ref());
    let (changes, done) = repeater_outputs::rework(&blocks, &outputs, |pos| {
        digits.contains(&pos)
    });
    let failed = tell_reworked(&done, &mut said)?;
    if failed > 0 {
        return Ok(Conversion { file: None, said });
    }
    // The original file, colours and all, with the changes, so it stays
    // as the builder made it.
    let file = standardize::edit(data, &changes)?;
    let after = Blocks::read(&probe::simulatable(&file)?)?;
    tell_read_back(&after, &outputs, &mut said)?;
    match (test, &before) {
        (Some(test), Some(before)) => {
            writeln!(said, "  test before: {}", verdict(before))?;
            writeln!(said, "  test after:  {}", verdict(&test.run(&after)))?;
        }
        _ => {
            if let Some(category) = category {
                writeln!(said, "  no test for {category} of {width:?}")?;
            }
        }
    }
    // One interface for both, from the colours, or else from probing.
    let mut lines = lines;
    if (lines.inputs.is_empty() || lines.outputs.is_empty())
        && let Ok(found) = probe::recognize_as(data, category)
        && found.found.is_some()
    {
        lines = interface::read(&Blocks::read(&found.file)?);
    }
    compare_tables(&blocks, &after, &lines, &mut said)?;
    Ok(Conversion {
        file: Some(file),
        said,
    })
}

/// The output blocks of `lines` that a test, as `before` ran it, reads as
/// strengths, four bits a block, as a hex adder's sum or a popcount's one
/// block: these take a comparator.
fn digit_outputs(
    lines: &Interface,
    before: Option<&simulate::Report>,
) -> HashSet<Pos> {
    match before {
        Some(simulate::Report::Ran(run)) => lines
            .outputs
            .iter()
            .filter(|port| {
                run.outputs.iter().any(|signal| {
                    signal.name == port.name
                        && signal.bits == 4 * port.bits.len()
                })
            })
            .flat_map(|port| port.bits.iter().map(|bit| bit.pos))
            .collect(),
        _ => HashSet::new(),
    }
}

/// Writes to `said` what came of each output, with a count of each;
/// how many couldn't be reworked.
fn tell_reworked(done: &[Reworked], said: &mut String) -> Result<usize> {
    let mut failed = 0;
    for reworked in done {
        match reworked {
            Reworked::Already { block } => {
                writeln!(said, "  already: output on {}", place(*block))?;
            }
            Reworked::Converted {
                block,
                now,
                cleared,
            } => {
                let kind = schem::short(now);
                let facing = Direction::facing(now).map_or("?", |f| f.name());
                write!(
                    said,
                    "  converted: output on {}: dust to {kind} facing \
                     {facing}",
                    place(*block)
                )?;
                if let Some((at, was)) = cleared {
                    write!(
                        said,
                        ", {} taken out of its front at {}",
                        schem::short(was).replace('_', " "),
                        place(*at)
                    )?;
                }
                writeln!(said)?;
            }
            Reworked::Not { block, at, why } => {
                failed += 1;
                writeln!(
                    said,
                    "  not converted: output on {}: {why}, at {}",
                    place(*block),
                    place(*at)
                )?;
            }
        }
    }
    let converted = done
        .iter()
        .filter(|r| matches!(r, Reworked::Converted { .. }))
        .count();
    writeln!(
        said,
        "  {} outputs: {converted} converted, {} already, {failed} not",
        done.len(),
        done.len() - converted - failed
    )?;
    Ok(failed)
}

/// Writes to `said` how many of `outputs`, today's, the proposed rule
/// finds in the reworked build `after`, and how many more it finds.
fn tell_read_back(
    after: &Blocks,
    outputs: &[Pos],
    said: &mut String,
) -> Result<()> {
    let found: HashSet<Pos> = repeater_outputs::find(after)
        .iter()
        .map(|o| o.block())
        .collect();
    let missed: Vec<String> = outputs
        .iter()
        .filter(|pos| !found.contains(*pos))
        .map(|&pos| place(pos))
        .collect();
    let extra = found.len() + missed.len() - outputs.len();
    writeln!(
        said,
        "  read back by the proposed rule: {} of {} outputs found{}{}",
        outputs.len() - missed.len(),
        outputs.len(),
        if missed.is_empty() {
            String::new()
        } else {
            format!(", missing {}", missed.join(", "))
        },
        if extra > 0 {
            format!(", {extra} more repeaters or comparators facing air")
        } else {
            String::new()
        }
    )?;
    Ok(())
}

/// Writes to `said` how the truth tables of `blocks` and `after` compare,
/// run through `lines`, and each output's weakest strength in each.
fn compare_tables(
    blocks: &Blocks,
    after: &Blocks,
    lines: &Interface,
    said: &mut String,
) -> Result<()> {
    match (table(blocks, lines), table(after, lines)) {
        (Ok(before), Ok(now)) => {
            let first = before
                .cases
                .iter()
                .zip(&now.cases)
                .position(|(b, n)| b.1 != n.1);
            let differ = first.map(|i| (&before.cases[i], &now.cases[i]));
            match differ {
                None => writeln!(
                    said,
                    "  truth table: same in all {} cases{}",
                    now.cases.len(),
                    if now.exhaustive { "" } else { " sampled" }
                )?,
                Some((b, n)) => {
                    let wrong = before
                        .cases
                        .iter()
                        .zip(&now.cases)
                        .filter(|(b, n)| b.1 != n.1)
                        .count();
                    writeln!(
                        said,
                        "  truth table: differs in {wrong} of {} cases, first \
                         inputs {:#b}: {:?} before, {:?} after",
                        now.cases.len(),
                        b.0,
                        b.1,
                        n.1
                    )?;
                    // A repeater reads the dust feeding it, a block before
                    // where the output's dust fell to 0 from a weak signal.
                    let arrived = first.map(|i| &now.arrivals[i]);
                    writeln!(
                        said,
                        "    strengths arriving after, in that case: {:?}",
                        arrived.cloned().unwrap_or_default()
                    )?;
                }
            }
            for (port, out) in lines.outputs.iter().enumerate() {
                writeln!(
                    said,
                    "  min SS of {}: dust before {:?}; arriving after {:?}",
                    out.name, before.shown[port], now.arriving[port]
                )?;
            }
        }
        (Err(err), _) | (_, Err(err)) => {
            writeln!(said, "  truth table not compared: {err}")?;
        }
    }
    Ok(())
}
