//! What a build's levers and outputs are, found by how it behaves rather than
//! by colour: each way its runs of levers and outputs could be an adder's
//! A, B, carry in, sum, and carry out, with each bus either way round, is
//! tried with the adder's test, and the one that adds is kept. The roles
//! found are coloured into the file, as the site's tests read them.
use std::{collections::HashMap, fmt};

use crate::{
    recognize::{
        colour,
        inspect::{self, Run},
        interface::{Bit, Interface, Port},
    },
    schem::{self, Blocks, Direction, Pos, Switch},
    simulate::{self, Test},
};

mod alu;
mod decoder;
mod hex_adder;
mod multiplier;
mod one_operand;
mod operands;
mod recognize;
mod register;
mod sequential;
mod shifter;

pub use alu::{AluProbed, alu};
pub use decoder::{DecoderProbed, decoder};
pub use hex_adder::{HexAdderProbed, hex_adder};
pub use multiplier::{MultiplierProbed, multiplier};
pub use one_operand::{Line, OneInputProbed, one_operand};
pub use operands::{OperandsProbed, operands_probe};
pub use recognize::{
    Found, Recognition, probe_as, recognize, recognize_as, recognize_watched,
    simulatable,
};
pub use register::{RegisterProbed, register};
pub use sequential::{SequentialProbed, sequential};
pub use shifter::shifter;

/// An adder's ports as found: each bus least significant bit first.
#[derive(Debug, Clone)]
pub struct Layout {
    pub a: Vec<Pos>,
    pub b: Vec<Pos>,
    pub carry_in: Option<Pos>,
    pub sum: Vec<Pos>,
    pub carry_out: Option<Pos>,
}

/// What probing found: the layout that adds, and its test's run.
pub struct Probed {
    pub layout: Layout,
    pub run: simulate::Run,
}

/// Why a probe found a build isn't the kind it tried. What it says is one
/// of the build's problems, word for word, as the site shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// Its lines don't fit the kind's roles, or its file couldn't be read
    /// or coloured, in these words.
    Unfit(String),
    /// The kind's test couldn't run on the roles tried, for this reason.
    NotRun(String),
    /// The kind's test ran on the roles coloured into `file`, getting
    /// `wrong` of its `cases` wrong: as near as a build comes to holding.
    /// `said` is that, with the first wrong case as evidence; see
    /// `Run::wrong_cases`.
    Wrong {
        wrong: u64,
        cases: u64,
        said: String,
        file: Vec<u8>,
    },
}

impl Fault {
    /// The fault of `run`, which got cases wrong on the roles coloured
    /// into `file`.
    fn wrong(run: &simulate::Run, file: &[u8]) -> Fault {
        Fault::Wrong {
            wrong: run.wrong,
            cases: run.cases,
            said: run.wrong_cases(),
            file: file.to_vec(),
        }
    }

    /// Of this, one try's fault, and `later`, the next try's, the one
    /// nearest holding: one whose test ran over one whose test couldn't,
    /// so the closest real try is the one shown; of two that ran, the one
    /// with the smaller share of its cases wrong, the earlier on a tie;
    /// else the later.
    #[must_use]
    pub fn nearer(self, later: Fault) -> Fault {
        match (&self, &later) {
            (
                Fault::Wrong { wrong, cases, .. },
                Fault::Wrong {
                    wrong: later_wrong,
                    cases: later_cases,
                    ..
                },
            ) => {
                let share =
                    |wrong: u64, of: u64| u128::from(wrong) * u128::from(of);
                if share(*later_wrong, *cases) < share(*wrong, *later_cases) {
                    later
                } else {
                    self
                }
            }
            (Fault::Wrong { .. }, _) => self,
            _ => later,
        }
    }

    /// Whether the kind's test ran, getting cases wrong.
    pub fn ran(&self) -> bool {
        matches!(self, Fault::Wrong { .. })
    }

    /// The file as the try that got cases wrong coloured it, so its test
    /// shows which.
    pub fn file(&self) -> Option<&[u8]> {
        match self {
            Fault::Wrong { file, .. } => Some(file),
            Fault::Unfit(_) | Fault::NotRun(_) => None,
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Fault::Unfit(why) => f.write_str(why),
            Fault::NotRun(reason) => write!(f, "Test couldn't run: {reason}"),
            Fault::Wrong { said, .. } => f.write_str(said),
        }
    }
}

