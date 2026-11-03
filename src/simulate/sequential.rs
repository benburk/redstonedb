//! Testing a counter, a shift register, or a T flip-flop: a value it holds,
//! shown on its widest output, that its control lines change as they pulse,
//! counting it up or down, shifting it toward its high or its low bit,
//! loading input A into it, clearing it, or toggling it.
//!
//! A counter has one way to be driven: input A, as wide as its value, a
//! clock button, which counts it up or down, and a write enable lever, with
//! which the clock loads A instead. A T flip-flop has one too: one bit,
//! toggled by each pulse of its one control line. A shift register's lines
//! are found by trying each, pulsed alone or with one of its other levers
//! held on, from a value no two operations agree on; one must shift.
//!
//! Each operation is then checked: a count through every value and round,
//! a load of every value of A, and a clear or a shift from many values.
//!
//! A pulse is a button's press, or a lever turned on and off, each run
//! until nothing changes. Status lines, such as a shift out, are left out.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};

use super::{
    Broken, Run, Signal, Value, Wrong,
    bench::{mask, patterns},
    circuit::{Circuit, MAX_TICKS},
    timing::{BitTiming, InputTiming, OutputTiming, Start, Takes, Timing},
};
use crate::{
    category,
    recognize::{colour, interface},
    schem::Blocks,
};

/// Which kind of build a test takes it for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Counter,
    ShiftRegister,
    TFlipFlop,
}

impl Kind {
    /// The category it tests for.
    fn category(self) -> category::Kind {
        match self {
            Kind::Counter => category::Kind::Counter,
            Kind::ShiftRegister => category::Kind::ShiftRegister,
            Kind::TFlipFlop => category::Kind::TFlipFlop,
        }
    }

    fn noun(self) -> &'static str {
        self.category().facts().noun
    }
}

/// What a pulse does to the value held.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Up,
    Down,
    Load,
    Clear,
    /// Every bit inverted, a T flip-flop's.
    Toggle,
    /// Toward the high bit, the low bit taking `in`.
    ShiftUp {
        fill: bool,
    },
    /// Toward the low bit, the high bit taking `in`.
    ShiftDown {
        fill: bool,
    },
}

impl Op {
    /// What it makes of `value`, `data` on A, `width` bits wide.
    fn apply(self, value: Value, data: Value, width: usize) -> Value {
        let all = mask(width);
        match self {
            Op::Up => value.wrapping_add(1) & all,
            Op::Down => value.wrapping_sub(1) & all,
            Op::Load => data & all,
            Op::Clear => 0,
            Op::Toggle => !value & all,
            Op::ShiftUp { fill } => (value << 1 | Value::from(fill)) & all,
            Op::ShiftDown { fill } => {
                (value >> 1 | Value::from(fill) << (width - 1)) & all
            }
        }
    }

    /// Such as `counts up`.
    fn words(self) -> &'static str {
        match self {
            Op::Up => "counts up",
            Op::Down => "counts down",
            Op::Load => "loads A",
            Op::Clear => "clears it",
            Op::Toggle => "inverts it",
            Op::ShiftUp { .. } => "shifts it up",
            Op::ShiftDown { .. } => "shifts it down",
        }
    }

    /// Whether it is checked over a run of pulses, as a count is.
    fn counts(self) -> bool {
        matches!(self, Op::Up | Op::Down | Op::Toggle)
    }

    fn shifts(self) -> bool {
        matches!(self, Op::ShiftUp { .. } | Op::ShiftDown { .. })
    }
}

/// The operation, if any, that took `value` to `got` with `data` on A:
/// one of `value` and `data` that no two operations agree on.
fn classify(value: Value, data: Value, got: Value, width: usize) -> Option<Op> {
    let ops = [
        Op::Up,
        Op::Down,
        Op::Clear,
        Op::Load,
        Op::ShiftUp { fill: false },
        Op::ShiftUp { fill: true },
        Op::ShiftDown { fill: false },
        Op::ShiftDown { fill: true },
    ];
    ops.into_iter()
        .find(|op| op.apply(value, data, width) == got)
}

