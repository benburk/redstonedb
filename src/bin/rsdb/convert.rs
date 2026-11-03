//! Porting builds saved under the old rule, which read inputs and outputs
//! by their colours, see [`legacy`], to the rule the site reads them by,
//! see [`repeater_outputs`], as the owner ports them by hand: each output
//! the old rule reads with its driver promoted where it can be,
//! [`repeater_outputs::promotable`], keeping the build its size, else
//! tapped, [`repeater_outputs::tap`], each signal-strength input it reads
//! by its colour fed, [`repeater_outputs::feed`], each sign naming an
//! output moved under its front, [`repeater_outputs::sign`], each
//! comparator with air behind taken out of a build that takes no
//! signal-strength inputs, as it drives nothing yet the proposed rule would
//! read it as one, and the port checked against the original by its
//! category's test, also through the lines the proposed rule reads for a
//! build with signal-strength inputs, its truth table, and, for a build
//! that stacks, the stacks it makes, and measured; a promoted port that
//! fails a check, or measures larger than tapping makes it, is tapped
//! instead. One build at a time, or every stored build but those left
//! unported, such as one the owner dropped, each in a process of its own,
//! writing the ports that pass. The original, and the port with its taps
//! coloured as its outputs were, are read and tested by the old rule,
//! [`legacy::reading`]; this is the one place the site still reads it.
//! Nothing here writes to the database.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Write as _,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use app::{
    category::Kind,
    import,
    models::{schematic, simulation},
    recognize::{
        inspect,
        interface::{self, Interface},
        legacy, probe,
        repeater_outputs::{self, Fed, Output, Signed, StrengthInput, Tapped},
    },
    schem::{self, Blocks, Direction, Pos, standardize},
    simulate::{self, Test},
};
use clap::Args;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::{
    outputs::{place, source, table, verdict},
    stack_check::stored_ways,
};

#[derive(Args)]
pub(super) struct Convert {
    /// A .schem file, or a stored build's id or the start of one; with
    /// `--all`, none
    #[arg(required_unless_present = "all")]
    file_or_id: Option<String>,
    /// Where to write the port, made when every output was tapped, whatever
    /// the checks found; nothing is written without it
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// The build's category, for its test; a stored build's own by default
    #[arg(long)]
    category: Option<String>,
    /// Read the old rule's outputs as an upload's, any lone block read on
    /// top, rather than in the site's output colours
    #[arg(long)]
    upload: bool,
    /// Port every stored build, each in a process of its own, and say
    /// which need a look and why
    #[arg(long, conflicts_with_all = ["file_or_id", "out"])]
    all: bool,
    /// With `--all`, where to write each port that passed every check, as
    /// `<category>_<id8>.schem`
    #[arg(long, requires = "all")]
    out_dir: Option<PathBuf>,
    /// With `--all`, a folder of ports made by hand, named as `--out-dir`
    /// names them, maybe after a prefix such as `NEEDS-FIX_`: their builds
    /// are still ported, to compare, but listed apart and not written
    #[arg(long, requires = "all")]
    hand_ported: Option<PathBuf>,
    /// With `--all`, builds left unported, by their id or its start, such
    /// as one the owner dropped; any port in the hand ports' `dropped`
    /// folder is left so too
    #[arg(long, value_delimiter = ',', requires = "all")]
    skip: Vec<String>,
    /// Where to write the port with its roles kept in the site's colours,
    /// as the site colours a build it recognizes: each tap's support as its
    /// output block was, and each fed comparator's support as its input
    /// block was, which turns white; for a test fixture standing for a file
    /// the site keeps
    #[arg(long, conflicts_with = "all")]
    coloured_out: Option<PathBuf>,
    /// Sides no tap goes toward, as the file's copies stack that way, for
    /// a file not stored, whose stacking says so otherwise, such as
    /// `north,south`
    #[arg(long, value_delimiter = ',', conflicts_with = "all")]
    avoid: Vec<String>,
    /// With `--all`, how many builds to port at once
    #[arg(long, default_value_t = 4, requires = "all")]
    jobs: usize,
    /// End with what came of the build as one line of JSON, for `--all`
    #[arg(long, hide = true)]
    outcome: bool,
}

/// The most memory porting one build may take, in KiB, as stacking it may
/// grow without bound in Redpiler.
const PORT_MEMORY_KIB: u64 = 6 * 1024 * 1024;

/// The longest porting one build may take.
const PORT_TIME: Duration = Duration::from_mins(30);

/// What came of porting a build, in a few words and lines.
#[derive(Serialize, Deserialize, Clone)]
struct Outcome {
    class: Class,
    /// Why it needs a look or can't be tested, short enough to group by.
    reason: String,
    /// The outputs that failed and why, a first case that differs, and the
    /// like.
    details: Vec<String>,
    /// Each output line's weakest strength arriving at its taps while on,
    /// least significant bit first, such as `output: 14 14 1`, 0 for a bit
    /// never on.
    #[serde(default)]
    min_ss: Vec<String>,
    /// How the original and the port stacked, for a build that stacks.
    #[serde(default)]
    stacked: Option<String>,
    /// Each comparator with air behind taken out of a build that takes no
    /// signal-strength inputs, such as `(4, 17, 11) into redstone wire`.
    #[serde(default)]
    removed: Vec<String>,
    /// How many outputs had their driver promoted, and how many were
    /// tapped.
    #[serde(default)]
    promoted: usize,
    #[serde(default)]
    tapped: usize,
    /// Why the port promoting drivers failed a check, when it did and the
    /// outputs were tapped instead.
    #[serde(default)]
    fell_back: Option<String>,
    /// The port's size as the site measures it, and the stored build's.
    #[serde(default)]
    measured: Option<[i64; 3]>,
    #[serde(default)]
    stored_size: Option<[i64; 3]>,
}

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
enum Class {
    /// Every output tapped and every check passed.
    Ported,
    /// Its outputs are already repeaters or comparators powering nothing.
    Already,
    /// Ported in part, or ported but some check differs from the original.
    Review,
    /// Nothing ran to check it by.
    Untestable,
}

impl Outcome {
    fn new(class: Class, reason: impl Into<String>) -> Self {
        Outcome {
            class,
            reason: reason.into(),
            details: Vec::new(),
            min_ss: Vec::new(),
            stacked: None,
            removed: Vec::new(),
            promoted: 0,
            tapped: 0,
            fell_back: None,
            measured: None,
            stored_size: None,
        }
    }
}

/// What `convert` made of a build: its port, when it was made, what it
/// found, in lines, and what came of it.
struct Conversion {
    file: Option<Vec<u8>>,
    /// The port with its roles kept in the site's colours, see
    /// [`keep_roles`], when it was made.
    coloured: Option<Vec<u8>>,
    said: String,
    outcome: Outcome,
}

/// Ports a build's outputs, as today's rule reads them, to the proposed
/// rule, says what came of each, and checks the port computes and stacks
/// as the original does; or with `--all`, every stored build.
pub(super) async fn convert_command(
    db: &SqlitePool,
    args: &Convert,
) -> Result<()> {
    if args.all {
        return convert_all(db, args).await;
    }
    let file_or_id = args.file_or_id.as_deref().context("name a build")?;
    let source = source(db, file_or_id).await?;
    println!("{}", source.name);
    let avoid = args
        .avoid
        .iter()
        .map(|side| {
            Direction::SIDES
                .into_iter()
                .find(|d| d.name() == side)
                .with_context(|| format!("no side {side}"))
        })
        .collect::<Result<Vec<Direction>>>()?;
    let conversion = convert(
        &source.data,
        (args.upload, &avoid),
        args.category
            .as_deref()
            .or(source.stored.as_ref().map(|s| s.category.as_str())),
        source.stored.as_ref(),
    )?;
    print!("{}", conversion.said);
    if let (Some(out), Some(file)) = (&args.coloured_out, &conversion.coloured)
    {
        fs::write(out, file)?;
        println!("wrote {}", out.display());
    }
    if let (Some(out), Some(file)) = (&args.out, &conversion.file) {
        fs::write(out, file)?;
        println!("wrote {}", out.display());
    }
    if args.outcome {
        println!("{}", serde_json::to_string(&conversion.outcome)?);
    }
    Ok(())
}

/// The build in `data`, saved under the old rule, ported as
/// [`port_legacy`] ports it, every read of its lines by the old rule.
fn convert(
    data: &[u8],
    (upload, avoid): (bool, &[Direction]),
    category: Option<&str>,
    stored: Option<&schematic::Schematic>,
) -> Result<Conversion> {
    legacy::reading(|| port_legacy(data, (upload, avoid), category, stored))
}

