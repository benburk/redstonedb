//! The check of a build by its function rather than its colours: its
//! inputs are its levers and buttons, found wherever they are, and
//! its outputs lone coloured blocks, each read through the dust or
//! repeater on it. What the site can put right itself, it does and lists;
//! what only the builder can, it names with the blocks at fault.
//!
//! The rules:
//!
//! - An input is a lever, or any button for a pulse, on a side of a solid
//!   block, a lamp too, its input block. The lever touches nothing else
//!   that carries power.
//! - The input block drives dust on any side, on top and below too,
//!   torches, repeaters, and comparators, and nothing else powers it or
//!   touches it but plain blocks, a target, and, on top, a redstone block,
//!   none of which power a solid block. One that drives dust is hard: a bus
//!   must drive it through a repeater.
//! - The lever's spot takes a repeater pointed into the input block, so a
//!   bus can drive the input: nothing below it but air or a solid block,
//!   and nothing behind it; and the build compiled with every lever so
//!   swapped is the same circuit, the proof by the rules of redstone.
//! - Every output, data or status, is a block with dust, a repeater, or a
//!   comparator on top, which is what is read, but a repeater or comparator
//!   powering a block in front of it, which carries the circuit on; and no
//!   block of its kind touching it, even diagonally, a kind marking outputs
//!   only where every block of it does: of an upload, any full block that
//!   carries power but a component, or stained glass of a colour, as plain
//!   and white glass hold a build's dust up; of the site's own files, the
//!   colours it marks outputs with, see [`SITE`].
//! - A bus's bits run in a line, evenly spaced: straight, or rising one
//!   block for each block across.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;

use crate::{
    schem::{self, Blocks, Direction, Pos, Switch, block, standardize},
    simulate,
};

/// What the check found: what it puts right, and what it can't.
#[derive(Default, Debug)]
pub struct Report {
    pub fixes: Vec<String>,
    pub errors: Vec<Problem>,
    /// What else it found, such as which inputs need strong power.
    pub notes: Vec<String>,
    /// The inputs and outputs, grouped into buses and lone lines.
    pub inputs: Vec<Run>,
    pub outputs: Vec<Run>,
}

/// Something only the builder can put right.
#[derive(Debug, Clone)]
pub struct Problem {
    /// A short name for the rule, to count problems by.
    pub rule: &'static str,
    /// The blocks at fault.
    pub at: Vec<Pos>,
    pub message: String,
}

/// Inputs or outputs in a line, evenly spaced, from one end to the other;
/// a lone line is a run of one.
#[derive(Debug, Clone)]
pub struct Run {
    pub bits: Vec<Pos>,
    /// From each bit to the next; zero for a lone line.
    pub step: Pos,
}

/// An input: the switch, and what its block drives.
struct Input {
    switch: Switch,
    /// Whether its block drives dust, so it needs strong power.
    hard: bool,
}

/// Which kinds of block mark a build's outputs, each only where every
/// block of it is alone with something reading it on top.
#[derive(Clone, Copy, Debug)]
pub enum Marks<'a> {
    /// Any kind that could, as [`could_mark`] says.
    Any,
    /// Only the kinds named, such as `yellow_concrete`.
    Kinds(&'a [&'a str]),
}

impl Marks<'_> {
    /// Whether a block of kind `name` may mark an output.
    fn takes(&self, name: &str) -> bool {
        match self {
            Marks::Any => could_mark(name),
            Marks::Kinds(kinds) => kinds.contains(&name),
        }
    }
}

/// Yellow concrete and yellow stained glass, the blocks a builder marks
/// each output with, its status lines too.
pub const YELLOW: Marks<'static> =
    Marks::Kinds(&["yellow_concrete", "yellow_stained_glass"]);

/// What an uploaded build's outputs are read as: any kind that could mark
/// one, until builders mark them as [`YELLOW`] says.
pub const UPLOAD: Marks<'static> = Marks::Any;

/// The blocks the site marks outputs with in the files it keeps: each
/// output colour and the status colour, see [`colour::is_output`], of
/// concrete or stained glass. Its own files are read back with these.
pub const SITE: Marks<'static> = Marks::Kinds(&[
    "yellow_concrete",
    "yellow_stained_glass",
    "orange_concrete",
    "orange_stained_glass",
    "brown_concrete",
    "brown_stained_glass",
    "pink_concrete",
    "pink_stained_glass",
]);

