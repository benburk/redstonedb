//! Measuring the rule proposed for outputs, a repeater or comparator
//! powering nothing, see [`repeater_outputs`], before the site reads
//! builds by it: what it finds in a build beside what today's rule finds,
//! a build reworked to it by rule, and copies of a build `//rstack`ed and
//! read by it. Nothing here writes to the database.

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
        probe::{self, Layout},
        repeater_outputs::{self, Output, Reworked},
        roles,
    },
    schem::{self, Blocks, Direction, Pos, rstack, stack, standardize},
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

#[derive(Args)]
pub(super) struct StackCheck {
    /// A .schem file, or a stored build's id or the start of one
    file_or_id: String,
    /// Copies in all, the original among them
    #[arg(long, default_value_t = 3)]
    copies: usize,
    /// The ways to stack, such as `west,east,up,down`; a stored build's own
    /// and their opposites by default, else those four
    #[arg(long, value_delimiter = ',')]
    dirs: Vec<String>,
    /// Blocks between copies; a stored build's own along that axis by
    /// default, else each of 1 to 4
    #[arg(long)]
    spacing: Option<i32>,
    /// The build's category, to test the stack as; a stored build's own,
    /// else what probing finds
    #[arg(long)]
    category: Option<String>,
    /// A stored build whose category and stacking to use, for a file made
    /// from it, such as one `convert` wrote
    #[arg(long)]
    like: Option<String>,
    /// Run the one way and spacing given here, in this process, exiting 0
    /// when it passes and 3 when not, as each stack is run apart
    #[arg(long, hide = true)]
    one: bool,
}

/// The most memory one stack's run may take, in KiB: Redpiler can grow
/// without bound compiling copies stacked so close their dust runs
/// together, so each stack runs in a process of its own held to this.
const STACK_MEMORY_KIB: u64 = 6 * 1024 * 1024;

/// How a stack run apart ended.
enum Ran {
    Passed,
    Failed,
    /// The process ended otherwise, as when Redpiler ran out of memory.
    Crashed,
}

/// Runs one way and spacing of `args` in a process of its own, held to
/// `STACK_MEMORY_KIB`, printing what it prints of that stack.
fn run_apart(
    args: &StackCheck,
    kind: Option<Kind>,
    dir: &str,
    spacing: i32,
) -> Result<Ran> {
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg(format!(
            "ulimit -v {STACK_MEMORY_KIB} && exec \"$0\" \"$@\""
        ))
        .arg(exe)
        .arg("stack-check")
        .arg(&args.file_or_id)
        .args(["--one", "--dirs", dir])
        .args(["--spacing", &spacing.to_string()])
        .args(["--copies", &args.copies.to_string()]);
    if let Some(kind) = kind {
        command.args(["--category", kind.name()]);
    }
    if let Some(like) = &args.like {
        command.args(["--like", like]);
    }
    let output = command.output()?;
    let out = String::from_utf8_lossy(&output.stdout);
    let start = format!("{dir} {spacing}, ");
    let mut printing = false;
    for line in out.lines() {
        printing |= line.starts_with(&start);
        if printing {
            println!("{line}");
        }
    }
    Ok(match output.status.code() {
        Some(0) => Ran::Passed,
        Some(3) => Ran::Failed,
        _ => {
            let err = String::from_utf8_lossy(&output.stderr);
            let last = err.lines().last().unwrap_or("").to_owned();
            if !printing {
                println!("{start}{} copies:", args.copies);
            }
            println!("  CRASHED ({}): {last}", output.status);
            Ran::Crashed
        }
    })
}

/// A build to measure: its name, its file, and its row when stored.
struct Source {
    name: String,
    data: Vec<u8>,
    stored: Option<schematic::Schematic>,
}