/// The build in `data`, saved under the old rule, ported to the proposed
/// rule as [`port_with`] ports it, first promoting the driver of each
/// output it can, see [`repeater_outputs::promotable`], which keeps the
/// build its size. Where that port fails a check, or runs none, the build
/// is ported again tapping every output instead, as promoting takes blocks
/// out, which only the checks say changes nothing, and that port is kept,
/// saying why; so too where the promoted port measures larger than
/// `stored` and the tapped one passes every check and measures smaller.
fn port_legacy(
    data: &[u8],
    (upload, avoid): (bool, &[Direction]),
    category: Option<&str>,
    stored: Option<&schematic::Schematic>,
) -> Result<Conversion> {
    let promoted = port_with(data, (upload, avoid, true), category, stored)?;
    // A port that doesn't settle says nothing of what it promoted.
    let unsettled = promoted.file.is_none()
        && promoted.outcome.reason == "the port doesn't settle";
    let volume = |size: Option<[i64; 3]>| size.map(|[x, y, z]| x * y * z);
    let measured = volume(promoted.outcome.measured);
    let grew = measured > volume(promoted.outcome.stored_size)
        && promoted.outcome.stored_size.is_some();
    let passed = promoted.outcome.class == Class::Ported;
    if (passed && !grew) || (promoted.outcome.promoted == 0 && !unsettled) {
        return Ok(promoted);
    }
    let mut tapped = port_with(data, (upload, avoid, false), category, stored)?;
    // A promoted port that passes is kept unless tapping makes it smaller.
    let better = !passed
        || (tapped.outcome.class == Class::Ported
            && volume(tapped.outcome.measured) < measured);
    if !better {
        return Ok(promoted);
    }
    let why = if passed {
        let [x, y, z] = promoted.outcome.measured.unwrap_or_default();
        format!("it measures {x} by {y} by {z}, larger than tapped")
    } else if promoted.outcome.reason.is_empty() {
        format!("{:?}", promoted.outcome.class).to_lowercase()
    } else {
        promoted.outcome.reason.clone()
    };
    let mut said = format!("  promoting drivers: {why}; tapped instead\n");
    for line in promoted.outcome.details.iter().take(3) {
        writeln!(said, "    {line}")?;
    }
    tapped.said = said + &tapped.said;
    tapped.outcome.fell_back = Some(why);
    Ok(tapped)
}

/// The build in `data` ported to the proposed rule by promoting its
/// outputs' drivers, with `promote`, where it can, and tapping the rest,
/// read as an upload's with `upload`, else in the site's colours, feeding
/// its signal-strength inputs, read by their colour, moving each output's
/// sign under its front, and taking out each comparator with air behind
/// where its category takes none, see [`dead_comparators`], then checked
/// against the original: by its category's test, as `category` gives it,
/// and `stored`'s width; case by case through one interface; read back by
/// the proposed rule; and, when `stored` stacks, stacked each way it does.
fn port_with(
    data: &[u8],
    (upload, avoid, promote): (bool, &[Direction], bool),
    category: Option<&str>,
    stored: Option<&schematic::Schematic>,
) -> Result<Conversion> {
    let mut said = String::new();
    let simulated = probe::simulatable(data)?;
    let blocks = Blocks::read(&simulated)?;
    let marks = if upload { legacy::UPLOAD } else { legacy::SITE };
    let today = legacy::inspect(&blocks, marks);
    if today.outputs.is_empty() {
        return no_outputs(data, &blocks, said);
    }
    let lines = interface::read(&blocks);
    let width = match (category, stored.and_then(|s| s.width)) {
        (Some(category), None) => {
            simulation::derived_width(category, true, data)?
        }
        (_, width) => width,
    };
    let test = category.and_then(|category| Test::of(category, width));
    let before = test.map(|test| test.run(&blocks));
    let digits = digit_outputs(&lines, before.as_ref());
    let mut along = stacking_sides(stored);
    along.extend(avoid);
    let pads = digit_pads(&blocks);
    let original = Original {
        blocks: &blocks,
        lines: table_lines(data, lines, category)?,
        category,
        width,
        test,
        before: before.as_ref(),
    };
    let dead = dead_comparators(&original, data, &today.outputs, marks, &pads);
    let ported = repeater_outputs::tapped(
        &dead.cleared(data)?,
        (&today.outputs, &pads),
        |pos| digits.contains(&pos),
        (&along, promote),
    );
    let ported = match ported {
        Ok(ported) => ported,
        Err(err) => return not_settled(&err, said),
    };
    // What a port's checks don't see: a sign left off its output's front,
    // or a comparator kept that changes the build.
    let kept = joined(
        tell_signed(&blocks, &ported.signed, &mut said)?,
        tell_dead(&blocks, &dead, &mut said)?,
    );
    let outcome = joined(
        joined(
            tell_tapped(&blocks, &ported.tapped, &mut said)?,
            tell_fed(&blocks, &ported.fed, &mut said)?,
        ),
        kept.clone(),
    );
    let removed: Vec<String> = dead
        .removed
        .iter()
        .map(|input| into(&blocks, input))
        .collect();
    let (moved, done) = (ported.moved, &ported.tapped);
    let Some(file) = ported.file else {
        let mut outcome = Outcome { removed, ..outcome };
        count(&mut outcome, &ported.tapped);
        return Ok(Conversion {
            file: None,
            coloured: None,
            said,
            outcome,
        });
    };
    if moved != [0; 3] {
        let moved = place(moved);
        writeln!(said, "  the box grew, every old position moved by {moved}")?;
    }
    let port = Port::new(file, moved, done, &ported.fed)?;
    let coloured = keep_roles(&port.file, &blocks, done, &ported.fed, moved)?;
    let mut outcome = check(&original, &port, done, &mut said)?;
    if kept.class == Class::Review {
        let min_ss = std::mem::take(&mut outcome.min_ss);
        outcome = joined(outcome, kept);
        outcome.min_ss = min_ss;
    }
    outcome.removed = removed;
    count(&mut outcome, done);
    measure(&port.file, stored, &mut said, &mut outcome)?;
    if outcome.class == Class::Ported
        && let Some(stored) = stored
    {
        let ways = stored_ways(Some(stored));
        if !ways.is_empty() {
            stack_both(stored, &port.file, &ways, &mut said, &mut outcome)?;
        }
    }
    Ok(Conversion {
        file: Some(port.file),
        coloured: Some(coloured),
        said,
        outcome,
    })
}

/// Writes to `said` the size the site measures the port in `file` by, and
/// `stored`'s, which `outcome` keeps.
fn measure(
    file: &[u8],
    stored: Option<&schematic::Schematic>,
    said: &mut String,
    outcome: &mut Outcome,
) -> Result<()> {
    let size = import::measure(&Blocks::read(file)?, true)?;
    let measured = [size.x, size.y, size.z].map(i64::from);
    let stored_size = stored.map(|s| [s.size.0, s.size.1, s.size.2]);
    let words = |[x, y, z]: [i64; 3]| format!("{x} by {y} by {z}");
    writeln!(
        said,
        "  measures {}{}",
        words(measured),
        match stored_size {
            Some(size) if size == measured => ", as stored".to_owned(),
            Some(size) => format!(", stored {}", words(size)),
            None => String::new(),
        }
    )?;
    outcome.measured = Some(measured);
    outcome.stored_size = stored_size;
    Ok(())
}

/// `outcome` with how many of `done`'s outputs had their driver promoted
/// and how many were tapped.
fn count(outcome: &mut Outcome, done: &[Tapped]) {
    outcome.promoted = done
        .iter()
        .filter(|t| matches!(t, Tapped::Promoted { .. }))
        .count();
    outcome.tapped = done
        .iter()
        .filter(|t| matches!(t, Tapped::Tapped { .. }))
        .count();
}

/// What came of a build whose port doesn't settle, for `err`, after what
/// `said` says so far: settling, as `//update` does, can fail where the
/// simulator can't run the build, as a piston's or a clock's.
fn not_settled(err: &anyhow::Error, mut said: String) -> Result<Conversion> {
    writeln!(said, "  not settled: {err}")?;
    let mut outcome = Outcome::new(Class::Review, "the port doesn't settle");
    outcome.details.push(format!("settling: {err}"));
    Ok(Conversion {
        file: None,
        coloured: None,
        said,
        outcome,
    })
}

/// What came of a build in `data`, of `blocks`, with no outputs by
/// today's rule, after what `said` says so far: nothing to tap, but each
/// sign naming an output by the proposed rule moved under its front, as
/// [`repeater_outputs::signed`] moves them, that file its port.
fn no_outputs(
    data: &[u8],
    blocks: &Blocks,
    mut said: String,
) -> Result<Conversion> {
    let found = repeater_outputs::find(blocks);
    writeln!(
        said,
        "  no outputs by today's rule; {} by the proposed rule",
        found.len()
    )?;
    if found.is_empty() {
        let outcome =
            Outcome::new(Class::Untestable, "no outputs by either rule");
        return Ok(Conversion {
            file: None,
            coloured: None,
            said,
            outcome,
        });
    }
    let (file, signed) = repeater_outputs::signed(data)?;
    let outcome = joined(
        Outcome::new(Class::Already, "outputs by the proposed rule only"),
        tell_signed(blocks, &signed, &mut said)?,
    );
    Ok(Conversion {
        coloured: file.clone(),
        file,
        said,
        outcome,
    })
}

/// Writes to `said` what came of each sign naming an output; what came of
/// them: ported, when one moved under its output's front and none was
/// left, already, when none needed moving, or to review, naming each left
/// where it was.
fn tell_signed(
    blocks: &Blocks,
    signed: &[Signed],
    said: &mut String,
) -> Result<Outcome> {
    let text = |sign: &Pos| {
        blocks
            .signs
            .get(sign)
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default()
    };
    let mut not = Vec::new();
    for each in signed {
        match each {
            Signed::Moved { block, sign, to } => writeln!(
                said,
                "  sign moved: \"{}\" on {} from {} to {}, under its front",
                text(sign),
                place(*block),
                place(*sign),
                place(*to)
            )?,
            Signed::Not { block, sign, why } => {
                let line = format!(
                    "sign \"{}\" on {} at {}: {why}",
                    text(sign),
                    place(*block),
                    place(*sign)
                );
                writeln!(said, "  not moved: {line}")?;
                not.push(line);
            }
        }
    }
    Ok(if !not.is_empty() {
        let mut outcome = Outcome::new(
            Class::Review,
            "a sign can't go under its output's front",
        );
        outcome.details = not;
        outcome
    } else if signed.is_empty() {
        Outcome::new(Class::Already, "no signs to move")
    } else {
        Outcome::new(Class::Ported, "")
    })
}