/// The check of an uploaded build, its outputs marked as [`UPLOAD`] says.
pub fn inspect(blocks: &Blocks) -> Report {
    inspect_marked(blocks, UPLOAD)
}

/// The check of a build whose outputs are marked as `marks` says, such as
/// [`SITE`] for a file the site keeps.
pub fn inspect_marked(blocks: &Blocks, marks: Marks) -> Report {
    let mut report = Report::default();
    let inputs = inputs(blocks, &mut report);
    let outputs = outputs(blocks, marks, &mut report);
    report.inputs = runs(
        &inputs
            .iter()
            .map(|input| {
                (input.switch.block, (input.switch.side, input.switch.button))
            })
            .collect::<Vec<_>>(),
    );
    report.outputs =
        runs(&outputs.iter().map(|&pos| (pos, ())).collect::<Vec<_>>());
    let hard = inputs.iter().filter(|input| input.hard).count();
    if !inputs.is_empty() {
        report.notes.push(format!(
            "{hard} of {} inputs drive dust, so a bus drives them through \
             repeaters",
            inputs.len()
        ));
    }
    report
}

/// The rule of a block behind a lever, where a bus's driver would feed the
/// repeater put in its place: fine on a control line, which a lever or
/// button drives, but not on a data input, which a bus may drive; no fault
/// until probing tells them apart.
pub const BLOCK_BEHIND: &str = "block behind a lever";

/// The rules that are faults only on a data input, each problem's first
/// block the input block.
pub const DATA_ONLY: [&str; 1] = [BLOCK_BEHIND];

impl Report {
    /// Its faults but those only a data input's, which probing tells
    /// apart; see `DATA_ONLY`.
    pub fn faults(&self) -> impl Iterator<Item = &Problem> {
        self.errors
            .iter()
            .filter(|problem| !DATA_ONLY.contains(&problem.rule))
    }

    /// Its problems that are faults only on a data input; see `DATA_ONLY`.
    pub fn data_only(&self) -> impl Iterator<Item = &Problem> {
        self.errors
            .iter()
            .filter(|problem| DATA_ONLY.contains(&problem.rule))
    }

    /// Each fault on a block of `data`, the data inputs probing found, of
    /// a rule only a data input breaks.
    pub fn data_faults(&self, data: &[Pos]) -> Vec<String> {
        self.data_only()
            .filter(|problem| {
                problem.at.first().is_some_and(|block| data.contains(block))
            })
            .map(|problem| format!("{}, on a data input", problem.message))
            .collect()
    }
}

fn problem(
    report: &mut Report,
    rule: &'static str,
    at: Vec<Pos>,
    message: String,
) {
    report.errors.push(Problem { rule, at, message });
}

/// Where a block is, for a message: x, y, z from the file's corner.
pub fn place(pos: Pos) -> String {
    format!("({}, {}, {})", pos[0], pos[1], pos[2])
}

fn short(blocks: &Blocks, pos: Pos) -> &str {
    schem::short(blocks.get(pos))
}

fn words(name: &str) -> String {
    name.replace('_', " ")
}

/// A switch named `name` as a fault's subject: `Lever` or `Button`.
fn noun(name: &str) -> &'static str {
    if name.ends_with("_button") {
        "Button"
    } else {
        "Lever"
    }
}

/// Whether a component at `from` pointing its `facing` points into `to`:
/// a repeater's or comparator's `facing` is the side it reads from.
fn points_into(blocks: &Blocks, from: Pos, to: Pos) -> bool {
    Direction::facing(blocks.get(from))
        .is_some_and(|facing| schem::step(from, facing.opposite()) == to)
}

fn reads_from(blocks: &Blocks, from: Pos, block: Pos) -> bool {
    Direction::facing(blocks.get(from))
        .is_some_and(|facing| schem::step(from, facing) == block)
}

fn inputs(blocks: &Blocks, report: &mut Report) -> Vec<Input> {
    let switches = blocks.switches();
    if switches.is_empty() {
        problem(report, "no inputs", Vec::new(), "No inputs".to_owned());
    }
    let mut found = Vec::new();
    for switch in switches {
        if !placed(blocks, &switch, report) {
            continue;
        }
        lever_touches(blocks, &switch, report);
        let hard = input_block(blocks, &switch, report);
        repeater_room(blocks, &switch, report);
        found.push(Input { switch, hard });
    }
    found
}

