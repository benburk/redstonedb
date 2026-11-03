//! What the rule for outputs, a repeater or comparator powering nothing,
//! see [`repeater_outputs`], finds in a build, beside what the old rule,
//! which read them by their colours, found, see [`legacy`]; `convert`
//! ports a build saved under the old rule to it, and `stack_check` stacks
//! copies of one. Nothing here writes to the database.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, bail};
use app::{
    category::Kind,
    models::{schematic, version},
    recognize::{
        inspect::{self, Run},
        interface::{self, Interface},
        legacy, probe,
        repeater_outputs::{self, Output},
    },
    schem::{self, Blocks, Direction, Pos},
    simulate::{self, Circuit},
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
                    place(*block)
                )
            })
            .collect();
        println!("  {}: {}", run_words(run), bits.join("; "));
    }
    print_strength_inputs(&blocks);
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
    if !repeater_outputs::strength_inputs(&blocks).is_empty() {
        // The roles of a build with hex digits are the shapes', as the hex
        // adder's probe reads them.
        let test = meant.and_then(Kind::test);
        let Some(test) = test else {
            println!("tested with these lines: no test for its category");
            return Ok(());
        };
        let simulated = Blocks::read(&probe::simulatable(&source.data)?)?;
        let lines = repeater_outputs::interface(&simulated);
        println!("tested with these lines: {}", lines_words(&lines));
        println!("  {}", verdict(&test.run_with(&simulated, lines)));
        return Ok(());
    }
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

/// Prints the signal-strength inputs the proposed rule finds in `blocks`,
/// comparators with air behind, in buses, and those today's rule reads by
/// their colour, input blocks with dust on top and no lever; and each
/// comparator of the first that drives none of the second, by the dust
/// it points into on one, which a build ported from today's rule keeps.
pub(super) fn print_strength_inputs(blocks: &Blocks) {
    let found = repeater_outputs::strength_inputs(blocks);
    let today = legacy::dust_inputs(blocks);
    println!(
        "signal-strength inputs by the proposed rule: {}; by today's \
         colours: {}",
        found.len(),
        today.len()
    );
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
                    place(input.at),
                    input.facing,
                    schem::short(blocks.get(input.front())).replace('_', " "),
                    place(input.front())
                )
            })
            .collect();
        println!("  {}: {}", run_words(&run), bits.join("; "));
    }
    let driving: BTreeSet<Pos> = today
        .keys()
        .filter_map(|&pad| repeater_outputs::driving(blocks, pad))
        .map(|input| input.at)
        .collect();
    let others: Vec<String> = found
        .iter()
        .filter(|input| !driving.contains(&input.at))
        .map(|input| place(input.at))
        .collect();
    if !today.is_empty() || !others.is_empty() {
        println!(
            "  driving today's inputs: {}; not: {}{}",
            driving.len(),
            others.len(),
            if others.is_empty() {
                String::new()
            } else {
                format!(" ({})", others.join(", "))
            }
        );
    }
}

/// Each line of `lines` in words, such as `inputs A 16 digits, carry in;
/// outputs output 8 comparators`.
fn lines_words(lines: &Interface) -> String {
    let each = |ports: &[interface::Port]| {
        ports
            .iter()
            .map(|port| {
                let unit = if port.signal_strength {
                    "digits"
                } else {
                    "bits"
                };
                format!("{} {} {unit}", port.name, port.bits.len())
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "inputs {}; outputs {}",
        each(&lines.inputs),
        each(&lines.outputs)
    )
}

/// Prints the outputs today's rule finds in `blocks`, read in the site's
/// colours and as an upload, and where they differ from `proposed`, the
/// blocks under the proposed rule's outputs.
fn print_today(blocks: &Blocks, proposed: &BTreeSet<Pos>) {
    for (label, marks) in [
        ("site colours", legacy::SITE),
        ("as an upload", legacy::UPLOAD),
    ] {
        let today = legacy::inspect(blocks, marks);
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
pub(super) struct Table {
    pub(super) cases: Vec<(u64, Vec<simulate::Value>)>,
    /// Each case's strength arriving at each output bit's repeater or
    /// comparator, the outputs' bits one after another.
    pub(super) arrivals: Vec<Vec<u8>>,
    pub(super) arriving: Vec<Vec<u8>>,
    pub(super) shown: Vec<Vec<u8>>,
    pub(super) exhaustive: bool,
}

/// The most input bits every combination of which is run; more are
/// sampled.
const EXHAUSTIVE_BITS: usize = 16;
const SAMPLES: usize = 4096;

/// Runs `blocks` through `interface` case by case, every combination of
/// its inputs, a signal-strength input's four bits a digit, or a fixed
/// sample of them when there are many. A build with a button isn't run
/// so.
pub(super) fn table(blocks: &Blocks, interface: &Interface) -> Result<Table> {
    if interface.inputs.is_empty() || interface.outputs.is_empty() {
        bail!("no inputs or no outputs to run");
    }
    let mut circuit = Circuit::new(blocks, interface)?;
    let widths: Vec<usize> = interface
        .inputs
        .iter()
        .map(|port| port.bits.len() * if port.signal_strength { 4 } else { 1 })
        .collect();
    for (port, input) in interface.inputs.iter().enumerate() {
        if !input.signal_strength
            && (0..input.bits.len()).any(|bit| circuit.is_button(port, bit))
        {
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
            circuit.set(port, (value >> shift & mask).into());
            shift += width;
        }
        circuit.settle()?;
        let got: Vec<simulate::Value> = (0..interface.outputs.len())
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
pub(super) fn verdict(report: &simulate::Report) -> String {
    match report {
        simulate::Report::Ran(run) if run.wrong == 0 => {
            format!("passes, all {} cases right, {}", run.cases, run.pattern)
        }
        simulate::Report::Ran(run) => run.wrong_cases(),
        simulate::Report::Broken { reason } => format!("broken: {reason}"),
        simulate::Report::Skipped { reason } => format!("skipped: {reason}"),
    }
}