/// The signal-strength inputs today's rule reads in `blocks` by their
/// colour, input blocks with dust on top, each with the side its dust is
/// open to, in order.
fn digit_pads(blocks: &Blocks) -> Vec<(Pos, Option<Direction>)> {
    let mut pads: Vec<(Pos, Option<Direction>)> =
        legacy::dust_inputs(blocks).into_iter().collect();
    pads.sort_unstable_by_key(|&(pad, _)| pad);
    pads
}

/// The lines a build's truth table is run through: `lines`, as its
/// colours mark them, or else, missing inputs or outputs, as probing the
/// build in `data` as `category` finds them.
fn table_lines(
    data: &[u8],
    lines: Interface,
    category: Option<&str>,
) -> Result<Interface> {
    if (lines.inputs.is_empty() || lines.outputs.is_empty())
        && let Ok(found) = probe::recognize_as(data, category)
        && found.found.is_some()
    {
        return Ok(interface::read(&Blocks::read(&found.file)?));
    }
    Ok(lines)
}

/// What came of the comparators with air behind of a build whose category
/// takes no signal-strength inputs: dead, as nothing behind them drives
/// them, yet the proposed rule would read each as an input.
#[derive(Default)]
struct Dead {
    /// Each taken out, as taking it out alone changed nothing.
    removed: Vec<StrengthInput>,
    /// Each kept, with what taking it out alone changed.
    kept: Vec<(StrengthInput, String)>,
}

impl Dead {
    /// The file in `data` without the comparators taken out.
    fn cleared(&self, data: &[u8]) -> Result<Vec<u8>> {
        if self.removed.is_empty() {
            return Ok(data.to_vec());
        }
        let air = self
            .removed
            .iter()
            .map(|input| (input.at, "minecraft:air".to_owned()))
            .collect();
        standardize::edit(data, &air)
    }
}

/// The comparators with air behind in `original`, when its category takes
/// no signal-strength inputs, each taken out but where
/// taking it out alone, the build then settled as a port is, changes what
/// the original does: the outputs today's rule reads, by `marks`, its
/// `outputs`; the outputs the proposed rule reads, as a repeater or
/// comparator that gave out into it then gives out into air; its test's
/// result; or its truth table, where it runs. One
/// that drives a signal-strength input of `pads`, read by its colour, is
/// left as it is, as feeding takes it for that input's.
fn dead_comparators(
    original: &Original,
    data: &[u8],
    outputs: &[inspect::Run],
    marks: legacy::Marks,
    pads: &[(Pos, Option<Direction>)],
) -> Dead {
    let mut dead = Dead::default();
    // A category off the list may take them, for all anyone knows.
    if original
        .category
        .and_then(Kind::named)
        .is_none_or(Kind::digit_inputs)
    {
        return dead;
    }
    let blocks = original.blocks;
    let driving: HashSet<Pos> = pads
        .iter()
        .filter_map(|&(pad, _)| repeater_outputs::driving(blocks, pad))
        .map(|input| input.at)
        .collect();
    let found: Vec<StrengthInput> = repeater_outputs::strength_inputs(blocks)
        .into_iter()
        .filter(|input| !driving.contains(&input.at))
        .collect();
    if found.is_empty() {
        return dead;
    }
    let bits = |outputs: &[inspect::Run]| -> HashSet<Pos> {
        outputs.iter().flat_map(|run| run.bits.clone()).collect()
    };
    let read = bits(outputs);
    let proposed = |blocks: &Blocks| -> HashSet<Pos> {
        repeater_outputs::find(blocks)
            .iter()
            .map(|o| o.at)
            .collect()
    };
    let ruled = proposed(blocks);
    let table_before = table(blocks, &original.lines).ok();
    for input in found {
        let air = BTreeMap::from([(input.at, "minecraft:air".to_owned())]);
        let after = standardize::edit(data, &air)
            .and_then(|file| schem::rstack::settle(&file))
            .and_then(|file| Blocks::read(&probe::simulatable(&file)?));
        let after = match after {
            Ok(after) => after,
            Err(err) => {
                dead.kept
                    .push((input, format!("not settled without it: {err}")));
                continue;
            }
        };
        let now = bits(&legacy::inspect(&after, marks).outputs);
        let new: Vec<String> = repeater_outputs::find(&after)
            .into_iter()
            .filter(|output| !ruled.contains(&output.at))
            .map(|output| {
                format!(
                    "{} at {} giving out {}",
                    output.kind(),
                    place(output.at),
                    output.points()
                )
            })
            .collect();
        let changed = if now != read {
            Some(format!(
                "today's rule reads {} outputs without it, {} with",
                now.len(),
                read.len()
            ))
        } else if !new.is_empty() {
            Some(format!(
                "the proposed rule reads an output without it that it gave \
                 out into: {}",
                new.join(", ")
            ))
        } else if let (Some(test), Some(before)) =
            (original.test, original.before)
            && let (was, then) = (verdict(before), verdict(&test.run(&after)))
            && was != then
        {
            Some(format!("its test {was} with it, {then} without"))
        } else if let Some(before) = &table_before
            && let Ok(then) = table(&after, &original.lines)
            && let Some(i) = (0..before.cases.len())
                .find(|&i| before.cases[i] != then.cases[i])
        {
            Some(format!(
                "its truth table differs, first inputs {:#b}: outputs {:?} \
                 with it, {:?} without",
                before.cases[i].0, before.cases[i].1, then.cases[i].1
            ))
        } else {
            None
        };
        match changed {
            Some(why) => dead.kept.push((input, why)),
            None => dead.removed.push(input),
        }
    }
    dead
}

/// Writes to `said` what came of each comparator with air behind, `dead`;
/// what came of them: to review, naming each kept and why, else already.
fn tell_dead(
    blocks: &Blocks,
    dead: &Dead,
    said: &mut String,
) -> Result<Outcome> {
    let into = |input: &StrengthInput| into(blocks, input);
    for input in &dead.removed {
        writeln!(
            said,
            "  removed: comparator with air behind at {}, facing {}",
            into(input),
            input.facing
        )?;
    }
    for (input, why) in &dead.kept {
        writeln!(
            said,
            "  kept: comparator with air behind at {}: {why}",
            into(input)
        )?;
    }
    if dead.removed.is_empty() && dead.kept.is_empty() {
        return Ok(Outcome::new(Class::Already, ""));
    }
    writeln!(
        said,
        "  {} comparators with air behind, in a build that takes no \
         signal-strength inputs: {} removed, {} kept",
        dead.removed.len() + dead.kept.len(),
        dead.removed.len(),
        dead.kept.len()
    )?;
    Ok(if dead.kept.is_empty() {
        Outcome::new(Class::Already, "")
    } else {
        let mut outcome = Outcome::new(
            Class::Review,
            "a comparator with air behind changes the build without it",
        );
        outcome.details = dead
            .kept
            .iter()
            .map(|(input, why)| {
                format!("comparator with air behind at {}: {why}", into(input))
            })
            .collect();
        outcome
    })
}

/// A comparator with air behind in words, where it stands and what it
/// gives out into, such as `(4, 17, 11) into redstone wire`.
fn into(blocks: &Blocks, input: &StrengthInput) -> String {
    format!(
        "{} into {}",
        place(input.at),
        schem::short(blocks.get(input.front())).replace('_', " ")
    )
}

/// The original build a port is checked against.
struct Original<'a> {
    /// Its blocks, made simulatable.
    blocks: &'a Blocks,
    /// The lines its truth table is run through: as its colours mark
    /// them, or else as probing finds them.
    lines: Interface,
    category: Option<&'a str>,
    width: Option<i64>,
    test: Option<Test>,
    /// Its category's test run on it.
    before: Option<&'a simulate::Report>,
}

/// Checks `port`, `done` tapping `original`'s outputs, against `original`,
/// writing to `said` what each check found: read back by the proposed
/// rule, by the category's test, and case by case through one interface.
/// Ported when every check that ran agrees and one ran; to review, the
/// first that differs its reason, with every difference; else untestable.
fn check(
    original: &Original,
    port: &Port,
    done: &[Tapped],
    said: &mut String,
) -> Result<Outcome> {
    // Each check that differs from the original, with its reason, and how
    // many checks ran.
    let mut differ: Vec<(&str, Vec<String>)> = Vec::new();
    if let Some(missed) = read_back(port, said)? {
        differ.push(("a tap isn't read back", vec![missed]));
    }
    if let Some(missed) = read_back_inputs(port, said) {
        differ.push(("a fed input isn't read back", vec![missed]));
    }
    let mut checks = 0;
    if let (Some(test), Some(before)) = (original.test, original.before) {
        // The category's test, its outputs read at the taps.
        let checked = test_copy(&port.file, original.blocks, done, port.moved)?;
        let (was, now) = (verdict(before), verdict(&test.run(&checked)));
        writeln!(said, "  test before: {was}")?;
        writeln!(said, "  test after:  {now}")?;
        checks += usize::from(matches!(before, simulate::Report::Ran(_)));
        if was != now {
            differ.push((
                "the test's result changed",
                vec![
                    format!("test before: {was}"),
                    format!("test after: {now}"),
                ],
            ));
        }
        // A build with signal-strength inputs, which probing doesn't read,
        // tested too through the lines the proposed rule reads, colours
        // playing no part.
        if !port.fed.is_empty() {
            let lines = repeater_outputs::interface(&port.after);
            let ruled = verdict(&test.run_with(&port.after, lines));
            writeln!(said, "  test by the proposed rule: {ruled}")?;
            if was != ruled {
                differ.push((
                    "the test by the proposed rule differs",
                    vec![
                        format!("test before: {was}"),
                        format!("test by the proposed rule: {ruled}"),
                    ],
                ));
            }
        }
    } else if let Some(category) = original.category {
        writeln!(said, "  no test for {category} of {:?}", original.width)?;
    }
    // One interface for both, its outputs moved to the taps.
    let lines = &original.lines;
    let moved_lines = moved_interface(lines, &port.now_at, port.moved);
    let mut no_table = None;
    let mut min_ss = Vec::new();
    let on_off = on_off_outputs(&moved_lines, &port.after);
    match compare_tables(
        (original.blocks, lines),
        (&port.after, &moved_lines),
        &on_off,
        said,
        &mut min_ss,
    )? {
        Compared::Same => checks += 1,
        Compared::Differs(first) => {
            differ.push(("the truth table changed", vec![first]));
        }
        Compared::Not(why) => no_table = Some(why),
    }
    let outcome = if let Some((reason, _)) = differ.first() {
        let mut outcome = Outcome::new(Class::Review, *reason);
        outcome.details = differ.into_iter().flat_map(|(_, d)| d).collect();
        outcome
    } else if checks == 0 {
        let mut outcome = Outcome::new(
            Class::Untestable,
            if original.test.is_none() {
                "no test for its category, and no truth table"
            } else {
                "its test didn't run, and no truth table"
            },
        );
        outcome
            .details
            .extend(no_table.map(|why| format!("truth table not run: {why}")));
        outcome
    } else {
        Outcome::new(Class::Ported, "")
    };
    Ok(Outcome { min_ss, ..outcome })
}