/// Whether the switch is placed as an input's: on a side of a solid
/// block.
fn placed(blocks: &Blocks, switch: &Switch, report: &mut Report) -> bool {
    let (lever, block) = (switch.pos, switch.block);
    let name = short(blocks, lever).to_owned();
    let at = place(lever);
    if matches!(switch.side, Direction::Up | Direction::Down) {
        let where_ = if switch.side == Direction::Up {
            "top"
        } else {
            "bottom"
        };
        problem(
            report,
            "switch on top or bottom",
            vec![lever],
            format!("{} at {at}: on {where_} of block", noun(&name)),
        );
        return false;
    }
    let block_name = short(blocks, block).to_owned();
    if !block::holds_input(&block_name) {
        problem(
            report,
            "input block not solid",
            vec![block],
            format!(
                "{} at {at}: on non-solid {}",
                noun(&name),
                words(&block_name)
            ),
        );
        return false;
    }
    true
}

/// What the switch itself touches: it powers dust and components beside
/// it, not only its block.
fn lever_touches(blocks: &Blocks, switch: &Switch, report: &mut Report) {
    let (lever, block) = (switch.pos, switch.block);
    let name = short(blocks, lever).to_owned();
    let at = place(lever);
    // What the lever itself touches: it powers dust and components
    // beside it, not only its block.
    for side in Direction::ALL {
        let next = schem::step(lever, side);
        if next == block {
            continue;
        }
        let touched = short(blocks, next);
        if matches!(
            touched,
            "redstone_wire"
                | "repeater"
                | "comparator"
                | "redstone_lamp"
                | "piston"
                | "sticky_piston"
        ) {
            problem(
                report,
                "switch touches redstone",
                vec![lever, next],
                format!(
                    "{} at {at}: touches {} at {}",
                    noun(&name),
                    words(touched),
                    place(next)
                ),
            );
        }
    }
}

/// What the input block drives and what else touches it; whether it
/// drives dust, so needs strong power.
fn input_block(blocks: &Blocks, switch: &Switch, report: &mut Report) -> bool {
    let (lever, block) = (switch.pos, switch.block);
    let mut dust = 0;
    let mut drives = 0;
    for side in Direction::ALL {
        let next = schem::step(block, side);
        if next == lever {
            continue;
        }
        let state = blocks.get(next);
        let next_name = schem::short(state);
        let wrong = |what: &str| (vec![block, next], what.to_owned());
        let fault = match next_name {
            // Dust on any side, on top and below too, takes its power, a
            // lever's or a bus's repeater's alike.
            "redstone_wire" => {
                dust += 1;
                drives += 1;
                None
            }
            "redstone_torch" | "redstone_wall_torch" => {
                if schem::torch_block(next, state) == Some(block) {
                    drives += 1;
                    None
                } else if side == Direction::Down {
                    Some(wrong("torch below powers it"))
                } else {
                    None
                }
            }
            // A repeater or comparator reading it takes its bit, at full
            // strength whether a lever or a bus's repeater powers it.
            "repeater" | "comparator" if reads_from(blocks, next, block) => {
                drives += 1;
                None
            }
            "repeater" | "comparator" if points_into(blocks, next, block) => {
                Some(wrong(&format!("{next_name} points into it")))
            }
            // A target takes no power from the block it touches, on any
            // side.
            "repeater" | "comparator" | "lever" | "air" | "target" => None,
            "redstone_block" if side == Direction::Up => None,
            _ if next_name.ends_with("_button")
                || next_name.ends_with("sign")
                || !block::is_component(next_name)
                || schem::slots(state).is_some() =>
            {
                None
            }
            _ => Some(wrong(&format!("touches {}", words(next_name)))),
        };
        if let Some((at, what)) = fault {
            problem(
                report,
                "input block wiring",
                at,
                format!("Input at {}: {what}", place(block)),
            );
        }
    }
    if drives == 0 {
        problem(
            report,
            "input drives nothing",
            vec![block],
            format!("Input at {}: drives nothing", place(block)),
        );
    }
    dust > 0
}