/// A role probing tries a block in, which painting marks by its colour,
/// as the site's tests read a build's roles from colours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    A,
    B,
    /// A fused multiply-add's addend.
    C,
    /// A control line, such as a carry in.
    Control,
    /// An output bus, by its place among them, the lowest bits first; at
    /// most as many as `colour::OUTPUT_COLOURS`.
    Output(usize),
    /// A status line, such as a carry out.
    Status,
}

impl Role {
    /// The colour that marks it.
    fn colour(self) -> &'static str {
        match self {
            Role::A => colour::INPUT_COLOURS[0],
            Role::B => colour::INPUT_COLOURS[1],
            Role::C => colour::INPUT_COLOURS[2],
            Role::Control => colour::CONTROL,
            Role::Output(n) => colour::OUTPUT_COLOURS[n],
            Role::Status => colour::STATUS,
        }
    }
}

/// The role each block plays in one try, a later mark taking a block from
/// an earlier one.
#[derive(Default)]
struct Roles(HashMap<Pos, Role>);

impl Roles {
    /// Gives each of `blocks` `role`.
    fn mark<'p>(
        &mut self,
        role: Role,
        blocks: impl IntoIterator<Item = &'p Pos>,
    ) {
        self.0.extend(blocks.into_iter().map(|&pos| (pos, role)));
    }

    /// Gives each line of `lines` an output's role, the first the lowest
    /// bits; lines past the output colours get none.
    fn outputs<'p>(&mut self, lines: impl IntoIterator<Item = &'p [Pos]>) {
        let places = 0..colour::OUTPUT_COLOURS.len();
        for (bits, n) in lines.into_iter().zip(places) {
            self.mark(Role::Output(n), bits);
        }
    }
}

/// `roles` painted into `data` and run through
/// `test`, after a few telling cases when `quick_first`, so a try far from
/// holding is not run in full: the file and its run when every case comes
/// out right, else why not. A file that can't be painted or read fails
/// the outer result, ending the probe.
fn try_roles(
    data: &[u8],
    roles: &Roles,
    test: Test,
    quick_first: bool,
) -> Result<Result<(Vec<u8>, simulate::Run), Fault>, Fault> {
    let unfit = |err: anyhow::Error| Fault::Unfit(err.to_string());
    let file = paint(data, roles).map_err(unfit)?;
    let coloured = Blocks::read(&file).map_err(unfit)?;
    if quick_first {
        match test.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => return Ok(Err(Fault::wrong(&run, &file))),
            Err(err) => return Ok(Err(Fault::NotRun(err.to_string()))),
        }
    }
    Ok(match test.run(&coloured) {
        simulate::Report::Ran(run) if run.wrong == 0 => Ok((file, *run)),
        simulate::Report::Ran(run) => Err(Fault::wrong(&run, &file)),
        simulate::Report::Broken { reason }
        | simulate::Report::Skipped { reason } => Err(Fault::NotRun(reason)),
    })
}

impl Layout {
    /// The interface the adder's test reads, its ports labelled as the
    /// colours would mark them.
    pub fn interface(&self, blocks: &Blocks) -> Interface {
        let switches: HashMap<Pos, Switch> = blocks
            .switches()
            .into_iter()
            .map(|switch| (switch.block, switch))
            .collect();
        let input = |pos: Pos| {
            let switch = &switches[&pos];
            Bit {
                pos,
                entry: Some(switch.side),
                hard: Direction::ALL.iter().any(|&side| {
                    schem::id(blocks.get(schem::step(pos, side)))
                        == "minecraft:redstone_wire"
                }),
                inverted: switch.on,
            }
        };
        let output = |pos: Pos| Bit {
            pos,
            entry: None,
            hard: false,
            inverted: false,
        };
        let port = |name: &str, block: &str, bits: Vec<Bit>| Port {
            name: name.to_owned(),
            block: block.to_owned(),
            labelled: false,
            bits,
            strength: 1,
            words: Vec::new(),
            signal_strength: false,
        };
        let mut inputs = vec![
            port(
                "A",
                "lime concrete",
                self.a.iter().map(|&p| input(p)).collect(),
            ),
            port(
                "B",
                "light blue concrete",
                self.b.iter().map(|&p| input(p)).collect(),
            ),
        ];
        if let Some(pos) = self.carry_in {
            inputs.push(port("carry in", "magenta concrete", vec![input(pos)]));
        }
        let mut outputs = vec![port(
            "output",
            "yellow concrete",
            self.sum.iter().map(|&p| output(p)).collect(),
        )];
        if let Some(pos) = self.carry_out {
            outputs.push(port("carry out", "pink concrete", vec![output(pos)]));
        }
        Interface { inputs, outputs }
    }