/// A way to make an operation: pulsing control line `pulse`, with control
/// lever `held` on, if any; each by its place among the control lines.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Way {
    pulse: usize,
    held: Option<usize>,
}

/// A build's lines, by their ports: input A, if it has one, its control
/// lines, each one bit, and the output showing the value it holds.
struct Lines {
    data: Option<usize>,
    controls: Vec<usize>,
    state: usize,
    width: usize,
    names: Vec<String>,
    state_name: String,
    data_name: Option<(String, usize)>,
}

/// The lines of the build in `blocks`, as its colours mark them.
fn lines(blocks: &Blocks, kind: Kind) -> Result<(interface::Interface, Lines)> {
    let mut interface = interface::read(blocks);
    // A counter's button is its clock and its lever its write enable, and
    // a T flip-flop's one control line its toggle.
    interface::name_lines(kind.category(), blocks, &mut interface);
    let mut data = None;
    let mut controls = Vec::new();
    for (i, port) in interface.inputs.iter().enumerate() {
        if port.signal_strength {
            bail!(
                "Input {} takes a signal strength, which the {} test does \
                 not drive.",
                port.name,
                kind.noun()
            );
        }
        if port.block.starts_with(colour::CONTROL) {
            if port.bits.len() != 1 {
                broken!(
                    "Control line {} has {} bits; each is one.",
                    port.name,
                    port.bits.len()
                );
            }
            controls.push(i);
        } else if data.replace(i).is_some() {
            broken!(
                "A {} takes at most one input bus, A, to load, but this \
                 has more.",
                kind.noun()
            );
        }
    }
    let Some((state, port)) = interface
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, port)| !port.block.starts_with(colour::STATUS))
        .max_by_key(|(_, port)| port.bits.len())
    else {
        broken!("A {} shows the value it holds on an output.", kind.noun());
    };
    if controls.is_empty() {
        broken!("A {} has a control line to pulse.", kind.noun());
    }
    let width = port.bits.len();
    if width > 32 {
        bail!("Its value has {width} bits, wider than the simulator tests.");
    }
    let lines = Lines {
        data,
        names: controls
            .iter()
            .map(|&i| interface.inputs[i].name.clone())
            .collect(),
        state_name: port.name.clone(),
        data_name: data.map(|i| {
            (
                interface.inputs[i].name.clone(),
                interface.inputs[i].bits.len(),
            )
        }),
        controls,
        state,
        width,
    };
    Ok((interface, lines))
}

/// The build running, with its lines.
struct Machine<'a> {
    circuit: Circuit,
    lines: &'a Lines,
}