/// Whether the spot behind the switch, where a bus's driver would feed
/// the repeater put in its place, is free.
fn free_behind(blocks: &Blocks, switch: &Switch) -> bool {
    let behind = short(blocks, schem::step(switch.pos, switch.side));
    behind == "air" || behind.ends_with("sign")
}

/// Room for a repeater in the switch's place, standing on a block, so a
/// bus can drive the input; the spot behind it may be taken only on a
/// control line, see `BLOCK_BEHIND`.
fn repeater_room(blocks: &Blocks, switch: &Switch, report: &mut Report) {
    let lever = switch.pos;
    let name = short(blocks, lever).to_owned();
    let at = place(lever);
    let below = short(blocks, schem::step(lever, Direction::Down));
    let behind = schem::step(lever, switch.side);
    let behind_name = short(blocks, behind);
    if !(below == "air" || block::holds_input(below)) {
        problem(
            report,
            "no room for a repeater",
            vec![lever],
            format!(
                "{} at {at}: {} below blocks repeater",
                noun(&name),
                words(below)
            ),
        );
    } else if !free_behind(blocks, switch) {
        problem(
            report,
            BLOCK_BEHIND,
            vec![switch.block, lever, behind],
            format!(
                "{} at {at}: {} behind blocks repeater",
                noun(&name),
                words(behind_name)
            ),
        );
    }
}

/// The outputs: blocks of the kinds `marks` takes, each with dust, a
/// repeater, or a comparator on top reading it out, as [`reads_out`] says,
/// and no block of its own kind touching it, even diagonally, of a kind
/// every block of which is so. An input's block, under its switch, is
/// never one.
fn outputs(blocks: &Blocks, marks: Marks, report: &mut Report) -> Vec<Pos> {
    // A kind marks outputs only where every block of it could: one a build
    // is built of, such as lime glass, also stands where nothing reads it.
    let found: Vec<Pos> = kinds(blocks, |name| marks.takes(name))
        .into_values()
        .filter(|members| members.iter().all(|&(_, could)| could))
        .flatten()
        .map(|(pos, _)| pos)
        .collect();
    if found.is_empty() {
        problem(report, "no outputs", Vec::new(), "No outputs".to_owned());
    }
    found
}

/// The kinds of block that might mark a build's outputs where it marks
/// none as [`YELLOW`] says, each with how many blocks it has: any kind
/// that could, as [`could_mark`] says, every block of which is alone, with
/// something reading it on top, as an output's is. The most blocks first.
/// Which of them, if any, marks its outputs only its builder can say: a hex
/// adder's digit inputs or a build's glass under dust may be lone too.
pub fn candidates(blocks: &Blocks) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = kinds(blocks, could_mark)
        .into_iter()
        .filter(|(_, members)| members.iter().all(|&(_, could)| could))
        .map(|(name, members)| (name.to_owned(), members.len()))
        .collect();
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found
}

/// Each kind of block `wanted` takes but the inputs', with its blocks, and
/// whether each could mark an output: alone, with dust, a repeater, or a
/// comparator on.
fn kinds(
    blocks: &Blocks,
    wanted: impl Fn(&str) -> bool,
) -> BTreeMap<&str, Vec<(Pos, bool)>> {
    let switched: HashSet<Pos> =
        blocks.switches().iter().map(|s| s.block).collect();
    let mut kinds: BTreeMap<&str, Vec<(Pos, bool)>> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        if !wanted(name) || switched.contains(&pos) {
            continue;
        }
        let read = reads_out(blocks, pos);
        kinds
            .entry(name)
            .or_default()
            .push((pos, read && lone(blocks, pos, name)));
    }
    kinds
}

/// Whether what stands on the block at `pos` reads it out of the build:
/// dust, or a repeater or comparator but one powering a block in front of
/// it, which carries the build's signal on rather than out, so the block
/// under it only holds it up.
fn reads_out(blocks: &Blocks, pos: Pos) -> bool {
    let top = schem::step(pos, Direction::Up);
    match short(blocks, top) {
        "redstone_wire" => true,
        "repeater" | "comparator" => {
            let powers = Direction::facing(blocks.get(top)).is_some_and(|f| {
                block::solid(blocks.get(schem::step(top, f.opposite())))
            });
            !powers
        }
        _ => false,
    }
}