    /// Where its parts are, for a message.
    pub fn describe(&self) -> String {
        let bus = |bits: &[Pos]| match bits {
            [one] => inspect::place(*one),
            [first, .., last] => format!(
                "{} bits from {} to {}",
                bits.len(),
                inspect::place(*first),
                inspect::place(*last)
            ),
            [] => "none".to_owned(),
        };
        let mut parts = vec![
            format!("A: {}", bus(&self.a)),
            format!("B: {}", bus(&self.b)),
        ];
        if let Some(pos) = self.carry_in {
            parts.push(format!("carry in: {}", inspect::place(pos)));
        }
        parts.push(format!("sum: {}", bus(&self.sum)));
        if let Some(pos) = self.carry_out {
            parts.push(format!("carry out: {}", inspect::place(pos)));
        }
        parts.join("; ")
    }
}

/// A and B, least significant bit first, and a carry in.
type Operands = (Vec<Pos>, Vec<Pos>, Option<Pos>);

/// The sum, least significant bit first, and a carry out.
type Results = (Vec<Pos>, Option<Pos>);

/// The run's bits either way round.
fn both(bits: &[Pos]) -> [Vec<Pos>; 2] {
    let forward = bits.to_vec();
    let mut back = forward.clone();
    back.reverse();
    [forward, back]
}

/// The run's bits from the end its step leaves, so parallel runs come
/// the same way round.
fn ordered(run: &Run) -> Vec<Pos> {
    let mut bits = run.bits.clone();
    if run.step.iter().sum::<i32>() < 0 {
        bits.reverse();
    }
    bits
}

/// Two parallel runs of one length as one bus whose bits alternate between
/// them: either run first, either way round.
fn interleaved(x: &Run, y: &Run) -> Vec<Vec<Pos>> {
    if x.bits.len() != y.bits.len() {
        return Vec::new();
    }
    let (xs, ys) = (ordered(x), ordered(y));
    let mut found = Vec::new();
    for (first, second) in [(&xs, &ys), (&ys, &xs)] {
        for back in [false, true] {
            let (mut f, mut s) = (first.clone(), second.clone());
            if back {
                f.reverse();
                s.reverse();
            }
            found.push(f.iter().zip(&s).flat_map(|(a, b)| [*a, *b]).collect());
        }
    }
    found
}

fn split(runs: &[Run]) -> (Vec<&Run>, Vec<Pos>) {
    let buses = runs.iter().filter(|run| run.bits.len() > 1).collect();
    let lones = runs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .map(|run| run.bits[0])
        .collect();
    (buses, lones)
}

/// Where `runs`' lone bits are, such as `(1, 2, 3), (4, 5, 6)`, when one
/// or two are all that keep the rest in line.
fn strays(runs: &[Run]) -> Option<String> {
    let lone: Vec<String> = runs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .map(|run| inspect::place(run.bits[0]))
        .collect();
    (1..=2).contains(&lone.len()).then(|| lone.join(", "))
}

/// `runs`' lengths, such as `7, 7, 1, 1`.
fn lengths(runs: &[Run]) -> String {
    let each: Vec<String> =
        runs.iter().map(|run| run.bits.len().to_string()).collect();
    each.join(", ")
}

