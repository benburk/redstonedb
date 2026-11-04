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
//! author's choice, so it is not checked.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::atomic::AtomicBool,
};

use anyhow::Result;

use super::{
    Broken, Run, Timing, Value,
    adder::adder_cases,
    bench::{self, Bench, Case, cases, mask, observe, paired, sweep},
    circuit::{input_bits, output_bits},
    rng::Rng,
    timing,
};
use crate::{
    recognize::{colour, interface::Interface},
    schem::Blocks,
};

/// An operation an ALU may offer. They are ordered as a result lists
/// them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Op {
    Add,
    Sub,
    /// Add with every bit of the result inverted.
    Nadd,
    /// Subtract with every bit of the result inverted.
    Nsub,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
    Shl,
    /// Shift right, the top bit taking 0.
    Shr,
    /// Shift right keeping the sign, the top bit kept.
    Asr,
}

impl Op {
    /// Every operation, in the order a result lists them.
    const ALL: [Op; 13] = [
        Op::Add,
        Op::Sub,
        Op::Nadd,
        Op::Nsub,
        Op::And,
        Op::Or,
        Op::Xor,
        Op::Nand,
        Op::Nor,
        Op::Xnor,
        Op::Shl,
        Op::Shr,
        Op::Asr,
    ];

    /// The operation a word on a control line's sign names, such as shr
    /// for `rsh`; `None` for any other word.
    pub fn named(word: &str) -> Option<Op> {
        let word = match word {
            "rsh" | "lsr" | "srl" => "shr",
            "lsh" | "lsl" | "sll" => "shl",
            "sra" => "asr",
            "subtract" => "sub",
            other => other,
        };
        Op::ALL.into_iter().find(|op| op.name() == word)
    }