impl Machine<'_> {
    /// The value held.
    fn read(&self) -> Value {
        self.circuit.get(self.lines.state)
    }

    /// Whether control line `line` is a button.
    fn button(&self, line: usize) -> bool {
        self.circuit.is_button(self.lines.controls[line], 0)
    }

    /// Makes `way` with `data` on A, running until nothing changes.
    fn apply(&mut self, way: Way, data: Value) -> Result<()> {
        if let Some(port) = self.lines.data {
            self.circuit.set(port, data);
            self.circuit.settle()?;
        }
        if let Some(held) = way.held {
            self.circuit.set(self.lines.controls[held], 1);
            self.circuit.settle()?;
        }
        self.pulse(way.pulse)?;
        if let Some(held) = way.held {
            self.circuit.set(self.lines.controls[held], 0);
            self.circuit.settle()?;
        }
        Ok(())
    }

    /// Brings the value held to `value`, as `setter` sets it: loading it,
    /// or counting to it.
    fn reach(&mut self, setter: Setter, value: Value) -> Result<()> {
        if setter.1 == Op::Load {
            return self.apply(setter.0, value);
        }
        for _ in 0..=mask(self.lines.width) {
            if self.read() == value {
                return Ok(());
            }
            self.apply(setter.0, 0)?;
        }
        bail!("Counting never reached {value}.")
    }

    /// Pulses `way` with A as it is, watching the value held tick by tick
    /// until nothing changes: each bit's change ticks, from the pulse.
    fn watch(&mut self, way: Way) -> Result<Vec<Vec<i32>>> {
        let width = self.lines.width;
        if let Some(held) = way.held {
            self.circuit.set(self.lines.controls[held], 1);
            self.circuit.settle()?;
        }
        let port = self.lines.controls[way.pulse];
        let button = self.button(way.pulse);
        if button {
            self.circuit.press(port, 0);
        } else {
            self.circuit.set(port, 1);
        }
        let mut ticks: Vec<Vec<i32>> = vec![Vec::new(); width];
        let mut last = self.read();
        let mut tick = 0;
        while self.circuit.step() {
            tick += 1;
            if tick > MAX_TICKS.cast_signed() {
                bail!("It was still changing {MAX_TICKS} ticks after a pulse.");
            }
            let now = self.read();
            for (bit, changes) in ticks.iter_mut().enumerate() {
                if (now ^ last) >> bit & 1 == 1 {
                    changes.push(tick);
                }
            }
            last = now;
        }
        if !button {
            self.circuit.set(port, 0);
            self.circuit.settle()?;
        }
        if let Some(held) = way.held {
            self.circuit.set(self.lines.controls[held], 0);
            self.circuit.settle()?;
        }
        Ok(ticks)
    }

    /// Pulses control line `line`: presses its button, or turns its lever
    /// on and off.
    fn pulse(&mut self, line: usize) -> Result<()> {
        let port = self.lines.controls[line];
        if self.button(line) {
            self.circuit.press(port, 0);
        } else {
            self.circuit.set(port, 1);
            self.circuit.settle()?;
            self.circuit.set(port, 0);
        }
        self.circuit.settle()?;
        Ok(())
    }
}

/// What a build was found to do, and the cases that checked it.
struct Found {
    ways: Vec<(Way, Op)>,
    cases: u64,
    wrong: u64,
    examples: Vec<Wrong>,
}

/// The most wrong cases a run keeps to show.
const EXAMPLES: usize = 10;

/// The most pulses a count is checked over.
const MOST_COUNTS: Value = 300;

/// The pulses a toggle is checked over: enough that one toggling only on
/// some pulses, or every other, shows it.
const TOGGLES: Value = 16;

/// A value of `width` bits no two operations agree on from, with `salt`
/// for a second such value: neither all off nor all on, its bits mixed.
fn telling(width: usize, salt: Value) -> Value {
    let all = mask(width);
    let value = Value::from(
        0xB2D4_96A5_C3E1_7F0B_u64
            .rotate_left(u32::try_from(salt * 7).unwrap_or(0)),
    ) & all;
    if width < 3 || value == 0 || value == all {
        // Narrow builds have few values; any not 0 does.
        return (1 + salt) & all;
    }
    value
}

/// Every way to make an operation: each control line pulsed alone, then
/// with each other lever held on.
fn ways(machine: &Machine) -> Vec<Way> {
    let n = machine.lines.controls.len();
    let mut ways: Vec<Way> =
        (0..n).map(|pulse| Way { pulse, held: None }).collect();
    for pulse in 0..n {
        for held in (0..n).filter(|&held| held != pulse) {
            if !machine.button(held) {
                ways.push(Way {
                    pulse,
                    held: Some(held),
                });
            }
        }
    }
    ways
}

/// Runs the counter or shift register in `blocks` through its cases.
pub fn check(blocks: &Blocks, kind: Kind) -> Result<Run> {
    let (interface, lines) = lines(blocks, kind)?;
    let fresh = || -> Result<Machine> {
        let mut circuit = Circuit::new(blocks, &interface)?;
        circuit.settle()?;
        Ok(Machine {
            circuit,
            lines: &lines,
        })
    };
    let found = find(&fresh, &lines, kind)?;
    Ok(run(&lines, found))
}

