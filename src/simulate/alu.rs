//! Testing an ALU: two operands, A and B, and control lines that choose an
//! operation. A control line's sign names the operations it takes part in,
//! as `and nand sub`, and choosing one turns on every line that names it,
//! flipping its lever from where it was saved, so a line saved on, which
//! is inverted, goes off. Each operation the signs name is checked on every
//! case. With few enough lines, each other combination of them is tried on
//! a few telling cases, and one that mostly gives an operation the signs do
//! not name, three cases in four, as an ALU that adds with every line off
//! does, is checked on every case too.
//!
//! A line named `carry in` is no operation: add takes it, as A + B + carry
//! in, and the rest hold it off. A status line is checked where its meaning
//! is plain: a carry out for add and subtract, as the adder gives it, so a
//! subtract, A + ~B + 1, carries when nothing is borrowed; an overflow, taken
//! as signed, for add and subtract; an `lsb shift out`, the bit a shift
//! right drops, for shr and asr; and a zero for every operation.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::{
    Broken, Run, Timing,
    adder::adder_cases,
    bench::{Bench, Case, cases, mask, observe, paired, sweep},
    circuit::{input_bits, output_bits},
    timing,
};
use crate::{interface::Interface, lint, schem::Blocks};

/// The operations an ALU may offer, in the order a result lists them.
const OPS: [&str; 11] = [
    "add", "sub", "and", "or", "xor", "nand", "nor", "xnor", "shl", "shr",
    "asr",
];

/// The operation a word on a control line's sign names, such as `shr` for
/// `rsh`; `None` for any other word.
pub fn op(word: &str) -> Option<&'static str> {
    let word = match word {
        "rsh" | "lsr" | "srl" => "shr",
        "lsh" | "lsl" | "sll" => "shl",
        "sra" => "asr",
        "subtract" => "sub",
        other => other,
    };
    OPS.into_iter().find(|op| *op == word)
}

/// What `op` gives for A and B of `width` bits, with `carry` into an add,
/// before it is cut to `width` bits.
fn apply(op: &str, a: u64, b: u64, carry: u64, width: usize) -> u64 {
    let top = 1 << (width - 1);
    match op {
        "add" => a + b + carry,
        "sub" => a.wrapping_sub(b),
        "and" => a & b,
        "or" => a | b,
        "xor" => a ^ b,
        "nand" => !(a & b),
        "nor" => !(a | b),
        "xnor" => !(a ^ b),
        "shl" => a << 1,
        "shr" => a >> 1,
        "asr" => a >> 1 | a & top,
        _ => 0,
    }
}

