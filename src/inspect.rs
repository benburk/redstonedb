//! The check of a build by its function rather than its colours: its
//! inputs are its levers and stone buttons, found wherever they are, and
//! its outputs blocks of yellow concrete, each read through the dust or
//! repeater on it. What the site can put right itself, it does and lists;
//! what only the builder can, it names with the blocks at fault.
//!
//! The rules:
//!
//! - An input is a lever, or a stone button for a pulse, on a side of a
//!   solid block, its input block. Other buttons' longer pulses are
//!   errors, never swapped, as a build may rely on them. The lever touches
//!   nothing else that carries power.
//! - The input block drives dust, torches, and repeaters, at most one dust,
//!   and nothing else powers or touches it. One that drives dust is hard:
//!   a bus must drive it through a repeater. Comparators reading inputs
//!   are for signal strengths, which are not taken yet.
//! - The lever's spot takes a repeater pointed into the input block, so a
//!   bus can drive the input: nothing below it but air or a solid block,
//!   and nothing behind it.
//! - Every output, data or status, is a block of yellow concrete with dust
//!   or a repeater on top, which is what is read. Yellow concrete marks
//!   nothing else.
//! - A bus's bits run in a line, evenly spaced: straight, or rising one
//!   block for each block across.

use std::collections::{BTreeMap, HashSet};

use anyhow::Result;

use crate::{
    lint,
    schem::{self, Blocks, Direction, Pos, Switch},
};

const YELLOW: &str = "yellow_concrete";

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
        problem(
            report,
            "no inputs",
            Vec::new(),
            "It has no levers or buttons. Each input bit needs a lever, or a \
             stone button for a pulse, on the side of a block."
                .to_owned(),
        );
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
            format!(
                "The {} at {at} gives a longer pulse than a stone button. \
                 Pulse inputs use stone buttons.",
                words(&name)
            ),
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
            format!(
                "The {} at {at} is on the {where_} of its block. Put it \
                 on a side.",
                words(&name)
            ),
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
                "The {} at {at} hangs on a {}. An input's block is a \
                 solid block.",
                words(&name),
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
                    "The {} at {at} touches a {} at {}, which it powers \
                     as well as its block.",
                    words(&name),
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
                Direction::Up => Some(wrong("has dust on top")),
                Direction::Down => Some(wrong("has dust under it")),
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
                    Some(wrong("has a torch under it, which powers it"))
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
                Some(wrong(
                    "is read by a comparator, which is for signal strengths, \
                 not taken yet",
                ))
            }
            "repeater" | "comparator" if points_into(blocks, next, block) => {
                Some(wrong(&format!(
                    "has a {next_name} pointed into it, which powers it"
                )))
            }
            "repeater" | "comparator" | "lever" | "air" => None,
            _ if next_name.ends_with("_button")
                || next_name.ends_with("sign")
                || !lint::is_component(next_name)
                || schem::slots(state).is_some() =>
            {
                None
            }
            _ => Some(wrong(&format!("touches a {}", words(next_name)))),
        };
        if let Some((at, what)) = fault {
            problem(
                report,
                "input block wiring",
                at,
                format!("The input block at {} {what}.", place(block)),
            );
        }
    }
    if dust > 1 {
        problem(
            report,
            "input drives two dusts",
            vec![block],
            format!(
                "The input block at {} drives {dust} dusts. An input \
                 drives one dust at most, besides torches and repeaters.",
                place(block)
            ),
        );
    }
    if drives == 0 && !compared {
        problem(
            report,
            "input drives nothing",
            vec![block],
            format!(
                "The input block at {} drives nothing: no dust, torch, or \
                 repeater reads it.",
                place(block)
            ),
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
                "Under the {} at {at} is a {}, so a repeater can't take \
                 its place for a bus to drive the input.",
                words(&name),
                words(below)
            ),
        );
    } else if behind_name != "air" && !behind_name.ends_with("sign") {
        problem(
            report,
            "no room for a repeater",
            vec![lever, behind],
            format!(
                "Behind the {} at {at} is a {} at {}, so a repeater in \
                 its place can't be driven.",
                words(&name),
                words(behind_name),
                place(behind)
            ),
        );
    }
}

/// The outputs: blocks of yellow concrete, each read through the dust or
/// repeater on it. Output blocks of the old status and bus colours become
/// yellow, as which output is which is found by probing.
fn outputs(blocks: &Blocks, report: &mut Report) -> Vec<Pos> {
    let switched: HashSet<Pos> =
        blocks.switches().iter().map(|s| s.block).collect();
    let marked: Vec<(Pos, &str)> = blocks
        .iter()
        .map(|(pos, state)| (pos, schem::short(state)))
        .filter(|(pos, name)| lint::is_output(name) && !switched.contains(pos))
        .collect();
    if marked.is_empty() {
        problem(
            report,
            "no outputs",
            Vec::new(),
            "It has no outputs. Mark each output bit, data or status, with \
             a block of yellow concrete with its dust or repeater on top."
                .to_owned(),
        );
        return Vec::new();
    }
    let recoloured = marked.iter().filter(|(_, name)| *name != YELLOW).count();
    if recoloured > 0 {
        report.fixes.push(format!(
            "{recoloured} output blocks of other colours become yellow \
             concrete"
        ));
    }
    for &(pos, name) in &marked {
        let top = schem::step(pos, Direction::Up);
        let at = place(pos);
        match short(blocks, top) {
            "redstone_wire" | "repeater" => {}
            "comparator" => problem(
                report,
                "output read by a comparator",
                vec![pos, top],
                format!(
                    "The output at {at} is read by the comparator on it, \
                     which is for signal strengths, not taken yet."
                ),
            ),
            _ => problem(
                report,
                "output not read",
                vec![pos],
                format!(
                    "The {} at {at} has no dust or repeater on top. Yellow \
                     concrete marks an output, read by the dust or repeater \
                     on it; use another block where it isn't one.",
                    words(name)
                ),
            ),
        }
    }
    marked.into_iter().map(|(pos, _)| pos).collect()
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
    if adder && !report.outputs.is_empty() {
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