/// The ways `lines`' build makes each operation, and its cases.
fn find<'a>(
    fresh: &dyn Fn() -> Result<Machine<'a>>,
    lines: &Lines,
    kind: Kind,
) -> Result<Found> {
    match kind {
        Kind::Counter => return counter(fresh, lines),
        Kind::TFlipFlop => return toggle(fresh, lines),
        Kind::ShiftRegister => {}
    }
    let all_ways = ways(&fresh()?);
    let setter = setter(fresh, &all_ways, lines.width, kind)?;
    let found = operations(fresh, &all_ways, setter, lines.width)?;
    if !found.iter().any(|&(_, op)| op.shifts()) {
        broken!(
            "No control line shifts its value, which a shift register does."
        );
    }
    let mut tally = Tally::default();
    for &(way, op) in &found {
        tally.check(fresh, lines, setter, way, op)?;
    }
    Ok(Found {
        ways: found,
        cases: tally.cases,
        wrong: tally.wrong,
        examples: tally.examples,
    })
}

/// How a counter, of `lines`, makes its operations, and its cases: its
/// clock, pressed, counts up or down, and pressed with its write enable on
/// loads A.
fn counter<'a>(
    fresh: &dyn Fn() -> Result<Machine<'a>>,
    lines: &Lines,
) -> Result<Found> {
    let at = |name: &str| lines.names.iter().position(|line| line == name);
    let (Some(clock), Some(enable), 2, Some(width)) = (
        at("clock"),
        at("write enable"),
        lines.controls.len(),
        lines.data_name.as_ref().map(|&(_, bits)| bits),
    ) else {
        broken!(
            "A counter has input A, a clock button, and a write enable \
             lever, but this has {}.",
            lines
                .data_name
                .iter()
                .map(|(name, _)| name)
                .chain(&lines.names)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        );
    };
    if width != lines.width {
        broken!(
            "Input A has {width} bits, but the value it loads has {}.",
            lines.width
        );
    }
    let load = Way {
        pulse: clock,
        held: Some(enable),
    };
    let clock = Way {
        pulse: clock,
        held: None,
    };
    let first = telling(lines.width, 0);
    let mut machine = fresh()?;
    machine.apply(load, first)?;
    if machine.read() != first {
        broken!("Its clock, with write enable on, doesn't load A.");
    }
    machine.apply(clock, 0)?;
    let got = machine.read();
    let Some(count @ (Op::Up | Op::Down)) =
        classify(first, 0, got, lines.width).filter(|_| got != first)
    else {
        broken!("Its clock doesn't count: from {first} it gave {got}.");
    };
    let setter = (load, Op::Load);
    let ways = vec![(clock, count), (load, Op::Load)];
    let mut tally = Tally::default();
    for &(way, op) in &ways {
        tally.check(fresh, lines, setter, way, op)?;
    }
    Ok(Found {
        ways,
        cases: tally.cases,
        wrong: tally.wrong,
        examples: tally.examples,
    })
}

/// How a T flip-flop, of `lines`, makes its one operation, and its cases:
/// each pulse of its one control line toggles the bit it holds.
fn toggle<'a>(
    fresh: &dyn Fn() -> Result<Machine<'a>>,
    lines: &Lines,
) -> Result<Found> {
    let ([_], None, 1) =
        (lines.controls.as_slice(), &lines.data_name, lines.width)
    else {
        broken!(
            "A T flip-flop has one input, its toggle, and one output bit, \
             but this has {} inputs and {} output bits.",
            lines.controls.len() + usize::from(lines.data.is_some()),
            lines.width
        );
    };
    let way = Way {
        pulse: 0,
        held: None,
    };
    let mut tally = Tally::default();
    tally.check(fresh, lines, (way, Op::Toggle), way, Op::Toggle)?;
    Ok(Found {
        ways: vec![(way, Op::Toggle)],
        cases: tally.cases,
        wrong: tally.wrong,
        examples: tally.examples,
    })
}

/// A way to set the value held, and how it sets it: a load, or else a
/// count, to count to it.
type Setter = (Way, Op);