/// What is wrong with outputs `runs` as a `width`-bit sum and carry out:
/// too few; one or two out of line; uneven gaps along the axis they
/// spread furthest on; or, evenly spaced, not in a line.
fn output_fault(runs: &[Run], width: usize) -> String {
    let mut bits: Vec<Pos> =
        runs.iter().flat_map(|run| run.bits.clone()).collect();
    if bits.len() < width {
        return format!("Output bus: {} outputs, need {width}", bits.len());
    }
    if let Some(at) = strays(runs) {
        return format!("Output bus: out of line at {at}");
    }
    let extent = |axis: usize| {
        let (lo, hi) = bits.iter().fold((i32::MAX, i32::MIN), |(lo, hi), b| {
            (lo.min(b[axis]), hi.max(b[axis]))
        });
        hi - lo
    };
    let axis = (0..3).max_by_key(|&axis| extent(axis)).unwrap_or(1);
    bits.sort_by_key(|bit| bit[axis]);
    let gaps: Vec<i32> =
        bits.windows(2).map(|w| w[1][axis] - w[0][axis]).collect();
    if gaps.windows(2).any(|w| w[0] != w[1]) {
        let gaps: Vec<String> = gaps.iter().map(ToString::to_string).collect();
        let name = ["x", "y", "z"][axis];
        format!(
            "Output bus: uneven spacing, {name} gaps {}",
            gaps.join(", ")
        )
    } else {
        "Output bus: not in a line".to_owned()
    }
}

/// Every way the input runs could be A, B, and a carry in: two equal buses
/// and at most one lone lever; one bus of both, halved or interleaved; a
/// bus with the carry in at one end, in line with it; four equal buses,
/// each operand two of them with its bits alternating; or, for one bit,
/// two or three lone levers.
fn operands(runs: &[Run]) -> Result<Vec<Operands>, Fault> {
    let (buses, lones) = split(runs);
    let mut pairs: Vec<Operands> = Vec::new();
    let mut add = |x: &[Pos], y: &[Pos], carry: Option<Pos>| {
        for (p, q) in [(x, y), (y, x)] {
            for a in both(p) {
                for b in both(q) {
                    pairs.push((a.clone(), b, carry));
                }
            }
        }
    };
    match (buses.as_slice(), lones.as_slice()) {
        ([x, y], _) if lones.len() <= 1 && x.bits.len() == y.bits.len() => {
            add(&x.bits, &y.bits, lones.first().copied());
        }
        ([x, y], []) if x.bits.len().abs_diff(y.bits.len()) == 1 => {
            let (short, long) = if x.bits.len() < y.bits.len() {
                (x, y)
            } else {
                (y, x)
            };
            let n = short.bits.len();
            add(&short.bits, &long.bits[..n], Some(long.bits[n]));
            add(&short.bits, &long.bits[1..], Some(long.bits[0]));
        }
        ([x], _) if lones.len() <= 1 && x.bits.len() % 2 == 0 => {
            let n = x.bits.len() / 2;
            let carry = lones.first().copied();
            add(&x.bits[..n], &x.bits[n..], carry);
            let even: Vec<Pos> = x.bits.iter().step_by(2).copied().collect();
            let odd: Vec<Pos> =
                x.bits.iter().skip(1).step_by(2).copied().collect();
            add(&even, &odd, carry);
        }
        // Each operand two parallel lines, its bits alternating between
        // them, as a build adding twice its height's bits does.
        ([w, x, y, z], _)
            if lones.len() <= 1
                && [x, y, z].iter().all(|r| r.bits.len() == w.bits.len()) =>
        {
            let carry = lones.first().copied();
            for ((p, q), (r, t)) in
                [((w, x), (y, z)), ((w, y), (x, z)), ((w, z), (x, y))]
            {
                for a in interleaved(p, q) {
                    for b in interleaved(r, t) {
                        pairs.push((a.clone(), b.clone(), carry));
                        pairs.push((b, a.clone(), carry));
                    }
                }
            }
        }
        ([], [p, q]) => add(&[*p], &[*q], None),
        ([], [p, q, r]) => {
            for (x, y, carry) in [(p, q, r), (p, r, q), (q, r, p)] {
                add(&[*x], &[*y], Some(*carry));
            }
        }
        _ => {
            return Err(Fault::Unfit(strays(runs).map_or_else(
                || format!("Input buses: line lengths {}", lengths(runs)),
                |at| format!("Input bus: out of line at {at}"),
            )));
        }
    }
    Ok(pairs)
}

