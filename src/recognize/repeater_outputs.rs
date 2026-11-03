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
//! `convert`, and `stack-check` measure what the rule would find, `convert`
//! porting a build to it by [`tap`], as the owner ports one by hand.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::{
    category::Kind,
    recognize::{
        colour,
        inspect::{self, Problem, Report, Run},
        probe::{self, Fault, Found, Layout},
    },
    schem::{self, Blocks, Direction, Pos},
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

/// The sides a tap is tried on, the build's front, north, first.
const OUTWARD: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

/// An output, as today's rule reads it, tapped so this rule reads it: the
/// block it was read by whitened, what stands on it left, and beside it,
/// one block out of the build, a support with a repeater or comparator on
/// top reading what stands on the output block and giving out into air.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tapped {
    /// A repeater or comparator powering nothing stands on it already, and
    /// it is left as it is.
    Already { block: Pos },
    /// Tapped toward `side`: the support at `support`, white concrete, or
    /// white stained glass with `glass` as a torch is under it, and on it
    /// the repeater or comparator `now`. `clear` when every cell from the
    /// output block to the box's edge that way was air, the face the output
    /// sits on.
    Tapped {
        block: Pos,
        side: Direction,
        support: Pos,
        glass: bool,
        clear: bool,
        now: String,
    },
    /// It can't be tapped, for `why`.
    Not { block: Pos, why: String },
}

impl Tapped {
    /// The output block it was made of.
    pub fn block(&self) -> Pos {
        match self {
            Tapped::Already { block }
            | Tapped::Tapped { block, .. }
            | Tapped::Not { block, .. } => *block,
        }
    }
}

/// The default of an output block's kind: white stained glass for stained
/// glass, else white concrete.
fn whitened(state: &str) -> &'static str {
    if schem::short(state).ends_with("_stained_glass") {
        "minecraft:white_stained_glass"
    } else {
        "minecraft:white_concrete"
    }
}

/// Whether a block in a cell would be inside the box of `blocks`.
fn inside(blocks: &Blocks, pos: Pos) -> bool {
    (0..3).all(|axis| (0..blocks.size[axis]).contains(&pos[axis]))
}

/// Whether every cell from `block` out to the box's edge toward `side`, at
/// its level and the one above, is air: the face of the build the output
/// sits on.
fn clear_to_edge(blocks: &Blocks, block: Pos, side: Direction) -> bool {
    let mut at = schem::step(block, side);
    while inside(blocks, at) {
        if !is_air(blocks.get(at))
            || !is_air(blocks.get(schem::step(at, Direction::Up)))
        {
            return false;
        }
        at = schem::step(at, side);
    }
    true
}

/// The cells a tap toward `side` from `block` takes: the support, the
/// repeater on it, and the air it gives out into. Why the tap can't be
/// made, when one isn't air, or is a cell another tap took.
fn tap_cells(
    blocks: &Blocks,
    block: Pos,
    side: Direction,
    taken: &BTreeMap<Pos, String>,
) -> Result<(Pos, Pos), String> {
    let support = schem::step(block, side);
    let top = schem::step(support, Direction::Up);
    let front = schem::step(top, side);
    for (cell, what) in
        [(support, "support"), (top, "repeater"), (front, "front")]
    {
        let state = taken
            .get(&cell)
            .map_or_else(|| blocks.get(cell), String::as_str);
        if !is_air(state) {
            let reason = if what == "front" {
                "it would give out into"
            } else {
                "its place holds"
            };
            return Err(format!(
                "{side}: {} at {}, which the {what} {reason}",
                schem::short(state).replace('_', " "),
                inspect::place(cell)
            ));
        }
    }
    Ok((support, top))
}

