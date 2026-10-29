//! The check of a build by its function rather than its colours: its
//! inputs are its levers and stone buttons, found wherever they are, and
//! its outputs lone coloured blocks, each read through the dust or
//! repeater on it. What the site can put right itself, it does and lists;
//! what only the builder can, it names with the blocks at fault.
//!
//! The rules:
//!
//! - An input is a lever, or a stone button for a pulse, on a side of a
//!   solid block, its input block. Other buttons' longer pulses are
//!   errors, never swapped, as a build may rely on them. The lever touches
//!   nothing else that carries power.
//! - The input block drives dust, torches, and repeaters, at most two dusts,
//!   and nothing else powers it or touches it but plain blocks and, on
//!   top, a target or redstone block, which never power a solid block. One that drives dust is hard:
//!   a bus must drive it through a repeater. Comparators reading inputs
//!   are for signal strengths, which are not taken yet.
//! - The lever's spot takes a repeater pointed into the input block, so a
//!   bus can drive the input: nothing below it but air or a solid block,
//!   and nothing behind it; and the build compiled with every lever so
//!   swapped is the same circuit, the proof by the rules of redstone.
//! - Every output, data or status, is a coloured block with dust or a
//!   repeater on top, which is what is read, and no block of its colour
//!   touching it, even diagonally, of a kind used for nothing else. Any
//!   colour but white, a build's plain block, and gray, which marks logic
//!   only a stack's first copy keeps.
//! - A bus's bits run in a line, evenly spaced: straight, or rising one
//!   block for each block across.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;

