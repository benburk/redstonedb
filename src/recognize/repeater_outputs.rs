//! A rule for outputs proposed in place of their colours, measured here
//! before the site adopts it: an output is a repeater or comparator
//! powering nothing, the cell in front of it, the side it gives out from,
//! air. A repeater gives a bit, on or off; a comparator a strength from 0
//! to 15. Colour plays no part, so any block may hold one up. Outputs side
//! by side, evenly spaced and facing the same way, are a bus; one alone is
//! a line. Inputs are as the check by function finds them, levers and
//! buttons on any block.
//!
//! Nothing that recognizes an upload reads outputs so yet: `rsdb outputs`,
//! `convert`, and `stack-check` measure what the rule would find.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::{
    category::Kind,
    recognize::{
        colour,
        inspect::{self, Problem, Report, Run},
        probe::{self, Fault, Found, Layout},
    },
    schem::{self, Blocks, Direction, Pos, dust::Side},
};

/// A repeater or comparator powering nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
    /// Where the repeater or comparator stands.
    pub at: Pos,
    /// A comparator, read as a strength, rather than a repeater, read as a
    /// bit.
    pub comparator: bool,
    /// Its block state's `facing`: the side it reads from, its input. It
    /// gives out the other way.
    pub facing: Direction,
}

impl Output {
    /// The way it gives out, opposite its `facing`.
    pub fn points(self) -> Direction {
        self.facing.opposite()
    }

    /// The cell it would power, which is air.
    pub fn front(self) -> Pos {
        schem::step(self.at, self.points())
    }

    /// The cell it reads from.
    pub fn behind(self) -> Pos {
        schem::step(self.at, self.facing)
    }

    /// The block holding it up, which the site's tests read an output by,
    /// as what stands on it.
    pub fn block(self) -> Pos {
        schem::step(self.at, Direction::Down)
    }

    /// `repeater` or `comparator`.
    pub fn kind(self) -> &'static str {
        if self.comparator {
            "comparator"
        } else {
            "repeater"
        }
    }
}

/// Whether `state` is air of any kind.
fn is_air(state: &str) -> bool {
    matches!(schem::short(state), "air" | "cave_air" | "void_air")
}

/// Every repeater and comparator in `blocks` whose front is air, beyond
/// the file's box too, in the file's order.
pub fn find(blocks: &Blocks) -> Vec<Output> {
    blocks
        .iter()
        .filter_map(|(at, state)| {
            let comparator = match schem::short(state) {
                "repeater" => false,
                "comparator" => true,
                _ => return None,
            };
            let facing = Direction::facing(state)?;
            let output = Output {
                at,
                comparator,
                facing,
            };
            is_air(blocks.get(output.front())).then_some(output)
        })
        .collect()
}

/// `outputs` grouped into buses, each a line of outputs evenly spaced and
/// facing the same way, the longest first, then the lone lines; each run
/// is of the outputs' own places.
pub fn lines(outputs: &[Output]) -> Vec<Run> {
    let bits: Vec<(Pos, Direction)> =
        outputs.iter().map(|o| (o.at, o.facing)).collect();
    inspect::runs(&bits)
}

/// The output colour of the block under `output`, such as `yellow`, the
/// role today's rule reads it by; `None` for any other block.
pub fn colour_under(blocks: &Blocks, output: Output) -> Option<&str> {
    colour::output_colour(schem::short(blocks.get(output.block())))
}

/// The check by function with outputs read by this rule: its inputs and
/// their problems as [`inspect::inspect_marked`] finds them, and its
/// outputs `find`'s, each run of the blocks holding them up, where the
/// site's tests read an output. An output with nothing under it in the
/// file, at its bottom, can't be read so, and is a problem.
pub fn report(blocks: &Blocks) -> Report {
    let mut report =
        inspect::inspect_marked(blocks, inspect::Marks::Kinds(&[]));
    report.errors.retain(|problem| problem.rule != "no outputs");
    let (held, bottom): (Vec<Output>, Vec<Output>) =
        find(blocks).into_iter().partition(|o| o.at[1] > 0);
    for output in &bottom {
        report.errors.push(Problem {
            rule: "output at the bottom",
            at: vec![output.at],
            message: format!(
                "Output {} at {}: nothing under it in the file",
                output.kind(),
                inspect::place(output.at)
            ),
        });
    }
    if held.is_empty() && bottom.is_empty() {
        report.errors.push(Problem {
            rule: "no outputs",
            at: Vec::new(),
            message: "No outputs".to_owned(),
        });
    }
    report.outputs = lines(&held)
        .into_iter()
        .map(|run| Run {
            bits: run
                .bits
                .iter()
                .map(|&at| schem::step(at, Direction::Down))
                .collect(),
            step: run.step,
        })
        .collect();
    report
}