/// Every way the output runs could be an adder's sum of `width` bits and a
/// carry out: a bus and at most one lone output, its carry out or no line
/// of the adder's, as a comparator ending a chain may give out into air;
/// a bus one longer, its last bit the carry out or the sum's top bit; two
/// equal buses, the sum's bits alternating between them; or, for one bit,
/// lone outputs.
fn results(runs: &[Run], width: usize) -> Result<Vec<Results>, Fault> {
    let (buses, lones) = split(runs);
    let mut found = Vec::new();
    match (buses.as_slice(), lones.as_slice()) {
        ([sum], _) if lones.len() <= 1 && sum.bits.len() == width => {
            for bits in both(&sum.bits) {
                found.push((bits, lones.first().copied()));
            }
            if !lones.is_empty() {
                for bits in both(&sum.bits) {
                    found.push((bits, None));
                }
            }
        }
        ([sum], []) if sum.bits.len() == width + 1 => {
            for bits in both(&sum.bits) {
                found.push((bits[..width].to_vec(), Some(bits[width])));
                found.push((bits, None));
            }
        }
        ([x, y], _)
            if lones.len() <= 1
                && x.bits.len() == y.bits.len()
                && x.bits.len() * 2 == width =>
        {
            for bits in interleaved(x, y) {
                found.push((bits, lones.first().copied()));
            }
        }
        ([], [sum]) if width == 1 => {
            found.push((vec![*sum], None));
        }
        ([], [p, q]) if width == 1 => {
            found.push((vec![*p], Some(*q)));
            found.push((vec![*q], Some(*p)));
        }
        _ => {
            return Err(Fault::Unfit(output_fault(runs, width)));
        }
    }
    Ok(found)
}