/// A port, as checks read it.
struct Port {
    file: Vec<u8>,
    /// How far every old position moved, as the box grew.
    moved: Pos,
    /// Where each output is read now, by the block it was read by: the
    /// block under its tap or its promoted driver, else where it was, each
    /// moved with the box.
    now_at: HashMap<Pos, Pos>,
    /// Its blocks, made simulatable.
    after: Blocks,
    /// Where each signal-strength input's comparator stands now, by the
    /// input block it was read by, moved with the box: those fed and those
    /// fed already.
    fed: Vec<(Pos, Pos)>,
}

impl Port {
    fn new(
        file: Vec<u8>,
        moved: Pos,
        done: &[Tapped],
        fed: &[Fed],
    ) -> Result<Self> {
        let now_at = done
            .iter()
            .map(|t| {
                let at = t.output(false).map_or(t.block(), Output::block);
                (t.block(), schem::add(at, moved))
            })
            .collect();
        let after = Blocks::read(&probe::simulatable(&file)?)?;
        let fed = fed
            .iter()
            .filter_map(|fed| match fed {
                Fed::Fed {
                    pad, comparator, ..
                } => Some((*pad, schem::add(*comparator, moved))),
                Fed::Already { pad } => {
                    let pad = schem::add(*pad, moved);
                    repeater_outputs::driving(&after, pad)
                        .map(|input| (pad, input.at))
                }
                Fed::Not { .. } => None,
            })
            .collect();
        Ok(Port {
            file,
            moved,
            now_at,
            after,
            fed,
        })
    }
}

/// Writes to `said` how many of the port's outputs the proposed rule reads
/// back; those it misses, in words, if any.
fn read_back(port: &Port, said: &mut String) -> Result<Option<String>> {
    let found: HashSet<Pos> = repeater_outputs::find(&port.after)
        .iter()
        .map(|o| o.block())
        .collect();
    let missed: Vec<String> = port
        .now_at
        .iter()
        .filter(|(_, at)| !found.contains(*at))
        .map(|(block, at)| format!("{} (now {})", place(*block), place(*at)))
        .collect();
    writeln!(
        said,
        "  read back by the proposed rule: {} of {} outputs found, {} \
         repeaters or comparators facing air in all",
        port.now_at.len() - missed.len(),
        port.now_at.len(),
        found.len()
    )?;
    Ok((!missed.is_empty())
        .then(|| format!("not read back: {}", missed.join(", "))))
}

/// Writes to `said` how many of the port's fed inputs the proposed rule
/// reads back as signal-strength inputs; those it misses, in words, if
/// any.
fn read_back_inputs(port: &Port, said: &mut String) -> Option<String> {
    if port.fed.is_empty() {
        return None;
    }
    let found: HashSet<Pos> = repeater_outputs::strength_inputs(&port.after)
        .iter()
        .map(|input| input.at)
        .collect();
    let missed: Vec<String> = port
        .fed
        .iter()
        .filter(|(_, at)| !found.contains(at))
        .map(|(pad, at)| format!("{} (comparator {})", place(*pad), place(*at)))
        .collect();
    writeln!(
        said,
        "  inputs read back by the proposed rule: {} of {} found, {} \
         comparators with air behind in all",
        port.fed.len() - missed.len(),
        port.fed.len(),
        found.len()
    )
    .ok()?;
    (!missed.is_empty())
        .then(|| format!("not read back: {}", missed.join(", ")))
}

/// What came of a port's outputs, `outputs`, and of its inputs, `inputs`,
/// as one: to review when either is, with both reasons; ported when either
/// is; else already.
fn joined(outputs: Outcome, inputs: Outcome) -> Outcome {
    match (outputs.class, inputs.class) {
        (Class::Review, Class::Review) => {
            let mut outcome = Outcome::new(
                Class::Review,
                format!("{}; {}", outputs.reason, inputs.reason),
            );
            outcome.details = [outputs.details, inputs.details].concat();
            outcome
        }
        (Class::Review, _) => outputs,
        (_, Class::Review) => inputs,
        (Class::Ported, _) | (_, Class::Ported) => {
            Outcome::new(Class::Ported, "")
        }
        _ => outputs,
    }
}

/// Writes to `said` what came of each signal-strength input; what came of
/// them: ported, each fed or fed already, already, when none needed
/// feeding, or to review, naming each that couldn't be.
fn tell_fed(
    blocks: &Blocks,
    fed: &[Fed],
    said: &mut String,
) -> Result<Outcome> {
    if fed.is_empty() {
        return Ok(Outcome::new(Class::Already, "no signal-strength inputs"));
    }
    let mut not = Vec::new();
    let mut sides: BTreeMap<&str, usize> = BTreeMap::new();
    for each in fed {
        match each {
            Fed::Already { pad } => {
                writeln!(said, "  already: input on {}", place(*pad))?;
            }
            Fed::Fed {
                pad,
                side,
                comparator,
                support,
                glass,
            } => {
                *sides.entry(side.name()).or_default() += 1;
                let held = match (support, glass) {
                    (None, _) => {
                        let under = schem::step(*comparator, Direction::Down);
                        schem::short(blocks.get(under)).replace('_', " ")
                    }
                    (Some(_), false) => "white concrete put there".to_owned(),
                    (Some(at), true) => format!(
                        "white stained glass put there over {}",
                        schem::short(
                            blocks.get(schem::step(*at, Direction::Down))
                        )
                        .replace('_', " ")
                    ),
                };
                writeln!(
                    said,
                    "  fed: input on {} ({}) {side}: comparator at {} on {held}",
                    place(*pad),
                    schem::short(blocks.get(*pad)).replace('_', " "),
                    place(*comparator)
                )?;
            }
            Fed::Not { pad, why } => {
                writeln!(said, "  not fed: input on {}: {why}", place(*pad))?;
                not.push(format!("input on {}: {why}", place(*pad)));
            }
        }
    }
    let count: usize = sides.values().sum();
    let ways: Vec<String> = sides
        .iter()
        .map(|(side, n)| format!("{n} {side}"))
        .collect();
    writeln!(
        said,
        "  {} signal-strength inputs: {count} fed{}, {} already, {} not",
        fed.len(),
        if ways.is_empty() {
            String::new()
        } else {
            format!(" ({})", ways.join(", "))
        },
        fed.len() - count - not.len(),
        not.len()
    )?;
    Ok(if !not.is_empty() {
        let mut outcome =
            Outcome::new(Class::Review, "not fed: every side blocked");
        outcome.details = not;
        outcome
    } else if count == 0 {
        Outcome::new(Class::Already, "every input already one")
    } else {
        Outcome::new(Class::Ported, "")
    })
}