/// What probing makes of a build whose outputs this rule reads.
pub struct Probed {
    pub kind: Kind,
    /// Its data inputs, its file coloured by the roles found, and what it
    /// was found to be, in words.
    pub found: Found,
    /// For an adder, the roles found.
    pub layout: Option<Layout>,
}

/// The build in `data` probed with its outputs read by this rule, as
/// `meant` alone, or as each kind probing tries in turn, as
/// [`probe::recognize_as`] does with outputs read by colour: the first
/// kind it holds as, or the fault of each it was tried as. A build with
/// faults in its inputs or outputs is probed all the same, as a measure.
pub fn probe(
    data: &[u8],
    meant: Option<Kind>,
) -> Result<Result<Probed, Vec<(Kind, Fault)>>> {
    let simulated = probe::simulatable(data)?;
    let blocks = Blocks::read(&simulated)?;
    let report = report(&blocks);
    let mut faults = Vec::new();
    for (kind, ..) in Kind::probed() {
        if meant.is_some_and(|meant| probed_as(meant) != kind) {
            continue;
        }
        if kind == Kind::Adder {
            match probe::adder(&simulated, &blocks, &report) {
                Ok(probed) => {
                    let layout = probed.layout.clone();
                    let kind = probed.run.adder_category(false);
                    let found = format!(
                        "{}: all {} cases right, {}",
                        kind.a(),
                        probed.run.cases,
                        probed.run.pattern
                    );
                    let file = probe::colour(&simulated, &layout)?;
                    let data_inputs = [layout.a.clone(), layout.b.clone()];
                    return Ok(Ok(Probed {
                        kind,
                        found: (data_inputs.concat(), file, found),
                        layout: Some(layout),
                    }));
                }
                Err(fault) => faults.push((kind, fault)),
            }
            continue;
        }
        match probe::probe_as(kind, &simulated, &blocks, &report)? {
            Ok(found) => {
                return Ok(Ok(Probed {
                    kind,
                    found,
                    layout: None,
                }));
            }
            Err(fault) => faults.push((kind, fault)),
        }
    }
    Ok(Err(faults))
}

/// The kind `kind` is probed as, such as an adder for a subtractor.
fn probed_as(kind: Kind) -> Kind {
    match kind.facts().probe {
        crate::category::Probing::As(other) => other,
        _ => kind,
    }
}

/// An output, as today's rule reads it, made one this rule reads, or why
/// it can't be made one by rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reworked {
    /// A repeater or comparator powering nothing stands on it already.
    Already { block: Pos },
    /// The dust on `block` is now `now`, fed from the one side the dust
    /// was, giving out into air; `cleared` is a block taken out of its
    /// front, which touched nothing else.
    Converted {
        block: Pos,
        now: String,
        cleared: Option<(Pos, String)>,
    },
    /// It can't be made one by rule, for `why`, at `at`.
    Not { block: Pos, at: Pos, why: String },
}