/// What reads the output block, standing on it, that a tap can read from:
/// dust, or else why not.
fn tappable(blocks: &Blocks, top: Pos) -> Result<(), String> {
    match schem::short(blocks.get(top)) {
        "redstone_wire" => Ok(()),
        name @ ("repeater" | "comparator") => {
            let state = blocks.get(top);
            let front = Direction::facing(state)
                .map_or(top, |f| schem::step(top, f.opposite()));
            Err(format!(
                "its {name} gives out into {} at {}, and a tap reads only dust",
                schem::short(blocks.get(front)).replace('_', " "),
                inspect::place(front)
            ))
        }
        other => Err(format!("{} on it, not dust", other.replace('_', " "))),
    }
}

/// The side a run's `bits` are all tapped on, so a bus stays a bus: of
/// the sides every bit can be tapped on, as `taken` leaves them, one not
/// `along` the way copies of the build stack, whose taps would reach into
/// the next copy, then the one whose cells out to the box's edge are air
/// for the most bits, then the first in `OUTWARD`. `None` when no side
/// serves every bit.
fn run_side(
    blocks: &Blocks,
    bits: &[Pos],
    taken: &BTreeMap<Pos, String>,
    along: &[Direction],
) -> Option<Direction> {
    OUTWARD
        .iter()
        .enumerate()
        .filter(|&(_, &side)| {
            let mut taken = taken.clone();
            bits.iter().all(|&block| {
                tap_cells(blocks, block, side, &taken).is_ok_and(|(s, t)| {
                    taken.insert(s, "minecraft:stone".to_owned());
                    taken.insert(t, "minecraft:stone".to_owned());
                    true
                })
            })
        })
        .max_by_key(|&(first, &side)| {
            let clear = bits
                .iter()
                .filter(|&&block| clear_to_edge(blocks, block, side))
                .count();
            (!along.contains(&side), clear, std::cmp::Reverse(first))
        })
        .map(|(_, &side)| side)
}

/// The output on `block` tapped toward `shared`, its run's side, or with
/// none the first side it can be tapped on, those not `along` the way
/// copies stack first, then those clear to the box's edge, its changes
/// added to `changes`; or why it can't be.
fn tap_one(
    blocks: &Blocks,
    block: Pos,
    (shared, along): (Option<Direction>, &[Direction]),
    comparator: impl Fn(Pos) -> bool,
    changes: &mut BTreeMap<Pos, String>,
) -> Tapped {
    let sides = shared.map_or_else(
        || {
            let mut sides = OUTWARD.to_vec();
            sides.sort_by_key(|&side| {
                (along.contains(&side), !clear_to_edge(blocks, block, side))
            });
            sides
        },
        |side| vec![side],
    );
    let mut blocked = Vec::new();
    let mut tapped = None;
    for side in sides {
        match tap_cells(blocks, block, side, changes) {
            Ok(cells) => {
                tapped = Some((side, cells));
                break;
            }
            Err(why) => blocked.push(why),
        }
    }
    let Some((side, (support, top))) = tapped else {
        return Tapped::Not {
            block,
            why: format!("every side is blocked: {}", blocked.join("; ")),
        };
    };
    // A torch powers the block over it, and a solid block over dust cuts
    // the dust's climb to dust beside it, as glass does neither.
    let under = schem::short(blocks.get(schem::step(support, Direction::Down)));
    let glass = matches!(
        under,
        "redstone_torch" | "redstone_wall_torch" | "redstone_wire"
    );
    let reads = side.opposite();
    let now = if comparator(block) {
        format!(
            "minecraft:comparator[facing={reads},mode=compare,powered=false]"
        )
    } else {
        format!(
            "minecraft:repeater[delay=1,facing={reads},locked=false,\
             powered=false]"
        )
    };
    changes.insert(block, whitened(blocks.get(block)).to_owned());
    let held = if glass {
        "minecraft:white_stained_glass"
    } else {
        "minecraft:white_concrete"
    };
    changes.insert(support, held.to_owned());
    changes.insert(top, now.clone());
    Tapped::Tapped {
        block,
        side,
        support,
        glass,
        clear: clear_to_edge(blocks, block, side),
        now,
    }
}

