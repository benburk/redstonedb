//! Testing an ALU: two operands, A and B, and control lines that choose an
//! operation. A control line's sign names the operations it takes part in,
//! as `and nand sub`, and choosing one turns on every line that names it:
//! its lever on, or off for a line found to read inverted, as `polarized`
//! finds them. Each operation the signs name is checked on every
//! case. With few enough lines, each other combination of them is tried on
//! a few telling cases, and one that mostly gives an operation the signs do
//! not name, three cases in four, as an ALU that adds with every line off
//! does, is checked on every case too. So is one that gives an operation
//! whose sign chooses the same lines as another's, as two lines both signed
//! `nadd nsub` do, which the signs cannot tell apart.
//!
//! Nadd and nsub are add and subtract with every bit of the result
//! inverted, as an ALU that hands its result on through a torch gives them.
//!
//! A line named `carry in` is no operation: add and nadd take it, adding
//! it to A and B, and the rest hold it off. A status line is checked where
//! its meaning is plain: a carry out for add and subtract, as the adder
//! gives it, so a subtract, A + ~B + 1, carries when nothing is borrowed;
//! an overflow, taken as signed, for add and subtract; an `lsb shift out`,
//! the bit a shift right drops, for shr and asr; and a zero for every
//! operation. Whether a nadd's or nsub's carry out is inverted too is the
//! builder's choice, so it is not checked.

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

use anyhow::Result;

use super::{
    Broken, Run, Timing,
    adder::adder_cases,
    bench::{self, Bench, Case, cases, mask, observe, paired, sweep},
    circuit::{input_bits, output_bits},
    timing,
};
use crate::{colour, interface::Interface, schem::Blocks};

/// The operations an ALU may offer, in the order a result lists them.
const OPS: [&str; 13] = [
    "add", "sub", "nadd", "nsub", "and", "or", "xor", "nand", "nor", "xnor",
    "shl", "shr", "asr",
];

/// Whether `op` takes the carry in: an add, its result inverted or not.
fn takes_carry(op: &str) -> bool {
    matches!(op, "add" | "nadd")
}

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
        "nadd" => !(a + b + carry),
        "nsub" => !a.wrapping_sub(b),
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
    /// Which control lines read inverted, on with their lever off, in the
    /// order of `lines`.
    inverted: Vec<bool>,
    /// Whether the carry in reads inverted.
    carry_inverted: bool,
}

/// Reads an ALU's ports: two operands of one width, a carry in if a
/// control line is named so, the other control lines, its outputs, and its
/// status lines whose meaning is plain.
fn ports(interface: &Interface) -> Result<Alu> {
    let control =
        |i: &usize| interface.inputs[*i].block.starts_with(colour::CONTROL);
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
    let carry_in = lines.iter().copied().find(|i| named(i, &colour::CARRY_IN));
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
        |i: &usize| interface.outputs[*i].block.starts_with(colour::STATUS);
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
        data,
        data_bits,
        carry_out: find(&|i| says(i, &["carry", "out"]) || says(i, &["cout"])),
        overflow: find(&|i| says(i, &["overflow"])),
        shift_out: find(&|i| says(i, &["shift", "out"])),
        zero: find(&|i| says(i, &["zero"])),
        inverted: vec![false; lines.len()],
        carry_inverted: false,
        lines,
    })
}

/// What an ALU's lever settings give on the telling cases, each looked
/// at once.
struct Looks<'a> {
    blocks: &'a Blocks,
    interface: &'a Interface,
    alu: &'a Alu,
    telling: Vec<Case>,
    /// What each setting, a bit per control line, gave, its carry in's
    /// lever off.
    seen: HashMap<u64, Vec<u64>>,
}

impl Looks<'_> {
    /// What `levers` give, looked at once.
    fn look(&mut self, levers: u64) -> Result<&[u64]> {
        if !self.seen.contains_key(&levers) {
            let alu = self.alu;
            let bench = Bench {
                inputs: vec![alu.a, alu.b],
                outputs: alu.data.clone(),
                pattern: String::new(),
                held: (0..alu.lines.len())
                    .map(|k| (alu.lines[k].0, levers >> k & 1))
                    .chain(alu.carry_in.map(|port| (port, 0)))
                    .collect(),
                care: u64::MAX,
                presses: Vec::new(),
                invertible: false,
            };
            let got = observe(
                self.blocks,
                self.interface,
                &bench,
                &self.telling,
                None,
            )?;
            self.seen.insert(levers, got);
        }
        Ok(&self.seen[&levers])
    }

    /// How many telling cases `got` gives `op` right with `carry` added.
    fn right_with(&self, op: &str, got: &[u64], carry: u64) -> usize {
        let width = self.alu.width;
        self.telling
            .iter()
            .zip(got)
            .filter(|&(case, &got)| {
                apply(op, case[0], case[1], carry, width) & mask(width)
                    == got & mask(width)
            })
            .count()
    }

    /// How many telling cases `got` gives `op` right, an add with the carry
    /// in either way.
    fn right(&self, op: &str, got: &[u64]) -> usize {
        (0..=u64::from(takes_carry(op)))
            .map(|carry| self.right_with(op, got, carry))
            .max()
            .unwrap_or(0)
    }
}