/// The blocks a dust's driver may be beside it, which a repeater or
/// comparator in its place reads from too: dust, a torch, a redstone
/// block, a lever, a target, or any block a solid one, which carries the
/// power that drove the dust.
fn feeds_a_diode(blocks: &Blocks, from: Pos, to: Pos) -> Result<(), String> {
    let state = blocks.get(from);
    let name = schem::short(state);
    match name {
        "redstone_wire"
        | "redstone_torch"
        | "redstone_wall_torch"
        | "redstone_block"
        | "lever"
        | "target" => Ok(()),
        "repeater" | "comparator" => {
            let into = Direction::facing(state).is_some_and(|facing| {
                schem::step(from, facing.opposite()) == to
            });
            if into {
                Ok(())
            } else {
                Err(format!("a {name} beside it, not pointing into it"))
            }
        }
        _ if name.ends_with("_button") => Ok(()),
        _ if crate::schem::block::solid(state) => Ok(()),
        _ if is_air(state)
            && schem::short(blocks.get(schem::step(from, Direction::Down)))
                == "redstone_wire" =>
        {
            Err("dust a block lower feeds it, which a repeater can't read"
                .to_owned())
        }
        _ => Err(format!("fed from {}", name.replace('_', " "))),
    }
}

/// Whether the block at `pos` is a solid block something powers strongly,
/// so dust beside it takes its power: a repeater or comparator pointing
/// into it, a torch under it, or a lever or button on it.
fn strongly_powered(blocks: &Blocks, pos: Pos) -> bool {
    if !crate::schem::block::solid(blocks.get(pos)) {
        return false;
    }
    let switched = blocks.switches().iter().any(|s| s.block == pos);
    switched
        || Direction::ALL.iter().any(|&side| {
            let next = schem::step(pos, side);
            let state = blocks.get(next);
            match schem::short(state) {
                "repeater" | "comparator" => Direction::facing(state)
                    .is_some_and(|f| schem::step(next, f.opposite()) == pos),
                "redstone_torch" | "redstone_wall_torch" => {
                    side == Direction::Down
                }
                _ => false,
            }
        })
}

/// Whether a block of `state` may be taken out of an output's front: a
/// plain block, not a component, with nothing around it but air, the
/// output, and the block holding it up.
fn clearable(blocks: &Blocks, front: Pos, keep: &[Pos]) -> bool {
    let state = blocks.get(front);
    let name = schem::short(state);
    !crate::schem::block::is_component(name)
        && !blocks.signs.contains_key(&front)
        && Direction::ALL.iter().all(|&side| {
            let next = schem::step(front, side);
            keep.contains(&next) || is_air(blocks.get(next))
        })
}

/// What reads the block at `block` that dust on it powered and a repeater
/// on it won't: a repeater or comparator reading it, a torch on it, or a
/// lamp, piston, or the like beside it. `None` when nothing does.
fn reads_block(blocks: &Blocks, block: Pos) -> Option<(Pos, String)> {
    Direction::ALL.iter().find_map(|&side| {
        let next = schem::step(block, side);
        let state = blocks.get(next);
        let name = schem::short(state);
        let reads = match name {
            "repeater" | "comparator" => Direction::facing(state)
                .is_some_and(|facing| schem::step(next, facing) == block),
            "redstone_torch" | "redstone_wall_torch" => {
                schem::torch_block(next, state) == Some(block)
            }
            "redstone_lamp" | "piston" | "sticky_piston" | "dropper"
            | "dispenser" | "note_block" => true,
            _ => false,
        };
        reads.then(|| (next, name.replace('_', " ")))
    })
}

/// Each output in `outputs`, a block today's rule reads by what stands on
/// it, made one this rule reads where that can be done by rule: dust on it
/// joined one side only, to what drives it, becomes a repeater, or with
/// `comparator` a comparator, reading from that side, so it gives out the
/// other way, into air, or into a plain block touching nothing else, which
/// is taken out. Nothing may point into its sides, which would lock a
/// repeater or feed a comparator, and nothing may read the block under it,
/// which the dust powered. The changes to make, with what each output
/// came to.
pub fn rework(
    blocks: &Blocks,
    outputs: &[Pos],
    comparator: impl Fn(Pos) -> bool,
) -> (BTreeMap<Pos, String>, Vec<Reworked>) {
    let mut changes = BTreeMap::new();
    let mut done = Vec::new();
    let found: Vec<Pos> = find(blocks).iter().map(|o| o.block()).collect();
    for &block in outputs {
        let top = schem::step(block, Direction::Up);
        let state = blocks.get(top);
        let not = |at: Pos, why: String| Reworked::Not { block, at, why };
        let reworked = match schem::short(state) {
            "repeater" | "comparator" if found.contains(&block) => {
                Reworked::Already { block }
            }
            "repeater" | "comparator" => {
                let front = Direction::facing(state)
                    .map(|f| schem::step(top, f.opposite()));
                let front = front.unwrap_or(top);
                not(
                    front,
                    format!(
                        "its {} powers {} in front",
                        schem::short(state),
                        schem::short(blocks.get(front)).replace('_', " ")
                    ),
                )
            }
            "redstone_wire" => {
                match dust_output(blocks, block, top, state, &comparator) {
                    Ok((now, cleared)) => {
                        changes.insert(top, now.clone());
                        if let Some((front, _)) = &cleared {
                            changes.insert(*front, schem::AIR.to_owned());
                        }
                        Reworked::Converted {
                            block,
                            now,
                            cleared,
                        }
                    }
                    Err((at, why)) => not(at, why),
                }
            }
            other => {
                not(top, format!("{} on it, not dust", other.replace('_', " ")))
            }
        };
        done.push(reworked);
    }
    (changes, done)
}

