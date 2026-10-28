//! The colour-free check of a build: its inputs are its levers and stone
//! buttons, its outputs its redstone lamps, and nothing about it is read
//! from colour. What the site can put right itself, it does and lists; what
//! only the builder can, it names with the blocks at fault.
//!
//! The rules:
//!
//! - An input is a lever, or a stone button for a pulse, on a side of a
//!   solid block, its input block. Other buttons' longer pulses are
//!   errors, never swapped, as a build may rely on them. The lever touches nothing else that
//!   carries power.
//! - The input block drives dust, torches, and repeaters, at most one dust,
//!   and nothing else powers or touches it. One that drives dust is hard:
//!   a bus must drive it through a repeater. Comparators reading inputs
//!   are for signal strengths, which are not taken yet.
//! - The lever's spot takes a repeater pointed into the input block, so a
//!   bus can drive the input: nothing below it but air or a solid block,
//!   and nothing behind it.
//! - An output is a redstone lamp with one driver, dust on top of it, dust
//!   pointed into its side, or a repeater or comparator pointed into it,
//!   and nothing else around it but air, glass, and other lamps, so nothing
//!   else can light it.
//! - A bus's bits run in a line, evenly spaced: straight, or rising one
//!   block for each block across.
//!
//! A build marked the old way, with coloured output blocks read through the
//! dust or repeater on top, has a lamp put where that dust or repeater
//! points.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;

use crate::{
    interface::{Bit, Interface, Port},
    lint,
    schem::{self, Blocks, Direction, Pos, Switch},
    simulate::{self, Circuit},
    standardize,
};

const LAMP: &str = "minecraft:redstone_lamp";

const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