/// Each run of `outputs`, today's rule's, tapped as the owner taps them by
/// hand: each output block whitened, the dust on it left, and one block out
/// of the build beside it a support, white concrete, or white stained glass
/// over a torch, which powers the block above it, or over dust, whose climb
/// a solid block would cut, with a repeater on top,
/// or a comparator where `comparator` says the output is read as a
/// strength, reading the dust and giving out into air. A run's bits are
/// tapped the same way, so a bus stays a bus: among the sides every bit
/// can be tapped on, one not `along` the way copies of the build stack,
/// whose taps would reach into the next copy, then the side whose cells
/// out to the box's edge are air for most of its bits, the face it sits
/// on, then the front, north, before others alike; failing one, each bit
/// its own. A cell beyond the box is air, which the box grows to hold. The
/// changes, some maybe beyond the box, with what came of each output.
pub fn tap(
    blocks: &Blocks,
    outputs: &[Run],
    comparator: impl Fn(Pos) -> bool,
    along: &[Direction],
) -> (BTreeMap<Pos, String>, Vec<Tapped>) {
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    let mut done = Vec::new();
    let found: Vec<Pos> = find(blocks).iter().map(|o| o.block()).collect();
    for run in outputs {
        let mut bits = Vec::new();
        for &block in &run.bits {
            let top = schem::step(block, Direction::Up);
            if found.contains(&block) {
                done.push(Tapped::Already { block });
            } else if let Err(why) = tappable(blocks, top) {
                done.push(Tapped::Not { block, why });
            } else {
                bits.push(block);
            }
        }
        let shared = run_side(blocks, &bits, &changes, along);
        for block in bits {
            done.push(tap_one(
                blocks,
                block,
                (shared, along),
                &comparator,
                &mut changes,
            ));
        }
    }
    (changes, done)
}