/// Whether a block of kind `name` could mark an output where a build
/// marks none with [`YELLOW`]: any full block that carries power but a
/// redstone component, such as a target or lamp, or stained glass of any
/// colour but white. Plain glass and white glass are what a build holds
/// dust up with, and the site stores its own plain transparent blocks as
/// white glass.
fn could_mark(name: &str) -> bool {
    if let Some(colour) = name.strip_suffix("_stained_glass") {
        return colour != "white";
    }
    !block::is_component(name)
        && !name.ends_with("glass")
        && block::solid(&format!("minecraft:{name}"))
}

/// Whether no block of kind `name` touches `pos`, even at a corner.
fn lone(blocks: &Blocks, pos: Pos, name: &str) -> bool {
    let near = -1..=1;
    !near.clone().any(|dx| {
        near.clone().any(|dy| {
            near.clone().any(|dz| {
                [dx, dy, dz] != [0, 0, 0]
                    && short(blocks, [pos[0] + dx, pos[1] + dy, pos[2] + dz])
                        == name
            })
        })
    })
}

/// The positions grouped into runs: lines of evenly spaced bits, the
/// longest first, each bit sharing its `kind` (for an input, its lever's
/// side and whether it is a button) with the rest of its run; what is
/// left, lone lines.
fn runs<K: PartialEq + Copy>(bits: &[(Pos, K)]) -> Vec<Run> {
    let kind: BTreeMap<Pos, K> = bits.iter().copied().collect();
    let mut left: HashSet<Pos> = kind.keys().copied().collect();
    let mut found = Vec::new();
    let steps = steps();
    loop {
        let mut best: Option<Run> = None;
        let mut starts: Vec<Pos> = left.iter().copied().collect();
        starts.sort_unstable();
        for &start in &starts {
            for &step in &steps {
                // Only from an end, so each line is walked once.
                if left.contains(&schem::add(start, neg(step)))
                    && kind[&schem::add(start, neg(step))] == kind[&start]
                {
                    continue;
                }
                let mut run = vec![start];
                let mut next = schem::add(start, step);
                while left.contains(&next) && kind[&next] == kind[&start] {
                    run.push(next);
                    next = schem::add(next, step);
                }
                if run.len() >= 2
                    && best.as_ref().is_none_or(|b| run.len() > b.bits.len())
                {
                    best = Some(Run { bits: run, step });
                }
            }
        }
        let Some(run) = best else { break };
        for bit in &run.bits {
            left.remove(bit);
        }
        found.push(run);
    }
    let mut lone: Vec<Pos> = left.into_iter().collect();
    lone.sort_unstable();
    found.extend(lone.into_iter().map(|bit| Run {
        bits: vec![bit],
        step: [0, 0, 0],
    }));
    found
}

fn neg(pos: Pos) -> Pos {
    [-pos[0], -pos[1], -pos[2]]
}

/// Every step a run may take, up to 8 blocks apart: along one axis, or
/// rising one block for each block across.
fn steps() -> Vec<Pos> {
    let mut steps = Vec::new();
    for s in 1..=8 {
        steps.extend([[s, 0, 0], [0, s, 0], [0, 0, s], [-s, 0, 0], [0, 0, -s]]);
        steps.extend([[s, s, 0], [-s, s, 0], [0, s, s], [0, s, -s]]);
    }
    steps
}

/// The check of the file in `data`, as `inspect_marked` makes it with
/// `marks`; with `adder`, then probed as an adder.
pub fn inspect_file(data: &[u8], marks: Marks, adder: bool) -> Result<Report> {
    let blocks = Blocks::read(data)?;
    let mut report = inspect_marked(&blocks, marks);
    if !report
        .errors
        .iter()
        .any(|p| p.rule == "no room for a repeater")
    {
        for lever in not_bus_drivable(data)? {
            problem(
                &mut report,
                "lever not bus-drivable",
                vec![lever],
                format!("Lever at {}: not bus-drivable", place(lever)),
            );
        }
    }
    if adder && !report.inputs.is_empty() && !report.outputs.is_empty() {
        match crate::recognize::probe::adder(&blocks, &report) {
            Ok(probed) => report.notes.push(format!(
                "It adds, as {}: all {} cases right, {}.",
                probed.layout.describe(),
                probed.run.cases,
                probed.run.pattern
            )),
            Err(message) => {
                problem(
                    &mut report,
                    "not an adder",
                    Vec::new(),
                    message.to_string(),
                );
            }
        }
    }
    Ok(report)
}