/// An ALU's ports, by their place in its interface.
struct Alu {
    a: usize,
    b: usize,
    /// A's and B's bits.
    width: usize,
    carry_in: Option<usize>,
    /// Each control line choosing the operation, with the operations its
    /// sign names.
    lines: Vec<(usize, Vec<&'static str>)>,
    /// The outputs holding the result, the first holding the lowest bits.
    data: Vec<usize>,
    data_bits: usize,
    carry_out: Option<usize>,
    overflow: Option<usize>,
    /// The bit a shift right drops, as `lsb shift out` names it.
    shift_out: Option<usize>,
    zero: Option<usize>,
}

/// Reads an ALU's ports: two operands of one width, a carry in if a
/// control line is named so, the other control lines, its outputs, and its
/// status lines whose meaning is plain.
fn ports(interface: &Interface) -> Result<Alu> {
    let control =
        |i: &usize| interface.inputs[*i].block.starts_with(lint::CONTROL);
    let (lines, operands): (Vec<usize>, Vec<usize>) =
        (0..interface.inputs.len()).partition(control);
    let &[a, b] = operands.as_slice() else {
        broken!(
            "An ALU takes two operands, A and B, but this has {}.",
            operands.len()
        );
    };
    let width = input_bits(&interface.inputs[a]);
    let b_width = input_bits(&interface.inputs[b]);
    if width != b_width {
        broken!("A has {width} bits but B has {b_width}; an ALU's match.");
    }
    let named = |i: &usize, names: &[&str]| {
        names.contains(&interface.inputs[*i].name.as_str())
    };
    let carry_in = lines.iter().copied().find(|i| named(i, &lint::CARRY_IN));
    let lines: Vec<(usize, Vec<&'static str>)> = lines
        .into_iter()
        .filter(|i| Some(*i) != carry_in)
        .map(|i| {
            let ops = interface.inputs[i].words.iter().filter_map(|w| op(w));
            (i, ops.collect())
        })
        .collect();
    if lines.is_empty() {
        broken!(
            "An ALU has control lines choosing its operation, but this has none."
        );
    }
    let status =
        |i: &usize| interface.outputs[*i].block.starts_with(lint::STATUS);
    let (statuses, data): (Vec<usize>, Vec<usize>) =
        (0..interface.outputs.len()).partition(status);
    let data_bits: usize = data
        .iter()
        .map(|&i| output_bits(interface, &interface.outputs[i]))
        .sum();
    if data_bits < width {
        broken!(
            "A and B have {width} bits, but the outputs have only {data_bits}."
        );
    }
    let says = |i: &usize, words: &[&str]| {
        let on = &interface.outputs[*i].words;
        words.iter().all(|word| on.iter().any(|w| w == word))
    };
    let find =
        |test: &dyn Fn(&usize) -> bool| statuses.iter().copied().find(test);
    Ok(Alu {
        a,
        b,
        width,
        carry_in,
        lines,
        data,
        data_bits,
        carry_out: find(&|i| says(i, &["carry", "out"]) || says(i, &["cout"])),
        overflow: find(&|i| says(i, &["overflow"])),
        shift_out: find(&|i| says(i, &["shift", "out"])),
        zero: find(&|i| says(i, &["zero"])),
    })
}

/// One operation and the control lines that choose it.
struct Selection {
    op: &'static str,
    /// Each control line's value, and the carry in held off.
    held: Vec<(usize, u64)>,
    /// How it reads in a result, such as `add` for one a sign names, or
    /// `or (and + xor on)` for one found by trying the lines.
    label: String,
}

/// Every line off but those in `on`, with the carry in held off.
fn hold(alu: &Alu, on: impl Fn(usize) -> bool) -> Vec<(usize, u64)> {
    alu.lines
        .iter()
        .enumerate()
        .map(|(k, &(port, _))| (port, u64::from(on(k))))
        .chain(alu.carry_in.map(|port| (port, 0)))
        .collect()
}

/// The operations to check: each a sign names, then each another
/// combination of at most six lines gives on the test patterns.
fn selections(
    blocks: &Blocks,
    interface: &Interface,
    alu: &Alu,
) -> Result<Vec<Selection>> {
    let mut found: Vec<Selection> = OPS
        .into_iter()
        .filter(|op| alu.lines.iter().any(|(_, ops)| ops.contains(op)))
        .map(|op| Selection {
            op,
            held: hold(alu, |k| alu.lines[k].1.contains(&op)),
            label: op.to_owned(),
        })
        .collect();
    let combinations: u64 = if alu.lines.len() <= 6 {
        1 << alu.lines.len()
    } else {
        1
    };
    let (telling, _) = paired(&[alu.width, alu.width]);
    for combination in 0..combinations {
        let held = hold(alu, |k| combination >> k & 1 == 1);
        if found.iter().any(|s| s.held == held) {
            continue;
        }
        let bench = Bench {
            inputs: vec![alu.a, alu.b],
            outputs: alu.data.clone(),
            pattern: String::new(),
            held: held.clone(),
            care: u64::MAX,
            presses: Vec::new(),
        };
        let got = observe(blocks, interface, &bench, &telling, None)?;
        let right = |op: &str| {
            telling
                .iter()
                .zip(&got)
                .filter(|&(case, &got)| {
                    apply(op, case[0], case[1], 0, alu.width) & mask(alu.width)
                        == got & mask(alu.width)
                })
                .count()
        };
        // The operation it gets most right, if three in four, so one with
        // a few wrong bits is still checked, and its faults found.
        let new = OPS
            .into_iter()
            .filter(|op| !found.iter().any(|s| s.op == *op))
            .map(|op| (op, right(op)))
            .filter(|&(_, n)| n * 4 >= telling.len() * 3)
            .max_by_key(|&(_, n)| n)
            .map(|(op, _)| op);
        if let Some(op) = new {
            let on: Vec<&str> = alu
                .lines
                .iter()
                .enumerate()
                .filter(|(k, _)| combination >> k & 1 == 1)
                .map(|(_, &(port, _))| interface.inputs[port].name.as_str())
                .collect();
            let lines = if on.is_empty() {
                "no lines on".to_owned()
            } else {
                format!("{} on", on.join(" + "))
            };
            found.push(Selection {
                op,
                held,
                label: format!("{op} ({lines})"),
            });
        }
    }
    found.sort_by_key(|s| OPS.iter().position(|op| *op == s.op));
    Ok(found)
}

/// Runs an ALU through every operation its signs name, and any other its
/// control lines are found to give, and checks every result, or with
/// `quick` a few telling ones of each.
pub fn check(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = crate::interface::read(blocks);
    let alu = ports(&interface)?;
    let found = selections(blocks, &interface, &alu)?;
    let mut runs = Vec::new();
    for selection in &found {
        runs.push(check_one(blocks, &interface, &alu, selection, quick)?);
    }
    let mut merged = runs.remove(0);
    for run in runs {
        merged.absorb(run);
    }
    merged.pattern = found
        .iter()
        .map(|s| s.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(merged)
}

/// The status lines checked, in the order they follow the result: each
/// with whether `op` decides it and its value for a case.
type Status = (usize, bool, fn(&str, u64, u64, u64, usize) -> bool);

/// Checks one operation on every case, or on each byte's every case and
/// the test patterns when there are too many, or with `quick` on a few
/// telling ones.
fn check_one(
    blocks: &Blocks,
    interface: &Interface,
    alu: &Alu,
    selection: &Selection,
    quick: bool,
) -> Result<Run> {
    let op = selection.op;
    let width = alu.width;
    let carry = op == "add" && alu.carry_in.is_some();
    let mut inputs = vec![alu.a, alu.b];
    inputs.extend(alu.carry_in);
    let held: Vec<(usize, u64)> = selection
        .held
        .iter()
        .copied()
        .filter(|&(port, _)| Some(port) != alu.carry_in)
        .collect();
    let arithmetic = matches!(op, "add" | "sub");
    let statuses: Vec<Status> = [
        (
            alu.carry_out,
            arithmetic,
            carry_out as fn(&str, u64, u64, u64, usize) -> bool,
        ),
        (alu.overflow, arithmetic, overflow),
        (alu.shift_out, matches!(op, "shr" | "asr"), shift_out),
        (alu.zero, true, zero),
    ]
    .into_iter()
    .filter_map(|(port, decides, value)| Some((port?, decides, value)))
    .collect();
    let mut outputs = alu.data.clone();
    outputs.extend(statuses.iter().map(|&(port, ..)| port));
    let mut care = mask(alu.data_bits);
    for (k, &(_, decides, _)) in statuses.iter().enumerate() {
        if decides {
            care |= 1 << (alu.data_bits + k);
        }
    }
    let bench = Bench {
        inputs,
        outputs,
        pattern: selection.label.clone(),
        held,
        care,
        presses: Vec::new(),
    };
    let widths: Vec<usize> = if carry {
        vec![width, width, 1]
    } else {
        vec![width, width]
    };
    let (cases, exhaustive, patterns) = if quick {
        let (telling, _) = paired(&widths);
        let count = telling.len() as u64;
        (telling, false, count)
    } else {
        cases(&widths, || adder_cases(width, carry, 8))
    };
    let data_bits = alu.data_bits;
    let expected = |case: &Case| {
        let result = apply(op, case[0], case[1], case[2], width) & mask(width);
        statuses.iter().enumerate().fold(
            result,
            |value, (k, &(_, _, status))| {
                let on = status(op, case[0], case[1], case[2], width);
                value | u64::from(on) << (data_bits + k)
            },
        )
    };
    let mut run = sweep(blocks, interface, &bench, &cases, None, expected)?
        .covering(exhaustive, patterns);
    for wrong in &mut run.examples {
        wrong.op = Some(selection.label.clone());
    }
    Ok(run)
}

/// Whether the adder carries out of `width` bits: an add of A, B, and
/// `carry`, or a subtract, A + ~B + 1, which carries when A is at least B,
/// borrowing nothing.
fn carry_out(op: &str, a: u64, b: u64, carry: u64, width: usize) -> bool {
    if op == "sub" {
        return a >= b;
    }
    (a + b + carry) >> width & 1 == 1
}

/// Whether `op` on A and B overflows, as signed numbers of `width` bits.
fn overflow(op: &str, a: u64, b: u64, carry: u64, width: usize) -> bool {
    let top = 1 << (width - 1);
    let result = apply(op, a, b, carry, width);
    let (sa, sb, sr) = (a & top != 0, b & top != 0, result & top != 0);
    match op {
        "add" => sa == sb && sr != sa,
        "sub" => sa != sb && sr != sa,
        _ => false,
    }
}

/// Whether a shift right of A drops a 1: A's lowest bit.
fn shift_out(_: &str, a: u64, _: u64, _: u64, _: usize) -> bool {
    a & 1 == 1
}

/// Whether `op`'s result is 0.
fn zero(op: &str, a: u64, b: u64, carry: u64, width: usize) -> bool {
    apply(op, a, b, carry, width) & mask(width) == 0
}

/// Measures each operation's timing, and returns the slowest's, synced
/// only when every operation's result is.
pub fn time(blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
    let interface = crate::interface::read(blocks);
    let alu = ports(&interface)?;
    let mut slowest: Option<Timing> = None;
    let mut synced = true;
    for selection in selections(blocks, &interface, &alu)? {
        let mut inputs = vec![alu.a, alu.b];
        let mut held = selection.held.clone();
        if selection.op == "add"
            && let Some(port) = alu.carry_in
        {
            inputs.push(port);
            held.retain(|&(p, _)| p != port);
        }
        let timing =
            timing::measure(blocks, &interface, &inputs, &held, None, cancel)?;
        synced &= timing
            .outputs
            .iter()
            .filter(|output| !output.status)
            .all(|output| output.glitches == 0 && output.staggered == 0);
        if slowest
            .as_ref()
            .is_none_or(|s| timing.latency() > s.latency())
        {
            slowest = Some(timing);
        }
    }
    let mut timing =
        slowest.ok_or_else(|| Broken("no operation to time".to_owned()))?;
    timing.synced = synced;
    Ok(timing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_may_name_a_shift_by_another_name() {
        assert_eq!(op("rsh"), Some("shr"));
        assert_eq!(op("sra"), Some("asr"));
        assert_eq!(op("and"), Some("and"));
        assert_eq!(op("inverted"), None);
    }

    #[test]
    fn each_operation_gives_its_result_on_eight_bits() {
        let eight = |op: &str, a, b| apply(op, a, b, 0, 8) & 0xff;
        assert_eq!(eight("sub", 5, 7), 254);
        assert_eq!(eight("nand", 0b1100, 0b1010), 0b1111_0111);
        assert_eq!(eight("asr", 0b1000_0010, 0), 0b1100_0001);
        assert_eq!(eight("shr", 0b1000_0010, 0), 0b0100_0001);
        assert!(carry_out("add", 200, 100, 0, 8));
        assert!(carry_out("sub", 5, 3, 0, 8) && !carry_out("sub", 3, 5, 0, 8));
        assert!(overflow("add", 100, 100, 0, 8));
        assert!(overflow("sub", 0x80, 1, 0, 8));
        assert!(!overflow("and", 0xff, 0xff, 0, 8));
    }
}