/// The levers choosing `op` on `alu`, a bit per control line, its lines
/// read as `inverted`.
fn levers(alu: &Alu, op: &str, inverted: u64) -> u64 {
    (0..alu.lines.len()).fold(0u64, |levers, k| {
        let on = u64::from(alu.lines[k].1.contains(&op));
        levers | (on ^ (inverted >> k & 1)) << k
    })
}

/// An ALU's ports, and which of its lines read inverted, found from what
/// lever settings give on the telling cases. Every line plain, and each as
/// its lever was saved, are the first readings tried. Each setting looked
/// at that gives a named operation names a reading too: the lines that
/// operation turns on, flipped where the levers are on. Each reading is
/// tried on every named operation, until one gives them all; when none is
/// left, the settings one line from the best reading's are looked at. Its
/// carry in reads inverted when its add, the carry's lever off, adds one.
fn polarized(blocks: &Blocks, interface: &Interface) -> Result<Alu> {
    let mut alu = ports(interface)?;
    let named: Vec<&'static str> = OPS
        .into_iter()
        .filter(|op| alu.lines.iter().any(|(_, ops)| ops.contains(op)))
        .collect();
    let count = alu.lines.len();
    let saved = (0..count).fold(0u64, |saved, k| {
        let port = &interface.inputs[alu.lines[k].0];
        let on = !port.bits.is_empty() && port.bits.iter().all(|b| b.inverted);
        saved | u64::from(on) << k
    });
    let (inverted, carry_inverted) = {
        let mut looks = Looks {
            blocks,
            interface,
            alu: &alu,
            telling: paired(&[alu.width, alu.width]).0,
            seen: HashMap::new(),
        };
        let inverted = search(&mut looks, &named, saved)?;
        let carry_inverted = alu.carry_in.is_some()
            && carry_inverted(&mut looks, &named, inverted)?;
        (inverted, carry_inverted)
    };
    alu.inverted = (0..count).map(|k| inverted >> k & 1 == 1).collect();
    alu.carry_inverted = carry_inverted;
    Ok(alu)
}