/// The level sides a tap would reach from one copy of `stored` into the
/// next: along each way it stacks, west and east for a build stacked west,
/// north and south for one stacked north, and each side whose layers the
/// box its later copies repeat leaves out, where the copy before stands.
fn stacking_sides(stored: Option<&schematic::Schematic>) -> Vec<Direction> {
    let mut sides = Vec::new();
    for way in stored.iter().flat_map(|s| &s.stacking) {
        if way.west > 0 {
            sides.extend([Direction::West, Direction::East]);
        }
        if way.north > 0 {
            sides.extend([Direction::North, Direction::South]);
        }
        if let Some(faces) = way.faces {
            for (n, side) in [
                (faces.low[0], Direction::West),
                (faces.low[2], Direction::North),
                (faces.high[0], Direction::East),
                (faces.high[2], Direction::South),
            ] {
                if n > 0 {
                    sides.push(side);
                }
            }
        }
    }
    sides
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

/// Writes to `said` what came of each output; what came of the build so
/// far: ported, every output promoted, tapped, or one already, already, when none
/// needed a tap, or to review, naming each that couldn't be.
fn tell_tapped(
    blocks: &Blocks,
    done: &[Tapped],
    said: &mut String,
) -> Result<Outcome> {
    let mut not = Vec::new();
    let mut sides: BTreeMap<&str, usize> = BTreeMap::new();
    let mut promoted = 0;
    for tapped in done {
        match tapped {
            Tapped::Already { block } => {
                writeln!(said, "  already: output on {}", place(*block))?;
            }
            Tapped::Tapped {
                block,
                side,
                support,
                glass,
                clear,
                now,
            } => {
                *sides.entry(side.name()).or_default() += 1;
                writeln!(
                    said,
                    "  tapped: output on {} ({}) {side}: {} on {} at {}{}",
                    place(*block),
                    schem::short(blocks.get(*block)).replace('_', " "),
                    schem::short(now),
                    if *glass {
                        let under = schem::step(*support, Direction::Down);
                        format!(
                            "white stained glass over {}",
                            schem::short(blocks.get(under)).replace('_', " ")
                        )
                    } else {
                        "white concrete".to_owned()
                    },
                    place(*support),
                    if *clear {
                        ""
                    } else {
                        ", not the face it sits on"
                    }
                )?;
            }
            Tapped::Promoted { .. } => {
                promoted += 1;
                writeln!(
                    said,
                    "  promoted: {}",
                    promoted_words(blocks, tapped)
                )?;
            }
            Tapped::Not { block, why } => {
                writeln!(
                    said,
                    "  not tapped: output on {}: {why}",
                    place(*block)
                )?;
                not.push((*block, why.clone()));
            }
        }
    }
    let tapped: usize = sides.values().sum();
    let ways: Vec<String> = sides
        .iter()
        .map(|(side, n)| format!("{n} {side}"))
        .collect();
    writeln!(
        said,
        "  {} outputs: {promoted} promoted, {tapped} tapped{}, {} already, {} \
         not",
        done.len(),
        if ways.is_empty() {
            String::new()
        } else {
            format!(" ({})", ways.join(", "))
        },
        done.len() - promoted - tapped - not.len(),
        not.len()
    )?;
    if !not.is_empty() {
        let mut reasons: Vec<&str> =
            not.iter().map(|(_, why)| not_reason(why)).collect();
        reasons.sort_unstable();
        reasons.dedup();
        let mut outcome = Outcome::new(Class::Review, reasons.join("; "));
        outcome.details = not
            .iter()
            .map(|(block, why)| format!("output on {}: {why}", place(*block)))
            .collect();
        return Ok(outcome);
    }
    Ok(if tapped + promoted == 0 {
        Outcome::new(Class::Already, "every output already one")
    } else {
        Outcome::new(Class::Ported, "")
    })
}

/// What came of an output whose driver was promoted, `tapped`, of the
/// original's `blocks`, in words.
fn promoted_words(blocks: &Blocks, tapped: &Tapped) -> String {
    let Tapped::Promoted {
        block,
        driver,
        torch,
    } = tapped
    else {
        return String::new();
    };
    let moved = torch.map_or_else(String::new, |(from, to)| {
        format!("its torch at {} moved to {}, and ", place(from), place(to))
    });
    format!(
        "output on {} ({}): {moved}its {} at {} gives out {} into air",
        place(*block),
        schem::short(blocks.get(*block)).replace('_', " "),
        driver.kind(),
        place(driver.at),
        driver.points()
    )
}

/// A short reason, to group by, for an output not tapped for `why`.
fn not_reason(why: &str) -> &'static str {
    if why.starts_with("every side is blocked") {
        "not tapped: every side blocked"
    } else if why.starts_with("its repeater")
        || why.starts_with("its comparator")
    {
        "not tapped: read by a repeater or comparator that powers something"
    } else {
        "not tapped: something other than dust on it"
    }
}

/// The port in `file`, which moved every position of `original` by
/// `moved`, with the roles the old rule read in `original`'s colours kept
/// in the site's colours, as probing colours a build it recognizes: each
/// tap or promoted driver of `done` with its support coloured as its
/// output block was, of
/// concrete or stained glass as the support is, its sign moved under its
/// front already, see [`repeater_outputs::sign`]; each input of `fed`
/// with its comparator's support coloured as its input block was, and that
/// block, which only holds dust now, whitened.
fn keep_roles(
    file: &[u8],
    original: &Blocks,
    done: &[Tapped],
    fed: &[Fed],
    moved: Pos,
) -> Result<Vec<u8>> {
    let port = Blocks::read(file)?;
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    let material = |at: Pos| {
        if schem::short(port.get(at)).ends_with("glass") {
            "stained_glass"
        } else {
            "concrete"
        }
    };
    for tapped in done {
        let Some(output) = tapped.output(false) else {
            continue;
        };
        let Some(colour) =
            legacy::output_colour(schem::short(original.get(tapped.block())))
        else {
            continue;
        };
        let support = schem::add(output.block(), moved);
        changes.insert(
            support,
            format!("minecraft:{colour}_{}", material(support)),
        );
    }
    for input in fed {
        let pad = schem::add(input.pad(), moved);
        let support = match input {
            Fed::Fed {
                comparator,
                support,
                ..
            } => support.map_or_else(
                || schem::step(schem::add(*comparator, moved), Direction::Down),
                |support| schem::add(support, moved),
            ),
            Fed::Already { .. } => {
                let Some(driver) = repeater_outputs::driving(&port, pad) else {
                    continue;
                };
                schem::step(driver.at, Direction::Down)
            }
            Fed::Not { .. } => continue,
        };
        let Some(colour) =
            legacy::input_mark(schem::short(original.get(input.pad())))
        else {
            continue;
        };
        changes.insert(
            support,
            format!("minecraft:{colour}_{}", material(support)),
        );
        changes.insert(pad, format!("minecraft:white_{}", material(pad)));
    }
    Ok(standardize::edit_grown(file, &changes)?.0)
}

/// The port in `file` as the category's test reads it, its outputs at the
/// taps and promoted drivers: each support coloured as its output block was, of concrete or
/// stained glass as the support is, and each sign naming an output block
/// read as naming its support. A test reads outputs by colour, so this is
/// how it reads the taps until the site reads them by the proposed rule.
fn test_copy(
    file: &[u8],
    original: &Blocks,
    done: &[Tapped],
    moved: Pos,
) -> Result<Blocks> {
    let port = Blocks::read(file)?;
    let mut changes = BTreeMap::new();
    let mut signs = Vec::new();
    for tapped in done {
        let Some(output) = tapped.output(false) else {
            continue;
        };
        let block = tapped.block();
        let Some(colour) =
            legacy::output_colour(schem::short(original.get(block)))
        else {
            continue;
        };
        let support = schem::add(output.block(), moved);
        let material = if schem::short(port.get(support)).ends_with("glass") {
            "stained_glass"
        } else {
            "concrete"
        };
        changes.insert(support, format!("minecraft:{colour}_{material}"));
        signs.push((schem::add(block, moved), support));
    }
    let coloured = standardize::edit(file, &changes)?;
    let mut blocks = Blocks::read(&probe::simulatable(&coloured)?)?;
    for (block, support) in signs {
        let naming: Vec<Pos> = blocks
            .signs
            .keys()
            .copied()
            .filter(|&pos| blocks.sign_block(pos) == Some(block))
            .collect();
        for pos in naming {
            // Kept by the place over the support, which reads, as a
            // standing sign's would, as on the block under it.
            if let Some(text) = blocks.signs.remove(&pos) {
                let over = schem::step(support, Direction::Up);
                blocks.signs.insert(over, text);
            }
        }
    }
    Ok(blocks)
}

/// `lines` with every bit moved by `moved`, as the box grew, and each
/// output bit read where `now_at` says it is read now.
fn moved_interface(
    lines: &Interface,
    now_at: &HashMap<Pos, Pos>,
    moved: Pos,
) -> Interface {
    let shift = |pos: Pos| schem::add(pos, moved);
    let mut moved = Interface {
        inputs: lines.inputs.clone(),
        outputs: lines.outputs.clone(),
    };
    for port in &mut moved.inputs {
        for bit in &mut port.bits {
            bit.pos = shift(bit.pos);
        }
    }
    for port in &mut moved.outputs {
        for bit in &mut port.bits {
            bit.pos = now_at.get(&bit.pos).copied().unwrap_or(shift(bit.pos));
        }
    }
    moved
}

/// Which outputs of `lines`, a port's, `after` its blocks, a truth table
/// compares as on or off: those read a digit a block, as every output of
/// a build with a signal-strength input is but a status line, whose taps
/// are repeaters, which give out 15 for any strength on, as an output of
/// bits does.
fn on_off_outputs(lines: &Interface, after: &Blocks) -> Vec<bool> {
    let digits = lines.inputs.iter().any(|port| port.signal_strength);
    lines
        .outputs
        .iter()
        .map(|port| {
            digits
                && !port.status()
                && port.bits.iter().all(|bit| {
                    let top = schem::step(bit.pos, Direction::Up);
                    schem::short(after.get(top)) == "repeater"
                })
        })
        .collect()
}

/// How two truth tables compared.
enum Compared {
    Same,
    /// The first case that differs, in words.
    Differs(String),
    /// Why they weren't run.
    Not(String),
}