/// The way, of `all_ways`, that sets the value of a build of `width`
/// bits: one that loads A, or else, for a narrow build, one that counts.
fn setter<'a>(
    fresh: &dyn Fn() -> Result<Machine<'a>>,
    all_ways: &[Way],
    width: usize,
    kind: Kind,
) -> Result<Setter> {
    let first = telling(width, 0);
    for &way in all_ways {
        let mut machine = fresh()?;
        machine.apply(way, first)?;
        if machine.read() == first {
            return Ok((way, Op::Load));
        }
    }
    if width <= 10 {
        for &way in all_ways {
            let mut machine = fresh()?;
            let start = machine.read();
            machine.apply(way, 0)?;
            let got = machine.read();
            if let Some(op @ (Op::Up | Op::Down)) =
                classify(start, 0, got, width)
                && got != start
            {
                return Ok((way, op));
            }
        }
    }
    broken!(
        "No control line loads A or counts, so its value can't be set to \
         try the others; a {} needs one that {}.",
        kind.noun(),
        match kind {
            Kind::Counter => "counts",
            Kind::ShiftRegister | Kind::TFlipFlop => "loads A",
        }
    );
}

/// What each of `all_ways` does, from a value no two operations agree on:
/// with A all off, as a count or shift is made, and then with A on another
/// such value, which only a load takes. Each operation once, by the first
/// way found, a lever held that changes nothing of a line's own operation
/// being no part of it.
fn operations<'a>(
    fresh: &dyn Fn() -> Result<Machine<'a>>,
    all_ways: &[Way],
    setter: Setter,
    width: usize,
) -> Result<Vec<(Way, Op)>> {
    let (first, second) = (telling(width, 0), telling(width, 1));
    let mut found: Vec<(Way, Op)> = Vec::new();
    for &way in all_ways {
        let tried = |data: Value| -> Result<Value> {
            let mut machine = fresh()?;
            machine.reach(setter, first)?;
            machine.apply(way, data)?;
            Ok(machine.read())
        };
        let op = if tried(second)? == second {
            Some(Op::Load)
        } else {
            let got = tried(0)?;
            classify(first, 0, got, width)
                .filter(|&op| got != first && op != Op::Load)
        };
        let Some(op) = op else {
            continue;
        };
        let held_for_nothing = way.held.is_some()
            && found
                .iter()
                .any(|&(other, same)| other.pulse == way.pulse && same == op);
        if !held_for_nothing && !found.iter().any(|&(_, same)| same == op) {
            found.push((way, op));
        }
    }
    Ok(found)
}

/// The cases checked, and those wrong.
#[derive(Default)]
struct Tally {
    cases: u64,
    wrong: u64,
    examples: Vec<Wrong>,
}

impl Tally {
    /// Counts a case: `got` from `before` with `data` on A, where `expected`
    /// was due, made as `what` says.
    fn note(
        &mut self,
        before: Value,
        data: Value,
        expected: Value,
        got: Value,
        what: &str,
    ) {
        self.cases += 1;
        if got != expected {
            self.wrong += 1;
            if self.examples.len() < EXAMPLES {
                self.examples.push(Wrong {
                    op: Some(what.to_owned()),
                    inputs: vec![before, data],
                    expected: vec![expected],
                    got: vec![got],
                });
            }
        }
    }

    /// Checks operation `op`, made by `way`: a count through every value
    /// and round, a load of every value of A, or a clear or a shift from
    /// many values, each set by `setter`.
    fn check<'a>(
        &mut self,
        fresh: &dyn Fn() -> Result<Machine<'a>>,
        lines: &Lines,
        setter: Setter,
        way: Way,
        op: Op,
    ) -> Result<()> {
        let width = lines.width;
        let what = format!("{} {}", way_words(lines, way), op.words());
        let mut machine = fresh()?;
        if op.counts() {
            let pulses = if op == Op::Toggle {
                TOGGLES
            } else {
                (mask(width) + 2).min(MOST_COUNTS)
            };
            for _ in 0..pulses {
                let before = machine.read();
                machine.apply(way, 0)?;
                self.note(
                    before,
                    0,
                    op.apply(before, 0, width),
                    machine.read(),
                    &what,
                );
            }
            return Ok(());
        }
        let values: Vec<Value> = if width <= 8 {
            (0..=mask(width)).collect()
        } else {
            patterns(width)
        };
        // Counting to each value is slow, so fewer are tried.
        let tried = if setter.1 == Op::Load {
            values.len()
        } else {
            32
        };
        for &value in values.iter().take(tried) {
            // A is all off but for a load, as a count or shift is made.
            let data = if op == Op::Load { value } else { 0 };
            if op != Op::Load {
                machine.reach(setter, value)?;
            }
            let before = machine.read();
            machine.apply(way, data)?;
            self.note(
                before,
                data,
                op.apply(before, data, width),
                machine.read(),
                &what,
            );
        }
        Ok(())
    }
}