    /// Its name, as a sign and a result write it and as its flag.
    pub fn name(self) -> &'static str {
        match self {
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Nadd => "nadd",
            Op::Nsub => "nsub",
            Op::And => "and",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::Nand => "nand",
            Op::Nor => "nor",
            Op::Xnor => "xnor",
            Op::Shl => "shl",
            Op::Shr => "shr",
            Op::Asr => "asr",
        }
    }

    /// Whether it takes the carry in: an add, its result inverted or not.
    fn takes_carry(self) -> bool {
        matches!(self, Op::Add | Op::Nadd)
    }

    /// What it gives for A and B of `width` bits, with `carry` into an
    /// add, before it is cut to `width` bits.
    fn apply(self, a: Value, b: Value, carry: Value, width: usize) -> Value {
        let top = 1 << (width - 1);
        match self {
            Op::Add => a + b + carry,
            Op::Sub => a.wrapping_sub(b),
            Op::Nadd => !(a + b + carry),
            Op::Nsub => !a.wrapping_sub(b),
            Op::And => a & b,
            Op::Or => a | b,
            Op::Xor => a ^ b,
            Op::Nand => !(a & b),
            Op::Nor => !(a | b),
            Op::Xnor => !(a ^ b),
            Op::Shl => a << 1,
            Op::Shr => a >> 1,
            Op::Asr => a >> 1 | a & top,
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The name of the operation a word on a control line's sign names, such
/// as `shr` for `rsh`, as its flag reads; `None` for any other word.
pub fn op(word: &str) -> Option<&'static str> {
    Op::named(word).map(Op::name)
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
    lines: Vec<(usize, Vec<Op>)>,
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
    let control = |i: &usize| interface.inputs[*i].control();
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
    let lines: Vec<(usize, Vec<Op>)> = lines
        .into_iter()
        .filter(|i| Some(*i) != carry_in)
        .map(|i| {
            let ops = interface.inputs[i]
                .words
                .iter()
                .filter_map(|w| Op::named(w));
            (i, ops.collect())
        })
        .collect();
    if lines.is_empty() {
        broken!(
            "An ALU has control lines choosing its operation, but this has none."
        );
    }
    let status = |i: &usize| interface.outputs[*i].status();
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
    seen: HashMap<u64, Vec<Value>>,
}

impl Looks<'_> {
    /// What `levers` give, looked at once.
    fn look(&mut self, levers: u64) -> Result<&[Value]> {
        if !self.seen.contains_key(&levers) {
            let alu = self.alu;
            let bench = Bench {
                inputs: vec![alu.a, alu.b],
                outputs: alu.data.clone(),
                pattern: String::new(),
                held: (0..alu.lines.len())
                    .map(|k| (alu.lines[k].0, Value::from(levers >> k & 1)))
                    .chain(alu.carry_in.map(|port| (port, 0)))
                    .collect(),
                care: Value::MAX,
                press: None,
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
    fn right_with(&self, op: Op, got: &[Value], carry: Value) -> usize {
        let width = self.alu.width;
        self.telling
            .iter()
            .zip(got)
            .filter(|&(case, &got)| {
                op.apply(case[0], case[1], carry, width) & mask(width)
                    == got & mask(width)
            })
            .count()
    }

    /// How many telling cases `got` gives `op` right, an add with the carry
    /// in either way.
    fn right(&self, op: Op, got: &[Value]) -> usize {
        (0..=Value::from(op.takes_carry()))
            .map(|carry| self.right_with(op, got, carry))
            .max()
            .unwrap_or(0)
    }
}

/// The levers choosing `op` on `alu`, a bit per control line, its lines
/// read as `inverted`.
fn levers(alu: &Alu, op: Op, inverted: u64) -> u64 {
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
    let named: Vec<Op> = Op::ALL
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
    named: &[Op],
    inverted: u64,
) -> Result<bool> {
    let alu = looks.alu;
    let settings: Vec<u64> = match named.iter().find(|op| op.takes_carry()) {
        Some(&op) => vec![levers(alu, op, inverted)],
        None if alu.lines.len() <= 6 => (0..1u64 << alu.lines.len())
            .map(|on| on ^ inverted)
            .collect(),
        None => vec![inverted],
    };
    let enough = looks.telling.len() * 3;
    for setting in settings {
        let got = looks.look(setting)?.to_vec();
        for op in [Op::Add, Op::Nadd] {
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
fn search(looks: &mut Looks, named: &[Op], saved: u64) -> Result<u64> {
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
            for &op in named {
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
            for &op in named {
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
                for &op in named {
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
    op: Op,
    /// Each control line's value, and the carry in held off.
    held: Vec<(usize, Value)>,
    /// How it reads in a result, such as `add` for one a sign names, or
    /// `or (and + xor on)` for one found by trying the lines.
    label: String,
}

/// The levers setting every line off but those in `on`, an inverted
/// line's lever on when it is off, with the carry in held off.
fn hold(alu: &Alu, on: impl Fn(usize) -> bool) -> Vec<(usize, Value)> {
    alu.lines
        .iter()
        .enumerate()
        .map(|(k, &(port, _))| (port, Value::from(on(k) != alu.inverted[k])))
        .chain(
            alu.carry_in
                .map(|port| (port, Value::from(alu.carry_inverted))),
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
    let named: Vec<Selection> = Op::ALL
        .into_iter()
        .filter(|op| alu.lines.iter().any(|(_, ops)| ops.contains(op)))
        .map(|op| Selection {
            op,
            held: hold(alu, |k| alu.lines[k].1.contains(&op)),
            label: op.name().to_owned(),
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
            care: Value::MAX,
            press: None,
            invertible: false,
        };
        let got = observe(blocks, interface, &bench, &telling, None)?;
        let right = |op: Op| {
            telling
                .iter()
                .zip(&got)
                .filter(|&(case, &got)| {
                    op.apply(case[0], case[1], 0, alu.width) & mask(alu.width)
                        == got & mask(alu.width)
                })
                .count()
        };
        // The operation it gets most right, if three in four, so one with
        // a few wrong bits is still checked, and its faults found.
        let new = Op::ALL
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
    found.sort_by_key(|s| s.op);
    Ok(found)
}

/// Runs an ALU through every operation its signs name, and any other its
/// control lines are found to give, and checks every result, or with
/// `quick` a few telling ones of each.
pub fn check(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = crate::recognize::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let found = selections(blocks, &interface, &alu)?;
    if found.is_empty() {
        broken!("No setting of its control lines gives an operation.");
    }
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
type Status = (usize, bool, fn(Op, Value, Value, Value, usize) -> bool);

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
    let carry = op.takes_carry() && alu.carry_in.is_some();
    // An operation taking the carry in drives it as a case; any other
    // holds it at 0, its lever as `hold` sets it.
    let mut inputs = vec![alu.a, alu.b];
    inputs.extend(alu.carry_in.filter(|_| carry));
    let held: Vec<(usize, Value)> = selection
        .held
        .iter()
        .copied()
        .filter(|&(port, _)| !carry || Some(port) != alu.carry_in)
        .collect();
    let arithmetic = matches!(op, Op::Add | Op::Sub);
    let statuses: Vec<Status> = [
        (
            alu.carry_out,
            arithmetic,
            carry_out as fn(Op, Value, Value, Value, usize) -> bool,
        ),
        (alu.overflow, arithmetic, overflow),
        (alu.shift_out, matches!(op, Op::Shr | Op::Asr), shift_out),
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
        press: None,
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
    let carry_inverted = Value::from(carry && alu.carry_inverted);
    let expected = |case: &Case| {
        let carry = case[2] ^ carry_inverted;
        let result = op.apply(case[0], case[1], carry, width) & mask(width);
        statuses.iter().enumerate().fold(
            result,
            |value, (k, &(_, _, status))| {
                let on = status(op, case[0], case[1], carry, width);
                value | Value::from(on) << (data_bits + k)
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
fn carry_out(op: Op, a: Value, b: Value, carry: Value, width: usize) -> bool {
    if op == Op::Sub {
        return a >= b;
    }
    (a + b + carry) >> width & 1 == 1
}

/// Whether `op` on A and B overflows, as signed numbers of `width` bits.
/// Only an add or subtract does; the overflow line is checked for no
/// other.
fn overflow(op: Op, a: Value, b: Value, carry: Value, width: usize) -> bool {
    let top = 1 << (width - 1);
    let result = op.apply(a, b, carry, width);
    let (sa, sb, sr) = (a & top != 0, b & top != 0, result & top != 0);
    match op {
        Op::Add => sa == sb && sr != sa,
        Op::Sub => sa != sb && sr != sa,
        Op::Nadd
        | Op::Nsub
        | Op::And
        | Op::Or
        | Op::Xor
        | Op::Nand
        | Op::Nor
        | Op::Xnor
        | Op::Shl
        | Op::Shr
        | Op::Asr => false,
    }
}

/// Whether a shift right of A drops a 1: A's lowest bit.
fn shift_out(_: Op, a: Value, _: Value, _: Value, _: usize) -> bool {
    a & 1 == 1
}

/// Whether `op`'s result is 0.
fn zero(op: Op, a: Value, b: Value, carry: Value, width: usize) -> bool {
    op.apply(a, b, carry, width) & mask(width) == 0
}

/// Measures each operation's timing, and returns the slowest's, synced
/// only when every operation's result is.
pub fn time(blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
    let interface = crate::recognize::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let timings =
        selections(blocks, &interface, &alu)?
            .into_iter()
            .map(|selection| {
                let mut inputs = vec![alu.a, alu.b];
                let mut held = selection.held;
                if selection.op.takes_carry()
                    && let Some(port) = alu.carry_in
                {
                    inputs.push(port);
                    held.retain(|&(p, _)| p != port);
                }
                timing::measure(
                    blocks, &interface, &inputs, &held, None, cancel,
                )
            });
    // Its status lines, such as a carry out, may settle in steps.
    let synced = |timing: &Timing| {
        timing
            .outputs
            .iter()
            .filter(|output| !output.status)
            .all(|output| output.glitches == 0 && output.staggered == 0)
    };
    timing::slowest(timings, synced, "no operation to time")
}

/// The flags the ALU in `blocks` shows: each operation its test checks but
/// add and subtract, which every ALU is taken to have, and its carry in,
/// and its carry out, overflow, and zero, whose meaning is plain.
pub fn flags(blocks: &Blocks) -> Result<Vec<String>> {
    let interface = crate::recognize::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let mut flags: Vec<String> = selections(blocks, &interface, &alu)?
        .iter()
        .map(|selection| selection.op)
        .filter(|op| !matches!(op, Op::Add | Op::Sub))
        .map(|op| op.name().to_owned())
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
    let interface = crate::recognize::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let outputs: Vec<usize> = (0..interface.outputs.len()).collect();
    bench::remembers_any(
        blocks,
        &interface,
        (&[alu.a, alu.b], &outputs),
        selections(blocks, &interface, &alu)?
            .into_iter()
            .map(|selection| selection.held),
    )
}

/// How often the ALU in `blocks`, timed as `timing`, takes a new case, as
/// `throughput::shortest` streams them: each case A and B, and one of its
/// operations, its control lines set as a decoder does, timed against the
/// operands as works best, as a control line is, and a carry in for an
/// add. The shortest period at any timing of its lines; `None` when none up
/// to twice its slowest output keeps every result right.
pub fn throughput(blocks: &Blocks, timing: &Timing) -> Result<Option<i32>> {
    let interface = crate::recognize::interface::read(blocks);
    let alu = polarized(blocks, &interface)?;
    let ops = selections(blocks, &interface, &alu)?;
    if ops.is_empty() {
        return Ok(None);
    }
    super::throughput::controlled(
        blocks,
        &interface,
        timing,
        |port| alu.lines.iter().any(|&(line, _)| line == port),
        |count| streamed(&interface, &alu, &ops, count),
    )
}

/// A fixed stream of `count` cases for `alu`, from a simple generator:
/// each a value for every input, A and B, an operation's control lines,
/// and a carry in for an add.
fn streamed(
    interface: &Interface,
    alu: &Alu,
    ops: &[Selection],
    count: usize,
) -> Vec<Vec<Value>> {
    let mut rng = Rng::default();
    let all = mask(alu.width);
    (0..count)
        .map(|_| {
            let op = &ops[rng.below(ops.len())];
            let mut case = vec![0; interface.inputs.len()];
            case[alu.a] = Value::from(rng.draw()) & all;
            case[alu.b] = Value::from(rng.draw()) & all;
            for &(port, value) in &op.held {
                case[port] = value;
            }
            if op.op.takes_carry()
                && let Some(port) = alu.carry_in
            {
                case[port] = Value::from(rng.draw() & 1);
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
        assert_eq!(Op::named("rsh"), Some(Op::Shr));
        assert_eq!(Op::named("sra"), Some(Op::Asr));
        assert_eq!(Op::named("and"), Some(Op::And));
        assert_eq!(Op::named("inverted"), None);
        assert_eq!(op("lsl"), Some("shl"));
    }

    #[test]
    fn each_operation_gives_its_result_on_eight_bits() {
        let eight = |op: Op, a, b| op.apply(a, b, 0, 8) & 0xff;
        assert_eq!(eight(Op::Sub, 5, 7), 254);
        assert_eq!(eight(Op::Nand, 0b1100, 0b1010), 0b1111_0111);
        assert_eq!(eight(Op::Asr, 0b1000_0010, 0), 0b1100_0001);
        assert_eq!(eight(Op::Shr, 0b1000_0010, 0), 0b0100_0001);
        assert_eq!(eight(Op::Nadd, 5, 7), !0b1100 & 0xff);
        assert_eq!(eight(Op::Nsub, 7, 5), 0b1111_1101);
        assert_eq!(eight(Op::Nsub, 5, 7), 0b0000_0001);
        assert!(carry_out(Op::Add, 200, 100, 0, 8));
        assert!(
            carry_out(Op::Sub, 5, 3, 0, 8) && !carry_out(Op::Sub, 3, 5, 0, 8)
        );
        assert!(overflow(Op::Add, 100, 100, 0, 8));
        assert!(overflow(Op::Sub, 0x80, 1, 0, 8));
        assert!(!overflow(Op::And, 0xff, 0xff, 0, 8));
    }
}