/// Writes to `said` how the truth tables of `before` and `after`, each a
/// build and the interface it is run through, compare, and each output's
/// weakest strength in each: shown by the dust on it before, and arriving
/// at its repeater or comparator after, which `min_ss` keeps.
fn compare_tables(
    (blocks, lines): (&Blocks, &Interface),
    (after, after_lines): (&Blocks, &Interface),
    on_off: &[bool],
    said: &mut String,
    min_ss: &mut Vec<String>,
) -> Result<Compared> {
    let (before, now) = match (table(blocks, lines), table(after, after_lines))
    {
        (Ok(before), Ok(now)) => (before, now),
        (Err(err), _) | (_, Err(err)) => {
            writeln!(said, "  truth table not compared: {err}")?;
            return Ok(Compared::Not(err.to_string()));
        }
    };
    // Each output's values, a digit read as on or off where `on_off` says.
    let read = |values: &[simulate::Value]| -> Vec<simulate::Value> {
        values
            .iter()
            .zip(on_off.iter().chain(std::iter::repeat(&false)))
            .map(|(&value, &on_off)| {
                if on_off {
                    (0..32).fold(0, |read, digit| {
                        let on = value >> (4 * digit) & 15 != 0;
                        read | simulate::Value::from(on) << digit
                    })
                } else {
                    value
                }
            })
            .collect()
    };
    let differ: Vec<usize> = (0..before.cases.len())
        .filter(|&i| read(&before.cases[i].1) != read(&now.cases[i].1))
        .collect();
    let compared = match differ.first() {
        None => {
            writeln!(
                said,
                "  truth table: same in all {} cases{}",
                now.cases.len(),
                if now.exhaustive { "" } else { " sampled" }
            )?;
            Compared::Same
        }
        Some(&i) => {
            let (b, n) = (&before.cases[i], &now.cases[i]);
            let names: Vec<&str> =
                lines.outputs.iter().map(|p| p.name.as_str()).collect();
            let first = format!(
                "differs in {} of {} cases, first inputs {:#b}: outputs {} \
                 {:?} before, {:?} after; strengths arriving at the taps \
                 then {:?}",
                differ.len(),
                now.cases.len(),
                b.0,
                names.join(", "),
                b.1,
                n.1,
                now.arrivals[i]
            );
            writeln!(said, "  truth table: {first}")?;
            Compared::Differs(first)
        }
    };
    for (port, out) in lines.outputs.iter().enumerate() {
        writeln!(
            said,
            "  min SS of {}: dust before {:?}; arriving at the taps {:?}",
            out.name, before.shown[port], now.arriving[port]
        )?;
        let each: Vec<String> =
            now.arriving[port].iter().map(ToString::to_string).collect();
        min_ss.push(format!("{}: {}", out.name, each.join(" ")));
    }
    Ok(compared)
}

/// The ways `stored` stacks that pass the stack check, as `rsdb
/// stack-check` runs it on `target` (a stored build's id or a file), each
/// way at its stored spacing; the lines it printed.
fn passing_stacks(
    target: &str,
    like: &str,
    ways: &[(&str, i64)],
) -> Result<(Vec<String>, String)> {
    let names: Vec<&str> = ways.iter().map(|(way, _)| *way).collect();
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("stack-check").arg(target).args([
        "--like",
        like,
        "--dirs",
        &names.join(","),
    ]);
    // One way alone may be stacked at a spacing other than its stored one.
    if let [(_, spacing)] = ways {
        command.args(["--spacing", &spacing.to_string()]);
    }
    let output = command.stderr(Stdio::null()).output()?;
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    let passing = printed
        .lines()
        .find_map(|line| line.strip_prefix("passing: "))
        .filter(|list| *list != "none")
        .map(|list| list.split(", ").map(str::to_owned).collect())
        .unwrap_or_default();
    Ok((passing, printed))
}

/// Stacks the original `stored` and its port `file` each way `stored`
/// stacks, as `rsdb stack-check` does, and writes to `said` which pass;
/// the port is to review when it loses one the original passes.
fn stack_both(
    stored: &schematic::Schematic,
    file: &[u8],
    ways: &[(&str, i64)],
    said: &mut String,
    outcome: &mut Outcome,
) -> Result<()> {
    let path = std::env::temp_dir().join(format!(
        "rsdb-convert-{}-{}.schem",
        std::process::id(),
        &stored.id[..8]
    ));
    fs::write(&path, file)?;
    let target = path.to_string_lossy();
    let port = passing_stacks(&target, &stored.id, ways);
    let (port, printed) = port?;
    let (original, _) = passing_stacks(&stored.id, &stored.id, ways)?;
    // A way the port loses, tried a block further apart, as its box grew.
    let mut further = Vec::new();
    for &(way, spacing) in ways {
        if original.contains(&format!("{way} {spacing}"))
            && !port.contains(&format!("{way} {spacing}"))
        {
            let (passing, _) =
                passing_stacks(&target, &stored.id, &[(way, spacing + 1)])?;
            further.extend(passing);
        }
    }
    fs::remove_file(&path)?;
    let words = |list: &[String]| {
        if list.is_empty() {
            "none".to_owned()
        } else {
            list.join(", ")
        }
    };
    let stacked: Vec<String> =
        ways.iter().map(|(w, n)| format!("{w} {n}")).collect();
    let line = format!(
        "stacked {}: original passes {}; port passes {}",
        stacked.join(", "),
        words(&original),
        words(&port)
    );
    writeln!(said, "  {line}")?;
    let lost: Vec<&String> =
        original.iter().filter(|way| !port.contains(way)).collect();
    if !lost.is_empty() {
        let regained = lost.iter().all(|way| {
            let (name, spacing) = way.split_once(' ').unwrap_or((way, "0"));
            let spacing: i64 = spacing.parse().unwrap_or(0);
            further.contains(&format!("{name} {}", spacing + 1))
        });
        outcome.class = Class::Review;
        let reason = if regained {
            "a stack the original passes needs a block more between copies"
        } else {
            "a stack the original passes fails"
        };
        reason.clone_into(&mut outcome.reason);
        outcome.details.push(format!(
            "stack check: original passes {}, port {}{}",
            words(&original),
            words(&port),
            if further.is_empty() {
                String::new()
            } else {
                format!(
                    "; a block further apart, the port passes {}",
                    further.join(", ")
                )
            }
        ));
        // Why, as the stack check says it of the port.
        outcome.details.extend(
            printed
                .lines()
                .filter(|line| {
                    line.contains("FAILS")
                        || line.contains("fails")
                        || line.contains("now powers")
                        || line.contains("pasted over")
                })
                .take(6)
                .map(|line| format!("  {}", line.trim())),
        );
    } else if original.is_empty() {
        outcome.details.push(format!(
            "stack check: no stored way passes for the original either ({})",
            stacked.join(", ")
        ));
    }
    outcome.stacked = Some(line);
    Ok(())
}

/// A build `--all` ports, and what came of it.
struct Ported {
    id: String,
    category: String,
    name: String,
    outcome: Outcome,
    /// What the run printed of it.
    said: String,
    /// A hand port's file name, when the owner ported it by hand.
    hand: Option<String>,
    /// How the port compares with the hand port.
    against_hand: Option<String>,
}

/// Ports every stored build but those `args.skip` names or the hand ports'
/// `dropped` folder holds, `args.jobs` at a time, each in a process of
/// its own held to `PORT_MEMORY_KIB` and `PORT_TIME`; writes each port that
/// passed every check to `args.out_dir`, unless the owner ported it by
/// hand; then prints every build not fully ported, grouped by reason, the
/// hand ports apart, and counts of each.
async fn convert_all(db: &SqlitePool, args: &Convert) -> Result<()> {
    let builds = schematic::all(db).await?;
    if let Some(dir) = &args.out_dir {
        fs::create_dir_all(dir)?;
    }
    let hand: HashMap<String, String> = match &args.hand_ported {
        Some(dir) => hand_ports(dir)?,
        None => HashMap::new(),
    };
    let skip = left_unported(&args.skip, args.hand_ported.as_deref())?;
    let (skipped, builds): (Vec<_>, Vec<_>) = builds
        .into_iter()
        .partition(|s| skip.iter().any(|id| s.id.starts_with(id.as_str())));
    let scratch = std::env::temp_dir()
        .join(format!("rsdb-convert-all-{}", std::process::id()));
    fs::create_dir_all(&scratch)?;
    let queue: Arc<Mutex<Vec<(String, String, String)>>> =
        Arc::new(Mutex::new(
            builds
                .iter()
                .rev()
                .map(|s| (s.id.clone(), s.category.clone(), s.filename()))
                .collect(),
        ));
    let results: Arc<Mutex<Vec<Ported>>> = Arc::new(Mutex::new(Vec::new()));
    let total = builds.len();
    std::thread::scope(|scope| {
        for _ in 0..args.jobs.max(1) {
            let (queue, results) = (queue.clone(), results.clone());
            let (scratch, hand, args) = (&scratch, &hand, args);
            scope.spawn(move || {
                loop {
                    let Some((id, category, name)) =
                        queue.lock().expect("queue").pop()
                    else {
                        break;
                    };
                    let file = scratch.join(format!("{}.schem", &id[..8]));
                    let ported = port_apart(&id, &category, &name, &file);
                    let ported = finish_port(ported, &file, hand, args);
                    let mut results = results.lock().expect("results");
                    results.push(ported);
                    eprintln!(
                        "{}/{total} {} {category}",
                        results.len(),
                        &id[..8]
                    );
                }
            });
        }
    });
    fs::remove_dir_all(&scratch).ok();
    let mut results = Arc::try_unwrap(results)
        .ok()
        .context("ports still running")?
        .into_inner()
        .expect("results");
    results.sort_by(|a, b| (&a.category, &a.id).cmp(&(&b.category, &b.id)));
    print_review(&results);
    println!();
    println!("== LEFT UNPORTED ({}), as asked", skipped.len());
    for s in &skipped {
        println!("  {} {}  {}", &s.id[..8], s.category, s.filename());
    }
    Ok(())
}

/// The hand ports in `dir`, by the eight characters of their build's id,
/// with their file names.
fn hand_ports(dir: &Path) -> Result<HashMap<String, String>> {
    let mut found = HashMap::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".schem") else {
            continue;
        };
        if let Some((_, id8)) = stem.rsplit_once('_')
            && id8.len() == 8
        {
            found.insert(id8.to_owned(), name.clone());
        }
    }
    Ok(found)
}