use crate::{
    lint,
    schem::{self, Blocks, Direction, Pos, Switch},
    simulate, standardize,
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

pub fn inspect(blocks: &Blocks) -> Report {
    let mut report = Report::default();
    let inputs = inputs(blocks, &mut report);
    let outputs = outputs(blocks, &mut report);
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

/// A full block a lever can hang on and dust stand on.
fn solid(name: &str) -> bool {
    !lint::is_component(name)
        && !name.contains("glass")
        && !name.ends_with("slab")
        && !name.ends_with("stairs")
        && !name.ends_with("sign")
        && name != "air"
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
/// block. A button not of stone becomes one.
fn placed(blocks: &Blocks, switch: &Switch, report: &mut Report) -> bool {
    let (lever, block) = (switch.pos, switch.block);
    let name = short(blocks, lever).to_owned();
    let at = place(lever);
    // Buttons' pulses differ in length, which a build may rely on, so one
    // is never swapped for another.
    if switch.button && name != "stone_button" {
        problem(
            report,
            "button not stone",
            vec![lever],
            format!("Button at {at}: {} (need stone)", words(&name)),
        );
    }
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
    if !solid(&block_name) {
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
    let mut compared = false;
    for side in Direction::ALL {
        let next = schem::step(block, side);
        if next == lever {
            continue;
        }
        let state = blocks.get(next);
        let next_name = schem::short(state);
        let wrong = |what: &str| (vec![block, next], what.to_owned());
        let fault = match next_name {
            "redstone_wire" => match side {
                Direction::Up => Some(wrong("dust on top")),
                Direction::Down => Some(wrong("dust below")),
                _ => {
                    dust += 1;
                    drives += 1;
                    None
                }
            },
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
            "repeater" if reads_from(blocks, next, block) => {
                drives += 1;
                None
            }
            "comparator" if reads_from(blocks, next, block) => {
                compared = true;
                Some(wrong("read by comparator"))
            }
            "repeater" | "comparator" if points_into(blocks, next, block) => {
                Some(wrong(&format!("{next_name} points into it")))
            }
            "repeater" | "comparator" | "lever" | "air" => None,
            "target" | "redstone_block" if side == Direction::Up => None,
            _ if next_name.ends_with("_button")
                || next_name.ends_with("sign")
                || !lint::is_component(next_name)
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
    if dust > 2 {
        problem(
            report,
            "input drives three dusts",
            vec![block],
            format!("Input at {}: drives {dust} dusts (max 2)", place(block)),
        );
    }
    if drives == 0 && !compared {
        problem(
            report,
            "input drives nothing",
            vec![block],
            format!("Input at {}: drives nothing", place(block)),
        );
    }
    dust > 0
}

/// Room for a repeater in the switch's place, standing on a block, so a
/// bus can drive the input.
fn repeater_room(blocks: &Blocks, switch: &Switch, report: &mut Report) {
    let lever = switch.pos;
    let name = short(blocks, lever).to_owned();
    let at = place(lever);
    let below = short(blocks, schem::step(lever, Direction::Down));
    let behind = schem::step(lever, switch.side);
    let behind_name = short(blocks, behind);
    if !(below == "air" || solid(below)) {
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
    } else if behind_name != "air" && !behind_name.ends_with("sign") {
        problem(
            report,
            "no room for a repeater",
            vec![lever, behind],
            format!(
                "{} at {at}: {} behind blocks repeater",
                noun(&name),
                words(behind_name)
            ),
        );
    }
}

/// The outputs: coloured blocks, each with dust or a repeater on top,
/// which is what is read, and no block of its own colour touching it, even
/// diagonally, of a kind every block of which is so. Any colour marks one but white and gray: white is a build's
/// plain block, and gray marks logic only a stack's first copy keeps.
fn outputs(blocks: &Blocks, report: &mut Report) -> Vec<Pos> {
    let switched: HashSet<Pos> =
        blocks.switches().iter().map(|s| s.block).collect();
    // Each kind of coloured block but the inputs', with its blocks, and
    // whether each could mark an output: alone, with dust or a repeater on.
    let mut kinds: BTreeMap<&str, Vec<(Pos, bool)>> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        if !marks(name) || switched.contains(&pos) {
            continue;
        }
        let read = matches!(
            short(blocks, schem::step(pos, Direction::Up)),
            "redstone_wire" | "repeater"
        );
        kinds
            .entry(name)
            .or_default()
            .push((pos, read && lone(blocks, pos, name)));
    }
    // A kind marks outputs only where every block of it could: one a build
    // is built of, such as lime glass, also stands where nothing reads it.
    let found: Vec<Pos> = kinds
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

/// Whether a block of kind `name` can mark an output: a coloured one,
/// neither white nor gray.
fn marks(name: &str) -> bool {
    let coloured = ["_concrete", "_wool", "_terracotta", "_stained_glass"]
        .iter()
        .any(|kind| name.ends_with(kind));
    coloured && !name.starts_with("white_") && !name.starts_with("gray_")
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

/// The check of the file in `data`, as `inspect` makes it; with `adder`,
/// then probed as an adder.
pub fn inspect_file(data: &[u8], adder: bool) -> Result<Report> {
    let blocks = Blocks::read(data)?;
    let mut report = inspect(&blocks);
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
        match crate::probe::adder(&blocks, &report) {
            Ok(probed) => report.notes.push(format!(
                "It adds, as {}: all {} cases right, {}.",
                probed.layout.describe(),
                probed.run.cases,
                probed.run.pattern
            )),
            Err(message) => {
                problem(&mut report, "not an adder", Vec::new(), message);
            }
        }
    }
    Ok(report)
}

/// The levers whose place a repeater can't take: the build in `data`
/// compiled with every lever swapped for a repeater on a block, pointed
/// into its input block and driven by a lever behind it, against the
/// build as it is. Where the two differ but for those spots, a bus could
/// not drive the input through a repeater as the lever does. Read by the
/// rules of redstone, with no simulation.
pub fn not_bus_drivable(data: &[u8]) -> Result<Vec<Pos>> {
    let blocks = Blocks::read(data)?;
    let levers: Vec<Switch> = blocks
        .switches()
        .into_iter()
        .filter(|s| {
            !s.button && !matches!(s.side, Direction::Up | Direction::Down)
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
        assert_eq!(
            faults_with("redstone_block", false),
            ["Input at (0, 1, 1): touches redstone block"]
        );
    }
}
