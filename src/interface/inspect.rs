//! The check of a build by its function rather than its colours: its
//! inputs are its levers and buttons, and its comparators with nothing
//! behind them, found wherever they are, and its outputs its repeaters and
//! comparators powering nothing, on any block. What the site can put right
//! itself, it does and lists; what only the author can, it names with the
//! blocks at fault.
//!
//! The rules:
//!
//! - An input is a lever, or any button for a pulse, on a side of a solid
//!   block, a lamp too, its input block. The lever touches nothing else
//!   that carries power.
//! - A signal-strength input, taking a value from 0 to 15, is a comparator
//!   with nothing behind it, the cell its `facing` points at air, giving
//!   out into the build; a tester drives it by putting a barrel there. See
//!   [`repeater_outputs::strength_input`].
//! - The input block drives dust on any side, on top and below too,
//!   torches, repeaters, and comparators, and nothing else powers it or
//!   touches it but plain blocks, a target, and, on top, a redstone block,
//!   none of which power a solid block. One that drives dust is hard: a bus
//!   must drive it through a repeater.
//! - The lever's spot takes a repeater pointed into the input block, so a
//!   bus can drive the input: nothing below it but air or a solid block,
//!   and nothing behind it; and the build compiled with every lever so
//!   swapped is the same circuit, the proof by the rules of redstone.
//! - Every output, data or status, is a repeater or comparator powering
//!   nothing, the cell in front of it air, on a block of any kind or
//!   colour; see [`repeater_outputs::find`]. A repeater gives a bit; whether
//!   a comparator's strength is read as a bit or a value from 0 to 15 its
//!   category's test decides. Each is read by the block holding it up, as
//!   a signal-strength input is.
//! - A bus's bits run in a line, evenly spaced: straight, or rising one
//!   block for each block across, or two such lines with their bits
//!   alternating, as [`repeater_outputs::lines`] groups outputs.

use std::collections::{BTreeMap, HashSet};

use crate::{
    interface::repeater_outputs,
    schem::{self, Blocks, Direction, Pos, Switch, block},
};

/// What the check found: what it puts right, and what it can't.
#[derive(Default, Debug)]
pub struct Report {
    pub fixes: Vec<String>,
    pub errors: Vec<Problem>,
    /// What else it found, such as which inputs need strong power.
    pub notes: Vec<String>,
    /// The levers and buttons, each by its input block, grouped into buses
    /// and lone lines.
    pub inputs: Vec<Run>,
    /// The signal-strength inputs, each by the block under its comparator,
    /// grouped as outputs are.
    pub digits: Vec<Run>,
    /// The outputs, each by the block under its repeater or comparator,
    /// grouped into buses and lone lines.
    pub outputs: Vec<Run>,
}

/// Something only the author can put right.
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

/// What the upload page says of a build with no inputs, which tells how
/// to mark one.
pub const NO_INPUTS: &str = "Inputs: none found — mark each with a lever \
                             or button, or a comparator with air behind for \
                             a signal strength";

/// What the upload page says of a build with no outputs, which tells how
/// to mark one.
pub const NO_OUTPUTS: &str =
    "Outputs: none found — mark each with a repeater or comparator facing air";