/// The builds `--all` leaves unported, by their id or its start: those
/// `skip` names, and each the owner dropped, as a port of it in the
/// `dropped` folder of `hand_ported` says.
fn left_unported(
    skip: &[String],
    hand_ported: Option<&Path>,
) -> Result<Vec<String>> {
    let mut left = skip.to_vec();
    if let Some(dropped) = hand_ported.map(|dir| dir.join("dropped"))
        && dropped.is_dir()
    {
        left.extend(hand_ports(&dropped)?.into_keys());
    }
    left.sort_unstable();
    left.dedup();
    Ok(left)
}

/// Ports the stored build `id` in a process of its own, held to
/// `PORT_MEMORY_KIB` and `PORT_TIME`, writing any port to `file`.
fn port_apart(id: &str, category: &str, name: &str, file: &Path) -> Ported {
    let failed = |reason: String, said: String| Ported {
        id: id.to_owned(),
        category: category.to_owned(),
        name: name.to_owned(),
        outcome: Outcome::new(Class::Untestable, reason),
        said,
        hand: None,
        against_hand: None,
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return failed(err.to_string(), String::new()),
    };
    let child = Command::new("sh")
        .arg("-c")
        .arg(format!("ulimit -v {PORT_MEMORY_KIB} && exec \"$0\" \"$@\""))
        .arg(exe)
        .args(["convert", id, "--outcome", "--out"])
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => return failed(err.to_string(), String::new()),
    };
    let start = Instant::now();
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).ok();
        text
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() > PORT_TIME => {
                child.kill().ok();
                child.wait().ok();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(_) => break None,
        }
    };
    let said = reader.join().unwrap_or_default();
    let mut stderr = String::new();
    if let Some(mut err) = child.stderr.take() {
        err.read_to_string(&mut stderr).ok();
    }
    let last = said.lines().last().unwrap_or("");
    match (status, serde_json::from_str::<Outcome>(last)) {
        (Some(status), Ok(outcome)) if status.success() => Ported {
            id: id.to_owned(),
            category: category.to_owned(),
            name: name.to_owned(),
            outcome,
            said: said[..said.len() - last.len()].to_owned(),
            hand: None,
            against_hand: None,
        },
        (None, _) => failed(
            format!("porting took over {} minutes", PORT_TIME.as_secs() / 60),
            said,
        ),
        (Some(status), _) => failed(
            format!(
                "porting ended {status}: {}",
                stderr.lines().last().unwrap_or("")
            ),
            said,
        ),
    }
}

/// `ported`, its port written to `args.out_dir` as
/// `<category>_<id8>.schem` when it passed every check and the owner didn't
/// port it by hand, and compared with the owner's port when they did.
fn finish_port(
    mut ported: Ported,
    file: &Path,
    hand: &HashMap<String, String>,
    args: &Convert,
) -> Ported {
    let id8 = ported.id[..8].to_owned();
    ported.hand = hand.get(&id8).cloned();
    let made = fs::read(file).ok();
    fs::remove_file(file).ok();
    if let (Some(name), Some(dir)) = (&ported.hand, &args.hand_ported) {
        ported.against_hand = Some(match (&made, fs::read(dir.join(name))) {
            (Some(made), Ok(by_hand)) => against(made, &by_hand),
            (None, _) => "no port made to compare".to_owned(),
            (_, Err(err)) => format!("hand port unread: {err}"),
        });
        return ported;
    }
    if ported.outcome.class == Class::Ported
        && let (Some(made), Some(dir)) = (&made, &args.out_dir)
    {
        let path = dir.join(format!("{}_{id8}.schem", ported.category));
        if let Err(err) = fs::write(&path, made) {
            ported.outcome =
                Outcome::new(Class::Untestable, format!("not written: {err}"));
        }
    }
    ported
}

/// How the port `made` compares with the owner's `by_hand`, in a line:
/// by the size the site measures and where in it each output stands, then
/// block by block.
fn against(made: &[u8], by_hand: &[u8]) -> String {
    let (Ok(made), Ok(by_hand)) = (Blocks::read(made), Blocks::read(by_hand))
    else {
        return "a file unread".to_owned();
    };
    // Each output's place in the measured box, and the box's size.
    let measured = |blocks: &Blocks| {
        let (low, high) = interface::measured(blocks);
        let mut outputs: Vec<Pos> = repeater_outputs::find(blocks)
            .iter()
            .map(|o| [0, 1, 2].map(|i| o.at[i] - low[i]))
            .collect();
        outputs.sort_unstable();
        ([0, 1, 2].map(|i| high[i] - low[i] + 1), outputs)
    };
    let ((size, outputs), (hand_size, hand_outputs)) =
        (measured(&made), measured(&by_hand));
    let sizes = if size == hand_size {
        format!("measures {} as the hand port does", place(size))
    } else {
        format!(
            "measures {} against the hand port's {}",
            place(size),
            place(hand_size)
        )
    };
    let placed = if outputs == hand_outputs {
        "its outputs where the hand port's are".to_owned()
    } else {
        let only = |a: &[Pos], b: &[Pos]| {
            a.iter()
                .filter(|at| !b.contains(at))
                .map(|&at| place(at))
                .collect::<Vec<_>>()
                .join(" ")
        };
        format!(
            "outputs differ, its own at {}, the hand port's at {}",
            only(&outputs, &hand_outputs),
            only(&hand_outputs, &outputs)
        )
    };
    format!("{sizes}, {placed}; {}", blocks_against(&made, &by_hand))
}

/// How the blocks of `made` compare with the owner's `by_hand`, in words.
fn blocks_against(made: &Blocks, by_hand: &Blocks) -> String {
    if made.size != by_hand.size {
        return format!(
            "box {} against the hand port's {}",
            place(made.size),
            place(by_hand.size)
        );
    }
    // Compared by block, not shape: settling can shape dust either way.
    let differ: Vec<String> = made
        .iter()
        .filter(|&(pos, state)| schem::id(state) != schem::id(by_hand.get(pos)))
        .map(|(pos, state)| {
            format!(
                "{} {} (hand: {})",
                place(pos),
                schem::short(state).replace('_', " "),
                schem::short(by_hand.get(pos)).replace('_', " ")
            )
        })
        .collect();
    if differ.is_empty() {
        "the same blocks as the hand port".to_owned()
    } else {
        format!(
            "{} blocks differ from the hand port: {}{}",
            differ.len(),
            differ
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
            if differ.len() > 4 { "; ..." } else { "" }
        )
    }
}

/// Prints the builds `--all` ported: counts of each outcome, every build
/// not fully ported grouped by reason with its details, and the hand ports
/// apart, with how each auto port compares.
fn print_review(results: &[Ported]) {
    let (hand, auto): (Vec<&Ported>, Vec<&Ported>) =
        results.iter().partition(|p| p.hand.is_some());
    let count =
        |class: Class| auto.iter().filter(|p| p.outcome.class == class).count();
    println!(
        "{} stored builds, {} of them ported by hand and listed apart",
        results.len(),
        hand.len()
    );
    println!("  ported cleanly: {}", count(Class::Ported));
    print_counts(&auto, &hand);
    println!("  already new-style: {}", count(Class::Already));
    for (class, label) in [
        (Class::Review, "needs review"),
        (Class::Untestable, "not testable"),
    ] {
        let mut reasons: BTreeMap<&str, usize> = BTreeMap::new();
        for p in auto.iter().filter(|p| p.outcome.class == class) {
            *reasons.entry(p.outcome.reason.as_str()).or_default() += 1;
        }
        println!("  {label}: {}", count(class));
        for (reason, n) in reasons {
            println!("    {n}  {reason}");
        }
    }
    for (class, label) in [
        (Class::Review, "NEEDS REVIEW"),
        (Class::Untestable, "NOT TESTABLE"),
    ] {
        let mut by_reason: BTreeMap<&str, Vec<&Ported>> = BTreeMap::new();
        for p in auto.iter().filter(|p| p.outcome.class == class) {
            by_reason
                .entry(p.outcome.reason.as_str())
                .or_default()
                .push(p);
        }
        for (reason, builds) in by_reason {
            println!();
            println!("== {label}: {reason} ({})", builds.len());
            for p in builds {
                print_one(p);
            }
        }
    }
    let already: Vec<String> = auto
        .iter()
        .filter(|p| p.outcome.class == Class::Already)
        .map(|p| {
            format!("{} {} ({})", &p.id[..8], p.category, p.outcome.reason)
        })
        .collect();
    println!();
    println!("== ALREADY NEW-STYLE ({})", already.len());
    for line in already {
        println!("  {line}");
    }
    println!();
    println!(
        "== PORTED BY HAND ({}), ported again to compare",
        hand.len()
    );
    for p in hand {
        println!(
            "{} {} {}: auto port {}{}; {}",
            &p.id[..8],
            p.category,
            p.hand.as_deref().unwrap_or(""),
            match p.outcome.class {
                Class::Ported => "passes every check",
                Class::Already => "finds nothing to do",
                Class::Review => "needs review",
                Class::Untestable => "can't be tested",
            },
            if p.outcome.reason.is_empty() {
                String::new()
            } else {
                format!(" ({})", p.outcome.reason)
            },
            p.against_hand.as_deref().unwrap_or("")
        );
        for line in &p.outcome.details {
            println!("    {line}");
        }
    }
    print_measures(results);
}

/// Prints how many outputs of the builds `auto` ported and of those
/// `hand` ported by hand had their driver promoted, how many were tapped,
/// and in how many builds promoting fell back to tapping.
fn print_counts(auto: &[&Ported], hand: &[&Ported]) {
    for (which, ported) in [("", auto), (", of those ported by hand", hand)] {
        let outputs = |of: fn(&Outcome) -> usize| {
            let builds = ported.iter().filter(|p| of(&p.outcome) > 0).count();
            (ported.iter().map(|p| of(&p.outcome)).sum::<usize>(), builds)
        };
        let (promoted, promoting) = outputs(|o| o.promoted);
        let (tapped, tapping) = outputs(|o| o.tapped);
        println!(
            "    outputs promoted{which}: {promoted} in {promoting} builds; \
             tapped: {tapped} in {tapping} builds; promoting fell back to \
             tapping in {} builds",
            ported
                .iter()
                .filter(|p| p.outcome.fell_back.is_some())
                .count()
        );
    }
}