/// Where and why an output can't be reworked by rule.
type Miss = (Pos, String);

/// The one side the dust at `top` is driven from, or where and why there
/// isn't one.
fn feed_side(blocks: &Blocks, top: Pos) -> Result<Direction, Miss> {
    // What drives the dust: each side it joins, and each block beside it
    // that something powers strongly, which dust takes power from without
    // joining it.
    let joins = crate::schem::dust::joins(blocks, top);
    let mut feeds: Vec<(Direction, Side)> = Direction::SIDES
        .into_iter()
        .zip(joins)
        .filter(|&(side, join)| {
            join != Side::None
                || strongly_powered(blocks, schem::step(top, side))
        })
        .collect();
    feeds.sort_by_key(|(side, _)| side.name());
    let feed = match feeds.as_slice() {
        [] => {
            return Err((top, "nothing beside its dust drives it".to_owned()));
        }
        [(side, Side::Up)] => {
            return Err((
                schem::step(schem::step(top, *side), Direction::Up),
                "its dust climbs to dust above, which a repeater can't read"
                    .to_owned(),
            ));
        }
        [(side, _)] => *side,
        more => {
            let sides: Vec<String> = more
                .iter()
                .map(|&(side, join)| {
                    let next = schem::step(top, side);
                    let what = if join == Side::None {
                        "a block powered strongly"
                    } else if join == Side::Up {
                        "dust above"
                    } else if !crate::schem::block::is_component(schem::short(
                        blocks.get(next),
                    )) {
                        "dust below"
                    } else {
                        schem::short(blocks.get(next))
                    };
                    format!("{side} to {}", what.replace('_', " "))
                })
                .collect();
            return Err((
                top,
                format!(
                    "its dust is driven or joined {} ways, {}",
                    more.len(),
                    sides.join(", ")
                ),
            ));
        }
    };
    Ok(feed)
}

