//! The rule the site reads outputs and signal-strength inputs by, in place
//! of their colours: an output is a repeater or comparator powering
//! nothing, the cell in front of it, the side it gives out from, air. A
//! repeater gives a bit, on or off; whether a comparator's strength is read
//! as a bit or a value from 0 to 15 its category's test decides. Colour
//! plays no part, so any block may hold one up. Outputs side by side,
//! evenly spaced and facing the same way, are a bus; one alone is a line.
//! Inputs are levers and buttons on any block, and, for a hex digit, a
//! comparator with nothing behind it, the cell its `facing` points at air,
//! giving out into the build, which a tester drives by putting a barrel
//! there; these too are read by where they stand, not their colours.
//!
//! [`crate::interface::inspect`] checks a build by this rule, and
//! [`interface`] reads its lines' roles by their shapes.

use std::collections::{BTreeMap, HashMap};

use crate::{
    interface::{
        self, Bit, Interface, Port,
        inspect::{self, Run},
    },
    schem::{self, Blocks, Direction, Pos, lines::Role},
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

    /// Where a sign naming it goes: a wall sign on the outward face of the
    /// block holding it up, under its front, which stays air.
    pub fn sign_place(self) -> Pos {
        schem::step(self.block(), self.points())
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

/// The sign naming each output of `blocks`, by the block holding the
/// output up: a wall sign on that block's outward face, under the
/// output's front, see [`Output::sign_place`]. A sign anywhere else on the
/// block names nothing.
pub fn signs(blocks: &Blocks) -> HashMap<Pos, &[String]> {
    find(blocks)
        .into_iter()
        .filter_map(|output| {
            let place = output.sign_place();
            let lines = blocks.signs.get(&place)?;
            (blocks.sign_block(place) == Some(output.block()))
                .then_some((output.block(), lines.as_slice()))
        })
        .collect()
}

/// `outputs` grouped into buses, each a line of outputs of one kind,
/// repeaters or comparators, evenly spaced and facing the same way, or two
/// such lines interleaved, facing the same way or opposite ways, the
/// longest first, then the lone lines; each run is of the outputs' own
/// places.
pub fn lines(outputs: &[Output]) -> Vec<Run> {
    let bits: Vec<(Pos, (Direction, bool))> = outputs
        .iter()
        .map(|o| (o.at, (o.facing, o.comparator)))
        .collect();
    // Two columns may give out on opposite faces, as a tall build's do.
    interleaved(inspect::runs(&bits), &bits, |a, b| {
        a.1 == b.1 && (a.0 == b.0 || a.0 == b.0.opposite())
    })
}

/// `runs` of `bits`, each with its kind, with each two that interleave
/// made one bus: as many bits each, two or more, the same step, kinds
/// that `alike` takes as one, the step even along one axis, and each bit
/// of one halfway between two of the other along it, as a bus whose bits
/// alternate between two columns, a layer apart where each column's are
/// two. Of the runs one could interleave with, the nearest; the longest
/// first again.
fn interleaved<K: Copy>(
    runs: Vec<Run>,
    bits: &[(Pos, K)],
    alike: impl Fn(K, K) -> bool,
) -> Vec<Run> {
    let kind: BTreeMap<Pos, K> = bits.iter().copied().collect();
    // The axis two runs interleave along and which comes first, if they do.
    let interleave = |a: &Run, b: &Run| -> Option<(usize, bool)> {
        if a.bits.len() != b.bits.len()
            || a.bits.len() < 2
            || a.step != b.step
            || !alike(kind[&a.bits[0]], kind[&b.bits[0]])
        {
            return None;
        }
        let axis = (0..3).find(|&axis| {
            a.step[axis] != 0
                && a.step[axis] % 2 == 0
                && (0..3).all(|o| o == axis || a.step[o] == 0)
        })?;
        let half = a.step[axis] / 2;
        let offset = |from: &Run, to: &Run, by: i32| {
            from.bits
                .iter()
                .zip(&to.bits)
                .all(|(f, t)| t[axis] == f[axis] + by)
        };
        if offset(a, b, half) {
            Some((axis, true))
        } else if offset(b, a, half) {
            Some((axis, false))
        } else {
            None
        }
    };
    let apart = |a: &Run, b: &Run| schem::distance(a.bits[0], b.bits[0]);
    let mut pairs: Vec<(i32, usize, usize)> = Vec::new();
    for i in 0..runs.len() {
        for j in i + 1..runs.len() {
            if interleave(&runs[i], &runs[j]).is_some() {
                pairs.push((apart(&runs[i], &runs[j]), i, j));
            }
        }
    }
    pairs.sort_unstable();
    let mut used = vec![false; runs.len()];
    let mut found = Vec::new();
    for (_, i, j) in pairs {
        if used[i] || used[j] {
            continue;
        }
        used[i] = true;
        used[j] = true;
        let Some((axis, a_first)) = interleave(&runs[i], &runs[j]) else {
            continue;
        };
        let (first, second) = if a_first {
            (&runs[i], &runs[j])
        } else {
            (&runs[j], &runs[i])
        };
        let mut step = [0; 3];
        step[axis] = first.step[axis] / 2;
        found.push(Run {
            bits: first
                .bits
                .iter()
                .zip(&second.bits)
                .flat_map(|(&f, &s)| [f, s])
                .collect(),
            step,
        });
    }
    found.extend(
        runs.into_iter()
            .zip(used)
            .filter(|(_, used)| !used)
            .map(|(run, _)| run),
    );
    found.sort_by_key(|run| std::cmp::Reverse(run.bits.len()));
    found
}

/// A comparator with nothing behind it, the cell its `facing` points at
/// air, giving out into the build: a signal-strength input, a hex digit
/// from 0 to 15, as strong as what a tester puts behind it, a barrel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrengthInput {
    /// Where the comparator stands.
    pub at: Pos,
    /// Its block state's `facing`: the side it reads from, where the
    /// tester's barrel goes.
    pub facing: Direction,
}

impl StrengthInput {
    /// The cell it reads from, which is air.
    pub fn behind(self) -> Pos {
        schem::step(self.at, self.facing)
    }

    /// The cell it gives out into, the build's.
    pub fn front(self) -> Pos {
        schem::step(self.at, self.facing.opposite())
    }
}

/// The comparator at `at` as a signal-strength input, when it is one: air
/// behind it, beyond the file's box too, and something in front of it.
pub fn strength_input(blocks: &Blocks, at: Pos) -> Option<StrengthInput> {
    let state = blocks.get(at);
    if schem::short(state) != "comparator" {
        return None;
    }
    let input = StrengthInput {
        at,
        facing: Direction::facing(state)?,
    };
    (is_air(blocks.get(input.behind())) && !is_air(blocks.get(input.front())))
        .then_some(input)
}

/// Every signal-strength input in `blocks`, in the file's order.
pub fn strength_inputs(blocks: &Blocks) -> Vec<StrengthInput> {
    blocks
        .iter()
        .filter_map(|(at, _)| strength_input(blocks, at))
        .collect()
}

/// `inputs` grouped into buses as [`lines`] groups outputs, each run of
/// the comparators' own places.
pub fn strength_lines(inputs: &[StrengthInput]) -> Vec<Run> {
    let bits: Vec<(Pos, Direction)> =
        inputs.iter().map(|i| (i.at, i.facing)).collect();
    interleaved(inspect::runs(&bits), &bits, |a, b| a == b)
}

/// The signal-strength input driving the digit read at `bit`: the
/// comparator on `bit`, the block under it, as the site reads a digit, or
/// at `bit` itself; or one beside the dust on `bit` pointing into it, as a
/// port of a build whose digit inputs the old rule read by their colour
/// keeps that block.
pub fn driving(blocks: &Blocks, bit: Pos) -> Option<StrengthInput> {
    let on = schem::step(bit, Direction::Up);
    if let Some(input) =
        strength_input(blocks, on).or_else(|| strength_input(blocks, bit))
    {
        return Some(input);
    }
    let dust = on;
    if schem::short(blocks.get(dust)) != "redstone_wire" {
        return None;
    }
    Direction::SIDES.into_iter().find_map(|side| {
        strength_input(blocks, schem::step(dust, side))
            .filter(|input| input.front() == dust)
    })
}

/// A build's lines as this rule reads them, named for the roles their
/// shapes give them, as a test reads the roles probing finds: its
/// signal-strength buses A, B, and on, the longest first, then its buses
/// of levers; a lone lever its control line, such as a carry in; each bus
/// of outputs an output, a comparator's read as a strength in a build
/// without signal-strength inputs, whose outputs each show a digit; and a
/// lone output beside a bus, or a lone repeater beside a comparator of a
/// build with signal-strength inputs, its status line, such as a carry
/// out. Each line is read by the blocks under its comparators and
/// repeaters, as [`inspect::inspect`] finds them.
pub fn interface(blocks: &Blocks) -> Interface {
    let switches: BTreeMap<Pos, schem::Switch> = blocks
        .switches()
        .into_iter()
        .map(|switch| (switch.block, switch))
        .collect();
    let report = inspect::inspect(blocks);
    let mut letters = (b'A'..=b'Z').map(|l| char::from(l).to_string());
    let mut operands = 0..=u8::MAX;
    let mut operand = |bits: Vec<Bit>, strength: bool| Port {
        name: letters.next().unwrap_or_default(),
        role: operands.next().map(Role::Operand),
        labelled: false,
        bits,
        strength: 1,
        words: Vec::new(),
        signal_strength: strength,
    };
    let mut inputs = Vec::new();
    for run in &report.digits {
        let mut bits = run.bits.clone();
        interface::lsb_first(blocks, &mut bits);
        let bits = bits.into_iter().map(Bit::at).collect();
        inputs.push(operand(bits, true));
    }
    let mut control = Vec::new();
    for run in &report.inputs {
        let mut members = run.bits.clone();
        interface::lsb_first(blocks, &mut members);
        let bits: Vec<Bit> = members
            .iter()
            .filter_map(|pos| {
                let switch = switches.get(pos)?;
                Some(Bit {
                    pos: *pos,
                    entry: Some(switch.side),
                    hard: Direction::ALL.iter().any(|&side| {
                        schem::short(blocks.get(schem::step(*pos, side)))
                            == "redstone_wire"
                    }),
                    inverted: switch.on,
                })
            })
            .collect();
        if bits.len() == 1 {
            control.push(Port {
                name: "control line".to_owned(),
                role: Some(Role::Control),
                labelled: false,
                bits,
                strength: 1,
                words: Vec::new(),
                signal_strength: false,
            });
        } else {
            inputs.push(operand(bits, false));
        }
    }
    inputs.extend(control);
    let digits = !report.digits.is_empty();
    let found = find(blocks);
    let by_block: BTreeMap<Pos, Output> =
        found.iter().map(|&o| (o.block(), o)).collect();
    let runs: Vec<Run> = report.outputs;
    let buses = runs.iter().filter(|run| run.bits.len() > 1).count();
    let comparators = found.iter().any(|o| o.comparator);
    let mut outputs = Vec::new();
    let mut places = 0..=u8::MAX;
    for (i, run) in runs.iter().enumerate() {
        let mut members = run.bits.clone();
        interface::lsb_first(blocks, &mut members);
        let comparator = members
            .iter()
            .all(|pos| by_block.get(pos).is_some_and(|o| o.comparator));
        let status = members.len() == 1
            && (buses > 0 || digits && comparators && !comparator);
        let (name, role) = if status {
            ("status line".to_owned(), Some(Role::Status))
        } else if i == 0 {
            ("output".to_owned(), places.next().map(Role::Output))
        } else {
            (format!("output {}", i + 1), places.next().map(Role::Output))
        };
        outputs.push(Port {
            name,
            role,
            labelled: false,
            bits: members.into_iter().map(Bit::at).collect(),
            strength: 1,
            words: Vec::new(),
            signal_strength: comparator && !digits && !status,
        });
    }
    Interface {
        inputs,
        outputs,
        binary_outputs: false,
    }
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
        let report = inspect::inspect(&blocks);
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
        assert_eq!(count("old/rca-1-repeater-a"), 1);
        assert_eq!(count("old/rca-1-repeater-b"), 1);
        assert_eq!(count("rca-1-repeater-c"), 2);
    }

    /// The owner's 8-bit carry-cancel adder reworked to give every output
    /// out through a repeater: its sum is one bus and its carry out a
    /// line.
    #[test]
    fn a_carry_cancel_adders_repeater_outputs_are_a_bus_and_a_line() {
        let data =
            std::fs::read("tests/fixtures/cca-8-repeater-out.schem").unwrap();
        let outputs = lines(&find(&Blocks::read(&data).unwrap()));
        let mut widths: Vec<usize> =
            outputs.iter().map(|run| run.bits.len()).collect();
        widths.sort_unstable();
        assert_eq!(widths, [1, 8]);
    }

    /// Lime concrete with dust on it, open to the south, a digit input as
    /// the old rule read it by its colour, in a box 3 by 3 by 3.
    fn pad(extra: &[(Pos, &str)]) -> Blocks {
        let wire = "redstone_wire[east=none,north=side,power=0,south=side,\
                    west=none]";
        let mut states: HashMap<Pos, String> = [
            ([1, 0, 1], "lime_concrete"),
            ([1, 1, 1], wire),
            ([1, 0, 0], "stone"),
            ([1, 1, 0], wire),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        for &(pos, state) in extra {
            states.insert(pos, format!("minecraft:{state}"));
        }
        Blocks::from_grid([3, 3, 3], &states)
    }

    const COMPARATOR: &str = "comparator[facing=south,mode=compare,\
                              powered=false]";

    /// A comparator with air behind it giving out into dust is a
    /// signal-strength input, and drives the digit on the block under the
    /// dust; with a block behind it, it reads that, and is none, and one
    /// giving out into air is an output.
    #[test]
    fn a_comparator_with_air_behind_is_a_strength_input() {
        let blocks = pad(&[([1, 1, 2], COMPARATOR)]);
        let found = strength_inputs(&blocks);
        assert_eq!(
            found,
            [StrengthInput {
                at: [1, 1, 2],
                facing: Direction::South,
            }]
        );
        assert_eq!(found[0].front(), [1, 1, 1]);
        assert_eq!(driving(&blocks, [1, 0, 1]), Some(found[0]));
        assert!(find(&blocks).is_empty());
        let behind = Blocks::from_grid(
            [3, 3, 4],
            &[
                ([1, 1, 1], "minecraft:redstone_wire".to_owned()),
                ([1, 1, 2], format!("minecraft:{COMPARATOR}")),
                ([1, 1, 3], "minecraft:barrel".to_owned()),
            ]
            .into_iter()
            .collect(),
        );
        assert!(strength_inputs(&behind).is_empty());
        let alone = Blocks::from_grid(
            [1, 1, 1],
            &[([0, 0, 0], format!("minecraft:{COMPARATOR}"))]
                .into_iter()
                .collect(),
        );
        assert!(strength_inputs(&alone).is_empty());
        assert_eq!(find(&alone).len(), 1);
    }

    /// Two lines evenly spaced two apart, each a layer above the other's,
    /// are one bus whose bits alternate between them, as a tall hex
    /// adder's digits do.
    #[test]
    fn two_interleaved_lines_are_one_bus() {
        let at = |x, y| StrengthInput {
            at: [x, y, 0],
            facing: Direction::North,
        };
        let inputs = [at(5, 3), at(5, 5), at(5, 7), at(6, 4), at(6, 6)];
        let runs = strength_lines(&inputs);
        assert_eq!(runs.len(), 2);
        let inputs =
            [at(5, 3), at(5, 5), at(5, 7), at(6, 4), at(6, 6), at(6, 8)];
        let runs = strength_lines(&inputs);
        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].bits,
            [
                [5, 3, 0],
                [6, 4, 0],
                [5, 5, 0],
                [6, 6, 0],
                [5, 7, 0],
                [6, 8, 0]
            ]
        );
    }

    /// A repeater on stone giving out east with a wall sign at `sign`
    /// facing `facing`, saying `carry out`.
    fn signed_output(sign: Pos, facing: &str) -> Blocks {
        let states: HashMap<Pos, String> = [
            ([1, 0, 1], "stone".to_owned()),
            (
                [1, 1, 1],
                "repeater[delay=1,facing=west,locked=false,powered=false]"
                    .to_owned(),
            ),
            (
                sign,
                format!("oak_wall_sign[facing={facing},waterlogged=false]"),
            ),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        let mut blocks = Blocks::from_grid([3, 2, 3], &states);
        let text = ["carry out", "", "", ""].map(str::to_owned).to_vec();
        blocks.signs.insert(sign, text);
        blocks
    }

    /// An output's sign is the one on the face of the block holding it up
    /// under its front; one on another face names nothing.
    #[test]
    fn an_outputs_sign_goes_under_its_front() {
        let under = signed_output([2, 0, 1], "east");
        let named = signs(&under);
        assert_eq!(
            named.get(&[1, 0, 1]).and_then(|lines| lines.first()),
            Some(&"carry out".to_owned())
        );
        let beside = signed_output([1, 0, 2], "south");
        assert!(signs(&beside).is_empty());
        assert_eq!(find(&beside)[0].sign_place(), [2, 0, 1]);
    }
}
