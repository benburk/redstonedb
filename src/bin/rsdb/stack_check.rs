//! Copies of a build stacked as the site's stacking test stacks them, the
//! first whole and the rest the box, each way asked, read by the rule
//! proposed for outputs, see [`repeater_outputs`]: where copies meet, what
//! a copy leaves behind, and whether the stack passes the site's own test
//! of its kind's copies, at the step asked. Nothing here writes to the
//! database.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Result;
use app::{
    category::Kind,
    models::schematic::{Faces, Orientation, Schematic},
    recognize::{
        interface,
        probe::{self, Layout},
        repeater_outputs::{self, Output},
        roles,
    },
    schem::{
        self, Blocks, Direction, Pos,
        rstack::{self, Way},
        stack,
    },
    simulate::{self, Circuit, Step, Test},
};
use clap::Args;
use sqlx::SqlitePool;

use super::{
    load,
    outputs::{describe, kind_of, place, source},
    resolve,
};

#[derive(Args)]
pub(super) struct StackCheck {
    /// A .schem file, or a stored build's id or the start of one
    file_or_id: String,
    /// Copies in all, the original among them
    #[arg(long, default_value_t = 3)]
    copies: usize,
    /// The ways to stack, of west, east, up, north, and south; a stored
    /// build's own and their opposites by default, else west, east, and up
    #[arg(long, value_delimiter = ',')]
    dirs: Vec<String>,
    /// Blocks between copies; a stored build's own along that axis by
    /// default, else each of 1 to 4
    #[arg(long)]
    spacing: Option<i64>,
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

/// The one copy, as the stacks are made and read from it.
struct Single {
    /// The file with its roles coloured in as probing found them, or as
    /// the closest adder layout had them, which the site's stacking test
    /// reads; else as it came, made simulatable.
    coloured: Vec<u8>,
    /// Its blocks as they came, made simulatable.
    blocks: Blocks,
    /// Its outputs by the proposed rule.
    outputs: Vec<Output>,
    kind: Option<Kind>,
    /// For an adder, its roles.
    layout: Option<Layout>,
    /// How its copies share their buses.
    buses: stack::Buses,
}

/// `//rstack`s a build each way asked as the site stacks it, settles each
/// stack, and tests it as the site tests its kind's copies.
pub(super) async fn stack_check_command(
    db: &SqlitePool,
    args: &StackCheck,
) -> Result<()> {
    let source = source(db, &args.file_or_id).await?;
    let like = match &args.like {
        Some(id) => Some(load(db, &resolve(db, id).await?).await?),
        None => source.stored.clone(),
    };
    let quiet = args.one;
    let say = |line: String| {
        if !quiet {
            println!("{line}");
        }
    };
    say(source.name.clone());
    let stored = stored_ways(like.as_ref());
    let dirs: Vec<String> = if !args.dirs.is_empty() {
        args.dirs.clone()
    } else if stored.is_empty() {
        ["west", "east", "up"].map(str::to_owned).to_vec()
    } else {
        let mut dirs: Vec<String> = Vec::new();
        for &(way, _) in &stored {
            // Copies can't hang below the first, so up has no opposite.
            let back = (way != "up").then(|| opposite(way));
            for name in std::iter::once(way).chain(back) {
                if !dirs.iter().any(|d| d == name) {
                    dirs.push(name.to_owned());
                }
            }
        }
        dirs
    };
    if !stored.is_empty() {
        let ways: Vec<String> =
            stored.iter().map(|(w, n)| format!("{w} {n}")).collect();
        say(format!("stored stacking: {}", ways.join(", ")));
    }
    let meant = kind_of(args.category.as_deref(), like.as_ref());
    let single = read_single(&source.data, meant, like.as_ref(), quiet)?;
    let test = single.kind.and_then(Kind::test);
    // The chained output, a carry out, and the way the rule predicts.
    let chained = chained(&single);
    let predicted = chained.map(|o| o.facing.name());
    if !quiet {
        match chained {
            Some(output) => println!(
                "  chained output: carry out, {}; predicted clean way: {}",
                describe(&single.blocks, output),
                output.facing
            ),
            None => println!("  chained output: none found, so no prediction"),
        }
        match test.and_then(|test| test.stacking(&single.coloured)) {
            Some(found) => println!("  site's own stacking: {}", words(&found)),
            None => println!("  site's own stacking: none for its kind"),
        }
    }
    let mut passing: Vec<String> = Vec::new();
    let mut crashed: Vec<String> = Vec::new();
    for dir in &dirs {
        let Ok(way) = Way::named(dir) else {
            println!(
                "{dir}: skipped, as the site stacks west, up, and north, and \
                 east and south as a mirror image"
            );
            continue;
        };
        let axis_stored = stored.iter().find(|(name, _)| {
            *name == way.name() || opposite(name) == way.name()
        });
        let given = args.spacing.or(axis_stored.map(|&(_, n)| n));
        let spacings: Vec<i64> = given.map_or((1..=4).collect(), |n| vec![n]);
        for spacing in spacings {
            let label = format!("{dir} {spacing}");
            if args.one {
                let passed =
                    stack_once(&single, way, spacing, args.copies, &label)?;
                std::process::exit(if passed { 0 } else { 3 });
            }
            match run_apart(args, single.kind, dir, spacing)? {
                Ran::Passed => passing.push(label),
                Ran::Failed => {}
                Ran::Crashed => crashed.push(label),
            }
        }
    }
    print_verdict(&passing, &crashed, predicted, &dirs);
    Ok(())
}

/// Each way a stored build stacks, by name, with its spacing.
pub(super) fn stored_ways(
    like: Option<&Schematic>,
) -> Vec<(&'static str, i64)> {
    like.iter()
        .flat_map(|s| &s.stacking)
        .flat_map(|d| {
            [(d.west, "west"), (d.up, "up"), (d.north, "north")]
                .into_iter()
                .filter(|&(n, _)| n > 0)
                .map(|(n, way)| (way, n))
                .collect::<Vec<_>>()
        })
        .collect()
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

/// The one copy in `data`, probed with its proposed-rule outputs as
/// `meant`, or as each kind probing tries, printing what was found unless
/// `quiet`; `like`, a stored build it was made from, gives the way its
/// bits run.
fn read_single(
    data: &[u8],
    meant: Option<Kind>,
    like: Option<&Schematic>,
    quiet: bool,
) -> Result<Single> {
    let simulated = probe::simulatable(data)?;
    let blocks = Blocks::read(&simulated)?;
    let outputs = repeater_outputs::find(&blocks);
    let say = |line: String| {
        if !quiet {
            println!("{line}");
        }
    };
    say(format!(
        "one copy: {} outputs by the proposed rule, {} levers and buttons",
        outputs.len(),
        blocks.switches().len()
    ));
    let (kind, layout, coloured) = match repeater_outputs::probe(data, meant)? {
        Ok(probed) => {
            say(format!("  probed: {}", probed.found.2));
            if let Some(layout) = &probed.layout {
                say(format!("  {}", layout.describe()));
            }
            (Some(probed.kind), probed.layout, probed.found.1)
        }
        Err(faults) => {
            say("  probed: nothing holds, a fault of the single build"
                .to_owned());
            for (kind, fault) in faults.iter().take(3) {
                say(format!("    {}: {fault}", kind.name()));
            }
            // The adder's closest layout, as its fault's file colours it,
            // so its copies can still be tried.
            let closest = faults
                .iter()
                .find(|(kind, _)| *kind == Kind::Adder)
                .and_then(|(_, fault)| fault.file());
            match closest.map(|file| (coloured_layout(file), file)) {
                Some((Ok(layout), file)) => {
                    say(format!(
                        "  closest adder layout: {}",
                        layout.describe()
                    ));
                    (meant.or(Some(Kind::Adder)), Some(layout), file.to_vec())
                }
                _ => (meant, None, simulated.clone()),
            }
        }
    };
    let buses = match kind {
        Some(Kind::Adder | Kind::Subtractor) | None => stack::Buses::own(),
        Some(kind) => stack::Buses::of(
            kind,
            like.map_or(Orientation::Horizontal, |s| s.orientation),
        ),
    };
    Ok(Single {
        coloured,
        blocks,
        outputs,
        kind,
        layout,
        buses,
    })
}

/// The single copy's chained output: its carry out, when that is a
/// repeater or comparator powering nothing.
fn chained(single: &Single) -> Option<Output> {
    let block = single.layout.as_ref()?.carry_out?;
    single.outputs.iter().copied().find(|o| o.block() == block)
}

/// What the site's stacking test found, in a line.
fn words(found: &simulate::Stacking) -> String {
    let step = |step: &Step| {
        let faces = step.faces.map_or("the box rule's box".to_owned(), |f| {
            format!("a box leaving out {}", f.describe())
        });
        format!("{}, {faces}", step.describe())
    };
    match found {
        simulate::Stacking::Stacks {
            step: s,
            copies,
            bits,
            cases,
        } => format!(
            "stacks every {}: {copies} copies, {bits} bits, {cases} cases right",
            step(s)
        ),
        simulate::Stacking::Registers {
            step: s, copies, ..
        }
        | simulate::Stacking::Wider {
            step: s, copies, ..
        } => {
            format!("stacks every {}, {copies} copies", step(s))
        }
        simulate::Stacking::Separate { ways } => {
            let each: Vec<String> = ways
                .iter()
                .map(|apart| {
                    format!(
                        "every {}, {} copies",
                        step(&apart.step),
                        apart.copies
                    )
                })
                .collect();
            format!("copies side by side, {}", each.join("; "))
        }
        simulate::Stacking::Not { step: s, reason } => format!(
            "doesn't stack{}: {reason}",
            s.as_ref()
                .map_or(String::new(), |s| format!(" every {}", step(s)))
        ),
    }
}

/// Runs one way and spacing of `args` in a process of its own, held to
/// `STACK_MEMORY_KIB`, printing what it prints of that stack.
fn run_apart(
    args: &StackCheck,
    kind: Option<Kind>,
    dir: &str,
    spacing: i64,
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
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(match output.status.code() {
        Some(0) => Ran::Passed,
        Some(3) => Ran::Failed,
        _ => {
            let err = String::from_utf8_lossy(&output.stderr);
            let last = err.lines().last().unwrap_or("").to_owned();
            println!("  CRASHED ({}): {last}", output.status);
            Ran::Crashed
        }
    })
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
    if !crashed.is_empty() {
        println!("crashed: {}", crashed.join(", "));
    }
    let Some(predicted) = predicted else {
        return;
    };
    if !dirs.iter().any(|d| d == predicted) {
        println!(
            "predicted {predicted}: not among the ways tried, the chained \
             output pointing across them"
        );
        return;
    }
    let matches = passing.iter().any(|p| p.starts_with(predicted));
    let others: Vec<&str> = passing
        .iter()
        .filter(|p| !p.starts_with(predicted))
        .map(String::as_str)
        .collect();
    println!(
        "predicted {predicted}: {}{}",
        if matches { "passes" } else { "does not pass" },
        if others.is_empty() {
            String::new()
        } else {
            format!("; also passing {}", others.join(", "))
        }
    );
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

/// The box a stack used, in words.
fn box_words(faces: Option<Faces>) -> String {
    faces.map_or("the box rule's box".to_owned(), |faces| {
        format!("a box leaving out {}", faces.describe())
    })
}

/// One stack of `single`, `copies` a `spacing` apart `way`, as the site's
/// stacking test stacks it: prints the box it used, what meeting copies
/// did, what the site's test of its kind's copies found, and for an adder
/// that fails, what happens to the carry at the seam; whether it passed.
fn stack_once(
    single: &Single,
    way: Way,
    spacing: i64,
    copies: usize,
    label: &str,
) -> Result<bool> {
    println!("{label}, {copies} copies:");
    let direction = way.direction(spacing, None);
    let step = Step::new(direction.west, direction.up, direction.north);
    let lying = way.lying(&single.coloured)?;
    let test = single.kind.and_then(Kind::test);
    // The site's own test of copies at this step: the box rule's box, then
    // boxes leaving out more of the first copy.
    let site = test.and_then(|test| test.stacks_at(&lying, step, copies));
    let faces = match &site {
        Some(Ok((found, _))) => found.faces.map(|f| way.flip_faces(f)),
        _ => None,
    };
    let stacked = rstack::stack_as_site(
        &single.coloured,
        way,
        spacing,
        copies,
        single.buses,
        faces,
    )?;
    print_box(single, &stacked, faces);
    let raw = Blocks::read(&stacked.data)?;
    print_seams(&stacked);
    print_outputs_met(&stacked, &raw, &single.outputs);
    print_orphans(&stacked, &raw, &single.blocks);
    let settled = match rstack::settle(&stacked.data) {
        Ok(settled) => settled,
        Err(err) => {
            println!("  doesn't settle: {err}");
            println!("  FAILS");
            return Ok(false);
        }
    };
    print_prunable(&stacked, &settled)?;
    let after = Blocks::read(&probe::simulatable(&settled)?)?;
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
    let mut passed = match &site {
        Some(Ok((_, cases))) => {
            println!(
                "  site's stacking test: passes, {cases} cases right, {}",
                box_words(faces)
            );
            true
        }
        Some(Err(reason)) => {
            println!("  site's stacking test: fails, {reason}");
            if way.mirrored() {
                println!(
                    "    (run on the mirror image stacked {}, whose bits the \
                     site reads from its south-east corner, the other end \
                     from this stack's)",
                    if way == Way::East { "west" } else { "north" }
                );
            }
            false
        }
        None => {
            println!(
                "  site's stacking test: none at a given step for {}",
                single.kind.map_or("its kind", Kind::name)
            );
            false
        }
    };
    if let Some(layout) = &single.layout {
        // The adder's own test through the one copy's roles, either end
        // lowest, which reads a mirrored stack the way it lies.
        let right = print_by_geometry(&after, &stacked, layout, copies);
        passed |= right;
        if !right {
            seam(single, layout, &stacked, (&raw, &after))?;
        }
    }
    println!("  {}", if passed { "PASSES" } else { "FAILS" });
    Ok(passed)
}

/// Prints the box later copies repeated, and what only the first copy
/// keeps: how many blocks, and its levers and buttons.
fn print_box(single: &Single, stacked: &rstack::Stacked, faces: Option<Faces>) {
    let switches: Vec<String> = single
        .blocks
        .switches()
        .iter()
        .filter(|s| stacked.first_only.contains(&s.pos))
        .map(|s| {
            format!(
                "{} on {} {}",
                schem::short(single.blocks.get(s.pos)).replace('_', " "),
                schem::short(single.blocks.get(s.block)).replace('_', " "),
                place(s.block)
            )
        })
        .collect();
    println!(
        "  box: {}; the first copy alone keeps {} blocks{}",
        box_words(faces),
        stacked.first_only.len(),
        if switches.is_empty() {
            String::new()
        } else {
            format!(", among them {}", switches.join("; "))
        }
    );
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
    for copy in 0..stacked.copies {
        for output in outputs {
            if copy > 0 && stacked.first_only.contains(&output.at) {
                continue;
            }
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
    let mut orphans = Vec::new();
    for copy in 0..stacked.copies {
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
    match roles::prune(settled, &[], stack::Buses::own()) {
        Ok(None) => println!("  prunable: 0"),
        Ok(Some(pruned)) => {
            let blocks = Blocks::read(settled)?;
            let gone: Vec<String> = pruned
                .gone
                .iter()
                .map(|&pos| {
                    format!(
                        "{} {} from copy {}",
                        place(pos),
                        schem::short(blocks.get(pos)).replace('_', " "),
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
    match by_geometry(after, stacked, layout, copies) {
        Ok((order, run, used)) => {
            // Levers the stack's roles leave out, as a later copy's carry
            // in left inside the stack.
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
            right
        }
        Err(why) => {
            println!("  by the one copy's roles: {why}");
            false
        }
    }
}

/// The stack tested as an adder whose every copy plays the roles the one
/// copy's `layout` gives it, its bits one copy after another, the carry in
/// the lowest copy's and the carry out the highest's, with either end
/// lowest: the end that does better and its run, and the input blocks it
/// drove; or why neither can run.
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
        let wide = wide_layout(stacked, layout, &order);
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

/// Each block's power in the single copy, A and B off, with its carry in
/// off and on.
type CarryPower = [HashMap<Pos, u8>; 2];

/// The power of every block of `blocks`, the one copy, A and B off and
/// its carry in each way, through `layout`.
fn carry_power(blocks: &Blocks, layout: &Layout) -> Result<CarryPower> {
    let interface = layout.interface(blocks);
    let mut circuit = Circuit::new(blocks, &interface)?;
    let mut each = [HashMap::new(), HashMap::new()];
    for (carry, power) in each.iter_mut().enumerate() {
        circuit.set(0, 0);
        circuit.set(1, 0);
        circuit.set(2, u64::try_from(carry)?);
        circuit.settle()?;
        for (pos, state) in blocks.iter() {
            if state != schem::AIR {
                power.insert(pos, circuit.power(pos));
            }
        }
    }
    Ok(each)
}

/// What a block does with the carry in, in a lone copy, in words.
fn follows(power: &CarryPower, pos: Pos) -> &'static str {
    let at = |carry: usize| power[carry].get(&pos).copied().unwrap_or(0) > 0;
    match (at(0), at(1)) {
        (false, true) => "on with a carry in, as the carry does",
        (true, false) => "on with no carry in, the carry inverted",
        (true, true) => "on either way",
        (false, false) => "off either way",
    }
}

/// Prints what happens to the carry at the first seam it crosses: the
/// first copy whose carry out a later copy covers, or whose carry out
/// powers another copy's block; that block, and how it follows the carry
/// in a lone copy, so whether the carry keeps its polarity there and which
/// block would need to change.
fn seam(
    single: &Single,
    layout: &Layout,
    stacked: &rstack::Stacked,
    (raw, after): (&Blocks, &Blocks),
) -> Result<()> {
    let Some(carry_out) = chained(single) else {
        println!("  seam: the carry out is no repeater or comparator output");
        return Ok(());
    };
    let Some(carry_in) = layout.carry_in else {
        println!("  seam: no carry in");
        return Ok(());
    };
    let power = carry_power(&single.blocks, layout)?;
    println!(
        "  seam: the lone copy's carry in {} drives: {}",
        place(carry_in),
        path(&single.blocks, &power, carry_in)
    );
    let local = |pos: Pos, copy: usize| -> Pos {
        let start = stacked.place(copy, [0, 0, 0]);
        [0, 1, 2].map(|axis| pos[axis] - start[axis])
    };
    let block =
        |pos: Pos| schem::short(single.blocks.get(pos)).replace('_', " ");
    let behind = carry_out.behind();
    for copy in 0..stacked.copies {
        let at = stacked.place(copy, carry_out.at);
        let now = raw.get(at);
        let by = stacked.copy_of.get(&at).copied().unwrap_or(copy);
        // Covered: the carry now leaves through the covering block, fed by
        // what fed the repeater, on with a carry.
        let (meets, how) = if by == copy
            && schem::short(now) == carry_out.kind()
            && Direction::facing(now) == Some(carry_out.facing)
        {
            let front = schem::step(at, carry_out.points());
            match stacked.copy_of.get(&front) {
                Some(&k) if k != copy => (Some((front, k)), "powers"),
                _ => (None, ""),
            }
        } else {
            (Some((at, by)), "is covered by")
        };
        let Some((cell, k)) = meets else {
            continue;
        };
        let mine = local(cell, k);
        let landing = follows(&power, mine);
        let verdict = match landing {
            "on with a carry in, as the carry does" => {
                "the carry keeps its polarity across the seam, so the \
                 fault is not here"
                    .to_owned()
            }
            "on with no carry in, the carry inverted" if how == "powers" => {
                format!(
                    "the carry changes polarity at the seam: copy {copy}'s \
                     repeater turns it on for a carry, where the tile's own \
                     carry in turns it on for none; the carry should land \
                     past the inverting torch on the carry in's path, or \
                     leave through a torch, so {} must change",
                    place(mine)
                )
            }
            "on with no carry in, the carry inverted" => {
                "it inverts the carry in, as a torch on the block that fed \
                 the repeater inverts the carry, so the polarity holds"
                    .to_owned()
            }
            _ => format!(
                "{} is not on the tile's carry in path in a lone copy, so \
                 the carry goes nowhere the next copy reads",
                place(mine)
            ),
        };
        println!(
            "  seam: copy {copy}'s carry out {} at {} (fed from its {}) {how} \
             copy {k}'s {} at its {}, {landing} in a lone copy: {verdict}",
            carry_out.kind(),
            place(at),
            place(behind),
            block(mine),
            place(mine)
        );
        return carry_across(
            after,
            stacked,
            layout,
            &power,
            &single.blocks,
            (copy, k),
        );
    }
    println!(
        "  seam: no copy's carry out meets another copy: each gives out into \
         air, so the carry never crosses a seam"
    );
    Ok(())
}

/// The blocks near the carry in at `start` in the lone copy whose power
/// follows the carry in, each with how, as far as six blocks along: the
/// carry in's path, where an inverting torch shows.
fn path(blocks: &Blocks, power: &CarryPower, start: Pos) -> String {
    let found: Vec<String> = carry_path(power, start)
        .into_iter()
        .map(|pos| {
            let on = power[1].get(&pos).copied().unwrap_or(0) > 0;
            format!(
                "{} {}{}",
                place(pos),
                schem::short(blocks.get(pos)).replace('_', " "),
                if on { "" } else { " (inverted)" }
            )
        })
        .collect();
    if found.is_empty() {
        "nothing that follows it".to_owned()
    } else {
        found.join("; ")
    }
}

/// The blocks whose power follows the carry in at `start` in the lone
/// copy, nearest first, as far as six blocks along.
fn carry_path(power: &CarryPower, start: Pos) -> Vec<Pos> {
    let changes = |pos: &Pos| {
        let at = |carry: usize| power[carry].get(pos).copied().unwrap_or(0) > 0;
        at(0) != at(1)
    };
    let mut seen: BTreeSet<Pos> = BTreeSet::from([start]);
    let mut frontier = vec![start];
    let mut found = Vec::new();
    for _ in 0..6 {
        let mut next = Vec::new();
        for pos in frontier {
            for side in Direction::ALL {
                let near = schem::step(pos, side);
                if seen.insert(near) && changes(&near) {
                    found.push(near);
                    next.push(near);
                }
            }
        }
        frontier = next;
    }
    found
}

/// The stack's adder layout with every copy playing the one copy's
/// `layout` roles, its bits copy by copy in `order`, lowest first, the
/// carry in the lowest copy's and the carry out the highest's.
fn wide_layout(
    stacked: &rstack::Stacked,
    layout: &Layout,
    order: &[usize],
) -> Layout {
    let each = |bits: &[Pos]| -> Vec<Pos> {
        order
            .iter()
            .flat_map(|&c| bits.iter().map(move |&p| stacked.place(c, p)))
            .collect()
    };
    Layout {
        a: each(&layout.a),
        b: each(&layout.b),
        carry_in: layout.carry_in.map(|p| stacked.place(order[0], p)),
        sum: each(&layout.sum),
        carry_out: order
            .last()
            .and_then(|&c| layout.carry_out.map(|p| stacked.place(c, p))),
    }
}

/// Prints how copy `to`'s carry in path follows a carry from copy `from`
/// in the stack, `after`, against how it follows its own carry in in a
/// lone copy, `power`: the first block on it that differs, where the
/// carry crossing the seam turns the other way, or that none does.
#[allow(clippy::too_many_arguments)]
fn carry_across(
    after: &Blocks,
    stacked: &rstack::Stacked,
    layout: &Layout,
    power: &CarryPower,
    lone: &Blocks,
    (from, to): (usize, usize),
) -> Result<()> {
    let Some(carry_in) = layout.carry_in else {
        return Ok(());
    };
    let order: Vec<usize> = if to > from {
        (0..stacked.copies).collect()
    } else {
        (0..stacked.copies).rev().collect()
    };
    let wide = wide_layout(stacked, layout, &order);
    let switched: HashSet<Pos> =
        after.switches().iter().map(|s| s.block).collect();
    if wide
        .a
        .iter()
        .chain(&wide.b)
        .chain(&wide.carry_in)
        .any(|p| !switched.contains(p))
    {
        println!("  seam: an input lost its lever, so the carry isn't traced");
        return Ok(());
    }
    let mut circuit = Circuit::new(after, &wide.interface(after))?;
    let width = layout.a.len();
    let at = order.iter().position(|&c| c == from).unwrap_or(0);
    let ones = ((1u64 << width) - 1) << (at * width);
    // The path as later copies repeat it: not a lever, nor what only the
    // first copy keeps.
    let cells: Vec<Pos> = carry_path(power, carry_in)
        .into_iter()
        .filter(|q| {
            !stacked.first_only.contains(q)
                && schem::short(lone.get(*q)) != "lever"
        })
        .collect();
    let mut stack_on: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
    for (carry, on) in stack_on.iter_mut().enumerate() {
        let value = if carry == 1 { ones } else { 0 };
        circuit.set(0, value);
        circuit.set(1, value);
        circuit.set(2, 0);
        circuit.settle()?;
        *on = cells
            .iter()
            .map(|&q| circuit.power(stacked.place(to, q)) > 0)
            .collect();
    }
    let lone_on =
        |carry: usize, q: Pos| power[carry].get(&q).copied().unwrap_or(0) > 0;
    let differs = cells.iter().enumerate().find(|&(i, &q)| {
        stack_on[0][i] != lone_on(0, q) || stack_on[1][i] != lone_on(1, q)
    });
    match differs {
        None => println!(
            "  seam: copy {to}'s carry in path follows copy {from}'s carry \
             as a lone copy's follows its carry in, block for block"
        ),
        Some((i, &q)) => {
            let word = |on: bool| if on { "on" } else { "off" };
            let inverted = stack_on[0][i] == lone_on(1, q)
                && stack_on[1][i] == lone_on(0, q);
            println!(
                "  seam: copy {to}'s {} at its {}, on the carry in path, is \
                 {} with no carry from copy {from} and {} with one, where a \
                 lone copy's is {} and {}{}",
                schem::short(lone.get(q)).replace('_', " "),
                place(q),
                word(stack_on[0][i]),
                word(stack_on[1][i]),
                word(lone_on(0, q)),
                word(lone_on(1, q)),
                if inverted {
                    ": the carry crosses the seam inverted here, so this \
                     block, or what drives it from the copy before, must \
                     change"
                } else {
                    ": the carry doesn't reach it as the carry in did"
                }
            );
        }
    }
    Ok(())
}
