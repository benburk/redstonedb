//! What a build's levers and outputs are, found by how it behaves rather than
//! by colour: each way its runs of levers and outputs could be an adder's
//! A, B, carry in, sum, and carry out, with each bus either way round, is
//! tried with the adder's test, and the one that adds is kept. The roles
//! found go with the file, see [`crate::schem::lines`], as the site's tests
//! read them; no block changes colour.
use std::fmt;

use crate::{
    recognize::{
        inspect::{self, Run},
        interface::{self, Bit, Interface, Port},
    },
    schem::{Blocks, Pos, lines},
    simulate::{self, Test},
};

pub(crate) use crate::schem::lines::{Role, Roles};

mod alu;
mod decoder;
mod hex_adder;
mod hex_to_binary;
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
pub use hex_to_binary::{HexToBinaryProbed, hex_to_binary};
pub use multiplier::{MultiplierProbed, multiplier};
pub use one_operand::{Line, OneInputProbed, one_operand};
pub use operands::{OperandsProbed, operands_probe};
pub use recognize::{
    Found, Recognition, kept_roles, probe_as, recognize, recognize_as,
    recognize_watched, simulatable,
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
    /// or given its roles, in these words.
    Unfit(String),
    /// The kind's test couldn't run on the roles tried, for this reason.
    NotRun(String),
    /// The kind's test ran on the roles `file` carries, getting `wrong` of
    /// its `cases` wrong: as near as a build comes to holding. `said` is
    /// that, with the first wrong case as evidence; see `Run::wrong_cases`.
    Wrong {
        wrong: u64,
        cases: u64,
        said: String,
        file: Vec<u8>,
    },
}

impl Fault {
    /// The fault of `run`, which got cases wrong on the roles `file`
    /// carries.
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