/// The file at `file_or_id`, or the current file of the stored build it
/// names.
async fn source(db: &SqlitePool, file_or_id: &str) -> Result<Source> {
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
fn kind_of(
    category: Option<&str>,
    stored: Option<&schematic::Schematic>,
) -> Option<Kind> {
    category
        .map(str::to_owned)
        .or_else(|| stored.map(|s| s.category.clone()))
        .and_then(|name| Kind::named(&name))
}

fn place(pos: Pos) -> String {
    inspect::place(pos)
}

/// An output in words, such as `repeater at (1, 2, 3) facing west, giving
/// out east, on yellow concrete`.
fn describe(blocks: &Blocks, output: Output) -> String {
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

/// A way to stack, as a step of one block.
fn unit(name: &str) -> Result<Pos> {
    Ok(match name {
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "up" => [0, 1, 0],
        "down" => [0, -1, 0],
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        _ => bail!("no way {name}: west, east, up, down, north, or south"),
    })
}

fn opposite(name: &str) -> &'static str {
    match name {
        "west" => "east",
        "east" => "west",
        "up" => "down",
        "down" => "up",
        "north" => "south",
        _ => "north",
    }
}

/// `//rstack`s a build each way asked, settles each stack, reads it by
/// the proposed rule, and tests it as a wider build of its kind.
pub(super) async fn stack_check_command(
    db: &SqlitePool,
    args: &StackCheck,
) -> Result<()> {
    let source = source(db, &args.file_or_id).await?;
    let like = match &args.like {
        Some(id) => Some(load(db, &resolve(db, id).await?).await?),
        None => source.stored.clone(),
    };
    println!("{}", source.name);
    // Each stored way, by name, with its spacing.
    let stored: Vec<(&str, i32)> = like
        .iter()
        .flat_map(|s| &s.stacking)
        .flat_map(|d| {
            [(d.west, "west"), (d.up, "up"), (d.north, "north")]
                .into_iter()
                .filter(|&(n, _)| n > 0)
                .map(|(n, way)| (way, i32::try_from(n).unwrap_or(0)))
                .collect::<Vec<_>>()
        })
        .collect();
    let dirs: Vec<String> = if !args.dirs.is_empty() {
        args.dirs.clone()
    } else if stored.is_empty() {
        ["west", "east", "up", "down"].map(str::to_owned).to_vec()
    } else {
        stored
            .iter()
            .flat_map(|&(way, _)| [way, opposite(way)])
            .map(str::to_owned)
            .collect()
    };
    if !stored.is_empty() {
        let ways: Vec<String> =
            stored.iter().map(|(w, n)| format!("{w} {n}")).collect();
        println!("stored stacking: {}", ways.join(", "));
    }
    let meant = kind_of(args.category.as_deref(), like.as_ref());
    let single = repeater_outputs::probe(&source.data, meant)?;
    let blocks = Blocks::read(&probe::simulatable(&source.data)?)?;
    let outputs = repeater_outputs::find(&blocks);
    println!(
        "one copy: {} outputs by the proposed rule, {} levers and buttons",
        outputs.len(),
        blocks.switches().len()
    );
    let (kind, layout) = one_copy(&single, meant);
    // The chained output, a carry out, and the way the rule predicts.
    let chained = layout
        .as_ref()
        .and_then(|l| l.carry_out)
        .and_then(|block| outputs.iter().copied().find(|o| o.block() == block));
    let predicted = chained.map(|o| o.facing.name());
    match chained {
        Some(output) => println!(
            "  chained output: carry out, {}; predicted clean way: {}",
            describe(&blocks, output),
            output.facing
        ),
        None => println!("  chained output: none found, so no prediction"),
    }
    let mut passing: Vec<String> = Vec::new();
    let mut crashed: Vec<String> = Vec::new();
    for dir in &dirs {
        let unit = unit(dir)?;
        let axis_stored = stored.iter().find(|(way, _)| {
            *way == dir.as_str() || opposite(way) == dir.as_str()
        });
        let given = args.spacing.or(axis_stored.map(|&(_, n)| n));
        let spacings: Vec<i32> = given.map_or((1..=4).collect(), |n| vec![n]);
        for spacing in spacings {
            let step = unit.map(|n| n * spacing);
            let label = format!("{dir} {spacing}");
            if args.one {
                let passed = stack_once(
                    &source.data,
                    &blocks,
                    &outputs,
                    step,
                    args.copies,
                    &label,
                    kind,
                    layout.as_ref(),
                )?;
                std::process::exit(if passed { 0 } else { 3 });
            }
            match run_apart(args, kind, dir, spacing)? {
                Ran::Passed => passing.push(label),
                Ran::Failed => {}
                Ran::Crashed => crashed.push(label),
            }
        }
    }
    print_verdict(&passing, &crashed, predicted, &dirs);
    Ok(())
}

/// Prints what probing made of the one copy, `single`, and gives its kind
/// and adder layout: what probing found, or else the kind `meant` and the
/// adder's closest layout, as its fault's file colours it, so its copies
/// can still be tried.
fn one_copy(
    single: &Result<repeater_outputs::Probed, Vec<(Kind, probe::Fault)>>,
    meant: Option<Kind>,
) -> (Option<Kind>, Option<Layout>) {
    match single {
        Ok(probed) => {
            println!("  probed: {}", probed.found.2);
            if let Some(layout) = &probed.layout {
                println!("  {}", layout.describe());
            }
            (Some(probed.kind), probed.layout.clone())
        }
        Err(faults) => {
            println!("  probed: nothing holds");
            for (kind, fault) in faults.iter().take(3) {
                println!("    {}: {fault}", kind.name());
            }
            let closest = faults
                .iter()
                .find(|(kind, _)| *kind == Kind::Adder)
                .and_then(|(_, fault)| fault.file())
                .and_then(|file| coloured_layout(file).ok());
            if let Some(layout) = &closest {
                println!("  closest adder layout: {}", layout.describe());
            }
            (meant, closest)
        }
    }
}

/// Prints which stacks passed and crashed, and whether the way the facing
/// rule `predicted` is among those that passed.
fn print_verdict(
    passing: &[String],
    crashed: &[String],
    predicted: Option<&str>,
    dirs: &[String],
) {
    println!(
        "passing: {}",
        if passing.is_empty() {
            "none".to_owned()
        } else {
            passing.join(", ")
        }
    );
    if let Some(predicted) = predicted.filter(|p| !dirs.iter().any(|d| d == p))
    {
        println!(
            "predicted {predicted}: not among the ways tried, the chained \
             output pointing across them"
        );
    }
    if !crashed.is_empty() {
        println!("crashed: {}", crashed.join(", "));
    }
    if let Some(predicted) = predicted.filter(|p| dirs.iter().any(|d| d == p)) {
        let matches = passing.iter().any(|p| p.starts_with(predicted));
        let others: Vec<&String> = passing
            .iter()
            .filter(|p| !p.starts_with(predicted))
            .collect();
        println!(
            "predicted {predicted}: {}{}",
            if matches { "passes" } else { "does not pass" },
            if others.is_empty() {
                String::new()
            } else {
                format!(
                    "; also passing {}",
                    others
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
    }
}

/// The adder layout the colours of the file in `data` mark: A lime, B light
/// blue, a carry in magenta, the sum yellow, a carry out pink, each bus
/// least significant bit first as the site reads it.
fn coloured_layout(data: &[u8]) -> Result<Layout> {
    let lines = interface::read(&Blocks::read(data)?);
    let bits = |ports: &[interface::Port], colour: &str| -> Vec<Pos> {
        ports
            .iter()
            .filter(|port| port.block.starts_with(colour))
            .flat_map(|port| port.bits.iter().map(|bit| bit.pos))
            .collect()
    };
    let one = |bits: Vec<Pos>| bits.first().copied();
    Ok(Layout {
        a: bits(&lines.inputs, "lime"),
        b: bits(&lines.inputs, "light blue"),
        carry_in: one(bits(&lines.inputs, "magenta")),
        sum: bits(&lines.outputs, "yellow"),
        carry_out: one(bits(&lines.outputs, "pink")),
    })
}

/// The components that stand on a block and fall without it.
fn needs_support(state: &str) -> bool {
    let name = schem::short(state);
    matches!(
        name,
        "redstone_wire" | "repeater" | "comparator" | "redstone_torch"
    ) || ((name == "lever" || name.ends_with("_button"))
        && schem::property(state, "face") == Some("floor"))
}

/// One stack, `copies` of the build in `data` a `step` apart: prints what
/// meeting copies did and how the stack tests, and whether it passed.
#[allow(clippy::too_many_arguments)]
fn stack_once(
    data: &[u8],
    blocks: &Blocks,
    outputs: &[Output],
    step: Pos,
    copies: usize,
    label: &str,
    kind: Option<Kind>,
    layout: Option<&Layout>,
) -> Result<bool> {
    println!("{label}, {copies} copies:");
    let stacked = rstack::rstack(data, step, copies)?;
    let raw = Blocks::read(&stacked.data)?;
    print_seams(&stacked);
    print_outputs_met(&stacked, &raw, outputs);
    print_orphans(&stacked, &raw, blocks);
    let settled = match rstack::settle(&stacked.data) {
        Ok(settled) => settled,
        Err(err) => {
            println!("  doesn't settle: {err}");
            println!("  FAILS");
            return Ok(false);
        }
    };
    print_prunable(&stacked, &settled)?;
    let simulated = probe::simulatable(&settled)?;
    let after = Blocks::read(&simulated)?;
    let found = repeater_outputs::find(&after);
    let runs: Vec<String> = repeater_outputs::lines(&found)
        .iter()
        .map(|r| r.bits.len().to_string())
        .collect();
    println!(
        "  stacked: {} outputs by the proposed rule, runs {}; {} levers and \
         buttons",
        found.len(),
        runs.join(" "),
        after.switches().len()
    );
    let mut passed = false;
    if let Some(layout) = layout {
        passed |= print_by_geometry(&after, &stacked, layout, copies);
    }
    match repeater_outputs::probe(&settled, kind)? {
        Ok(probed) => {
            passed = true;
            println!("  probed: {}", probed.found.2);
        }
        Err(faults) => {
            for (kind, fault) in faults.iter().take(2) {
                println!("  probed as {}: {fault}", kind.name());
            }
        }
    }
    println!("  {}", if passed { "PASSES" } else { "FAILS" });
    Ok(passed)
}

/// Prints how many blocks each copy put over another's that differed, and
/// the first few.
fn print_seams(stacked: &rstack::Stacked) {
    let mut seams: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for o in &stacked.overwrites {
        *seams.entry((o.copy, o.over)).or_default() += 1;
    }
    let seam_words: Vec<String> = seams
        .iter()
        .map(|((copy, over), n)| format!("copy {copy} over {over}: {n}"))
        .collect();
    println!(
        "  overwrites at seams: {}{}",
        stacked.overwrites.len(),
        if seam_words.is_empty() {
            String::new()
        } else {
            format!(" ({})", seam_words.join(", "))
        }
    );
    for o in stacked.overwrites.iter().take(6) {
        println!(
            "    {} {} by copy {}'s {}",
            place(o.at),
            schem::short(&o.was).replace('_', " "),
            o.copy,
            schem::short(&o.now).replace('_', " ")
        );
    }
    if stacked.overwrites.len() > 6 {
        println!("    and {} more", stacked.overwrites.len() - 6);
    }
}

/// Prints each copy's output a later copy pasted over, or put a block in
/// front of, which it now powers, as a carry passed on does.
fn print_outputs_met(
    stacked: &rstack::Stacked,
    raw: &Blocks,
    outputs: &[Output],
) {
    let copies = stacked.copies;
    // Each copy's outputs: pasted over, or now powering something.
    for copy in 0..copies {
        for output in outputs {
            let at = stacked.place(copy, output.at);
            let now = raw.get(at);
            let by = stacked.copy_of.get(&at).copied().unwrap_or(copy);
            let same = schem::short(now) == output.kind()
                && Direction::facing(now) == Some(output.facing);
            if !same {
                println!(
                    "  copy {copy}'s output {} at {} pasted over by copy \
                     {by}'s {}",
                    output.kind(),
                    place(at),
                    schem::short(now).replace('_', " ")
                );
                continue;
            }
            let front = schem::step(at, output.points());
            let there = raw.get(front);
            if schem::short(there) != "air" {
                let by = stacked.copy_of.get(&front).copied().unwrap_or(copy);
                println!(
                    "  copy {copy}'s output {} at {} now powers copy {by}'s \
                     {} at {}",
                    output.kind(),
                    place(at),
                    schem::short(there).replace('_', " "),
                    place(front)
                );
            }
        }
    }
}

/// Prints the blocks that held up what a later copy covered with
/// something that needs no holding up.
fn print_orphans(stacked: &rstack::Stacked, raw: &Blocks, blocks: &Blocks) {
    let copies = stacked.copies;
    // Blocks that held up what a later copy covered with something that
    // needs no holding up.
    let mut orphans = Vec::new();
    for copy in 0..copies {
        for (pos, state) in blocks.iter() {
            let top = schem::step(pos, Direction::Up);
            if !needs_support(blocks.get(top)) || state == schem::AIR {
                continue;
            }
            let (at, above) =
                (stacked.place(copy, pos), stacked.place(copy, top));
            if stacked.copy_of.get(&at) != Some(&copy) {
                continue;
            }
            if !needs_support(raw.get(above)) {
                let by = stacked.copy_of.get(&above).copied().unwrap_or(copy);
                orphans.push(format!(
                    "{} {} under copy {by}'s {}",
                    place(at),
                    schem::short(state).replace('_', " "),
                    schem::short(raw.get(above)).replace('_', " ")
                ));
            }
        }
    }
    if !orphans.is_empty() {
        println!(
            "  orphaned supports: {}: {}",
            orphans.len(),
            orphans
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

/// Prints what the site's pruning would take out of the settled stack,
/// junk a copy leaves at a seam, with the copy each block came from.
fn print_prunable(stacked: &rstack::Stacked, settled: &[u8]) -> Result<()> {
    // What the site's pruning would take out of the stack: junk a copy
    // leaves at a seam.
    match roles::prune(settled, &[], stack::Buses::own()) {
        Ok(None) => println!("  prunable: 0"),
        Ok(Some(pruned)) => {
            let raw_settled = Blocks::read(settled)?;
            let gone: Vec<String> = pruned
                .gone
                .iter()
                .map(|&pos| {
                    format!(
                        "{} {} from copy {}",
                        place(pos),
                        schem::short(raw_settled.get(pos)).replace('_', " "),
                        stacked.copy_of.get(&pos).unwrap_or(&0)
                    )
                })
                .collect();
            println!("  prunable: {}: {}", gone.len(), gone.join("; "));
        }
        Err(err) => println!("  prunable: not pruned, {err}"),
    }
    Ok(())
}

/// Prints how the stack in `after` tests as an adder whose every copy
/// plays the roles `layout` gives the one, and the levers no role uses;
/// whether it passed.
fn print_by_geometry(
    after: &Blocks,
    stacked: &rstack::Stacked,
    layout: &Layout,
    copies: usize,
) -> bool {
    let mut passed = false;
    match by_geometry(after, stacked, layout, copies) {
        Ok((order, run, used)) => {
            // Levers the stack's roles leave out, as a later copy's
            // carry in left inside the stack.
            let left: Vec<String> = after
                .switches()
                .iter()
                .filter(|switch| !used.contains(&switch.block))
                .map(|switch| {
                    format!(
                        "{} on {} {} from copy {}",
                        schem::short(after.get(switch.pos)).replace('_', " "),
                        schem::short(after.get(switch.block)).replace('_', " "),
                        place(switch.block),
                        stacked.copy_of.get(&switch.pos).unwrap_or(&0)
                    )
                })
                .collect();
            println!(
                "  levers no role uses: {}{}",
                left.len(),
                if left.is_empty() {
                    String::new()
                } else {
                    format!(": {}", left.join("; "))
                }
            );
            let right = run.wrong == 0;
            passed |= right;
            println!(
                "  as a {}-bit adder by the one copy's roles, copy {order} \
                     lowest: {}",
                layout.a.len() * copies,
                if right {
                    format!("passes, all {} cases right", run.cases)
                } else {
                    run.wrong_cases()
                }
            );
        }
        Err(why) => println!("  by the one copy's roles: {why}"),
    }
    passed
}

/// The stack tested as an adder whose every copy plays the roles the one
/// copy's `layout` gives it, its bits one copy after another, the carry in
/// the lowest copy's and the carry out the highest's, with either end
/// lowest: the end that does better and its run, or why neither can run.
fn by_geometry(
    blocks: &Blocks,
    stacked: &rstack::Stacked,
    layout: &Layout,
    copies: usize,
) -> Result<(usize, simulate::Run, Vec<Pos>), String> {
    let switched: HashSet<Pos> =
        blocks.switches().iter().map(|s| s.block).collect();
    let mut best: Option<(usize, simulate::Run, Vec<Pos>)> = None;
    let mut why = String::new();
    for lowest_first in [true, false] {
        let order: Vec<usize> = if lowest_first {
            (0..copies).collect()
        } else {
            (0..copies).rev().collect()
        };
        let each = |bits: &[Pos]| -> Vec<Pos> {
            order
                .iter()
                .flat_map(|&c| bits.iter().map(move |&p| stacked.place(c, p)))
                .collect()
        };
        let wide = Layout {
            a: each(&layout.a),
            b: each(&layout.b),
            carry_in: layout.carry_in.map(|p| stacked.place(order[0], p)),
            sum: each(&layout.sum),
            carry_out: layout
                .carry_out
                .map(|p| stacked.place(order[copies - 1], p)),
        };
        let inputs = wide.a.iter().chain(&wide.b).chain(&wide.carry_in);
        if let Some(missing) = inputs.clone().find(|p| !switched.contains(*p)) {
            why = format!("no lever left on input block {}", place(*missing));
            continue;
        }
        let read = |p: &Pos| {
            matches!(
                schem::short(blocks.get(schem::step(*p, Direction::Up))),
                "repeater" | "comparator" | "redstone_wire"
            )
        };
        let outs = wide.sum.iter().chain(&wide.carry_out);
        if let Some(missing) = outs.clone().find(|p| !read(p)) {
            why = format!("nothing reads out output block {}", place(*missing));
            continue;
        }
        match Test::Adder.check_with(blocks, wide.interface(blocks), false) {
            Ok(run) => {
                let better =
                    best.as_ref().is_none_or(|(_, b, _)| run.wrong < b.wrong);
                if better {
                    let used = inputs.copied().collect();
                    best = Some((order[0], run, used));
                }
            }
            Err(err) => why = err.to_string(),
        }
    }
    best.ok_or(why)
}