/// The levers whose place a repeater can't take: the build in `data`
/// compiled with every lever swapped for a repeater on a block, pointed
/// into its input block and driven by a lever behind it, against the
/// build as it is. Where the two differ but for those spots, a bus could
/// not drive the input through a repeater as the lever does. A lever with
/// a block behind it, which only a control line's may have, is left as it
/// is. Read by the rules of redstone, with no simulation.
pub fn not_bus_drivable(data: &[u8]) -> Result<Vec<Pos>> {
    let blocks = Blocks::read(data)?;
    let levers: Vec<Switch> = blocks
        .switches()
        .into_iter()
        .filter(|s| {
            !s.button
                && !matches!(s.side, Direction::Up | Direction::Down)
                && free_behind(&blocks, s)
        })
        .collect();
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    let mut spots: HashSet<Pos> = HashSet::new();
    for lever in &levers {
        let behind = schem::step(lever.pos, lever.side);
        changes.insert(
            lever.pos,
            format!(
                "minecraft:repeater[delay=1,facing={},locked=false,powered=false]",
                lever.side.name()
            ),
        );
        for spot in [lever.pos, behind] {
            let under = schem::step(spot, Direction::Down);
            if short(&blocks, under) == "air" && !changes.contains_key(&under) {
                changes.insert(under, "minecraft:white_concrete".to_owned());
            }
            spots.insert(spot);
        }
        changes.insert(
            behind,
            "minecraft:lever[face=floor,facing=north,powered=false]".to_owned(),
        );
    }
    let (swapped, moved) = standardize::edit_grown(data, &changes)?;
    let back = |p: Pos| [0, 1, 2].map(|axis| p[axis] - moved[axis]);
    let before = simulate::compiled(&blocks)?;
    let after = simulate::compiled(&Blocks::read(&swapped)?)?;
    let kinds = |kinds: BTreeMap<Pos, String>, map: &dyn Fn(Pos) -> Pos| {
        kinds
            .into_iter()
            .map(|(p, kind)| (map(p), kind))
            .filter(|(p, _)| !spots.contains(p))
            .collect::<BTreeMap<_, _>>()
    };
    let links = |links: BTreeMap<simulate::Link, u8>,
                 map: &dyn Fn(Pos) -> Pos| {
        links
            .into_iter()
            .map(|((from, to, side), ss)| ((map(from), map(to), side), ss))
            // Links out of each lever's spot are compared, the repeater's
            // against the lever's; those of the levers behind are not.
            .filter(|((from, to, _), _)| {
                let swapped = levers.iter().any(|l| l.pos == *from);
                !spots.contains(to) && (swapped || !spots.contains(from))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let (old_kinds, new_kinds) =
        (kinds(before.kinds, &|p| p), kinds(after.kinds, &back));
    let (old_links, new_links) =
        (links(before.links, &|p| p), links(after.links, &back));
    let mut wrong: BTreeSet<Pos> = BTreeSet::new();
    let changed_ends: Vec<Pos> = old_links
        .keys()
        .chain(new_links.keys())
        .filter(|k| old_links.get(*k) != new_links.get(*k))
        .flat_map(|(from, to, _)| [*from, *to])
        .chain(
            old_kinds
                .keys()
                .chain(new_kinds.keys())
                .filter(|p| old_kinds.get(*p) != new_kinds.get(*p))
                .copied(),
        )
        .collect();
    // Each difference is put down to the lever nearest it.
    for end in changed_ends {
        if let Some(lever) = levers.iter().min_by_key(|l| {
            (0..3)
                .map(|axis| (l.pos[axis] - end[axis]).abs())
                .sum::<i32>()
        }) {
            wrong.insert(lever.pos);
        }
    }
    Ok(wrong.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::recognize::colour;

    /// The input-block faults the check finds in an input, its lever on
    /// the south and dust to the north, with `other` beside its block, on
    /// top or on its east.
    fn faults_with(other: &str, on_top: bool) -> Vec<String> {
        let states: HashMap<Pos, String> = [
            ([0, 1, 1], "white_concrete"),
            ([0, 1, 2], "lever[face=wall,facing=south,powered=false]"),
            ([0, 0, 0], "white_concrete"),
            (
                [0, 1, 0],
                "redstone_wire[east=none,north=side,power=0,south=side,west=none]",
            ),
            (if on_top { [0, 2, 1] } else { [1, 1, 1] }, other),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        inspect(&Blocks::from_grid([2, 3, 3], &states))
            .errors
            .into_iter()
            .filter(|problem| problem.rule == "input block wiring")
            .map(|problem| problem.message)
            .collect()
    }

    #[test]
    fn a_plain_target_or_redstone_block_may_sit_on_an_input_block() {
        for block in ["stone", "target[power=0]", "redstone_block"] {
            assert_eq!(
                faults_with(block, true),
                Vec::<String>::new(),
                "{block}"
            );
        }
        // A target may touch it on any side.
        assert_eq!(faults_with("target[power=0]", false), Vec::<String>::new());
        assert_eq!(
            faults_with("redstone_block", false),
            ["Input at (0, 1, 1): touches redstone block"]
        );
    }

    /// A block behind a lever is a fault only on a data input, where a
    /// bus's driver would feed the repeater in the lever's place.
    #[test]
    fn only_a_data_input_needs_the_spot_behind_its_lever_free() {
        let states: HashMap<Pos, String> = [
            ([0, 1, 1], "white_concrete"),
            ([0, 1, 2], "lever[face=wall,facing=south,powered=false]"),
            ([0, 1, 3], "white_concrete"),
            ([0, 0, 0], "white_concrete"),
            (
                [0, 1, 0],
                "redstone_wire[east=none,north=side,power=0,south=side,west=none]",
            ),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        let report = inspect(&Blocks::from_grid([1, 2, 4], &states));
        let rules: Vec<&str> = report.faults().map(|p| p.rule).collect();
        assert_eq!(rules, ["no outputs"]);
        assert_eq!(report.data_faults(&[]), Vec::<String>::new());
        assert_eq!(
            report.data_faults(&[[0, 1, 1]]),
            [
                "Lever at (0, 1, 2): white concrete behind blocks repeater, \
                 on a data input"
            ]
        );
    }

    /// A lone block with a repeater on top is an output only when the
    /// repeater powers no block in front of it: one feeding a block carries
    /// the circuit on, and the block under it only holds it up.
    #[test]
    fn a_block_under_a_repeater_powering_a_block_is_no_output() {
        let outputs = |front: &str| {
            let states: HashMap<Pos, String> = [
                ([0, 0, 1], "yellow_concrete"),
                (
                    [0, 1, 1],
                    "repeater[delay=1,facing=south,locked=false,powered=false]",
                ),
                ([0, 1, 0], front),
            ]
            .into_iter()
            .filter(|(_, state)| *state != "air")
            .map(|(pos, state)| (pos, format!("minecraft:{state}")))
            .collect();
            inspect(&Blocks::from_grid([1, 2, 2], &states))
                .outputs
                .len()
        };
        assert_eq!(outputs("air"), 1, "a repeater carrying the bit out");
        assert_eq!(outputs("black_wool"), 0, "a repeater powering a block");
    }

    /// How many outputs a lone block of kind `block` with dust on top is,
    /// read as `marks` says.
    fn lone_outputs(block: &str, marks: Marks) -> usize {
        let states: HashMap<Pos, String> = [
            ([0, 0, 0], block),
            (
                [0, 1, 0],
                "redstone_wire[east=none,north=side,power=0,south=side,west=none]",
            ),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        inspect_marked(&Blocks::from_grid([1, 2, 1], &states), marks)
            .outputs
            .len()
    }

    /// Read as a builder marks them, only yellow concrete and yellow glass
    /// are outputs: not glass of another colour under dust, as a staircase
    /// is built of, nor a lamp, nor an input's colour, nor the colours the
    /// site marks status lines and further buses with.
    #[test]
    fn only_lone_yellow_concrete_or_glass_marks_an_output() {
        for block in ["yellow_concrete", "yellow_stained_glass"] {
            assert_eq!(lone_outputs(block, YELLOW), 1, "{block}");
        }
        for block in [
            "gray_stained_glass",
            "light_gray_stained_glass",
            "redstone_lamp",
            "lime_concrete",
            "light_blue_concrete",
            "pink_concrete",
            "orange_concrete",
            "brown_stained_glass",
            "yellow_wool",
            "stone",
        ] {
            assert_eq!(lone_outputs(block, YELLOW), 0, "{block}");
        }
    }

    /// Read with any lone block marking an output, as the kinds that might
    /// mark a build's outputs are listed, a lone block with dust on top is
    /// one whatever its colour or kind, but plain or white glass, which
    /// holds dust up, and a component, such as a target.
    #[test]
    fn any_lone_block_but_plain_glass_or_a_component_might_mark_an_output() {
        for block in [
            "yellow_concrete",
            "red_wool",
            "magenta_wool",
            "lime_concrete",
            "light_blue_stained_glass",
            "gray_stained_glass",
            "light_gray_stained_glass",
            "white_wool",
            "gold_block",
            "stone",
        ] {
            assert_eq!(lone_outputs(block, Marks::Any), 1, "{block}");
        }
        for block in ["glass", "white_stained_glass", "target", "redstone_lamp"]
        {
            assert_eq!(lone_outputs(block, Marks::Any), 0, "{block}");
        }
    }

    /// The site's own files are read with every colour it marks an output
    /// with, as [`colour::is_output`] reads them, and nothing else.
    #[test]
    fn the_sites_files_are_read_in_its_output_colours() {
        let Marks::Kinds(kinds) = SITE else {
            panic!("the site's marks are its colours");
        };
        for kind in kinds {
            assert!(colour::is_output(kind), "{kind}");
            assert_eq!(lone_outputs(kind, SITE), 1, "{kind}");
        }
        assert_eq!(kinds.len(), 2 * (colour::OUTPUT_COLOURS.len() + 1));
        for block in ["light_gray_stained_glass", "lime_concrete", "stone"] {
            assert_eq!(lone_outputs(block, SITE), 0, "{block}");
        }
    }

    /// A hex adder's digit inputs, lime and light blue blocks each taking
    /// a digit through the dust on it, are lone and read on top as an
    /// output is, but aren't one; its sum is.
    #[test]
    fn a_hex_adders_digit_inputs_are_no_outputs() {
        let data = std::fs::read("tests/fixtures/hex-cca-8.schem").unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let named = |report: &Report| -> BTreeMap<String, usize> {
            let mut named = BTreeMap::new();
            for bit in report.outputs.iter().flat_map(|run| &run.bits) {
                *named.entry(short(&blocks, *bit).to_owned()).or_default() += 1;
            }
            named
        };
        let candidates: Vec<String> = candidates(&blocks)
            .into_iter()
            .map(|(kind, _)| kind)
            .collect();
        assert!(candidates.contains(&"lime_concrete".to_owned()));
        assert_eq!(
            named(&inspect_marked(&blocks, YELLOW)),
            BTreeMap::from([("yellow_concrete".to_owned(), 8)])
        );
    }

    /// The owner's parity circuit as uploaded, its dust climbing light gray
    /// glass, read with yellow outputs is found a parity circuit; read with
    /// any lone block, its 34 glass blocks are outputs too, and it isn't.
    /// Listed as kinds that might mark outputs, the glass is among them.
    #[test]
    fn a_parity_circuit_on_a_glass_staircase_is_found_by_its_yellow_output() {
        let raw = std::fs::read("tests/fixtures/parity-8.schem").unwrap();
        let blocks = Blocks::read(&raw).unwrap();
        assert!(
            candidates(&blocks)
                .contains(&("light_gray_stained_glass".to_owned(), 34))
        );
        assert_eq!(inspect_marked(&blocks, YELLOW).outputs.len(), 1);
        let found = |marks| {
            crate::recognize::probe::recognize_watched(
                &raw,
                None,
                marks,
                &mut |_, _| {},
            )
            .unwrap()
            .found
        };
        assert_eq!(
            found(YELLOW).as_deref(),
            Some("a parity circuit: all 256 cases right, xor of A's bits")
        );
        assert_eq!(found(Marks::Any), None);
    }
}
