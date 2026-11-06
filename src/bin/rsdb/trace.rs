//! A build traced as a logic analyzer, as the site's logic analyzer page
//! traces it, in text: one change of its inputs tick by tick, or the change
//! its sweep found most worth seeing, with its races, each output's bits and
//! every component that changed by tick, what set each change off, and how
//! the sweep's changes settled. Nothing here writes to the database.

use std::fmt::Write as _;

use anyhow::{Result, bail};
use app::{
    category::Kind,
    import,
    recognize::repeater_outputs,
    simulate::trace::{self as traced, Case, Part},
    web::trace::{Analysis, analyze, becomes, case_words, verb},
};
use clap::Args;
use sqlx::SqlitePool;

use super::outputs::{kind_of, source};

/// The most races the sweep lists.
const RACES_LISTED: usize = 12;

#[derive(Args)]
pub(super) struct Trace {
    /// A .schem file, or a stored build's id or the start of one
    file_or_id: String,
    /// Name its lines as this category's test does, such as adder; a stored
    /// build's own by default, else what probing finds
    #[arg(long)]
    category: Option<String>,
    /// Each input's value before the change, such as `A=3,B=5`, an
    /// underscore for a space in a name, as `carry_in=1`; any not named are
    /// 0
    #[arg(long, value_delimiter = ',')]
    from: Vec<String>,
    /// Each input's value after it, such as `B=1`; any not named keep their
    /// value. With no change named, the one a sweep finds most worth seeing
    #[arg(long, value_delimiter = ',')]
    to: Vec<String>,
    /// The tick each input switches on, such as `A=1`; any not named switch
    /// at 0
    #[arg(long, value_delimiter = ',')]
    at: Vec<String>,
}

/// Prints the trace of the change `args` names.
pub(super) async fn trace_command(db: &SqlitePool, args: &Trace) -> Result<()> {
    let source = source(db, &args.file_or_id).await?;
    let mut kind = kind_of(args.category.as_deref(), source.stored.as_ref());
    if kind.is_none() && source.stored.is_none() {
        kind = repeater_outputs::probe(&source.data, None)?
            .ok()
            .map(|probed| probed.kind);
    }
    // A stored build carries its roles; a file is given the ones its
    // category's probing finds.
    let data = match (&source.stored, kind) {
        (None, Some(kind)) => import::with_roles(&source.data, kind.name())?,
        _ => source.data.clone(),
    };
    let mut pairs = Vec::new();
    for (side, given) in
        [("from", &args.from), ("to", &args.to), ("at", &args.at)]
    {
        for pair in given {
            let Some((name, value)) = pair.split_once('=') else {
                bail!("{pair} is not NAME=VALUE");
            };
            let name = name.trim().replace('_', " ");
            pairs.push((format!("{side}.{name}"), value.trim().to_owned()));
        }
    }
    let a = tokio::task::spawn_blocking(move || analyze(&data, kind, &pairs))
        .await?
        .map_err(anyhow::Error::msg)?;
    let what = kind.map_or_else(|| "a build".to_owned(), Kind::a);
    println!("{} as {what}: {}", source.name, case_words(&a, &a.case));
    if a.at.iter().any(|&at| at != 0) {
        let each: Vec<String> = a
            .inputs
            .iter()
            .zip(&a.at)
            .map(|(line, at)| format!("{} at {at}", line.name))
            .collect();
        println!("switching {}", each.join(", "));
    }
    if let Some(world) = &a.world {
        println!();
        println!("block by block:");
        print_outputs(&a, &world.trace);
    }
    for (i, run) in a.runs.iter().enumerate() {
        println!();
        if a.runs.len() > 1 {
            println!("redpiler {}, {}", i + 1, order(&a, i));
        } else {
            println!("redpiler:");
        }
        print_races(&a, i);
        print_outputs(&a, &run.trace);
        print_waves(&a, i);
    }
    println!();
    print_timeline(&a);
    println!();
    print_sweep(&args.file_or_id, &a);
    Ok(())
}