/// The file in `data` with each output of `outputs`, today's, tapped as
/// [`tap`] taps them, `along` the ways copies stack, its box grown to hold the taps, then as `//update`
/// leaves it, [`crate::schem::rstack::settle`]: dust shaped as the game shapes it, then
/// every component at rest, each lever as it was. The file, how far every
/// old position moved, and what came of each output; no file unless every
/// output was tapped or already one.
pub fn tapped(
    data: &[u8],
    outputs: &[Run],
    comparator: impl Fn(Pos) -> bool,
    along: &[Direction],
) -> Result<(Option<Vec<u8>>, Pos, Vec<Tapped>)> {
    let blocks = Blocks::read(data)?;
    let (changes, done) = tap(&blocks, outputs, comparator, along);
    if changes.is_empty()
        || done.iter().any(|t| matches!(t, Tapped::Not { .. }))
    {
        return Ok((None, [0; 3], done));
    }
    let (file, moved) = crate::schem::standardize::edit_grown(data, &changes)?;
    let file = crate::schem::rstack::settle(&file)?;
    Ok((Some(file), moved, done))
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

    /// An output block, yellow concrete at (2, 1, 2), with dust on it fed
    /// from the south by dust on stone, in a box 5 by 4 by 5, with `extra`
    /// blocks put in or, as air, taken out.
    fn tap_fixture(extra: &[(Pos, &str)]) -> Blocks {
        let wire = "redstone_wire[east=none,north=side,power=0,south=side,\
                    west=none]";
        let mut states: HashMap<Pos, String> = [
            ([2, 1, 2], "yellow_concrete"),
            ([2, 2, 2], wire),
            ([2, 1, 3], "stone"),
            ([2, 2, 3], wire),
            ([2, 1, 4], "stone"),
            ([2, 2, 4], wire),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        for &(pos, state) in extra {
            if state == "air" {
                states.remove(&pos);
            } else {
                states.insert(pos, format!("minecraft:{state}"));
            }
        }
        Blocks::from_grid([5, 4, 5], &states)
    }

    /// The one output of `tap_fixture` tapped, with `along` the sides
    /// copies stack toward.
    fn tap_one_output(
        blocks: &Blocks,
        along: &[Direction],
    ) -> (BTreeMap<Pos, String>, Tapped) {
        let run = Run {
            bits: vec![[2, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, mut done) = tap(blocks, &[run], |_| false, along);
        assert_eq!(done.len(), 1);
        (changes, done.remove(0))
    }

    /// With every side open, the tap goes north, the front: the output
    /// block whitened, a support of white concrete north of it, and on
    /// that a repeater facing south, reading the dust on the output block
    /// and giving out north into air.
    #[test]
    fn a_tap_goes_north_by_default() {
        let (changes, tapped) = tap_one_output(&tap_fixture(&[]), &[]);
        assert_eq!(
            tapped,
            Tapped::Tapped {
                block: [2, 1, 2],
                side: Direction::North,
                support: [2, 1, 1],
                glass: false,
                clear: true,
                now: "minecraft:repeater[delay=1,facing=south,locked=false,\
                      powered=false]"
                    .to_owned(),
            }
        );
        assert_eq!(changes[&[2, 1, 2]], "minecraft:white_concrete");
        assert_eq!(changes[&[2, 1, 1]], "minecraft:white_concrete");
        assert_eq!(changes.len(), 3);
        // Nothing else changes: the dust on the output block stays.
        assert!(!changes.contains_key(&[2, 2, 2]));
    }

    /// A block further north, out to the box's edge, makes north no face
    /// the output sits on though a tap fits there, so the tap goes to a
    /// side clear to the edge, west before east.
    #[test]
    fn a_tap_goes_toward_the_face_the_output_sits_on() {
        let blocks = tap_fixture(&[([2, 1, 0], "stone")]);
        let (_, tapped) = tap_one_output(&blocks, &[]);
        assert!(matches!(
            tapped,
            Tapped::Tapped {
                side: Direction::West,
                clear: true,
                ..
            }
        ));
    }

    /// A side copies stack toward is tried last, as a tap there would
    /// reach into the next copy.
    #[test]
    fn a_tap_avoids_the_way_copies_stack() {
        let along = [Direction::North, Direction::South];
        let (_, tapped) = tap_one_output(&tap_fixture(&[]), &along);
        assert!(matches!(
            tapped,
            Tapped::Tapped {
                side: Direction::West,
                ..
            }
        ));
    }

    /// A torch under the support would power it, and dust under it would
    /// have its climb cut, so the support is white stained glass; a
    /// stained glass output block is whitened as stained glass.
    #[test]
    fn a_support_over_a_torch_or_dust_is_glass() {
        for under in [
            "redstone_torch[lit=true]",
            "redstone_wall_torch[facing=north,lit=true]",
            "redstone_wire[east=none,north=none,power=0,south=none,\
             west=none]",
        ] {
            let blocks = tap_fixture(&[([2, 0, 1], under)]);
            let (changes, tapped) = tap_one_output(&blocks, &[]);
            assert!(
                matches!(tapped, Tapped::Tapped { glass: true, .. }),
                "{under}"
            );
            assert_eq!(changes[&[2, 1, 1]], "minecraft:white_stained_glass");
        }
        let blocks = tap_fixture(&[([2, 1, 2], "yellow_stained_glass")]);
        let (changes, tapped) = tap_one_output(&blocks, &[]);
        assert!(matches!(tapped, Tapped::Tapped { glass: false, .. }));
        assert_eq!(changes[&[2, 1, 2]], "minecraft:white_stained_glass");
        assert_eq!(changes[&[2, 1, 1]], "minecraft:white_concrete");
    }

    /// Where the support, the repeater, or the air it gives out into is
    /// taken on every side, the output isn't tapped, and why names each
    /// side's block; a cell beyond the box is air.
    #[test]
    fn an_output_blocked_on_every_side_is_not_tapped() {
        let blocks = tap_fixture(&[
            ([2, 2, 1], "stone"),
            ([1, 1, 2], "stone"),
            ([4, 2, 2], "stone"),
        ]);
        let (changes, tapped) = tap_one_output(&blocks, &[]);
        assert!(changes.is_empty());
        let Tapped::Not { why, .. } = tapped else {
            panic!("tapped: {tapped:?}");
        };
        assert!(why.starts_with("every side is blocked"), "{why}");
        for at in ["(2, 2, 1)", "(1, 1, 2)", "(4, 2, 2)", "(2, 1, 3)"] {
            assert!(why.contains(at), "{at} in {why}");
        }
        // At the west edge, closed north, south, and east, where its
        // repeater would give out into the dust of the other output, the
        // tap goes beyond the box.
        let blocks = tap_fixture(&[
            ([0, 2, 1], "stone"),
            ([0, 1, 3], "stone"),
            ([0, 1, 2], "yellow_concrete"),
            (
                [0, 2, 2],
                "redstone_wire[east=side,north=none,power=0,\
                          south=none,west=none]",
            ),
        ]);
        let run = Run {
            bits: vec![[0, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, done) = tap(&blocks, &[run], |_| false, &[]);
        assert!(matches!(
            done[0],
            Tapped::Tapped {
                side: Direction::West,
                support: [-1, 1, 2],
                ..
            }
        ));
        assert!(changes.contains_key(&[-1, 2, 2]));
    }

    /// An output read as a strength takes a comparator, and one read by a
    /// repeater that powers something can't be tapped, a tap reading dust
    /// only.
    #[test]
    fn a_strength_takes_a_comparator_and_a_repeater_reader_is_left() {
        let run = Run {
            bits: vec![[2, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, _) =
            tap(&tap_fixture(&[]), std::slice::from_ref(&run), |_| true, &[]);
        assert!(
            changes[&[2, 2, 1]]
                .starts_with("minecraft:comparator[facing=south")
        );
        let blocks = tap_fixture(&[
            (
                [2, 2, 2],
                "repeater[delay=1,facing=south,locked=false,powered=false]",
            ),
            ([2, 2, 1], "stone"),
        ]);
        let (changes, done) = tap(&blocks, &[run], |_| false, &[]);
        assert!(changes.is_empty());
        assert!(
            matches!(&done[0], Tapped::Not { why, .. } if why.contains("reads only dust"))
        );
    }

    /// A tapped file is settled as `//update` leaves it: the box grown to
    /// hold the taps, the dust on each output block shaped to join its
    /// repeater, so settling it again changes nothing, and the proposed
    /// rule finds every tap an output.
    #[test]
    fn a_tapped_file_is_settled() {
        let data = std::fs::read("tests/fixtures/inc-8.schem").unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let today = inspect::inspect_marked(&blocks, inspect::SITE);
        let (file, moved, done) =
            tapped(&data, &today.outputs, |_| false, &[]).unwrap();
        let file = file.expect("every output tapped");
        assert_eq!(
            crate::schem::rstack::settle(&file).unwrap(),
            file,
            "settled already"
        );
        let after = Blocks::read(&file).unwrap();
        let found: Vec<Pos> = find(&after).iter().map(|o| o.block()).collect();
        assert!(!done.is_empty());
        for tapped in &done {
            let Tapped::Tapped {
                block,
                side,
                support,
                ..
            } = tapped
            else {
                panic!("not tapped: {tapped:?}");
            };
            let top = schem::step(schem::add(*block, moved), Direction::Up);
            let dust = after.get(top);
            assert_eq!(schem::short(dust), "redstone_wire");
            assert_eq!(schem::property(dust, side.name()), Some("side"));
            assert_eq!(
                schem::short(after.get(schem::add(*block, moved))),
                "white_concrete"
            );
            assert!(found.contains(&schem::add(*support, moved)));
        }
    }
}