/// The repeater or comparator to put in place of the dust at `top`, in
/// `state`, on output block `block`, and any block taken out of its
/// front; or where and why there is none.
fn dust_output(
    blocks: &Blocks,
    block: Pos,
    top: Pos,
    state: &str,
    comparator: impl Fn(Pos) -> bool,
) -> Result<(String, Option<(Pos, String)>), Miss> {
    let feed = feed_side(blocks, top)?;
    // A block above or below it powered strongly drives the dust too, as
    // a torch under the output block does, which a repeater wouldn't take.
    let over = schem::step(top, Direction::Up);
    if schem::short(blocks.get(over)) == "redstone_wall_torch" {
        return Err((over, "a torch over its dust drives it too".to_owned()));
    }
    for side in [Direction::Up, Direction::Down] {
        let next = schem::step(top, side);
        if strongly_powered(blocks, next) {
            return Err((
                next,
                format!(
                    "the {} {} its dust is powered strongly, driving it too",
                    schem::short(blocks.get(next)).replace('_', " "),
                    if side == Direction::Up {
                        "over"
                    } else {
                        "under"
                    }
                ),
            ));
        }
    }
    let from = schem::step(top, feed);
    feeds_a_diode(blocks, from, top).map_err(|why| (from, why))?;
    // What points into the new component's sides would lock a repeater or
    // feed a comparator's side.
    for side in [feed.turned(), feed.turned().opposite()] {
        let next = schem::step(top, side);
        let state = blocks.get(next);
        if matches!(schem::short(state), "repeater" | "comparator")
            && Direction::facing(state)
                .is_some_and(|f| schem::step(next, f.opposite()) == top)
        {
            return Err((
                next,
                format!("a {} points into its side", schem::short(state)),
            ));
        }
    }
    if let Some((at, what)) = reads_block(blocks, block) {
        return Err((at, format!("{what} reads the block under it")));
    }
    let front = schem::step(top, feed.opposite());
    let cleared = if is_air(blocks.get(front)) {
        None
    } else if clearable(blocks, front, &[top]) {
        Some((front, blocks.get(front).to_owned()))
    } else {
        return Err((
            front,
            format!(
                "{} in front, touching more",
                schem::short(blocks.get(front)).replace('_', " ")
            ),
        ));
    };
    let powered = schem::property(state, "power").is_some_and(|p| p != "0");
    let now = if comparator(block) {
        format!(
            "minecraft:comparator[facing={feed},mode=compare,\
             powered={powered}]"
        )
    } else {
        format!(
            "minecraft:repeater[delay=1,facing={feed},locked=false,\
             powered={powered}]"
        )
    };
    Ok((now, cleared))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const LEVER: &str = "lever[face=wall,facing=west,powered=false]";

    /// A lever on a block driving a line of dust east into a repeater on
    /// stone, facing `facing`, with `front` east of it.
    fn line(facing: &str, front: &str) -> Blocks {
        let repeater = format!(
            "repeater[delay=1,facing={facing},locked=false,powered=false]"
        );
        let dust =
            "redstone_wire[east=side,north=none,power=0,south=none,west=side]";
        let states: HashMap<Pos, String> = [
            ([0, 1, 0], LEVER),
            ([1, 1, 0], "white_concrete"),
            ([2, 0, 0], "stone"),
            ([2, 1, 0], dust),
            ([3, 0, 0], "stone"),
            ([3, 1, 0], repeater.as_str()),
            ([4, 1, 0], front),
        ]
        .into_iter()
        .filter(|(_, state)| *state != "air")
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        Blocks::from_grid([6, 2, 1], &states)
    }

    /// A repeater's `facing` is the side it reads from, so one facing west
    /// gives out east; with air there it powers nothing and is an output.
    #[test]
    fn a_repeater_giving_out_into_air_is_an_output() {
        let blocks = line("west", "air");
        let found = find(&blocks);
        assert_eq!(
            found,
            [Output {
                at: [3, 1, 0],
                comparator: false,
                facing: Direction::West,
            }]
        );
        assert_eq!(found[0].points(), Direction::East);
        assert_eq!(found[0].front(), [4, 1, 0]);
        let report = report(&blocks);
        assert_eq!(report.outputs.len(), 1);
        assert_eq!(report.outputs[0].bits, [[3, 0, 0]]);
        assert_eq!(report.inputs.len(), 1);
    }

    /// A repeater powering a block, or dust, carries the circuit on, and
    /// one facing the other way gives out into the dust that feeds it.
    #[test]
    fn a_repeater_powering_anything_is_no_output() {
        for front in [
            "stone",
            "redstone_wire[east=none,north=none,power=0,south=none,west=side]",
        ] {
            assert_eq!(find(&line("west", front)), [], "{front}");
        }
        assert_eq!(find(&line("east", "air")), []);
    }

    /// A comparator facing air is an output too, whatever block holds it
    /// up, and a repeater at the edge of the file gives out beyond it.
    #[test]
    fn a_comparator_on_any_block_or_one_at_the_edge_is_an_output() {
        let states: HashMap<Pos, String> = [
            ([0, 0, 0], "minecraft:lime_wool".to_owned()),
            (
                [0, 1, 0],
                "minecraft:comparator[facing=south,mode=compare,\
                 powered=false]"
                    .to_owned(),
            ),
        ]
        .into_iter()
        .collect();
        let found = find(&Blocks::from_grid([1, 2, 1], &states));
        assert_eq!(found.len(), 1);
        assert!(found[0].comparator);
        assert_eq!(found[0].points(), Direction::North);
    }

    /// Evenly spaced outputs facing one way are a bus; one facing another
    /// way beside them is a line of its own.
    #[test]
    fn outputs_facing_one_way_evenly_spaced_are_a_bus() {
        let at = |x, facing| Output {
            at: [x, 1, 0],
            comparator: false,
            facing,
        };
        let outputs = [
            at(0, Direction::North),
            at(2, Direction::North),
            at(4, Direction::North),
            at(6, Direction::South),
        ];
        let runs = lines(&outputs);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].bits, [[0, 1, 0], [2, 1, 0], [4, 1, 0]]);
        assert_eq!(runs[1].bits, [[6, 1, 0]]);
    }

    /// Dust on an output block joined to the dust driving it, one side
    /// only, becomes a repeater reading from that side and giving out into
    /// air, whatever shape its state draws; dust that joins more dust
    /// beyond it carries the signal on, and is left, with why.
    #[test]
    fn dust_fed_from_one_side_becomes_a_repeater() {
        let wire = |east: &str, west: &str| {
            format!(
                "minecraft:redstone_wire[east={east},north=none,power=0,\
                 south=none,west={west}]"
            )
        };
        let grid = |end: &str, on: bool| -> Blocks {
            let mut states: HashMap<Pos, String> = [
                ([0, 1, 0], format!("minecraft:{LEVER}")),
                ([1, 1, 0], "minecraft:white_concrete".to_owned()),
                ([2, 0, 0], "minecraft:stone".to_owned()),
                ([2, 1, 0], wire("side", "side")),
                ([3, 0, 0], "minecraft:yellow_concrete".to_owned()),
                ([3, 1, 0], end.to_owned()),
            ]
            .into_iter()
            .collect();
            if on {
                states.insert([4, 0, 0], "minecraft:stone".to_owned());
                states.insert([4, 1, 0], wire("none", "side"));
            }
            Blocks::from_grid([5, 2, 1], &states)
        };
        // A line drawn through it joins only the dust west of it.
        let (changes, done) =
            rework(&grid(&wire("side", "side"), false), &[[3, 0, 0]], |_| {
                false
            });
        assert_eq!(
            changes[&[3, 1, 0]],
            "minecraft:repeater[delay=1,facing=west,locked=false,\
             powered=false]"
        );
        assert!(matches!(done[0], Reworked::Converted { cleared: None, .. }));
        let (changes, done) =
            rework(&grid(&wire("side", "side"), true), &[[3, 0, 0]], |_| false);
        assert!(changes.is_empty());
        assert!(
            matches!(&done[0], Reworked::Not { why, .. } if why.contains("joined 2 ways"))
        );
    }

    /// The owner's hand-made one-bit adders: the first two read their sum
    /// through dust on yellow concrete, so only the carry out's repeater is
    /// an output by this rule; the third gives both out through repeaters.
    #[test]
    fn the_owners_one_bit_adders_have_repeater_outputs() {
        let count = |name: &str| {
            let data =
                std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
            find(&Blocks::read(&data).unwrap()).len()
        };
        assert_eq!(count("rca-1-repeater-a"), 1);
        assert_eq!(count("rca-1-repeater-b"), 1);
        assert_eq!(count("rca-1-repeater-c"), 2);
    }

    /// The owner's 8-bit carry-cancel adder reworked to give every output
    /// out through a repeater probes as the adder it is with those outputs
    /// alone, its sum one bus and its carry out a line.
    #[test]
    #[ignore = "probes a whole build"]
    fn a_carry_cancel_adder_with_repeater_outputs_probes_as_an_adder() {
        let data =
            std::fs::read("tests/fixtures/cca-8-repeater-out.schem").unwrap();
        let probed = probe(&data, Some(Kind::Adder))
            .unwrap()
            .unwrap_or_else(|faults| panic!("{}", faults[0].1));
        let layout = probed.layout.unwrap();
        assert_eq!(layout.sum.len(), 8);
        assert!(layout.carry_out.is_some());
    }
}