    /// The file carrying the roles of the try that got cases wrong, so its
    /// test shows which.
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

/// `roles` given to `data` and run through `test`, after a few telling
/// cases when `quick_first`, so a try far from holding is not run in full:
/// the file and its run when every case comes out right, else why not. A
/// file that can't be read fails the outer result, ending the probe.
fn try_roles(
    data: &[u8],
    roles: &Roles,
    test: Test,
    quick_first: bool,
) -> Result<Result<(Vec<u8>, simulate::Run), Fault>, Fault> {
    let unfit = |err: anyhow::Error| Fault::Unfit(err.to_string());
    let file = given(data, roles).map_err(unfit)?;
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
    /// The interface the adder's test reads, its ports labelled as their
    /// roles name them.
    pub fn interface(&self, blocks: &Blocks) -> Interface {
        let switches = interface::switches(blocks);
        let input = |pos: Pos| Bit::input(blocks, &switches, pos);
        let output = Bit::at;
        let port = |name: &str, role: Role, bits: Vec<Bit>| Port {
            name: name.to_owned(),
            role: Some(role),
            labelled: false,
            bits,
            strength: 1,
            words: Vec::new(),
            signal_strength: false,
        };
        let mut inputs = vec![
            port(
                "A",
                Role::Operand(0),
                self.a.iter().map(|&p| input(p)).collect(),
            ),
            port(
                "B",
                Role::Operand(1),
                self.b.iter().map(|&p| input(p)).collect(),
            ),
        ];
        if let Some(pos) = self.carry_in {
            inputs.push(port("carry in", Role::Control, vec![input(pos)]));
        }
        let mut outputs = vec![port(
            "output",
            Role::Output(0),
            self.sum.iter().map(|&p| output(p)).collect(),
        )];
        if let Some(pos) = self.carry_out {
            outputs.push(port("carry out", Role::Status, vec![output(pos)]));
        }
        Interface {
            inputs,
            outputs,
            binary_outputs: false,
        }
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

/// Which of two operands probing finds interchangeable, as an adder's or a
/// gate's, whose test passes either way round, is A: the one the build's
/// own marking names A, `marked`, as
/// [`crate::recognize::legacy::operand_marks`] reads a stored build's old
/// colours while its roles are first found; else, or where the marking
/// names neither or both, the one holding the least place, west first, then
/// down, then north, as [`Pos`] orders them. An operand the test passes
/// only one way round is that way whatever its marking.
#[derive(Clone, Copy, Default)]
pub struct Order<'r> {
    pub marked: Option<&'r Roles>,
}

impl Order<'_> {
    /// Whether `a` and `b` are A and B in that order rather than the other
    /// way round.
    pub fn keeps(&self, a: &[Pos], b: &[Pos]) -> bool {
        let named = |bits: &[Pos], n: u8| {
            self.marked.map_or(0, |marked| {
                bits.iter()
                    .filter(|&&pos| marked.get(pos) == Some(Role::Operand(n)))
                    .count()
            })
        };
        let kept = named(a, 0) + named(b, 1);
        let swapped = named(a, 1) + named(b, 0);
        if kept != swapped {
            return kept > swapped;
        }
        a.iter().min() <= b.iter().min()
    }

    /// `a` and `b` as A and B, in the order [`Order::keeps`] prefers.
    pub fn of<T: AsRef<[Pos]>>(&self, a: T, b: T) -> (T, T) {
        if self.keeps(a.as_ref(), b.as_ref()) {
            (a, b)
        } else {
            (b, a)
        }
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
/// carry out: a bus and at most one lone output, its carry out; a bus one
/// longer, its last bit the carry out or the sum's top bit; two equal
/// buses, the sum's bits alternating between them; or, for one bit, lone
/// outputs. Every output is one of them, as an output computes something.
fn results(runs: &[Run], width: usize) -> Result<Vec<Results>, Fault> {
    let (buses, lones) = split(runs);
    let mut found = Vec::new();
    match (buses.as_slice(), lones.as_slice()) {
        ([sum], _) if lones.len() <= 1 && sum.bits.len() == width => {
            for bits in both(&sum.bits) {
                found.push((bits, lones.first().copied()));
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

/// Every way `report`'s runs of levers could be A, B, and a carry in, and
/// its runs of outputs the sum and a carry out, laid out loosely too when
/// `loose`; or why none fits.
fn layouts(
    report: &inspect::Report,
    loose: bool,
) -> Result<(Vec<Operands>, Vec<Results>), Fault> {
    let pairs = match operands(&report.inputs) {
        Err(_) if loose && !stray_operands(&report.inputs).is_empty() => {
            stray_operands(&report.inputs)
        }
        found => found?,
    };
    let width = pairs.first().map_or(0, |(a, _, _)| a.len());
    if Test::of(
        crate::category::Kind::Adder.name(),
        i64::try_from(width).ok(),
    )
    .is_none()
    {
        return Err(Fault::Unfit(format!("Width {width}: tested range 1-32")));
    }
    let outs = match results(&report.outputs, width) {
        Err(_)
            if loose && !spread_results(&report.outputs, width).is_empty() =>
        {
            spread_results(&report.outputs, width)
        }
        found => found?,
    };
    Ok((pairs, outs))
}

/// Each way two runs as wide as each other, with the bits left over beside
/// them, could be A and B, and a carry in: of the bits left over, two
/// alone, or three, one of them the carry in, each of the two joins a run
/// at the end nearer it, so A and B are each a bit wider than their runs,
/// as when a build's top bits stand out of line. Either bit may join
/// either run, and each operand either way round. None when the runs are
/// otherwise.
fn stray_operands(runs: &[Run]) -> Vec<Operands> {
    let mut buses: Vec<&Run> =
        runs.iter().filter(|run| run.bits.len() > 1).collect();
    buses.sort_by_key(|run| std::cmp::Reverse(run.bits.len()));
    let [one, other, rest @ ..] = buses.as_slice() else {
        return Vec::new();
    };
    if one.bits.len() != other.bits.len()
        || rest.iter().any(|run| run.bits.len() >= one.bits.len())
    {
        return Vec::new();
    }
    let left: Vec<Pos> = runs
        .iter()
        .filter(|run| !std::ptr::eq(*run, *one) && !std::ptr::eq(*run, *other))
        .flat_map(|run| run.bits.clone())
        .collect();
    // The bits left over that join, and any carry in.
    let joins: Vec<([Pos; 2], Option<Pos>)> = match *left.as_slice() {
        [first, second] => vec![([first, second], None)],
        [first, second, third] => vec![
            ([first, second], Some(third)),
            ([first, third], Some(second)),
            ([second, third], Some(first)),
        ],
        _ => return Vec::new(),
    };
    let joined = |run: &Run, stray: Pos| {
        let mut bits = ordered(run);
        let near = |end: Option<&Pos>| {
            end.map_or(i32::MAX, |&end| {
                (0..3).map(|i| (end[i] - stray[i]).abs()).sum()
            })
        };
        if near(bits.first()) < near(bits.last()) {
            bits.insert(0, stray);
        } else {
            bits.push(stray);
        }
        bits
    };
    let mut pairs = Vec::new();
    for ([first, second], carry) in joins {
        for (to_one, to_other) in [(first, second), (second, first)] {
            let (a, b) = (joined(one, to_one), joined(other, to_other));
            for (a, b) in [(&a, &b), (&b, &a)] {
                for a in both(a) {
                    for b in both(b) {
                        pairs.push((a.clone(), b, carry));
                    }
                }
            }
        }
    }
    pairs
}

/// Each way every output could be an adder's sum of `width` bits and a
/// carry out, however they are spaced: all of them in order along the
/// axis they spread furthest on, either way round, as the sum; or with one
/// more, the last of them the carry out. None for any other count.
fn spread_results(runs: &[Run], width: usize) -> Vec<Results> {
    let mut bits: Vec<Pos> =
        runs.iter().flat_map(|run| run.bits.clone()).collect();
    if bits.is_empty() || (bits.len() != width && bits.len() != width + 1) {
        return Vec::new();
    }
    let extent = |axis: usize| {
        let (lo, hi) = bits.iter().fold((i32::MAX, i32::MIN), |(lo, hi), b| {
            (lo.min(b[axis]), hi.max(b[axis]))
        });
        hi - lo
    };
    let Some(axis) = (0..3).max_by_key(|&axis| extent(axis)) else {
        return Vec::new();
    };
    bits.sort_by_key(|bit| (bit[axis], *bit));
    let mut found = Vec::new();
    for bits in both(&bits) {
        if bits.len() == width {
            found.push((bits, None));
        } else if bits.len() == width + 1 {
            found.push((bits[..width].to_vec(), Some(bits[width])));
        }
    }
    found
}

/// The adder `blocks` is, found from `report`'s runs of levers and outputs:
/// of every layout they could make, the one whose quick test gets fewest
/// wrong, then tested in full; or why none adds.
pub fn adder(
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> Result<Probed, Fault> {
    adder_from(data, blocks, report, false, Order::default())
}

/// As [`adder`], its lines laid out loosely too when `loose`, for a build
/// kept as built, whose lines needn't keep the conventions: where no
/// layout of its runs fits, A and B may each take one of the bits left
/// over beside two runs as wide as each other, at the end nearer it, as a
/// build whose top bits stand out of line does, and the sum may be every
/// output however spaced, in order along the way they spread, as its test
/// alone says which way round and which end is the carry out. Of A and B,
/// which add either way round, A is as `order` prefers, each layout tried
/// in that order first.
pub(super) fn adder_from(
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    loose: bool,
    order: Order,
) -> Result<Probed, Fault> {
    let (mut pairs, outs) = layouts(report, loose)?;
    pairs.sort_by_key(|(a, b, _)| !order.keeps(a, b));
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
    // carrying the closest layout's roles.
    let coloured = |layout: &Layout| {
        with_layout(data, layout).map_err(|err| Fault::Unfit(err.to_string()))
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

/// The file in `data` with `layout`'s roles, as the rest of the site reads
/// them: A, B, the carry in a control line, the sum an output, and the
/// carry out a status line. Its blocks stay as they are.
pub fn with_layout(data: &[u8], layout: &Layout) -> anyhow::Result<Vec<u8>> {
    given(data, &layout.roles())
}

impl Layout {
    /// Its roles, by block.
    pub fn roles(&self) -> Roles {
        let mut roles = Roles::default();
        roles.mark(Role::Operand(0), &self.a);
        roles.mark(Role::Operand(1), &self.b);
        roles.mark(Role::Control, &self.carry_in);
        roles.mark(Role::Output(0), &self.sum);
        roles.mark(Role::Status, &self.carry_out);
        roles
    }
}

/// The file in `data` carrying `roles`, in place of any it carried, its
/// blocks as they are.
fn given(data: &[u8], roles: &Roles) -> anyhow::Result<Vec<u8>> {
    lines::with(data, Some(roles))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::schem;

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
                    crate::recognize::colour::INPUT_COLOURS[0],
                    crate::recognize::colour::INPUT_COLOURS[1],
                    crate::recognize::colour::CONTROL,
                ]
                .iter()
                .any(|c| schem::short(state) == format!("{c}_concrete"))
            })
            .map(|(pos, _)| (pos, "minecraft:white_concrete".to_owned()))
            .collect();
        crate::schem::standardize::edit(&data, &changes).unwrap()
    }

    fn run(bits: &[Pos]) -> Run {
        Run {
            bits: bits.to_vec(),
            step: [0, 0, 0],
        }
    }

    /// Outputs however spaced are one sum, in order along the way they
    /// spread, either way round; with one more, the last is the carry out.
    #[test]
    fn unevenly_spaced_outputs_are_one_sum() {
        let runs = [
            run(&[[5, 0, 0], [5, 3, 0], [5, 6, 0]]),
            run(&[[4, 9, 0], [4, 11, 0]]),
            run(&[[4, 13, 0]]),
        ];
        let found = spread_results(&runs, 6);
        assert_eq!(found.len(), 2);
        let heights: Vec<i32> = found[0].0.iter().map(|p| p[1]).collect();
        assert_eq!(heights, [0, 3, 6, 9, 11, 13]);
        assert_eq!(found[0].1, None);
        assert_eq!(spread_results(&runs, 5)[0].1, Some([4, 13, 0]));
        assert!(spread_results(&runs, 4).is_empty());
        assert!(spread_results(&[], 0).is_empty());
    }

    /// Two runs as wide as each other and two bits left over beside them
    /// are A and B, each joined at its nearer end by one of them.
    #[test]
    fn bits_out_of_line_join_the_runs_beside_them() {
        let column = |x: i32| -> Vec<Pos> {
            (0..7).map(|i| [x, 1 + 2 * i, 9]).collect()
        };
        let runs = [
            run(&column(2)),
            run(&column(5)),
            run(&[[2, 15, 8]]),
            run(&[[5, 15, 8]]),
        ];
        let pairs = stray_operands(&runs);
        assert!(!pairs.is_empty());
        assert!(pairs.iter().all(|(a, b, carry)| {
            a.len() == 8 && b.len() == 8 && carry.is_none()
        }));
        assert!(pairs.iter().any(|(a, b, _)| {
            a.last() == Some(&[2, 15, 8])
                && a[6] == [2, 13, 9]
                && b.last() == Some(&[5, 15, 8])
        }));
        assert!(stray_operands(&runs[..3]).is_empty());
    }

    /// A subtractor whose output would read inverted one way round reads,
    /// with A and B swapped, as one whose borrow in is inverted, a control
    /// line an author may well invert, where an output may not be.
    #[test]
    fn a_subtractor_reads_its_borrow_in_inverted_not_its_output() {
        let data =
            std::fs::read("tests/fixtures/sub-16-torchless.schem").unwrap();
        let found = recognize(&data).unwrap().found.unwrap_or_default();
        assert!(found.contains("A − B − not borrow in"), "{found}");
    }
}