/// Which way run `i`'s first race went.
fn order(a: &Analysis, i: usize) -> String {
    let run = &a.runs[i];
    let Some(race) = run.races.first() else {
        return "another run:".to_owned();
    };
    let part = a.graph.nodes[race.node].part;
    let went = race
        .outcomes
        .iter()
        .find(|(_, outcome)| *outcome == race.ran)
        .map(|(ahead, _)| ahead.clone())
        .unwrap_or_default();
    let first = if went.is_empty() {
        a.label(race.node)
    } else {
        names(a, &went)
    };
    let was = run.trace.strength(race.node, race.tick - 1);
    let it = if went.is_empty() {
        "it".to_owned()
    } else {
        a.label(race.node)
    };
    format!(
        "when {first} goes first, {it} {}:",
        becomes(part, was, race.ran)
    )
}

fn names(a: &Analysis, nodes: &[usize]) -> String {
    nodes
        .iter()
        .map(|&n| a.label(n))
        .collect::<Vec<_>>()
        .join(" and ")
}

fn print_races(a: &Analysis, i: usize) {
    let run = &a.runs[i];
    if run.races.is_empty() {
        println!("no races");
        return;
    }
    println!(
        "races, where the order a tick runs its updates in decides; the \
         game's comes of where the build stands and which way it faces:"
    );
    for race in &run.races {
        let part = a.graph.nodes[race.node].part;
        let was = run.trace.strength(race.node, race.tick - 1);
        let node = a.label(race.node);
        if race.pulse {
            println!(
                "  tick {}, {node}: off, and {} may power it for an instant, \
                 so it pulses {} later, or not; this run it {}",
                race.tick,
                names(a, &race.rivals),
                part.delay(),
                if race.ran > 0 { "pulsed" } else { "stayed off" }
            );
            continue;
        }
        let outcomes: Vec<String> = race
            .outcomes
            .iter()
            .map(|(ahead, outcome)| {
                let who = if ahead.is_empty() {
                    format!("the {}", part.name())
                } else {
                    names(a, ahead)
                };
                format!("{who} first, it {}", becomes(part, was, *outcome))
            })
            .collect();
        println!(
            "  tick {}, {node}, against {}: {}",
            race.tick,
            names(a, &race.rivals),
            outcomes.join("; ")
        );
    }
}

fn print_outputs(a: &Analysis, trace: &traced::Trace) {
    for (port, line) in a.outputs.iter().enumerate() {
        let mut said = format!("{}: {}", line.name, trace.output(port, -1));
        for (tick, value) in trace.output_steps(port) {
            let _ = write!(said, ", {value} on tick {tick}");
        }
        let glitched: Vec<String> = (0..line.width)
            .filter(|&bit| trace.bit_changes(port, bit).len() > 1)
            .map(|bit| line.bit_name(bit))
            .collect();
        if !glitched.is_empty() {
            let _ = write!(said, "; glitched in {}", glitched.join(", "));
        }
        println!("{said}");
    }
}