/// The check of a build by function: its levers and buttons, its
/// signal-strength inputs, and its outputs, each read by the block
/// holding it, its colour playing no part.
pub fn inspect(blocks: &Blocks) -> Report {
    let mut report = Report::default();
    let inputs = inputs(blocks, &mut report);
    report.inputs = runs(
        &inputs
            .iter()
            .map(|input| {
                (input.switch.block, (input.switch.side, input.switch.button))
            })
            .collect::<Vec<_>>(),
    );
    report.digits = digits(blocks, &mut report);
    if blocks.switches().is_empty() && report.digits.is_empty() {
        problem(&mut report, "no inputs", Vec::new(), NO_INPUTS.to_owned());
    }
    report.outputs = outputs(blocks, &mut report);
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

/// Each of `found`, standing at the position `at` gives, by the block
/// under it, or a problem `rule` saying `what` stands at the file's bottom
/// with nothing under it, where the site can't read it.
fn held<T>(
    found: Vec<T>,
    at: impl Fn(&T) -> Pos,
    (rule, what): (&'static str, impl Fn(&T) -> String),
    report: &mut Report,
) -> Vec<T> {
    let (held, bottom): (Vec<T>, Vec<T>) =
        found.into_iter().partition(|one| at(one)[1] > 0);
    for one in &bottom {
        problem(
            report,
            rule,
            vec![at(one)],
            format!(
                "{} at {}: nothing under it in the file",
                what(one),
                place(at(one))
            ),
        );
    }
    held
}

/// The signal-strength inputs, comparators with air behind giving out into
/// the build, grouped into buses as [`repeater_outputs::strength_lines`]
/// groups them, each run of the blocks under them.
fn digits(blocks: &Blocks, report: &mut Report) -> Vec<Run> {
    let found = held(
        repeater_outputs::strength_inputs(blocks),
        |input| input.at,
        (
            "input at the bottom",
            |_: &repeater_outputs::StrengthInput| "Input comparator".to_owned(),
        ),
        report,
    );
    under(repeater_outputs::strength_lines(&found))
}

/// The outputs, repeaters and comparators powering nothing, grouped into
/// buses as [`repeater_outputs::lines`] groups them, each run of the blocks
/// under them.
fn outputs(blocks: &Blocks, report: &mut Report) -> Vec<Run> {
    let found = repeater_outputs::find(blocks);
    if found.is_empty() {
        problem(report, "no outputs", Vec::new(), NO_OUTPUTS.to_owned());
    }
    let found = held(
        found,
        |output| output.at,
        (
            "output at the bottom",
            |output: &repeater_outputs::Output| {
                format!("Output {}", output.kind())
            },
        ),
        report,
    );
    under(repeater_outputs::lines(&found))
}

/// `runs` of components, each of the blocks under them.
fn under(runs: Vec<Run>) -> Vec<Run> {
    runs.into_iter()
        .map(|run| Run {
            bits: run
                .bits
                .iter()
                .map(|&at| schem::step(at, Direction::Down))
                .collect(),
            step: run.step,
        })
        .collect()
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
    /// The inputs and outputs found, in short, as the upload page shows
    /// them: each run by its length, a lone line as 1, such as `Inputs:
    /// switches 8, 8, 1; comparators 8, 8. Outputs: 8, 1.`
    pub fn summary(&self) -> String {
        let lengths = |runs: &[Run]| {
            runs.iter()
                .map(|run| run.bits.len().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut inputs = Vec::new();
        if !self.inputs.is_empty() {
            inputs.push(format!("switches {}", lengths(&self.inputs)));
        }
        if !self.digits.is_empty() {
            inputs.push(format!("comparators {}", lengths(&self.digits)));
        }
        let inputs = if inputs.is_empty() {
            "none".to_owned()
        } else {
            inputs.join("; ")
        };
        let outputs = if self.outputs.is_empty() {
            "none".to_owned()
        } else {
            lengths(&self.outputs)
        };
        format!("Inputs: {inputs}. Outputs: {outputs}.")
    }

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

pub(crate) fn problem(
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

pub(crate) fn short(blocks: &Blocks, pos: Pos) -> &str {
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
pub(crate) fn free_behind(blocks: &Blocks, switch: &Switch) -> bool {
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

/// The positions grouped into runs: lines of evenly spaced bits, the
/// longest first, each bit sharing its `kind` (for an input, its lever's
/// side and whether it is a button) with the rest of its run; what is
/// left, lone lines.
pub fn runs<K: PartialEq + Copy>(bits: &[(Pos, K)]) -> Vec<Run> {
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

    /// The outputs `inspect` finds of a block `under` with `on` on top, a
    /// repeater or dust, and `front` north of it.
    fn outputs_of(under: &str, on: &str, front: &str) -> Vec<Run> {
        let states: HashMap<Pos, String> =
            [([0, 0, 1], under), ([0, 1, 1], on), ([0, 1, 0], front)]
                .into_iter()
                .filter(|(_, state)| *state != "air")
                .map(|(pos, state)| (pos, format!("minecraft:{state}")))
                .collect();
        inspect(&Blocks::from_grid([1, 2, 2], &states)).outputs
    }

    const REPEATER: &str =
        "repeater[delay=1,facing=south,locked=false,powered=false]";

    /// A repeater facing air is an output whatever block holds it up, read
    /// by that block; one powering a block in front of it carries the
    /// circuit on and isn't one.
    #[test]
    fn a_repeater_facing_air_on_any_block_is_an_output() {
        for under in ["yellow_concrete", "white_concrete", "stone"] {
            let found = outputs_of(under, REPEATER, "air");
            assert_eq!(found.len(), 1, "{under}");
            assert_eq!(found[0].bits, [[0, 0, 1]]);
        }
        assert!(
            outputs_of("yellow_concrete", REPEATER, "black_wool").is_empty()
        );
    }

    /// Dust on a lone yellow block, the old rule's output, is no output:
    /// colour plays no part.
    #[test]
    fn dust_on_a_coloured_block_is_no_output() {
        let dust =
            "redstone_wire[east=none,north=side,power=0,south=side,west=none]";
        assert!(outputs_of("yellow_concrete", dust, "air").is_empty());
    }

    /// A comparator with air behind giving out into dust is a
    /// signal-strength input, read by the block under it; its colour plays
    /// no part, and with no lever it is input enough.
    #[test]
    fn a_comparator_with_air_behind_is_a_digit_input() {
        let states: HashMap<Pos, String> = [
            ([0, 0, 2], "white_concrete"),
            (
                [0, 1, 2],
                "comparator[facing=south,mode=compare,powered=false]",
            ),
            ([0, 0, 1], "stone"),
            (
                [0, 1, 1],
                "redstone_wire[east=none,north=side,power=0,south=side,west=none]",
            ),
            ([0, 0, 0], "stone"),
            ([0, 1, 0], REPEATER),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        let report = inspect(&Blocks::from_grid([1, 2, 4], &states));
        assert_eq!(report.digits.len(), 1);
        assert_eq!(report.digits[0].bits, [[0, 0, 2]]);
        assert!(report.inputs.is_empty());
        assert!(!report.errors.iter().any(|p| p.rule == "no inputs"));
    }

    /// The owner's 8-digit hex adder, ported: its 16 digit inputs are
    /// comparators with air behind, read as two buses of digits and no
    /// outputs, and its sum and carry out are read by their repeaters and
    /// comparators alone.
    #[test]
    fn a_hex_adders_digit_inputs_are_no_outputs() {
        let data = std::fs::read("tests/fixtures/hex-cca-8.schem").unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let report = inspect(&blocks);
        let lengths = |runs: &[Run]| -> Vec<usize> {
            runs.iter().map(|run| run.bits.len()).collect()
        };
        assert_eq!(lengths(&report.digits), [8, 8]);
        assert_eq!(lengths(&report.outputs), [8, 1]);
    }

    /// The owner's parity circuit as uploaded, its dust climbing light gray
    /// glass, with its output on a repeater: the glass under its dust is no
    /// output.
    #[test]
    fn a_parity_circuit_on_a_glass_staircase_has_its_repeater_as_output() {
        let raw = std::fs::read("tests/fixtures/parity-8.schem").unwrap();
        let blocks = Blocks::read(&raw).unwrap();
        assert_eq!(inspect(&blocks).outputs.len(), 1);
    }
}