/// Whether an ALU's carry in reads inverted, its control lines read as
/// `inverted`: whether an add, the carry's lever off, adds one. The add is
/// a named operation's, or with none, the first any setting of at most
/// six lines gives, as one no sign names.
fn carry_inverted(
    looks: &mut Looks,
    named: &[&'static str],
    inverted: u64,
) -> Result<bool> {
    let alu = looks.alu;
    let settings: Vec<u64> = match named.iter().find(|op| takes_carry(op)) {
        Some(op) => vec![levers(alu, op, inverted)],
        None if alu.lines.len() <= 6 => (0..1u64 << alu.lines.len())
            .map(|on| on ^ inverted)
            .collect(),
        None => vec![inverted],
    };
    let enough = looks.telling.len() * 3;
    for setting in settings {
        let got = looks.look(setting)?.to_vec();
        for op in ["add", "nadd"] {
            let (off, on) =
                (looks.right_with(op, &got, 0), looks.right_with(op, &got, 1));
            if off.max(on) * 4 >= enough {
                return Ok(on > off);
            }
        }
    }
    Ok(false)
}

/// The reading of an ALU's control lines, a bit per line inverted, that
/// gives the most of its `named` operations' telling cases right, the
/// fewest inverted on a tie, searched as `polarized` says from every line
/// plain and from `saved`.
fn search(
    looks: &mut Looks,
    named: &[&'static str],
    saved: u64,
) -> Result<u64> {
    let alu = looks.alu;
    let perfect = named.len() * looks.telling.len();
    let mut readings = vec![0, saved];
    let mut tried: HashSet<u64> = HashSet::new();
    let mut widened: HashSet<u64> = HashSet::new();
    let mut best = (0, 0u64);
    loop {
        while let Some(reading) = readings.pop() {
            if !tried.insert(reading) {
                continue;
            }
            let mut total = 0;
            for op in named {
                let got = looks.look(levers(alu, op, reading))?.to_vec();
                total += looks.right(op, &got);
            }
            let fewer = reading.count_ones() < best.1.count_ones();
            if total > best.0 || total == best.0 && fewer {
                best = (total, reading);
            }
            if total == perfect {
                return Ok(reading);
            }
        }
        for (&setting, got) in &looks.seen {
            for op in named {
                if looks.right(op, got) == looks.telling.len() {
                    readings.push(setting ^ levers(alu, op, 0));
                }
            }
        }
        readings.retain(|reading| !tried.contains(reading));
        if readings.is_empty() {
            if !widened.insert(best.1) {
                return Ok(best.1);
            }
            for k in 0..alu.lines.len() {
                for op in named {
                    looks.look(levers(alu, op, best.1 ^ 1 << k))?;
                }
            }
        }
    }
}

impl Alu {
    /// The lines it reads inverted, by name.
    fn inverted_lines(&self, interface: &Interface) -> Vec<String> {
        self.lines
            .iter()
            .zip(&self.inverted)
            .filter(|&(_, &inverted)| inverted)
            .map(|(&(port, _), _)| port)
            .chain(self.carry_in.filter(|_| self.carry_inverted))
            .map(|port| interface.inputs[port].name.clone())
            .collect()
    }
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

/// The levers setting every line off but those in `on`, an inverted
/// line's lever on when it is off, with the carry in held off.
fn hold(alu: &Alu, on: impl Fn(usize) -> bool) -> Vec<(usize, u64)> {
    alu.lines
        .iter()
        .enumerate()
        .map(|(k, &(port, _))| (port, u64::from(on(k) != alu.inverted[k])))
        .chain(
            alu.carry_in
                .map(|port| (port, u64::from(alu.carry_inverted))),
        )
        .collect()
}

/// The operations to check: each a sign names, then each another
/// combination of at most six lines gives on the test patterns.
fn selections(
    blocks: &Blocks,
    interface: &Interface,
    alu: &Alu,
) -> Result<Vec<Selection>> {
    let named: Vec<Selection> = OPS
        .into_iter()
        .filter(|op| alu.lines.iter().any(|(_, ops)| ops.contains(op)))
        .map(|op| Selection {
            op,
            held: hold(alu, |k| alu.lines[k].1.contains(&op)),
            label: op.to_owned(),
        })
        .collect();
    // Operations whose signs choose the same lines, as two lines both
    // signed `nadd nsub`, are told apart by trying the lines, as one no
    // sign names is.
    let mut found: Vec<Selection> = Vec::new();
    for selection in &named {
        if named
            .iter()
            .filter(|other| other.held == selection.held)
            .count()
            == 1
        {
            found.push(Selection {
                op: selection.op,
                held: selection.held.clone(),
                label: selection.label.clone(),
            });
        }
    }
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
            invertible: false,
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
    let alu = polarized(blocks, &interface)?;
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
    merged.inverted = alu.inverted_lines(&interface);
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
    let carry = takes_carry(op) && alu.carry_in.is_some();
    // An operation taking the carry in drives it as a case; any other
    // holds it at 0, its lever as `hold` sets it.
    let mut inputs = vec![alu.a, alu.b];
    inputs.extend(alu.carry_in.filter(|_| carry));
    let held: Vec<(usize, u64)> = selection
        .held
        .iter()
        .copied()
        .filter(|&(port, _)| !carry || Some(port) != alu.carry_in)
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
        invertible: false,
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
    let carry_inverted = u64::from(carry && alu.carry_inverted);
    let expected = |case: &Case| {
        let carry = case[2] ^ carry_inverted;
        let result = apply(op, case[0], case[1], carry, width) & mask(width);
        statuses.iter().enumerate().fold(
            result,
            |value, (k, &(_, _, status))| {
                let on = status(op, case[0], case[1], carry, width);
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
    let alu = polarized(blocks, &interface)?;
    let mut slowest: Option<Timing> = None;
    let mut synced = true;
    for selection in selections(blocks, &interface, &alu)? {
        let mut inputs = vec![alu.a, alu.b];
        let mut held = selection.held.clone();
        if takes_carry(selection.op)
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

/// The flags the ALU in `blocks` shows: each operation its test checks but
/// add and subtract, which every ALU is taken to have, and its carry in,
/// and its carry out, overflow, and zero, whose meaning is plain.
pub fn flags(blocks: &Blocks) -> Result<Vec<String>> {
    let interface = crate::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let mut flags: Vec<String> = selections(blocks, &interface, &alu)?
        .iter()
        .map(|selection| selection.op)
        .filter(|op| !matches!(*op, "add" | "sub"))
        .map(str::to_owned)
        .collect();
    flags.dedup();
    let lines = [
        ("carry_in", alu.carry_in),
        ("carry_out", alu.carry_out),
        ("overflow", alu.overflow),
        ("zero", alu.zero),
    ];
    flags.extend(
        lines
            .into_iter()
            .filter(|(_, port)| port.is_some())
            .map(|(flag, _)| flag.to_owned()),
    );
    flags.sort();
    Ok(flags)
}

/// Whether the ALU in `blocks`, whose cases all came out right, holds
/// state, as `bench::remembers` tells of each of its operations, its
/// control lines held.
pub fn remembers(blocks: &Blocks) -> Result<bool> {
    let interface = crate::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    for selection in selections(blocks, &interface, &alu)? {
        let bench = Bench {
            inputs: vec![alu.a, alu.b],
            outputs: (0..interface.outputs.len()).collect(),
            pattern: selection.label.clone(),
            held: selection.held.clone(),
            care: u64::MAX,
            presses: Vec::new(),
            invertible: false,
        };
        if bench::remembers(blocks, &interface, &bench)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// How often the ALU in `blocks`, timed as `timing`, takes a new case, as
/// `throughput::shortest` streams them: each case A and B, and one of its
/// operations, its control lines set as a decoder does, timed against the
/// operands as works best, as a control line is, and a carry in for an
/// add. The shortest period at any timing of its lines; `None` when none up
/// to twice its slowest output keeps every result right.
pub fn throughput(blocks: &Blocks, timing: &Timing) -> Result<Option<i32>> {
    let interface = crate::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let ops = selections(blocks, &interface, &alu)?;
    if ops.is_empty() {
        return Ok(None);
    }
    let latency: Vec<i32> = timing
        .outputs
        .iter()
        .map(|o| o.latency + o.carried)
        .collect();
    if latency.len() != interface.outputs.len() {
        return Ok(None);
    }
    let slowest = latency.iter().copied().max().unwrap_or(1).max(1);
    let short = streamed(&interface, &alu, &ops, super::throughput::CASES);
    let long = streamed(&interface, &alu, &ops, super::throughput::LONG);
    let mut best: Option<i32> = None;
    for offset in -slowest..=slowest {
        let offsets: Vec<i32> = (0..interface.inputs.len())
            .map(|port| {
                if alu.lines.iter().any(|&(line, _)| line == port) {
                    offset
                } else {
                    0
                }
            })
            .collect();
        let most = best.map_or(2 * slowest, |b| b - 1);
        if most < 1 {
            break;
        }
        let found = super::throughput::shortest(
            blocks,
            (&short, &long),
            &offsets,
            &latency,
            most,
        )?;
        if found.is_some() {
            best = found;
        }
    }
    Ok(best)
}

/// A fixed stream of `count` cases for `alu`, from a simple generator:
/// each a value for every input, A and B, an operation's control lines,
/// and a carry in for an add.
fn streamed(
    interface: &Interface,
    alu: &Alu,
    ops: &[Selection],
    count: usize,
) -> Vec<Vec<u64>> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let all = mask(alu.width);
    (0..count)
        .map(|_| {
            let op =
                &ops[usize::try_from(next() % ops.len() as u64).unwrap_or(0)];
            let mut case = vec![0; interface.inputs.len()];
            case[alu.a] = next() & all;
            case[alu.b] = next() & all;
            for &(port, value) in &op.held {
                case[port] = value;
            }
            if takes_carry(op.op)
                && let Some(port) = alu.carry_in
            {
                case[port] = next() & 1;
            }
            case
        })
        .collect()
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
        assert_eq!(eight("nadd", 5, 7), !0b1100 & 0xff);
        assert_eq!(eight("nsub", 7, 5), 0b1111_1101);
        assert_eq!(eight("nsub", 5, 7), 0b0000_0001);
        assert!(carry_out("add", 200, 100, 0, 8));
        assert!(carry_out("sub", 5, 3, 0, 8) && !carry_out("sub", 3, 5, 0, 8));
        assert!(overflow("add", 100, 100, 0, 8));
        assert!(overflow("sub", 0x80, 1, 0, 8));
        assert!(!overflow("and", 0xff, 0xff, 0, 8));
    }
}