/// Prints how each build that stacks stacked, original and port, the
/// comparators with air behind taken out of each, and each port's weakest
/// strengths arriving at its taps, the weakest first.
fn print_measures(results: &[Ported]) {
    let stacked: Vec<&Ported> = results
        .iter()
        .filter(|p| p.outcome.stacked.is_some())
        .collect();
    println!();
    println!(
        "== STACK CHECKS OF PORTS OF BUILDS THAT STACK ({})",
        stacked.len()
    );
    for p in stacked {
        println!(
            "{} {}: {}{}",
            &p.id[..8],
            p.category,
            p.outcome.stacked.as_deref().unwrap_or(""),
            if p.hand.is_some() {
                " (ported by hand)"
            } else {
                ""
            }
        );
    }
    println!();
    println!(
        "== COMPARATORS WITH AIR BEHIND REMOVED, from builds that take no \
         signal-strength inputs"
    );
    for p in results.iter().filter(|p| !p.outcome.removed.is_empty()) {
        println!(
            "{} {}: {}",
            &p.id[..8],
            p.category,
            p.outcome.removed.join("; ")
        );
    }
    print_sizes(results);
    println!();
    println!("== SUPPORTS OF WHITE STAINED GLASS, over a torch or dust");
    for p in results {
        for line in p.said.lines().filter(|l| l.contains("glass over")) {
            println!("{} {}: {}", &p.id[..8], p.category, line.trim());
        }
    }
    let weakest = |p: &Ported| {
        p.outcome
            .min_ss
            .iter()
            .flat_map(|line| {
                line.split(' ').filter_map(|n| n.parse::<u8>().ok())
            })
            .filter(|&n| n > 0)
            .min()
    };
    let mut measured: Vec<(u8, &Ported)> = results
        .iter()
        .filter(|p| p.outcome.class == Class::Ported)
        .filter_map(|p| weakest(p).map(|n| (n, p)))
        .collect();
    measured.sort_by_key(|&(n, p)| (n, &p.id));
    println!();
    println!(
        "== WEAKEST STRENGTH ARRIVING AT THE TAPS OF EACH PORT ({}), least \
         significant bit first, 0 a bit never on",
        measured.len()
    );
    for (n, p) in measured {
        println!(
            "{n:>2}  {} {}: {}",
            &p.id[..8],
            p.category,
            p.outcome.min_ss.join("; ")
        );
    }
}

/// Prints each port that measures other than its stored build, and each
/// build whose promoted port fell back to tapping, and why.
fn print_sizes(results: &[Ported]) {
    let sized: Vec<&Ported> = results
        .iter()
        .filter(|p| {
            p.outcome.measured.is_some() && p.outcome.stored_size.is_some()
        })
        .collect();
    let grown: Vec<&&Ported> = sized
        .iter()
        .filter(|p| p.outcome.measured != p.outcome.stored_size)
        .collect();
    println!();
    println!(
        "== PORTS MEASURING OTHER THAN STORED ({} of {} measured)",
        grown.len(),
        sized.len()
    );
    for p in grown {
        let size = |s: Option<[i64; 3]>| {
            s.map_or_else(String::new, |[x, y, z]| format!("{x} by {y} by {z}"))
        };
        println!(
            "{} {}: measures {}, stored {}{}; {} promoted, {} tapped{}",
            &p.id[..8],
            p.category,
            size(p.outcome.measured),
            size(p.outcome.stored_size),
            if p.hand.is_some() {
                " (ported by hand)"
            } else {
                ""
            },
            p.outcome.promoted,
            p.outcome.tapped,
            p.outcome
                .fell_back
                .as_ref()
                .map_or_else(String::new, |why| {
                    format!(", promoting failed: {why}")
                })
        );
    }
    println!();
    println!("== PROMOTING FELL BACK TO TAPPING");
    for p in results.iter().filter(|p| p.outcome.fell_back.is_some()) {
        println!(
            "{} {}: {}",
            &p.id[..8],
            p.category,
            p.outcome.fell_back.as_deref().unwrap_or("")
        );
    }
}

/// Prints one build to review: its id, category, and name, its details,
/// and the sides its outputs were tapped on.
fn print_one(p: &Ported) {
    println!("{} {}  {}", &p.id[..8], p.category, p.name);
    for line in &p.outcome.details {
        println!("    {line}");
    }
    for line in p
        .said
        .lines()
        .filter(|l| l.contains(" outputs: ") || l.starts_with("  stacked "))
    {
        println!("    {}", line.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_ports_are_found_by_their_ids_eight_characters() {
        let dir = std::env::temp_dir()
            .join(format!("rsdb-hand-ports-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for name in ["adder_3cb61be7.schem", "NEEDS-FIX_adder_54926989.schem"] {
            fs::write(dir.join(name), b"").unwrap();
        }
        fs::write(dir.join("notes.txt"), b"").unwrap();
        let found = hand_ports(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found["54926989"], "NEEDS-FIX_adder_54926989.schem");
    }

    /// A build the owner dropped, its port in the hand ports' `dropped`
    /// folder, is left unported, as is one named to skip.
    #[test]
    fn a_dropped_build_is_left_unported() {
        let dir = std::env::temp_dir()
            .join(format!("rsdb-dropped-{}", std::process::id()));
        fs::create_dir_all(dir.join("dropped")).unwrap();
        fs::write(dir.join("adder_3cb61be7.schem"), b"").unwrap();
        fs::write(dir.join("dropped/adder_066515d8.schem"), b"").unwrap();
        let left = left_unported(&["9c1c".to_owned()], Some(&dir)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left, ["066515d8", "9c1c"]);
        assert!(left_unported(&[], None).unwrap().is_empty());
    }

    /// A one-bit XOR gate with `extra` put out west of it, on stone.
    fn xor_with(extra: &[(Pos, &str)]) -> Vec<u8> {
        let data = std::fs::read("tests/fixtures/old/xor-1.schem").unwrap();
        let changes: BTreeMap<Pos, String> = extra
            .iter()
            .flat_map(|&(at, state)| {
                [
                    (at, format!("minecraft:{state}")),
                    (
                        schem::step(at, Direction::Down),
                        "minecraft:stone".into(),
                    ),
                ]
            })
            .collect();
        standardize::edit_grown(&data, &changes).unwrap().0
    }

    const DEAD: &str = "comparator[facing=west,mode=compare,powered=false]";

    /// A comparator with air behind in a build that takes no
    /// signal-strength inputs drives nothing, so is taken out, and the
    /// port, which the proposed rule then reads no input in, checks as the
    /// original; but one whose going would leave a comparator that gave out
    /// into its side giving out into air, an output to the proposed rule,
    /// is kept, and the build is to review.
    #[test]
    fn a_comparator_with_air_behind_is_taken_out_where_nothing_changes() {
        let data = xor_with(&[([-3, 1, 0], DEAD), ([-2, 1, 0], "stone")]);
        let blocks = Blocks::read(&data).unwrap();
        assert_eq!(repeater_outputs::strength_inputs(&blocks).len(), 1);
        let conversion =
            convert(&data, (false, &[]), Some("xor_gate"), None).unwrap();
        assert_eq!(
            conversion.outcome.class,
            Class::Ported,
            "{}",
            conversion.said
        );
        assert_eq!(conversion.outcome.removed.len(), 1);
        assert!(conversion.said.contains("removed: comparator with air"));
        let port = Blocks::read(&conversion.file.unwrap()).unwrap();
        assert!(repeater_outputs::strength_inputs(&port).is_empty());

        let side = "comparator[facing=south,mode=compare,powered=false]";
        let data = xor_with(&[
            ([-3, 1, 0], DEAD),
            ([-2, 1, 0], "stone"),
            ([-3, 1, 1], side),
            ([-3, 1, 2], "barrel[facing=up,open=false]"),
        ]);
        let conversion =
            convert(&data, (false, &[]), Some("xor_gate"), None).unwrap();
        assert_eq!(
            conversion.outcome.class,
            Class::Review,
            "{}",
            conversion.said
        );
        assert!(conversion.outcome.removed.is_empty());
        assert!(
            conversion.outcome.details[0]
                .contains("reads an output without it"),
            "{:?}",
            conversion.outcome.details
        );
    }

    /// A hex adder takes signal-strength inputs, so a comparator with air
    /// behind in one is left for the proposed rule to read as an input.
    #[test]
    fn a_build_taking_digits_keeps_its_comparators_with_air_behind() {
        assert!(Kind::HexAdder.digit_inputs());
        assert!(Kind::HexToBinary.digit_inputs());
        assert!(!Kind::Multiplier.digit_inputs());
        let data = xor_with(&[([-3, 1, 0], DEAD), ([-2, 1, 0], "stone")]);
        let conversion =
            convert(&data, (false, &[]), Some("hex_adder"), None).unwrap();
        assert!(conversion.outcome.removed.is_empty());
        assert!(!conversion.said.contains("comparators with air behind,"));
    }

    #[test]
    fn reasons_group_why_an_output_was_not_tapped() {
        assert_eq!(
            not_reason("every side is blocked: north: stone at (1, 2, 3)"),
            "not tapped: every side blocked"
        );
        assert!(
            not_reason("its repeater gives out into stone")
                .contains("repeater")
        );
        assert!(
            not_reason("lever on it, not dust").contains("other than dust")
        );
    }
}