/// Such as `clock`, or `clock with write enable on`.
fn way_words(lines: &Lines, way: Way) -> String {
    match way.held {
        Some(held) => {
            format!("{} with {} on", lines.names[way.pulse], lines.names[held])
        }
        None => lines.names[way.pulse].clone(),
    }
}

/// The run `found` makes.
fn run(lines: &Lines, found: Found) -> Run {
    let pattern = found
        .ways
        .iter()
        .map(|&(way, op)| format!("{} {}", way_words(lines, way), op.words()))
        .collect::<Vec<_>>()
        .join("; ");
    let mut inputs: Vec<Signal> = lines
        .data_name
        .iter()
        .map(|(name, bits)| Signal {
            name: name.clone(),
            bits: *bits,
        })
        .collect();
    inputs.extend(lines.names.iter().map(|name| Signal {
        name: name.clone(),
        bits: 1,
    }));
    Run {
        pattern,
        cases: found.cases,
        exhaustive: lines.width <= 8,
        patterns: 0,
        by_digit: false,
        wrong: found.wrong,
        inputs,
        outputs: vec![Signal {
            name: lines.state_name.clone(),
            bits: lines.width,
        }],
        wrong_bits: Vec::new(),
        examples: found.examples,
        hint: None,
        inverted: Vec::new(),
        reading: None,
        strength: None,
        cells: Vec::new(),
        faults: Vec::new(),
        timing: None,
        stacking: None,
        design: None,
    }
}

/// How fast the counter or shift register in `blocks` changes its value:
/// for its counting or shifting operation, tick by tick from each pulse
/// until nothing changes, over a count through every value, or shifts from
/// many. Meant for one whose cases all came out right.
pub fn time(
    blocks: &Blocks,
    kind: Kind,
    cancel: &AtomicBool,
) -> Result<Timing> {
    let (interface, lines) = lines(blocks, kind)?;
    let fresh = || -> Result<Machine> {
        let mut circuit = Circuit::new(blocks, &interface)?;
        circuit.settle()?;
        Ok(Machine {
            circuit,
            lines: &lines,
        })
    };
    let found = find(&fresh, &lines, kind)?;
    let Some(&(way, op)) = found
        .ways
        .iter()
        .find(|&&(_, op)| op.counts() || op.shifts())
    else {
        bail!("It has no count or shift to time.");
    };
    let load = found
        .ways
        .iter()
        .find(|&&(_, op)| op == Op::Load)
        .map(|&(way, _)| way);
    let width = lines.width;
    let mut machine = fresh()?;
    let carried = machine.circuit.carried()[lines.state];
    let mut clocked = Clocked::new(width);
    for step in 0..(mask(width) + 1).min(MOST_COUNTS) {
        if cancel.load(Ordering::Relaxed) {
            bail!("The timing was cancelled.");
        }
        // A shift is timed from a fresh value each pulse, loaded when it
        // can be, as shifting runs the value out.
        if op.shifts()
            && let Some(load) = load
        {
            machine.apply(load, telling(width, step))?;
        }
        let ticks: Vec<Vec<i32>> = machine
            .watch(way)?
            .into_iter()
            .map(|bit| bit.into_iter().map(|tick| tick - carried).collect())
            .collect();
        clocked.add(&ticks);
    }
    Ok(clocked.timing(&lines, way, carried))
}

