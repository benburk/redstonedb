//! What a build's levers and outputs are, found by how it behaves rather than
//! by colour: each way its runs of levers and outputs could be an adder's
//! A, B, carry in, sum, and carry out, with each bus either way round, is
//! tried with the adder's test, and the one that adds is kept.

use std::collections::HashMap;

use crate::{
    inspect::{self, Run},
    interface::{Bit, Interface, Port},
    schem::{self, Blocks, Direction, Pos, Switch},
    simulate::{self, Test},
};

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

fn sizes(runs: &[Run]) -> String {
    let mut sizes: Vec<String> =
        runs.iter().map(|run| run.bits.len().to_string()).collect();
    match sizes.len() {
        0 => "none".to_owned(),
        1 => sizes.remove(0),
        _ => {
            let last = sizes.pop().unwrap_or_default();
            format!("{} and {last}", sizes.join(", "))
        }
    }
}

/// Every way the input runs could be A, B, and a carry in: two equal buses
/// and at most one lone lever; one bus of both, halved or interleaved; a
/// bus with the carry in at one end, in line with it; four equal buses,
/// each operand two of them with its bits alternating; or, for one bit,
/// two or three lone levers.
fn operands(runs: &[Run]) -> Result<Vec<Operands>, String> {
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
            return Err(format!(
                "Its levers don't make an adder's inputs. An adder takes A \
                 and B, two buses of levers each in a line, and at most one \
                 lone lever for carry in; its levers fall into runs of {}.",
                sizes(runs)
            ));
        }
    }
    Ok(pairs)
}

/// Every way the output runs could be an adder's sum of `width` bits and a
/// carry out: a bus and at most one lone output; a bus one longer, its last
/// bit the carry out or the sum's top bit; two equal buses, the sum's bits
/// alternating between them; or, for one bit, lone outputs.
fn results(runs: &[Run], width: usize) -> Result<Vec<Results>, String> {
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
        ([], [_]) if width == 1 => {
            return Err("It has one output. A 1-bit adder is a slice of a \
                        wider one, so it has its sum and its carry out, on a \
                        coloured block each."
                .to_owned());
        }
        ([], [p, q]) if width == 1 => {
            found.push((vec![*p], Some(*q)));
            found.push((vec![*q], Some(*p)));
        }
        _ => {
            return Err(format!(
                "Its outputs don't make an adder's. A {width}-bit adder has \
                 its sum on {width} outputs in a line, with at most one \
                 more for carry out; its outputs fall into runs of {}.",
                sizes(runs)
            ));
        }
    }
    Ok(found)
}

/// The adder `blocks` is, found from `report`'s runs of levers and outputs:
/// of every layout they could make, the one whose quick test gets fewest
/// wrong, then tested in full; or why none adds.
pub fn adder(
    blocks: &Blocks,
    report: &inspect::Report,
) -> Result<Probed, String> {
    let pairs = operands(&report.inputs)?;
    let width = pairs.first().map_or(0, |(a, _, _)| a.len());
    if Test::of("adder", i64::try_from(width).ok()).is_none() {
        return Err(format!(
            "Its operands are {width} bits wide; adders of 1 to 32 bits are \
             tested."
        ));
    }
    let outs = results(&report.outputs, width)?;
    let mut best: Option<(u64, u64, Layout)> = None;
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
                    let better =
                        best.as_ref().is_none_or(|(wrong, cases, _)| {
                            run.wrong * cases < wrong * run.cases
                        });
                    if better {
                        best = Some((run.wrong, run.cases, layout));
                    }
                    if run.wrong == 0 {
                        break;
                    }
                }
                Err(err) => reasons.push(err.to_string()),
            }
        }
        if best.as_ref().is_some_and(|(wrong, _, _)| *wrong == 0) {
            break;
        }
    }
    let Some((wrong, cases, layout)) = best else {
        reasons.dedup();
        return Err(format!(
            "It can't be run as an adder: {}",
            reasons.first().map_or("no layout fits", String::as_str)
        ));
    };
    if wrong > 0 {
        return Err(format!(
            "No way of reading its levers and outputs as an adder adds right. \
             The closest, {}, gets {wrong} of {cases} telling cases wrong.",
            layout.describe()
        ));
    }
    // A and B the other way round may add too, read more plainly, as a
    // subtractor's A − B rather than not (B − A − 1).
    let swapped = Layout {
        a: layout.b.clone(),
        b: layout.a.clone(),
        ..layout.clone()
    };
    let inversions = |pattern: &str| pattern.matches("not").count();
    let quick = |layout: &Layout| {
        Test::Adder
            .check_with(blocks, layout.interface(blocks), true)
            .ok()
            .filter(|run| run.wrong == 0)
    };
    let layout = match (quick(&layout), quick(&swapped)) {
        (Some(run), Some(other))
            if inversions(&other.pattern) < inversions(&run.pattern) =>
        {
            swapped
        }
        _ => layout,
    };
    let run = Test::Adder
        .check_with(blocks, layout.interface(blocks), false)
        .map_err(|err| format!("Its test couldn't run: {err}"))?;
    if run.wrong > 0 {
        let bits: Vec<String> = run
            .wrong_bits
            .iter()
            .map(|(bit, n)| format!("{bit} ({n})"))
            .collect();
        return Err(format!(
            "Read as {}, it gets {} of {} cases wrong: {}.",
            layout.describe(),
            run.wrong,
            run.cases,
            bits.join(", ")
        ));
    }
    Ok(Probed { layout, run })
}