/// The adder `blocks` is, found from `report`'s runs of levers and outputs:
/// of every layout they could make, the one whose quick test gets fewest
/// wrong, then tested in full; or why none adds.
pub fn adder(
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> Result<Probed, Fault> {
    let pairs = operands(&report.inputs)?;
    let width = pairs.first().map_or(0, |(a, _, _)| a.len());
    if Test::of(
        crate::category::Kind::Adder.name(),
        i64::try_from(width).ok(),
    )
    .is_none()
    {
        return Err(Fault::Unfit(format!("Width {width}: tested range 1-32")));
    }
    let outs = results(&report.outputs, width)?;
    let mut best: Option<(simulate::Run, Layout)> = None;
    let mut reasons: Vec<String> = Vec::new();
    for (a, b, carry_in) in &pairs {
        for (sum, carry_out) in &outs {
            let layout = Layout {
                a: a.clone(),
                b: b.clone(),
                carry_in: *carry_in,
                sum: sum.clone(),
                carry_out: *carry_out,
            };
            match Test::Adder.check_with(blocks, layout.interface(blocks), true)
            {
                Ok(run) => {
                    let better = best.as_ref().is_none_or(|(best, _)| {
                        run.wrong * best.cases < best.wrong * run.cases
                    });
                    let right = run.wrong == 0;
                    if better {
                        best = Some((run, layout));
                    }
                    if right {
                        break;
                    }
                }
                Err(err) => reasons.push(err.to_string()),
            }
        }
        if best.as_ref().is_some_and(|(run, _)| run.wrong == 0) {
            break;
        }
    }
    let Some((closest, layout)) = best else {
        reasons.dedup();
        return Err(Fault::Unfit(format!(
            "Can't simulate: {}",
            reasons.first().map_or("no layout fits", String::as_str)
        )));
    };
    // Its test ran, so its fault ranks with the others that did, the file
    // coloured as the closest layout has it.
    let coloured = |layout: &Layout| {
        colour(data, layout).map_err(|err| Fault::Unfit(err.to_string()))
    };
    if closest.wrong > 0 {
        return Err(Fault::wrong(&closest, &coloured(&layout)?));
    }
    // A and B the other way round may add too, read more plainly, with
    // fewer of its lines inverted.
    let swapped = Layout {
        a: layout.b.clone(),
        b: layout.a.clone(),
        ..layout.clone()
    };
    let quick = |layout: &Layout| {
        Test::Adder
            .check_with(blocks, layout.interface(blocks), true)
            .ok()
            .filter(|run| run.wrong == 0)
    };
    let layout = match (quick(&layout), quick(&swapped)) {
        (Some(run), Some(other))
            if !other.adder_variant().carry_in
                && run.adder_variant().carry_in =>
        {
            swapped
        }
        _ => layout,
    };
    let run = Test::Adder
        .check_with(blocks, layout.interface(blocks), false)
        .map_err(|err| Fault::NotRun(err.to_string()))?;
    if run.wrong > 0 {
        let bits: Vec<String> = run
            .wrong_bits
            .iter()
            .map(|(bit, n)| format!("{bit} ({n})"))
            .collect();
        let said =
            format!("{}; wrong bits {}", run.wrong_cases(), bits.join(", "));
        return Err(Fault::Wrong {
            wrong: run.wrong,
            cases: run.cases,
            said,
            file: coloured(&layout)?,
        });
    }
    Ok(Probed { layout, run })
}

/// The file in `data` with `layout`'s roles marked in colour, as the rest
/// of the site reads them: A lime, B light blue, carry in magenta, the sum
/// yellow, and carry out pink. Every other block of one of those colours
/// turns white, so none is taken for an input or output. A block keeps its
/// kind, stained glass staying glass, as dust treats it differently.
pub fn colour(data: &[u8], layout: &Layout) -> anyhow::Result<Vec<u8>> {
    let mut roles = Roles::default();
    roles.mark(Role::A, &layout.a);
    roles.mark(Role::B, &layout.b);
    roles.mark(Role::Control, &layout.carry_in);
    roles.mark(Role::Output(0), &layout.sum);
    roles.mark(Role::Status, &layout.carry_out);
    paint(data, &roles)
}

/// The file in `data` with each block of `roles` in its role's colour, and
/// every other block of a colour that marks a role, any of the site's
/// input, control, output, or status colours, turned white.
fn paint(data: &[u8], roles: &Roles) -> anyhow::Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let marks: Vec<&str> = colour::INPUT_COLOURS
        .into_iter()
        .chain([colour::CONTROL])
        .chain(colour::OUTPUT_COLOURS)
        .chain([colour::STATUS])
        .collect();
    let recoloured = |pos: Pos, name: &str| -> Option<String> {
        let glass = name.ends_with("_stained_glass");
        let colour = match roles.0.get(&pos) {
            Some(role) => role.colour(),
            None if marks.iter().any(|c| {
                ["_concrete", "_wool", "_terracotta", "_stained_glass"]
                    .iter()
                    .any(|kind| name == format!("{c}{kind}"))
            }) =>
            {
                "white"
            }
            None => return None,
        };
        let kind = if glass { "stained_glass" } else { "concrete" };
        Some(format!("minecraft:{colour}_{kind}"))
    };
    let changes = blocks
        .iter()
        .filter_map(|(pos, state)| {
            let name = schem::short(state);
            let want = recoloured(pos, name)?;
            (want != state).then_some((pos, want))
        })
        .collect();
    crate::schem::standardize::edit(data, &changes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// The fixture `name` with its input colours, lime, light blue, and
    /// magenta, turned white, as a build without colour conventions is.
    pub(super) fn uncoloured(name: &str) -> Vec<u8> {
        let data =
            std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let changes: BTreeMap<Pos, String> = blocks
            .iter()
            .filter(|&(_, state)| {
                [
                    colour::INPUT_COLOURS[0],
                    colour::INPUT_COLOURS[1],
                    colour::CONTROL,
                ]
                .iter()
                .any(|c| schem::short(state).starts_with(c))
            })
            .map(|(pos, _)| (pos, "minecraft:white_concrete".to_owned()))
            .collect();
        crate::schem::standardize::edit(&data, &changes).unwrap()
    }

    /// A subtractor whose output would read inverted one way round reads,
    /// with A and B swapped, as one whose borrow in is inverted, a control
    /// line a builder may well invert, where an output may not be.
    #[test]
    fn a_subtractor_reads_its_borrow_in_inverted_not_its_output() {
        let data =
            std::fs::read("tests/fixtures/sub-16-torchless.schem").unwrap();
        let found = recognize(&data).unwrap().found.unwrap_or_default();
        assert!(found.contains("A − B − not borrow in"), "{found}");
    }
}