/// The changes of the value held, each pulse's, as they are timed.
struct Clocked {
    bits: Vec<BitTiming>,
    glitches: u64,
    staggered: u64,
    changes: u64,
    /// The tick each pulse's change settled by.
    settles: Vec<i32>,
}

impl Clocked {
    fn new(width: usize) -> Self {
        Clocked {
            bits: vec![
                BitTiming {
                    first_change: 0,
                    latency: 0,
                    glitches: 0,
                };
                width
            ],
            glitches: 0,
            staggered: 0,
            changes: 0,
            settles: Vec::new(),
        }
    }

    /// Takes one pulse's change ticks, each bit's.
    fn add(&mut self, ticks: &[Vec<i32>]) {
        self.changes += 1;
        let firsts: Vec<i32> =
            ticks.iter().filter_map(|t| t.first().copied()).collect();
        if firsts.iter().any(|&t| t != firsts[0]) {
            self.staggered += 1;
        }
        if ticks.iter().any(|t| t.len() > 1) {
            self.glitches += 1;
        }
        for (bit, t) in self.bits.iter_mut().zip(ticks) {
            if let (Some(&first), Some(&end)) = (t.first(), t.last()) {
                bit.first_change = bit.first_change.max(first);
                bit.latency = bit.latency.max(end);
                if t.len() > 1 {
                    bit.glitches += 1;
                }
            }
        }
        if let Some(&end) = ticks.iter().filter_map(|t| t.last()).max() {
            self.settles.push(end);
        }
    }

    /// The timing its pulses of `way` give, `carried` the ticks its output
    /// takes to carry the value out.
    fn timing(mut self, lines: &Lines, way: Way, carried: i32) -> Timing {
        self.settles.sort_unstable();
        let settles = &self.settles;
        let at = |share: usize| {
            settles
                .get(
                    (settles.len() * share / 100)
                        .min(settles.len().saturating_sub(1)),
                )
                .copied()
                .unwrap_or(0)
        };
        let (median, p99) = (at(50), at(99));
        Timing {
            inputs: vec![InputTiming {
                name: lines.names[way.pulse].clone(),
                tick: Some(0),
                earliest: None,
            }],
            outputs: vec![OutputTiming {
                name: lines.state_name.clone(),
                status: false,
                latency: self
                    .bits
                    .iter()
                    .map(|bit| bit.latency)
                    .max()
                    .unwrap_or(0),
                first_change: self
                    .bits
                    .iter()
                    .map(|bit| bit.first_change)
                    .min()
                    .unwrap_or(0),
                carried,
                glitches: self.glitches,
                staggered: self.staggered,
                glitch: None,
                bits: self.bits,
            }],
            synced: self.glitches == 0 && self.staggered == 0,
            median,
            p99,
            slowest: self.settles.last().copied().unwrap_or(0),
            slowest_case: Vec::new(),
            changes: self.changes,
            patterns: false,
            start: Start::Clock,
            write: None,
            data: None,
            takes: Takes::Open,
            pulse: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From a telling value, with A all off, as a count, clear, or shift
    /// is tried, each gives a value of its own; a load is told by giving
    /// A's other telling value.
    #[test]
    fn each_operation_is_told_apart_from_a_telling_value() {
        for width in [3, 7, 8, 16] {
            let (value, data) = (telling(width, 0), telling(width, 1));
            assert_ne!(value, data);
            let ops = [
                Op::Up,
                Op::Down,
                Op::Clear,
                Op::ShiftUp { fill: false },
                Op::ShiftUp { fill: true },
                Op::ShiftDown { fill: false },
                Op::ShiftDown { fill: true },
            ];
            for op in ops {
                let got = op.apply(value, 0, width);
                assert_eq!(
                    classify(value, 0, got, width),
                    Some(op),
                    "{width} {op:?}"
                );
            }
            assert_eq!(Op::Load.apply(value, data, width), data);
        }
    }
}