/// What the check found: what it puts right, and what it can't.
#[derive(Default, Debug)]
pub struct Report {
    pub fixes: Vec<String>,
    pub errors: Vec<Problem>,
    /// What else it found, such as which inputs need strong power.
    pub notes: Vec<String>,
    /// The lamps it would add for outputs marked by colour.
    pub added: Vec<Added>,
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

/// A lamp put in for an output marked by colour, and the dust or repeater
/// on that output, which drives it.
#[derive(Debug, Clone, Copy)]
pub struct Added {
    pub lamp: Pos,
    pub driver: Pos,
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
    let lamps = outputs(blocks, &mut report);
    report.inputs = runs(
        &inputs
            .iter()
            .map(|input| {
                (input.switch.block, (input.switch.side, input.switch.button))
            })
            .collect::<Vec<_>>(),
    );
    report.outputs =
        runs(&lamps.iter().map(|&pos| (pos, ())).collect::<Vec<_>>());
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

/// Whether dust at `dust` points into `to`, beside it: dust powers the
/// blocks its ends point into.
fn dust_points_into(blocks: &Blocks, dust: Pos, to: Pos) -> bool {
    HORIZONTAL.iter().any(|&side| {
        schem::step(dust, side) == to
            && schem::property(blocks.get(dust), side.name())
                .is_some_and(|shape| shape != "none")
    })
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

/// The lamps, each checked; or, for a build marked the old way, where a
/// lamp would go for each coloured output.
fn outputs(blocks: &Blocks, report: &mut Report) -> Vec<Pos> {
    let lamps: Vec<Pos> = blocks
        .iter()
        .filter(|(_, state)| schem::id(state) == LAMP)
        .map(|(pos, _)| pos)
        .collect();
    if lamps.is_empty() {
        return old_outputs(blocks, report);
    }
    for &lamp in &lamps {
        let lighting = lighting(blocks, lamp);
        let at = place(lamp);
        let readable = |pos: Pos| {
            matches!(
                short(blocks, pos),
                "redstone_wire" | "repeater" | "comparator"
            )
        };
        match lighting.as_slice() {
            [driver] if readable(*driver) => {}
            [block] if solid(short(blocks, *block)) => {
                let sources = sources(blocks, *block, lamp);
                match sources.as_slice() {
                    [source] if readable(*source) => {}
                    _ => problem(
                        report,
                        "lamp's block has other drivers",
                        [vec![lamp, *block], sources.clone()].concat(),
                        format!(
                            "The lamp at {at} is lit through the {} at {}, \
                             which {}. A lamp lit through a block is read \
                             from the one dust or repeater powering that \
                             block.",
                            words(short(blocks, *block)),
                            place(*block),
                            if sources.is_empty() {
                                "nothing powers".to_owned()
                            } else {
                                format!(
                                    "is powered by {}",
                                    sources
                                        .iter()
                                        .map(|&d| format!(
                                            "the {} at {}",
                                            words(short(blocks, d)),
                                            place(d)
                                        ))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            }
                        ),
                    ),
                }
            }
            [] => problem(
                report,
                "lamp has no driver",
                vec![lamp],
                format!(
                    "Nothing lights the lamp at {at}: no dust on it or \
                     pointed into it, and no repeater pointed into it."
                ),
            ),
            [other] => problem(
                report,
                "lamp driven by something else",
                vec![lamp, *other],
                format!(
                    "The lamp at {at} is lit by the {} at {}. A lamp is \
                     driven by dust or a repeater, which the site reads.",
                    words(short(blocks, *other)),
                    place(*other)
                ),
            ),
            several => problem(
                report,
                "lamp has two drivers",
                [vec![lamp], several.to_vec()].concat(),
                format!(
                    "The lamp at {at} can be lit by {} things: {}. A lamp \
                     has one driver.",
                    several.len(),
                    several
                        .iter()
                        .map(|&d| format!(
                            "the {} at {}",
                            words(short(blocks, d)),
                            place(d)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
        }
    }
    lamps
}

/// Where a lamp goes for each output marked the old way, by colour, read
/// through the dust, repeater, or comparator on top: a spot that one
/// points into with nothing else around it.
fn old_outputs(blocks: &Blocks, report: &mut Report) -> Vec<Pos> {
    let marked: Vec<Pos> = blocks
        .iter()
        .filter(|(_, state)| lint::is_output(schem::short(state)))
        .map(|(pos, _)| pos)
        .collect();
    if marked.is_empty() {
        problem(
            report,
            "no outputs",
            Vec::new(),
            "It has no redstone lamps. Each output bit is a lamp, driven by \
             one dust or repeater."
                .to_owned(),
        );
        return Vec::new();
    }
    // Every spot each output could take a lamp in, by the way out of its
    // reader; the way most outputs can go is tried first.
    let marked_set: HashSet<Pos> = marked.iter().copied().collect();
    let spots: Vec<(Pos, Vec<(Direction, Pos)>)> = marked
        .iter()
        .map(|&block| (block, lamp_spots(blocks, &marked_set, block)))
        .collect();
    let mut ways: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, options) in &spots {
        for (way, _) in options {
            *ways.entry(way.name()).or_default() += 1;
        }
    }
    let best = ways.iter().max_by_key(|(_, n)| **n).map(|(way, _)| *way);
    let mut chosen = Vec::new();
    let mut failed = Vec::new();
    for (block, options) in spots {
        // The block itself first: the file keeps its size.
        let pick = options
            .iter()
            .find(|(way, _)| *way == Direction::Down)
            .or_else(|| {
                options.iter().find(|(way, _)| Some(way.name()) == best)
            })
            .or(options.first());
        match pick {
            Some(&(_, spot)) => chosen.push((block, spot)),
            None => failed.push(block),
        }
    }
    // A neighbouring output was let be beside a spot as a lamp to be; one
    // that keeps its block, as its lamp went elsewhere, may light the spot,
    // which then can't be used. Taking one away may undo another.
    loop {
        let replaced: HashSet<Pos> = chosen
            .iter()
            .filter(|(block, spot)| block == spot)
            .map(|&(block, _)| block)
            .collect();
        let lit = chosen.iter().position(|&(block, spot)| {
            let reader = schem::step(block, Direction::Up);
            Direction::ALL.iter().any(|&side| {
                let next = schem::step(spot, side);
                next != reader
                    && marked_set.contains(&next)
                    && !replaced.contains(&next)
                    && lights(blocks, next, spot)
            })
        });
        match lit {
            Some(i) => failed.push(chosen.remove(i).0),
            None => break,
        }
    }
    let mut lamps = Vec::new();
    for (block, spot) in chosen {
        lamps.push(spot);
        report.added.push(Added {
            lamp: spot,
            driver: schem::step(block, Direction::Up),
        });
    }
    if !lamps.is_empty() {
        report.fixes.push(format!(
            "{} outputs marked by colour get a lamp each, where the dust or \
             repeater on them points",
            lamps.len()
        ));
    }
    for block in failed {
        no_room(blocks, &marked_set, block, report);
    }
    lamps
}

/// Says why the output marked by colour at `block` can't take a lamp.
fn no_room(
    blocks: &Blocks,
    marked_set: &HashSet<Pos>,
    block: Pos,
    report: &mut Report,
) {
    let reader = schem::step(block, Direction::Up);
    let reader_name = short(blocks, reader);
    // What stops the block itself becoming the lamp.
    let blocker = Direction::ALL
        .iter()
        .map(|&side| schem::step(block, side))
        .find(|&next| {
            next != reader
                && !marked_set.contains(&next)
                && lights(blocks, next, block)
        });
    let why = match (reader_name, blocker) {
        ("redstone_wire", Some(next)) => {
            let by = if solid(short(blocks, next)) {
                powering(blocks, next, block)
                    .map(|p| {
                        format!(
                            ", powered by the {} at {},",
                            words(short(blocks, p)),
                            place(p)
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!(
                "the {} at {}{by} could light a lamp in its place, and \
                 the dust on it points to no free spot",
                words(short(blocks, next)),
                place(next)
            )
        }
        ("redstone_wire", None) => "a torch, repeater, or comparator \
            reads its block, which a lamp would not power, and the dust \
            on it points to no free spot"
            .to_owned(),
        ("repeater" | "comparator", _) => format!(
            "the spot the {} on it points into isn't free, with only \
             air around",
            words(reader_name)
        ),
        _ => "no dust or repeater stands on it to read it".to_owned(),
    };
    problem(
        report,
        "no room for a lamp",
        vec![block],
        format!(
            "The output at {} can't take a lamp: {why}. Put a lamp there \
             that only its dust or repeater drives.",
            place(block)
        ),
    );
}

/// The spots a lamp could go for a coloured output, each with the way
/// from the dust, repeater, or comparator on it: the block itself, down,
/// under dust, or beside that reader, where it points. Around the spot is
/// nothing but the reader, air, glass, and other outputs, which become
/// lamps too.
fn lamp_spots(
    blocks: &Blocks,
    marked: &HashSet<Pos>,
    block: Pos,
) -> Vec<(Direction, Pos)> {
    let reader = schem::step(block, Direction::Up);
    let name = short(blocks, reader);
    let ways: Vec<Direction> = match name {
        "repeater" | "comparator" => Direction::facing(blocks.get(reader))
            .map(Direction::opposite)
            .into_iter()
            .collect(),
        "redstone_wire" => std::iter::once(Direction::Down)
            .chain(HORIZONTAL.iter().copied().filter(|side| {
                schem::property(blocks.get(reader), side.name())
                    .is_some_and(|shape| shape == "side")
            }))
            .collect(),
        _ => Vec::new(),
    };
    ways.into_iter()
        .map(|way| (way, schem::step(reader, way)))
        .filter(|&(_, spot)| {
            // A new block on dust would cut the dust's climb past it; the
            // output block itself, already solid, cuts nothing.
            let free = spot == block
                || (short(blocks, spot) == "air"
                    && short(blocks, schem::step(spot, Direction::Down))
                        != "redstone_wire");
            // The block itself must light nothing else, as nothing reads a
            // lamp; and nothing but the reader may light the spot.
            free && (spot != block || !read_by_anything(blocks, block))
                && Direction::ALL.iter().all(|&side| {
                    let next = schem::step(spot, side);
                    next == reader
                        || marked.contains(&next)
                        || !lights(blocks, next, spot)
                })
        })
        .collect()
}

/// What around `pos` can power it, by the rules of redstone: dust on top
/// of it or pointed into it, a repeater, comparator, or observer pointed
/// into it, a torch beside or under it, a lever, button, or redstone block
/// beside it, or a solid block beside it that is powered.
pub fn lighting(blocks: &Blocks, pos: Pos) -> Vec<Pos> {
    Direction::ALL
        .iter()
        .map(|&side| schem::step(pos, side))
        .filter(|&next| lights(blocks, next, pos))
        .collect()
}

/// Whether what is at `from` can power `to`, beside it.
fn lights(blocks: &Blocks, from: Pos, to: Pos) -> bool {
    let state = blocks.get(from);
    let name = schem::short(state);
    match name {
        "redstone_wire" => {
            schem::step(from, Direction::Down) == to
                || dust_points_into(blocks, from, to)
        }
        "repeater" | "comparator" | "observer" => points_into(blocks, from, to),
        "redstone_torch" | "redstone_wall_torch" => {
            schem::torch_block(from, state) != Some(to)
        }
        "lever" | "redstone_block" => true,
        _ if name.ends_with("_button") => true,
        // A target conducts as a solid block does.
        _ if (solid(name) || name == "target") && name != "redstone_lamp" => {
            powered(blocks, from, to)
        }
        _ => false,
    }
}

/// Whether solid block `pos` can be powered by anything around it but
/// `besides`: dust on it or pointed into it, a repeater or comparator
/// pointed into it, a torch under it, or a lever or button on it.
fn powered(blocks: &Blocks, pos: Pos, besides: Pos) -> bool {
    Direction::ALL.iter().any(|&side| {
        let next = schem::step(pos, side);
        if next == besides {
            return false;
        }
        let state = blocks.get(next);
        match schem::short(state) {
            "redstone_wire" => {
                side == Direction::Up || dust_points_into(blocks, next, pos)
            }
            "repeater" | "comparator" => points_into(blocks, next, pos),
            "redstone_torch" => side == Direction::Down,
            "lever" => blocks
                .switches()
                .iter()
                .any(|s| s.pos == next && s.block == pos),
            name if name.ends_with("_button") => blocks
                .switches()
                .iter()
                .any(|s| s.pos == next && s.block == pos),
            _ => false,
        }
    })
}

/// Where lamp `lamp` is read: the one dust or repeater lighting it, or
/// lighting the one solid block that lights it; `None` when it has no such
/// one driver.
pub fn lamp_driver(blocks: &Blocks, lamp: Pos) -> Option<Pos> {
    let readable = |pos: Pos| {
        matches!(
            short(blocks, pos),
            "redstone_wire" | "repeater" | "comparator"
        )
    };
    match lighting(blocks, lamp).as_slice() {
        [driver] if readable(*driver) => Some(*driver),
        [block] if solid(short(blocks, *block)) => {
            match sources(blocks, *block, lamp).as_slice() {
                [source] if readable(*source) => Some(*source),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Everything powering solid block `pos`, other than `besides`.
fn sources(blocks: &Blocks, pos: Pos, besides: Pos) -> Vec<Pos> {
    Direction::ALL
        .iter()
        .map(|&side| schem::step(pos, side))
        .filter(|&next| next != besides && lights_block(blocks, next, pos))
        .collect()
}

/// What powers solid block `pos`, other than `besides`, if anything does.
fn powering(blocks: &Blocks, pos: Pos, besides: Pos) -> Option<Pos> {
    Direction::ALL
        .iter()
        .map(|&side| schem::step(pos, side))
        .find(|&next| {
            next != besides
                && powered(blocks, pos, besides)
                && lights_block(blocks, next, pos)
        })
}

/// Whether what is at `from` powers solid block `to` beside it.
fn lights_block(blocks: &Blocks, from: Pos, to: Pos) -> bool {
    let state = blocks.get(from);
    match schem::short(state) {
        "redstone_wire" => {
            schem::step(from, Direction::Down) == to
                || dust_points_into(blocks, from, to)
        }
        "repeater" | "comparator" => points_into(blocks, from, to),
        "redstone_torch" => schem::step(from, Direction::Up) == to,
        name if name == "lever" || name.ends_with("_button") => blocks
            .switches()
            .iter()
            .any(|s| s.pos == from && s.block == to),
        _ => false,
    }
}

/// Whether anything takes power from block `pos`: a torch on it, or a
/// repeater or comparator reading it.
fn read_by_anything(blocks: &Blocks, pos: Pos) -> bool {
    Direction::ALL.iter().any(|&side| {
        let next = schem::step(pos, side);
        let state = blocks.get(next);
        match schem::short(state) {
            "redstone_torch" | "redstone_wall_torch" => {
                schem::torch_block(next, state) == Some(pos)
            }
            "repeater" | "comparator" => reads_from(blocks, next, pos),
            _ => false,
        }
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

/// A lamp's block state as placed: unlit, as the simulator lights it.
const LAMP_STATE: &str = "minecraft:redstone_lamp[lit=false]";

/// The file with `added`'s lamps put in, its box grown to hold them, and
/// how far its old positions moved.
pub fn with_lamps(data: &[u8], added: &[Added]) -> Result<(Vec<u8>, Pos)> {
    let changes = added
        .iter()
        .map(|a| (a.lamp, LAMP_STATE.to_owned()))
        .collect();
    standardize::edit_grown(data, &changes)
}

/// What putting lamps in changed of the circuit, its components and the
/// links into them, besides the lamps, at `lamps` in `after`, whose old
/// positions moved by `moved`: empty when it computes the same.
pub fn circuit_changes(
    before: &[u8],
    after: &[u8],
    moved: Pos,
    added: &[Added],
) -> Result<Vec<String>> {
    let shift = |p: Pos| [0, 1, 2].map(|axis| p[axis] + moved[axis]);
    let back = |p: Pos| [0, 1, 2].map(|axis| p[axis] - moved[axis]);
    let lamps: HashSet<Pos> = added.iter().map(|a| shift(a.lamp)).collect();
    let lamps = &lamps;
    // The dust driving each lamp is what the tests read.
    let drivers: HashSet<Pos> = added.iter().map(|a| a.driver).collect();
    let old = simulate::compiled_reading(&Blocks::read(before)?, &drivers)?;
    let new = simulate::compiled_reading(
        &Blocks::read(after)?,
        &drivers.iter().map(|&p| shift(p)).collect::<HashSet<Pos>>(),
    )?;
    let new_kinds: BTreeMap<Pos, String> = new
        .kinds
        .into_iter()
        .filter(|(p, _)| !lamps.contains(p))
        .map(|(p, kind)| (back(p), kind))
        .collect();
    // Links into components only: dust is read through what it feeds.
    // Links into components only: dust is read through what it feeds. The
    // lamps, in the new file's positions, are left out of it.
    let links = |links: BTreeMap<simulate::Link, u8>,
                 kinds: &BTreeMap<Pos, String>,
                 lamps: &HashSet<Pos>,
                 map: &dyn Fn(Pos) -> Pos| {
        links
            .into_iter()
            .filter(|((from, to, _), _)| {
                !lamps.contains(from) && !lamps.contains(to)
            })
            .map(|((from, to, side), ss)| ((map(from), map(to), side), ss))
            .filter(|((_, to, _), _)| {
                kinds.contains_key(to) || drivers.contains(to)
            })
            .collect::<BTreeMap<_, _>>()
    };
    let old_links = links(old.links, &old.kinds, &HashSet::new(), &|p| p);
    let new_links = links(new.links, &new_kinds, lamps, &back);
    let mut found = Vec::new();
    let positions: BTreeSet<&Pos> =
        old.kinds.keys().chain(new_kinds.keys()).collect();
    for p in positions {
        if old.kinds.get(p) != new_kinds.get(p) {
            found.push(format!("the component at {}", place(*p)));
        }
    }
    let all: BTreeSet<_> = old_links.keys().chain(new_links.keys()).collect();
    for (from, to, _) in all
        .into_iter()
        .filter(|k| old_links.get(k) != new_links.get(k))
    {
        found.push(format!("the link from {} to {}", place(*from), place(*to)));
    }
    found.dedup();
    Ok(found)
}

/// Where each lamp, once the build settles, is lit with its driver off or
/// dark with it on, or stays dark while its driver is on for more than a
/// tick or two, over a run of `rounds` random settings of every lever and
/// presses of every button, watched tick by tick.
pub fn lamps_follow(
    data: &[u8],
    pairs: &[(Pos, Pos)],
    rounds: usize,
) -> Result<Vec<String>> {
    const LAG: usize = 2;
    let blocks = Blocks::read(data)?;
    let switches: Vec<Switch> = blocks
        .switches()
        .into_iter()
        .filter(|s| !matches!(s.side, Direction::Up | Direction::Down))
        .collect();
    let interface = Interface {
        inputs: switches
            .iter()
            .map(|s| Port {
                name: String::new(),
                block: String::new(),
                labelled: true,
                bits: vec![Bit {
                    pos: s.block,
                    entry: Some(s.side),
                    hard: false,
                    inverted: s.on,
                }],
                strength: 0,
                words: Vec::new(),
                signal_strength: false,
            })
            .collect(),
        outputs: Vec::new(),
    };
    let mut circuit = Circuit::new(&blocks, &interface)?;
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut wrong: BTreeMap<Pos, String> = BTreeMap::new();
    // Each driver's last few states, newest last.
    let mut history: Vec<Vec<bool>> =
        pairs.iter().map(|_| Vec::new()).collect();
    let mut check = |circuit: &Circuit,
                     history: &mut Vec<Vec<bool>>,
                     settled: bool| {
        for (i, &(lamp, driver)) in pairs.iter().enumerate() {
            let (lit, on) = (circuit.on(lamp), circuit.on(driver));
            let past = &mut history[i];
            past.push(on);
            if past.len() > LAG + 1 {
                past.remove(0);
            }
            // Between settled states a lamp may light while its dust shows
            // off: dust can carry a pulse shorter than a tick, which the
            // lamp holds; that glitch is the build's, not the lamp's.
            let fault = if settled {
                (lit != on).then_some(if lit { "lit" } else { "dark" })
            } else if !lit && past.len() > LAG && past.iter().all(|&was| was) {
                Some("dark")
            } else {
                None
            };
            if let Some(fault) = fault {
                let driver_state = if on { "on" } else { "off" };
                wrong.entry(lamp).or_insert_with(|| {
                    format!(
                        "The lamp at {} stayed {fault} while the {} at {} \
                         driving it was {driver_state}.",
                        place(lamp),
                        words(short(&blocks, driver)),
                        place(driver)
                    )
                });
            }
        }
    };
    for _ in 0..rounds {
        let bits = random();
        for (i, switch) in switches.iter().enumerate() {
            if switch.button {
                if (bits >> (i % 64)).trailing_zeros() >= 2 {
                    circuit.press(i, 0);
                }
            } else {
                circuit.set(i, bits >> (i % 64) & 1);
            }
        }
        let mut ticks = 0;
        while circuit.pending() && ticks < 1000 {
            circuit.tick();
            check(&circuit, &mut history, false);
            ticks += 1;
        }
        check(&circuit, &mut history, true);
    }
    Ok(wrong.into_values().collect())
}

/// The check of the file in `data`, as `inspect` makes it, then proven:
/// lamps put in for outputs marked by colour change nothing else of the
/// circuit, and every lamp follows its driver. With `adder`, and a lamp on
/// every output, it is then probed as an adder.
pub fn inspect_file(data: &[u8], adder: bool) -> Result<Report> {
    let blocks = Blocks::read(data)?;
    let mut report = inspect(&blocks);
    let (lamped, moved) = if report.added.is_empty() {
        (data.to_vec(), [0; 3])
    } else {
        with_lamps(data, &report.added)?
    };
    let shift = |p: Pos| [0, 1, 2].map(|axis| p[axis] + moved[axis]);
    let pairs: Vec<(Pos, Pos)> = if report.added.is_empty() {
        report
            .outputs
            .iter()
            .flat_map(|run| run.bits.iter().copied())
            .filter_map(|lamp| Some((lamp, lamp_driver(&blocks, lamp)?)))
            .collect()
    } else {
        let changes = circuit_changes(data, &lamped, moved, &report.added)?;
        if !changes.is_empty() {
            problem(
                &mut report,
                "lamps change the circuit",
                Vec::new(),
                format!(
                    "Putting lamps on its outputs would change its circuit: \
                     {}.",
                    changes.join(", ")
                ),
            );
        }
        report
            .added
            .iter()
            .map(|a| (shift(a.lamp), shift(a.driver)))
            .collect()
    };
    let lamps_missing = report
        .errors
        .iter()
        .any(|p| matches!(p.rule, "no room for a lamp" | "no outputs"));
    if adder && !lamps_missing {
        let lamped_blocks = Blocks::read(&lamped)?;
        let found = inspect(&lamped_blocks);
        match crate::probe::adder(&lamped_blocks, &found) {
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
    if !pairs.is_empty() {
        match lamps_follow(&lamped, &pairs, 256) {
            Ok(wrong) => {
                for message in wrong {
                    problem(
                        &mut report,
                        "lamp doesn't follow its driver",
                        Vec::new(),
                        message,
                    );
                }
            }
            Err(err) => problem(
                &mut report,
                "can't simulate",
                Vec::new(),
                format!("The simulator can't run it: {err:#}"),
            ),
        }
    }
    Ok(report)
}