/// Each output bit that changed and each component that did, one column a
/// tick, the first before the change: `#` on, `.` off, a comparator's
/// strength as a hex digit, a race's tick marked `!` below.
fn print_waves(a: &Analysis, i: usize) {
    let run = &a.runs[i];
    let trace = &run.trace;
    let ticks = trace.ticks();
    let mut rows: Vec<(String, String)> = Vec::new();
    for (port, line) in a.outputs.iter().enumerate() {
        for bit in 0..line.width {
            if trace.bit_changes(port, bit).is_empty() {
                continue;
            }
            let wave = (-1..ticks)
                .map(|t| {
                    if trace.output(port, t) >> bit & 1 == 1 {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect();
            rows.push((line.bit_name(bit), wave));
        }
    }
    let mut changed: Vec<(i32, usize)> = (0..a.graph.nodes.len())
        .filter(|&n| a.graph.nodes[n].part.ticks())
        .filter_map(|n| {
            let raced = run.races.iter().any(|r| r.node == n);
            (0..ticks)
                .find(|&t| trace.changed(n, t))
                .or(raced.then_some(ticks))
                .map(|t| (t, n))
        })
        .collect();
    changed.sort_unstable();
    for (_, node) in changed {
        let part = a.graph.nodes[node].part;
        let wave = (-1..ticks)
            .map(|t| glyph(part, trace.strength(node, t)))
            .collect();
        let raced = run.races.iter().any(|r| r.node == node);
        let label = a.label(node) + if raced { " (race)" } else { "" };
        rows.push((label, wave));
    }
    let width = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    let header: String = std::iter::once('b')
        .chain((0..ticks).map(|t| {
            char::from_digit(u32::try_from(t % 10).unwrap_or(0), 10)
                .unwrap_or('?')
        }))
        .collect();
    println!("{:width$}  {header}", "tick");
    for (label, wave) in &rows {
        println!("{label:width$}  {wave}");
    }
    let marks: String = (-1..ticks)
        .map(|t| {
            if run.races.iter().any(|r| r.tick == t) {
                '!'
            } else {
                ' '
            }
        })
        .collect();
    if marks.contains('!') {
        println!("{:width$}  {}", "race", marks.trim_end());
    }
}

fn glyph(part: Part, strength: u8) -> char {
    match (part, strength) {
        (_, 0) => '.',
        (Part::Comparator { .. }, s) if s < 15 => {
            char::from_digit(u32::from(s), 16).unwrap_or('?')
        }
        _ => '#',
    }
}

fn print_timeline(a: &Analysis) {
    let run = &a.runs[0];
    println!(
        "what happened{}:",
        if a.runs.len() > 1 {
            ", in the first run"
        } else {
            ""
        }
    );
    for change in run.trace.changes() {
        let part = a.graph.nodes[change.node].part;
        let causes = run.trace.causes(&a.graph, &change);
        let by: Vec<String> = causes
            .iter()
            .map(|cause| {
                let cause_part = a.graph.nodes[cause.node].part;
                format!(
                    "{} {} on tick {}",
                    a.label(cause.node),
                    verb(cause_part, cause, true),
                    cause.tick
                )
            })
            .collect();
        let raced = run
            .races
            .iter()
            .any(|r| r.node == change.node && r.tick == change.tick);
        println!(
            "  {:>3}  {} {}{}{}",
            change.tick,
            a.label(change.node),
            verb(part, &change, false),
            if raced { " (race)" } else { "" },
            if by.is_empty() {
                String::new()
            } else {
                format!(", from {}", by.join(", "))
            }
        );
    }
}

fn print_sweep(file_or_id: &str, a: &Analysis) {
    let Some(sweep) = &a.sweep else {
        return;
    };
    println!(
        "across {} changes, every input switching at tick 0: each input alone \
         from all off, and {} at random",
        sweep.cases, a.random
    );
    for (port, line) in a.outputs.iter().enumerate() {
        let settling = &sweep.outputs[port];
        let by: Vec<String> = settling
            .settled
            .iter()
            .map(|(tick, count)| format!("tick {tick} in {count}"))
            .collect();
        println!("  {} settles by {}", line.name, by.join(", "));
        if let Some((tick, case)) = &settling.slowest {
            println!("    slowest, tick {tick}: {}", case_words(a, case));
        }
        for (bit, (count, case)) in &settling.glitches {
            println!(
                "    {} glitched in {count}, as {}",
                line.bit_name(*bit),
                case_words(a, case)
            );
        }
    }
    if sweep.races.is_empty() {
        println!("  no races");
        return;
    }
    println!("  races, first in a change, most often first:");
    for seen in sweep.races.iter().take(RACES_LISTED) {
        println!(
            "    {} against {}, in {} changes, first on tick {} of {}",
            a.label(seen.race.node),
            names(a, &seen.race.rivals),
            seen.count,
            seen.race.tick,
            case_words(a, &seen.case)
        );
        println!("      rsdb trace {file_or_id} {}", flags(a, &seen.case));
    }
    if sweep.races.len() > RACES_LISTED {
        println!("    and {} more", sweep.races.len() - RACES_LISTED);
    }
}

/// The flags tracing `case`.
fn flags(a: &Analysis, case: &Case) -> String {
    let each = |values: &[u128]| {
        a.inputs
            .iter()
            .zip(values)
            .map(|(line, value)| {
                format!("{}={value}", line.name.replace(' ', "_"))
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("--from {} --to {}", each(&case.from), each(&case.to))
}
