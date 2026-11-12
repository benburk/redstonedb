//! When a combinational build's outputs settle, and whether they settle
//! cleanly.
//!
//! Every time is told from t = 0, as the data inputs every output waits
//! longest on change, so no operand is taken as the one others are timed
//! against and a build times the same whichever is named A. Each input's timing
//! is the latest tick at which it may change without delaying any output:
//! an output it reaches in fewer ticks than the input it waits longest on
//! lets it arrive that much later, and an input that is the slowest for
//! some output changes at t = 0. Each output's latency is the last tick it
//! settles by, from its slowest inputs changing, as from every input
//! changing at once. The build is synced when every output changes
//! cleanly, each bit at most once, and every data output's bits, across
//! them all, settle on the same tick; a status line, such as carry out,
//! may settle on a tick of its own. Latency is data in to data out: a
//! control line, such as a carry in, a decrement line, or a barrel
//! shifter's amount, slower than the data is switched before t = 0, as
//! early as it needs. Each output bit
//! is timed as its signal arrives at the repeater or comparator carrying it
//! out of the build, leaving out that one's own delay, as every timing
//! does, [`Carried`]. An output whose bit changes more than once glitches:
//! it pulses when it should stay put, or flips back and forth on its way to
//! a new value.
//!
//! Two kinds of change are measured, each from a settled state:
//!
//! - each input alone, from 0 to every value and back, with the other
//!   inputs held at every combination of theirs, which gives each input's
//!   timing;
//! - every input together, each at its timing, from all off to every
//!   combination and back;
//! - every input together, each at its timing, from one set of values
//!   straight to another drawn at random over every value each may take,
//!   the same each time: a race between two values neither of them all off
//!   shows only so.
//!
//! A build with too many input bits to try every value takes the test
//! patterns instead, such as all ones or alternate bits set, which make an
//! adder carry furthest. Every pair of input states is far too many to try
//! either way, so a worse change may exist than the worst of these.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::{
    Value,
    bench::{self, EXHAUSTIVE_BITS},
    circuit::{Carried, Circuit, MAX_TICKS, input_bits, output_bits},
    rng::Rng,
    value,
};
use crate::{interface::Interface, schem::Blocks};

/// When a build's inputs may change and its outputs settle, in ticks from
/// t = 0, as the slowest inputs change.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Timing {
    /// Each input, A first.
    pub inputs: Vec<InputTiming>,
    pub outputs: Vec<OutputTiming>,
    /// Whether every output changed only once, all its bits on the same
    /// tick, in every change tried.
    pub synced: bool,
    /// The tick by which the whole result settled, from all off to each
    /// combination of inputs: in half of them, in 99%, and in the slowest.
    pub median: i32,
    pub p99: i32,
    pub slowest: i32,
    /// The inputs of the slowest, in the order of `inputs`.
    #[serde(with = "value::many")]
    pub slowest_case: Vec<Value>,
    /// How many changes were measured.
    pub changes: u64,
    /// Whether the inputs took the test patterns rather than every value.
    pub patterns: bool,
    /// What its ticks count from.
    #[serde(default)]
    pub start: Start,
    /// For a register, the most ticks from pressing a cell's write button,
    /// its read held on, until the bus shows the new value; or, for a cell
    /// that takes its data as its write closes, from it closing.
    pub write: Option<i32>,
    /// For a register, the most ticks for data to pass through a cell, its
    /// read on: from its data in changing, with its write held open, or for
    /// a cell that takes its data as its write closes, as it closes, until
    /// the bus shows it. `None` for one that takes its data only as its
    /// write opens.
    pub data: Option<i32>,
    /// When a register's cells take their data.
    pub takes: Takes,
    /// For a monostable, the pulse it gives as A turns on.
    #[serde(default)]
    pub pulse: Option<Pulse>,
    /// For a counter that keeps counting while its clock is held on, the
    /// ticks from one count to the next then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_running: Option<i32>,
}

/// The pulse a monostable gives as input A changes on its edge.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    /// How many ticks it lasts.
    pub ticks: i32,
    /// Whether its output rests on, so the pulse turns it off.
    pub inverted: bool,
    #[serde(default)]
    pub edge: Edge,
}

/// Which changes of input A give a monostable's pulse.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    /// A turning on.
    #[default]
    Rising,
    /// A turning off.
    Falling,
    /// Either.
    Both,
}

impl Edge {
    /// Whether A turning on, if `on`, or off gives the pulse.
    pub fn triggers(self, on: bool) -> bool {
        match self {
            Edge::Rising => on,
            Edge::Falling => !on,
            Edge::Both => true,
        }
    }

    /// Such as `A turns on`.
    pub fn words(self) -> &'static str {
        match self {
            Edge::Rising => "A turns on",
            Edge::Falling => "A turns off",
            Edge::Both => "A turns on or off",
        }
    }

    /// Such as `A turning on`.
    pub fn turning(self) -> &'static str {
        match self {
            Edge::Rising => "A turning on",
            Edge::Falling => "A turning off",
            Edge::Both => "A turning on or off",
        }
    }
}

/// What a timing's ticks count from.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Start {
    /// The inputs changing, the slowest at t = 0.
    #[default]
    Input,
    /// A register's read lever turning on.
    Read,
    /// A buffer's write, its button pressed or lever turned on, or for one
    /// that takes its data as its write closes, it closing.
    Write,
    /// A counter's or shift register's clock, its button pressed, or its
    /// lever turned on, for a counter's 2 ticks.
    Clock,
}

/// When a register's cells take their data from their data in.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Takes {
    /// While their writes are open, or as they open.
    #[default]
    Open,
    /// As their writes close, their levers turning off or their buttons
    /// springing back, their buses empty while they write.
    Closing,
}

impl Timing {
    /// The latency of the slowest data output, leaving out status lines
    /// such as carry out: the build's latency. A register's, or a
    /// buffer's, is data in to data out, through a cell whose write is held
    /// open and read on, so a data loop's latency is its ALU's plus its
    /// registers'; or, for one that takes its data only as its write opens,
    /// from the write's button.
    /// A monostable's is from A changing on its edge to its pulse
    /// starting.
    pub fn latency(&self) -> Option<i32> {
        if self.pulse.is_some() {
            return self.outputs.iter().map(|output| output.first_change).max();
        }
        if self.start != Start::Input {
            return self.data.or(self.write).or_else(|| self.read());
        }
        self.read()
    }

    /// The slowest data output's ticks, from the inputs changing or, for a
    /// register, from a read lever turning on, or for a buffer, from a
    /// write.
    pub fn read(&self) -> Option<i32> {
        // A build whose only outputs are status lines, as a carry out's,
        // goes by them.
        let data = self.outputs.iter().any(|output| !output.status);
        self.outputs
            .iter()
            .filter(|output| !output.status || !data)
            .map(|output| output.latency)
            .max()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputTiming {
    pub name: String,
    /// The latest tick it may change, from t = 0, which every output's
    /// slowest input changes at; for a control line pressed after its
    /// operands, from theirs, maybe before them. `None` for an input no
    /// output follows.
    pub tick: Option<i32>,
    /// For a pulse, as a write button gives, the earliest tick it may
    /// come, before which it ends too soon; `None` for a line that may
    /// change as early as it likes.
    #[serde(default)]
    pub earliest: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputTiming {
    pub name: String,
    /// A status line, such as carry out, rather than a data output.
    pub status: bool,
    /// The last tick it settled by.
    pub latency: i32,
    /// The first tick it changed on.
    pub first_change: i32,
    /// The ticks the repeaters or comparators standing on its blocks take
    /// to carry it out of the build, left out of its ticks: the most of
    /// them, where its bits' differ.
    pub carried: i32,
    /// How many changes it glitched in: a bit of it changed more than once.
    pub glitches: u64,
    /// How many changes its bits changed on different ticks in, each once,
    /// passing through a value it should not show.
    pub staggered: u64,
    /// The first change from all off in which it glitched.
    pub glitch: Option<Glitch>,
    /// Each bit, least significant first.
    pub bits: Vec<BitTiming>,
}

/// One change in which an output glitched, to try in-game.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Glitch {
    /// The inputs it changed from, in the order of `Timing::inputs`; empty
    /// for all off, as every glitch was before others were measured.
    #[serde(
        default,
        with = "value::many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub from: Vec<Value>,
    /// The inputs it changed to, in the order of `Timing::inputs`.
    #[serde(with = "value::many")]
    pub case: Vec<Value>,
    /// The output's value before the change.
    #[serde(with = "value")]
    pub before: Value,
    /// Each tick the output changed on, from t = 0, and its value from
    /// then.
    #[serde(with = "value::many::ticked")]
    pub steps: Vec<(i32, Value)>,
}

impl Glitch {
    /// Whether the output should have stayed as it was, so it pulsed,
    /// rather than flipping back and forth on its way to a new value.
    pub fn pulse(&self) -> bool {
        self.steps
            .last()
            .is_none_or(|&(_, value)| value == self.before)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct BitTiming {
    pub first_change: i32,
    pub latency: i32,
    /// How many changes it changed more than once in.
    pub glitches: u64,
}

/// Measures the build whose input ports, by their place in `interface`, are
/// `inputs`, A first, using every core, with the `held` inputs set and
/// left alone, such as the control lines choosing an ALU's operation. Each
/// input takes the values `chosen` gives, 0 first, with whether they are a
/// selection rather than every one, as for an input in BCD; else every
/// value, or the test patterns when that would be too many. Changes drawn
/// at random take any value of each input's width when `any`, as an
/// adder's operands may, else only the values `chosen` gives, as only
/// valid BCD may drive a converter from it.
pub(super) fn measure(
    blocks: &Blocks,
    interface: &Interface,
    inputs: &[usize],
    held: &[(usize, Value)],
    (chosen, any): (Chosen, bool),
    cancel: &AtomicBool,
) -> Result<Timing> {
    let widths: Vec<usize> = inputs
        .iter()
        .map(|&port| input_bits(&interface.inputs[port]))
        .collect();
    let (values, patterns) = chosen.unwrap_or_else(|| values_to_try(&widths));
    measured(
        blocks,
        interface,
        (inputs, None),
        held,
        (values, patterns, any),
        cancel,
    )
}

/// The values each input takes, with whether they are a selection rather
/// than every one; `None` for every value, or the test patterns.
pub(super) type Chosen = Option<(Vec<Vec<Value>>, bool)>;

/// The changes a control group, such as an ALU's operation lines or a
/// barrel shifter's shift and amount, is timed through, beside the most
/// data inputs it is tried against every value of.
const STREAMED: u64 = 8192;

/// How many changes from one set of values straight to another at random
/// a build is timed by.
const WANDERED: u64 = 1000;

/// What a random change's number starts from, so a glitch from all off,
/// numbered by its combination, is the one kept as an example first.
const WANDERED_FROM: u64 = u64::MAX / 2;

/// The states each control line is toggled alone from, for its timing.
const SAMPLED: u64 = 512;

/// The most cases, data values by control settings, tried with every data
/// value; past it the data takes the test patterns.
const CONTROLLED_CASES: u64 = 4096;

/// [`measure`] for a build with control lines, such as an ALU's operation
/// or a barrel shifter's shift and amount, driven as a CPU drives them:
/// moved together to each of `settings`, every line's value in each, as its
/// decoder would, with its data `inputs`, each line on a tick of its own.
/// Latency is data in to data out, so t = 0 is when the data inputs the
/// outputs wait longest on change, and each control line, timed alone,
/// switches before 0 when it is slower than them. Besides each input alone
/// and all
/// together, data and control are streamed, each change from one case
/// straight to another, as instructions follow one another; every output
/// must change cleanly, the data on one tick, through each.
pub(super) fn measure_controlled(
    blocks: &Blocks,
    interface: &Interface,
    inputs: &[usize],
    settings: &[Vec<(usize, Value)>],
    held: &[(usize, Value)],
    cancel: &AtomicBool,
) -> Result<Timing> {
    let widths: Vec<usize> = inputs
        .iter()
        .map(|&port| input_bits(&interface.inputs[port]))
        .collect();
    let every: u64 = widths.iter().map(|&w| 1u64 << w.min(32)).product();
    let patterns =
        every.saturating_mul(settings.len() as u64) > CONTROLLED_CASES;
    let mut values: Vec<Vec<Value>> = widths
        .iter()
        .map(|&width| {
            if patterns {
                bench::patterns(width)
            } else {
                (0..1 << width).collect()
            }
        })
        .collect();
    values.push((0..settings.len() as Value).collect());
    measured(
        blocks,
        interface,
        (inputs, Some(settings)),
        held,
        (values, patterns, true),
        cancel,
    )
}

/// A control group's settings, each every line's value, switched together.
type Settings<'a> = &'a [Vec<(usize, Value)>];

/// [`measure`] and [`measure_controlled`]: `inputs` and the control group,
/// if any, taking `values`, the group's last, by setting, and at random
/// any value of their widths when `any`.
fn measured(
    blocks: &Blocks,
    interface: &Interface,
    (inputs, controls): (&[usize], Option<Settings>),
    held: &[(usize, Value)],
    (values, patterns, any): (Vec<Vec<Value>>, bool, bool),
    cancel: &AtomicBool,
) -> Result<Timing> {
    let outputs: Vec<usize> = interface
        .outputs
        .iter()
        .map(|port| output_bits(interface, port))
        .collect();
    let status: Vec<bool> = interface
        .outputs
        .iter()
        .map(crate::interface::Port::status)
        .collect();
    let carried = Circuit::new(blocks, interface)?.carried();
    let rig = Rig {
        blocks,
        interface,
        inputs,
        controls,
        held,
        values: &values,
        any,
        outputs: &outputs,
        status: &status,
        cancel,
    };

    // Each data input alone, and each control line, times from its own
    // change: the data inputs' and the lines' timings, each line's its own.
    let lines = rig.lines();
    let mut alone = Vec::new();
    for input in 0..inputs.len() {
        alone.push(rig.parallel(|circuit, part, parts| {
            rig.alone(circuit, input, None, part, parts)
        })?);
    }
    for line in 0..lines.len() {
        alone.push(rig.parallel(|circuit, part, parts| {
            rig.line_alone(circuit, line, part, parts)
        })?);
    }
    // A control line, such as a carry in or a decrement line, is timed
    // against the data, as the group's lines are.
    let data: Vec<bool> = (0..alone.len())
        .map(|i| {
            inputs
                .get(i)
                .is_some_and(|&port| !interface.inputs[port].control())
        })
        .collect();
    let ticks = delays(&alone, outputs.len(), &data);

    // Every input together, each at its timing, times from t = 0.
    let offsets: Vec<i32> = ticks.iter().map(|t| t.unwrap_or(0)).collect();
    let mut together = rig.parallel(|circuit, part, parts| {
        rig.together(circuit, &offsets, part, parts)
    })?;
    for (stats, offset) in alone[..inputs.len()].iter().zip(&offsets) {
        together.merge(stats, *offset);
    }
    // From one set of values straight to another, at random.
    together.merge(&rig.parallel(|c, p, n| rig.wander(c, &offsets, p, n))?, 0);
    // A line toggled alone may make a setting the group never takes, so
    // only its changes between settings, each line on its own tick, count.
    if controls.is_some() {
        let settings = rig.parallel(|circuit, part, parts| {
            rig.alone(circuit, inputs.len(), Some(&offsets), part, parts)
        })?;
        together.merge(&settings, 0);
        let streamed = rig.parallel(|circuit, part, parts| {
            rig.stream(circuit, &offsets, part, parts)
        })?;
        together.merge(&streamed, 0);
    }

    // The tick by which `percent` of the changes from all off settled.
    let percentile = |percent: u64| {
        let total: u64 = together.settled.values().sum();
        let mut seen = 0;
        for (&tick, &count) in &together.settled {
            seen += count;
            if seen * 100 >= total * percent {
                return tick;
            }
        }
        0
    };
    Ok(Timing {
        inputs: inputs
            .iter()
            .chain(&lines)
            .zip(&ticks)
            .map(|(&port, &tick)| InputTiming {
                name: interface.inputs[port].name.clone(),
                tick,
                earliest: None,
            })
            .collect(),
        outputs: output_timings(interface, &together, &carried),
        synced: together.synced(),
        median: percentile(50),
        p99: percentile(99),
        slowest: together.slowest.0,
        slowest_case: together.slowest.1.clone(),
        changes: together.changes,
        patterns,
        start: Start::Input,
        takes: Takes::Open,
        write: None,
        data: None,
        pulse: None,
        free_running: None,
    })
}

/// Each input's timing from what it did `alone`: the most ticks it may
/// change after t = 0 without any output it reaches settling later than
/// that output's slowest data input, changing at t = 0, settles it. Every
/// output waits longest on some data input, which changes at t = 0, so the
/// timing is the same whichever is named A; a control line, not `data`,
/// slower than them changes before t = 0; an input no output follows has
/// none. An output only control lines reach waits on them as on data.
fn delays(alone: &[Stats], outputs: usize, data: &[bool]) -> Vec<Option<i32>> {
    let latency =
        |stats: &Stats, output: usize| stats.outputs[output].latency();
    let slowest_of = |o: usize, only_data: bool| {
        alone
            .iter()
            .zip(data)
            .filter(|&(_, &is_data)| is_data || !only_data)
            .filter_map(|(stats, _)| latency(stats, o))
            .max()
    };
    let slowest: Vec<Option<i32>> = (0..outputs)
        .map(|o| slowest_of(o, true).or_else(|| slowest_of(o, false)))
        .collect();
    alone
        .iter()
        .map(|stats| {
            (0..outputs)
                .filter_map(|o| Some(slowest[o]? - latency(stats, o)?))
                .min()
        })
        .collect()
}

/// The most ticks before or after its operands a sequential build's
/// calculate is tried at.
const LATEST_PRESS: i32 = 64;

/// Per case, per output, per bit, the ticks it changed on.
type Runs = Vec<Vec<Vec<Vec<i32>>>>;

/// Times a build its `bench` drives by pressing a control line once its
/// inputs are set, as a sequential multiplier's calculate, as every
/// control line is timed: from the operands changing at tick 0, its
/// calculate pressed at the latest tick, before or after them, that adds
/// no delay to any result, which is its timing. That press is searched for
/// on the telling pairs of values, then checked on `every` case of its
/// test, pressing later until each comes out right, as a press too soon
/// for some operands gets them wrong. Each output's latency is the last
/// tick it changed by.
pub(super) fn pressed(
    blocks: &Blocks,
    interface: &Interface,
    bench: &bench::Bench,
    every: &[bench::Case],
    cancel: &AtomicBool,
) -> Result<Timing> {
    let (cases, _) = bench::paired(&bench.widths(interface));
    let expected = bench::observe(blocks, interface, bench, &cases, None)?;
    let on = |cases: &[bench::Case], expected: &[Value], offset: i32| {
        if cancel.load(Ordering::Relaxed) {
            bail!("The timing was cancelled.");
        }
        press_at(blocks, interface, bench, (cases, expected), offset)
    };
    let at = |offset: i32| on(&cases, &expected, offset);
    let settled = |runs: &Runs| {
        runs.iter()
            .flatten()
            .flatten()
            .filter_map(|t| t.last().copied())
            .max()
            .unwrap_or(0)
    };
    // Pressing sooner helps until the operands are what hold it up; from
    // there, the latest press that keeps the fewest ticks is its timing.
    let mut first = None;
    for offset in 0..=LATEST_PRESS {
        if let Some(found) = at(offset)? {
            first = Some((offset, found));
            break;
        }
    }
    let mut best = first.with_context(|| {
        format!(
            "No press of its calculate within {LATEST_PRESS} ticks of its \
             operands gives every product."
        )
    })?;
    if best.0 == 0 {
        for offset in (-LATEST_PRESS..0).rev() {
            match at(offset)? {
                Some(found) if settled(&found.0) < settled(&best.1.0) => {
                    best = (offset, found);
                }
                _ => break,
            }
        }
    }
    let every_expected = bench::observe(blocks, interface, bench, every, None)?;
    let mut found = None;
    for offset in best.0..=LATEST_PRESS {
        if let Some(runs) = on(every, &every_expected, offset)? {
            found = Some((offset, runs));
            break;
        }
    }
    let Some((offset, (runs, carried))) = found else {
        bail!(
            "No press of its calculate within {LATEST_PRESS} ticks of its operands gives every product."
        )
    };
    Ok(pressed_timing(
        interface, bench, every, &runs, offset, &carried,
    ))
}

/// What each case of `cases` showed, its operands set at tick 0 and its
/// calculate, the bench's press, pressed `offset` ticks from them; with
/// what each output's repeaters or comparators take to carry it out. `None`
/// when some case does not come out as `expected`.
fn press_at(
    blocks: &Blocks,
    interface: &Interface,
    bench: &bench::Bench,
    (cases, expected): (&[bench::Case], &[Value]),
    offset: i32,
) -> Result<Option<(Runs, Carried)>> {
    let Some(calculate) = bench.press else {
        bail!("Nothing is pressed.");
    };
    let widths = bench.output_widths(interface);
    let mut circuit = Circuit::new(blocks, interface)?;
    for &(port, value) in &bench.held {
        circuit.set(port, value);
    }
    circuit.settle()?;
    let carried = circuit.carried();
    let button = circuit.is_button(calculate, 0);
    let mut runs: Runs = Vec::with_capacity(cases.len());
    for (case, &want) in cases.iter().zip(expected) {
        let mut shown: Vec<Value> =
            bench.outputs.iter().map(|&o| circuit.get(o)).collect();
        let mut flips: Vec<Vec<Vec<i32>>> =
            widths.iter().map(|&w| vec![Vec::new(); w]).collect();
        let mut tick = offset.min(0);
        loop {
            if tick == offset {
                if button {
                    circuit.press(calculate, 0);
                } else {
                    circuit.set(calculate, 1);
                }
            } else if tick == offset + 1 && !button {
                circuit.set(calculate, 0);
            }
            if tick == 0 {
                for (i, &port) in bench.inputs.iter().enumerate() {
                    circuit.set(port, case[i]);
                }
            }
            circuit.tick();
            tick += 1;
            for (o, (&port, last)) in
                bench.outputs.iter().zip(&mut shown).enumerate()
            {
                let now = circuit.get(port);
                for (bit, ticks) in flips[o].iter_mut().enumerate() {
                    if (now ^ *last) >> bit & 1 == 1 {
                        ticks.push(tick - carried.bit(port, bit));
                    }
                }
                *last = now;
            }
            if tick > offset.max(0) + 1 && !circuit.pending() {
                break;
            }
            // Still changing, it never settles on a result: wrong.
            if tick > i32::try_from(MAX_TICKS)? {
                return Ok(None);
            }
        }
        let mut got = 0;
        let mut shift = 0;
        for (&port, &width) in bench.outputs.iter().zip(&widths) {
            got |= circuit.get(port) << shift;
            shift += width;
        }
        if got != want {
            return Ok(None);
        }
        runs.push(flips);
    }
    Ok(Some((runs, carried)))
}

/// The timing `pressed` found: `runs` holding, per case, per output, per
/// bit, the ticks it changed on, its calculate pressed `offset` ticks from
/// its operands, before them when negative.
fn pressed_timing(
    interface: &Interface,
    bench: &bench::Bench,
    cases: &[bench::Case],
    runs: &[Vec<Vec<Vec<i32>>>],
    offset: i32,
    carried: &Carried,
) -> Timing {
    let last = |ticks: &[i32]| ticks.last().copied().unwrap_or(0);
    let outputs: Vec<OutputTiming> = bench
        .outputs
        .iter()
        .enumerate()
        .map(|(o, &port)| {
            let name = interface.outputs[port].name.clone();
            pressed_output(name, runs, o, carried.port(port))
        })
        .collect();
    // Each case's settling, the last tick any output changed.
    let mut settled: Vec<(i32, usize)> = runs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (r.iter().flatten().map(|t| last(t)).max().unwrap_or(0), i)
        })
        .collect();
    settled.sort_unstable();
    let at = |percent: usize| {
        settled
            .get(
                (settled.len() * percent / 100)
                    .min(settled.len().saturating_sub(1)),
            )
            .map_or(0, |s| s.0)
    };
    let (slowest, slowest_at) = settled.last().copied().unwrap_or((0, 0));
    let mut inputs: Vec<InputTiming> = bench
        .inputs
        .iter()
        .map(|&port| InputTiming {
            name: interface.inputs[port].name.clone(),
            tick: Some(0),
            earliest: None,
        })
        .collect();
    if let Some(calculate) = bench.press {
        inputs.push(InputTiming {
            name: interface.inputs[calculate].name.clone(),
            tick: Some(offset),
            earliest: None,
        });
    }
    let mut slowest_case = cases
        .get(slowest_at)
        .map_or_else(Vec::new, |c| c[..bench.inputs.len()].to_vec());
    slowest_case.push(1);
    Timing {
        inputs,
        synced: outputs.iter().all(|o| o.glitches == 0 && o.staggered == 0)
            && !runs.iter().any(|r| apart(r)),
        outputs,
        median: at(50),
        p99: at(99),
        slowest,
        slowest_case,
        changes: cases.len() as u64,
        patterns: true,
        start: Start::Input,
        takes: Takes::default(),
        write: None,
        data: None,
        pulse: None,
        free_running: None,
    }
}

/// Whether the outputs one run of a press changed, each bit's change
/// ticks by output, settled on different ticks.
fn apart(run: &[Vec<Vec<i32>>]) -> bool {
    let settled: Vec<i32> = run
        .iter()
        .filter_map(|bits| bits.iter().filter_map(|t| t.last()).max())
        .copied()
        .collect();
    settled.windows(2).any(|w| w[0] != w[1])
}

/// Output `o`'s timing among `runs`, named `name`, carried out of the
/// build by `carried` ticks of its repeaters or comparators.
fn pressed_output(
    name: String,
    runs: &[Vec<Vec<Vec<i32>>>],
    o: usize,
    carried: i32,
) -> OutputTiming {
    let last = |ticks: &[i32]| ticks.last().copied().unwrap_or(0);

    let bits: Vec<BitTiming> = (0..runs.first().map_or(0, |r| r[o].len()))
        .map(|bit| BitTiming {
            first_change: runs
                .iter()
                .filter_map(|r| r[o][bit].first().copied())
                .min()
                .unwrap_or(0),
            latency: runs.iter().map(|r| last(&r[o][bit])).max().unwrap_or(0),
            glitches: runs.iter().filter(|r| r[o][bit].len() > 1).count()
                as u64,
        })
        .collect();
    let glitches = runs
        .iter()
        .filter(|r| r[o].iter().any(|t| t.len() > 1))
        .count() as u64;
    let staggered = runs
        .iter()
        .filter(|r| {
            let once: Vec<i32> =
                r[o].iter().filter(|t| t.len() == 1).map(|t| t[0]).collect();
            once.windows(2).any(|w| w[0] != w[1])
        })
        .count() as u64;
    OutputTiming {
        name,
        status: false,
        latency: bits.iter().map(|b| b.latency).max().unwrap_or(0),
        first_change: bits.iter().map(|b| b.first_change).min().unwrap_or(0),
        carried,
        glitches,
        staggered,
        glitch: None,
        bits,
    }
}

/// Each output's timing, from what every change together found, with the
/// ticks `carried` its repeaters or comparators take to carry it out.
fn output_timings(
    interface: &Interface,
    together: &Stats,
    carried: &Carried,
) -> Vec<OutputTiming> {
    interface
        .outputs
        .iter()
        .zip(&together.outputs)
        .enumerate()
        .map(|(o, (port, stats))| OutputTiming {
            name: port.name.clone(),
            status: port.status(),
            latency: stats.latency().unwrap_or(0),
            first_change: stats.first_change().unwrap_or(0),
            carried: carried.port(o),
            glitches: stats.glitches,
            staggered: stats.staggered,
            glitch: stats.glitch.as_ref().map(|(_, glitch)| glitch.clone()),
            bits: stats
                .bits
                .iter()
                .map(|bit| BitTiming {
                    first_change: bit.first.unwrap_or(0),
                    latency: bit.last.unwrap_or(0),
                    glitches: bit.glitches,
                })
                .collect(),
        })
        .collect()
}

/// The values each input of `widths` bits takes: every one, or the test
/// patterns when that would be too many, which the `bool` says.
fn values_to_try(widths: &[usize]) -> (Vec<Vec<Value>>, bool) {
    let patterns = widths.iter().sum::<usize>() > EXHAUSTIVE_BITS;
    let values = widths
        .iter()
        .map(|&width| {
            if patterns {
                bench::patterns(width)
            } else {
                (0..1 << width).collect()
            }
        })
        .collect();
    (values, patterns)
}

/// What a measurement needs, shared by its threads.
struct Rig<'a> {
    blocks: &'a Blocks,
    interface: &'a Interface,
    /// The data inputs, each a port.
    inputs: &'a [usize],
    /// The control group, if any, an input after the data inputs: every
    /// line's value in each of its settings, switched together.
    controls: Option<Settings<'a>>,
    /// Inputs set before anything is measured and left alone.
    held: &'a [(usize, Value)],
    /// The values each input takes, 0 among them.
    values: &'a [Vec<Value>],
    /// Each output port's width.
    outputs: &'a [usize],
    /// Whether each output port is a status line, such as carry out.
    status: &'a [bool],
    /// Whether a change drawn at random may take any value of each data
    /// input's width, rather than only those in `values`.
    any: bool,
    cancel: &'a AtomicBool,
}

impl Rig<'_> {
    /// Runs `work` on every core, each thread with its own circuit and its
    /// share of the work, and merges what they found.
    fn parallel(
        &self,
        work: impl Fn(&mut Circuit, u64, u64) -> Result<Stats> + Sync,
    ) -> Result<Stats> {
        let parts = thread::available_parallelism()
            .map_or(1, |n| u64::try_from(n.get()).unwrap_or(1));
        let found: Vec<Result<Stats>> = thread::scope(|scope| {
            let threads: Vec<_> = (0..parts)
                .map(|part| {
                    let work = &work;
                    scope.spawn(move || {
                        let mut circuit =
                            Circuit::new(self.blocks, self.interface)?;
                        for &(port, value) in self.held {
                            circuit.set(port, value);
                        }
                        work(&mut circuit, part, parts)
                    })
                })
                .collect();
            threads
                .into_iter()
                .map(|thread| thread.join().expect("a timing thread panicked"))
                .collect()
        });
        let mut merged = Stats::new(self.outputs, self.status);
        for stats in found {
            merged.merge(&stats?, 0);
        }
        Ok(merged)
    }

    /// How many inputs it drives: its data inputs, and its control group as
    /// one.
    fn count(&self) -> usize {
        self.inputs.len() + usize::from(self.controls.is_some())
    }

    /// The ports input `input` sets for `value`: its own, or every control
    /// line's for the setting `value` numbers, 0 its first.
    fn sets(&self, input: usize, value: Value) -> Vec<(usize, Value)> {
        match (self.inputs.get(input), self.controls) {
            (Some(&port), _) => vec![(port, value)],
            (None, Some(settings)) => {
                settings[usize::try_from(value).unwrap_or(0)].clone()
            }
            (None, None) => Vec::new(),
        }
    }

    /// Sets input `input` to `value` at once.
    fn set(&self, circuit: &mut Circuit, input: usize, value: Value) {
        for (port, value) in self.sets(input, value) {
            circuit.set(port, value);
        }
    }

    /// The changes setting input `input` to `value`, each port at its
    /// `offsets` tick, a data input's by its place and a control line's
    /// after them, by its place among [`Rig::lines`]; all at 0 without.
    fn changes(
        &self,
        input: usize,
        value: Value,
        offsets: Option<&[i32]>,
    ) -> Vec<(i32, usize, Value)> {
        let lines = self.lines();
        let tick = |port: usize| {
            let Some(offsets) = offsets else { return 0 };
            match lines.iter().position(|&line| line == port) {
                Some(line) if input >= self.inputs.len() => {
                    offsets[self.inputs.len() + line]
                }
                _ => offsets[input],
            }
        };
        self.sets(input, value)
            .into_iter()
            .map(|(port, value)| (tick(port), port, value))
            .collect()
    }

    /// The control group's lines, in the order its settings give them.
    fn lines(&self) -> Vec<usize> {
        let mut lines: Vec<usize> = Vec::new();
        let first = self.controls.and_then(|settings| settings.first());
        for &(port, _) in first.into_iter().flatten() {
            if !lines.contains(&port) {
                lines.push(port);
            }
        }
        lines
    }

    /// Input values as `Timing::inputs` gives them: the data inputs', then
    /// each control line's in the group's setting.
    fn shown(&self, values: &[Value]) -> Vec<Value> {
        let mut shown = values[..self.inputs.len()].to_vec();
        if let Some(&setting) = values.get(self.inputs.len()) {
            shown.extend(
                self.sets(self.inputs.len(), setting)
                    .into_iter()
                    .map(|(_, value)| value),
            );
        }
        shown
    }

    /// Input `input` alone from 0 to each of its values and back, with the
    /// others held at each combination of theirs: the `part`th of `parts`
    /// shares of those combinations.
    fn alone(
        &self,
        circuit: &mut Circuit,
        input: usize,
        offsets: Option<&[i32]>,
        part: u64,
        parts: u64,
    ) -> Result<Stats> {
        let mut stats = Stats::new(self.outputs, self.status);
        let others: Vec<usize> =
            (0..self.count()).filter(|&i| i != input).collect();
        let combinations: u64 = others
            .iter()
            .map(|&i| self.values[i].len() as u64)
            .product();
        for rest in (part..combinations).step_by(usize::try_from(parts)?) {
            self.check_cancel()?;
            for (&other, value) in others.iter().zip(self.pick(&others, rest)) {
                self.set(circuit, other, value);
            }
            self.set(circuit, input, 0);
            circuit.settle()?;
            for &value in self.values[input].iter().filter(|&&v| v != 0) {
                let on = self.changes(input, value, offsets);
                transition(circuit, &on, None, &mut stats)?;
                let off = self.changes(input, 0, offsets);
                transition(circuit, &off, None, &mut stats)?;
            }
        }
        Ok(stats)
    }

    /// Every input together, each at its `offsets` tick, from all off to
    /// each combination and back: the `part`th of `parts` shares of them.
    fn together(
        &self,
        circuit: &mut Circuit,
        offsets: &[i32],
        part: u64,
        parts: u64,
    ) -> Result<Stats> {
        let mut stats = Stats::new(self.outputs, self.status);
        for input in 0..self.count() {
            self.set(circuit, input, 0);
        }
        circuit.settle()?;
        let every: Vec<usize> = (0..self.count()).collect();
        let combinations: u64 =
            self.values.iter().map(|v| v.len() as u64).product();
        for case in (part..combinations).step_by(usize::try_from(parts)?) {
            if case % 256 == 0 {
                self.check_cancel()?;
            }
            let values = self.pick(&every, case);
            let changes = |off: bool| -> Vec<(i32, usize, Value)> {
                values
                    .iter()
                    .enumerate()
                    .filter(|&(_, &value)| value != 0)
                    .flat_map(|(i, &value)| {
                        let value = if off { 0 } else { value };
                        self.changes(i, value, Some(offsets))
                    })
                    .collect()
            };
            let shown = self.shown(&values);
            let up = transition(
                circuit,
                &changes(false),
                Some((case, &[], &shown)),
                &mut stats,
            )?;
            if let Some(tick) = up {
                *stats.settled.entry(tick).or_default() += 1;
                if tick > stats.slowest.0 || stats.slowest.1.is_empty() {
                    stats.slowest = (tick, shown);
                }
            }
            transition(circuit, &changes(true), None, &mut stats)?;
        }
        Ok(stats)
    }

    /// Control line `line` of [`Rig::lines`] toggled alone and back, from
    /// each of [`SAMPLED`] states drawn at random, the data at a value and
    /// the group at a setting: the `part`th of `parts` shares of them. Only
    /// its timing counts, as a line toggled alone may make a setting the
    /// group never takes.
    fn line_alone(
        &self,
        circuit: &mut Circuit,
        line: usize,
        part: u64,
        parts: u64,
    ) -> Result<Stats> {
        let mut stats = Stats::new(self.outputs, self.status);
        let toggled = self.lines()[line];
        let mut rng = Rng::default();
        for _ in 0..=part {
            rng.draw();
        }
        for sample in (part..SAMPLED).step_by(usize::try_from(parts)?) {
            if sample % 64 == 0 {
                self.check_cancel()?;
            }
            let mut held = 0;
            for (input, choices) in self.values.iter().enumerate() {
                let value = choices[rng.below(choices.len())];
                for (set, value) in self.sets(input, value) {
                    circuit.set(set, value);
                    if set == toggled {
                        held = value;
                    }
                }
            }
            circuit.settle()?;
            transition(circuit, &[(0, toggled, held ^ 1)], None, &mut stats)?;
            transition(circuit, &[(0, toggled, held)], None, &mut stats)?;
        }
        Ok(stats)
    }

    /// Data and control together, each at its `offsets` tick, from one case
    /// straight to another drawn at random, as instructions follow one
    /// another: the `part`th of `parts` shares of [`STREAMED`] changes.
    fn stream(
        &self,
        circuit: &mut Circuit,
        offsets: &[i32],
        part: u64,
        parts: u64,
    ) -> Result<Stats> {
        let mut stats = Stats::new(self.outputs, self.status);
        for input in 0..self.count() {
            self.set(circuit, input, 0);
        }
        circuit.settle()?;
        let mut rng = Rng::default();
        for _ in 0..=part {
            rng.draw();
        }
        let mut now = vec![0; self.count()];
        for change in (part..STREAMED).step_by(usize::try_from(parts)?) {
            if change % 256 == 0 {
                self.check_cancel()?;
            }
            let next: Vec<Value> = self
                .values
                .iter()
                .map(|choices| choices[rng.below(choices.len())])
                .collect();
            let changes: Vec<(i32, usize, Value)> = (0..self.count())
                .filter(|&i| next[i] != now[i])
                .flat_map(|i| self.changes(i, next[i], Some(offsets)))
                .collect();
            transition(circuit, &changes, None, &mut stats)?;
            now = next;
        }
        Ok(stats)
    }

    /// Every input together, each at its `offsets` tick, from one set of
    /// values straight to another, each data input's drawn at random over
    /// every value of its width and the control group's among its settings,
    /// the same each time however many shares there are: the `part`th of
    /// `parts` shares of [`WANDERED`] changes. Each is numbered after every
    /// change from all off, so a glitch kept as an example is one from all
    /// off when there is one.
    fn wander(
        &self,
        circuit: &mut Circuit,
        offsets: &[i32],
        part: u64,
        parts: u64,
    ) -> Result<Stats> {
        let mut stats = Stats::new(self.outputs, self.status);
        let widths: Vec<usize> = self
            .inputs
            .iter()
            .map(|&port| input_bits(&self.interface.inputs[port]))
            .collect();
        // Every state drawn first, the same whatever the share: change `k`
        // goes from state `k` to state `k + 1`.
        let mut rng = Rng::default();
        let walk: Vec<Vec<Value>> = (0..=WANDERED)
            .map(|_| {
                (0..self.count())
                    .map(|input| {
                        if let Some(&width) =
                            widths.get(input).filter(|_| self.any)
                        {
                            let value = Value::from(rng.draw()) << 64
                                | Value::from(rng.draw());
                            value & bench::mask(width)
                        } else {
                            let settings = &self.values[input];
                            settings[rng.below(settings.len())]
                        }
                    })
                    .collect()
            })
            .collect();
        for change in (part..WANDERED).step_by(usize::try_from(parts)?) {
            if change % 64 == 0 {
                self.check_cancel()?;
            }
            let k = usize::try_from(change)?;
            let (now, next) = (&walk[k], &walk[k + 1]);
            for (input, &value) in now.iter().enumerate() {
                self.set(circuit, input, value);
            }
            circuit.settle()?;
            let changes: Vec<(i32, usize, Value)> = (0..self.count())
                .filter(|&i| next[i] != now[i])
                .flat_map(|i| self.changes(i, next[i], Some(offsets)))
                .collect();
            let (from, to) = (self.shown(now), self.shown(next));
            transition(
                circuit,
                &changes,
                Some((WANDERED_FROM + change, &from, &to)),
                &mut stats,
            )?;
        }
        Ok(stats)
    }

    /// The values of `inputs` in their `index`th combination, the first
    /// changing fastest.
    fn pick(&self, inputs: &[usize], mut index: u64) -> Vec<Value> {
        inputs
            .iter()
            .map(|&input| {
                let choices = &self.values[input];
                let count = choices.len() as u64;
                let value =
                    choices[usize::try_from(index % count).unwrap_or(0)];
                index /= count;
                value
            })
            .collect()
    }

    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            bail!("The timing was cancelled.");
        }
        Ok(())
    }
}

/// Makes each change at its tick, given as (tick, input port, value), as
/// the input's own blocks take it, so set as many ticks sooner as its
/// drivers take, and runs until nothing is left to update, recording how
/// each output bit changed, less the ticks the components standing on its
/// blocks take to carry it out. A change from all off gives its `case`,
/// its number and input values, to keep as an example of a glitch. Returns
/// the last tick any output changed on, if one did.
fn transition(
    circuit: &mut Circuit,
    changes: &[(i32, usize, Value)],
    case: Option<(u64, &[Value], &[Value])>,
    stats: &mut Stats,
) -> Result<Option<i32>> {
    let lead = circuit.lead();
    let mut changes: Vec<(i32, usize, Value)> = changes
        .iter()
        .map(|&(tick, port, value)| (tick - lead[port], port, value))
        .collect();
    changes.sort_unstable_by_key(|&(tick, ..)| tick);
    let start = changes.first().map_or(0, |&(tick, ..)| tick);
    let mut shown: Vec<Value> =
        (0..stats.outputs.len()).map(|o| circuit.get(o)).collect();
    let before = shown.clone();
    let carried = circuit.carried();
    // Per output, each tick it changed on and its value from then.
    let mut steps: Vec<Vec<(i32, Value)>> = vec![Vec::new(); shown.len()];
    // Per output, per bit: how many times it changed, first, and last.
    let mut seen: Vec<Vec<(u32, i32, i32)>> = stats
        .outputs
        .iter()
        .map(|o| vec![(0, 0, 0); o.bits.len()])
        .collect();
    let mut next = 0;
    let mut tick = start;
    loop {
        let mut set = false;
        while let Some(&(at, port, value)) = changes.get(next)
            && at == tick
        {
            circuit.set(port, value);
            next += 1;
            set = true;
        }
        if set {
            circuit.show();
        }
        for (output, bits) in seen.iter_mut().enumerate() {
            let now = circuit.get(output);
            let flipped = now ^ shown[output];
            if flipped != 0 {
                steps[output].push((tick, now));
            }
            for (bit, (count, first, last)) in bits.iter_mut().enumerate() {
                if flipped >> bit & 1 == 1 {
                    let tick = tick - carried.bit(output, bit);
                    if *count == 0 {
                        *first = tick;
                    }
                    *count += 1;
                    *last = tick;
                }
            }
            shown[output] = now;
        }
        if next == changes.len() && !circuit.pending() {
            break;
        }
        if (tick - start).unsigned_abs() > MAX_TICKS {
            bail!(
                "It was still changing {MAX_TICKS} ticks after its inputs changed."
            );
        }
        circuit.tick();
        tick += 1;
    }
    stats.changes += 1;
    let mut settled = None;
    // The tick each data output moved settled by, which must agree.
    let mut data_settled = Vec::new();
    for (o, (output, bits)) in stats.outputs.iter_mut().zip(&seen).enumerate() {
        let moved: Vec<&(u32, i32, i32)> =
            bits.iter().filter(|b| b.0 > 0).collect();
        if moved.is_empty() {
            continue;
        }
        for (bit, &(count, first, last)) in output.bits.iter_mut().zip(bits) {
            if count > 0 {
                bit.record(first, last, count > 1);
            }
        }
        let last = moved.iter().map(|b| b.2).max().unwrap_or(0);
        if moved.iter().any(|b| b.0 > 1) {
            output.glitches += 1;
            if let Some((number, from, values)) = case
                && output.glitch.as_ref().is_none_or(|(n, _)| number < *n)
            {
                let glitch = Glitch {
                    from: from.to_vec(),
                    case: values.to_vec(),
                    before: before[o],
                    steps: carried.arrived_steps(o, before[o], &steps[o]),
                };
                output.glitch = Some((number, glitch));
            }
        } else if moved.iter().any(|b| b.2 != last) {
            output.staggered += 1;
        }
        if !stats.status[o] {
            data_settled.push(last);
        }
        settled = settled.max(Some(last));
    }
    if data_settled.windows(2).any(|w| w[0] != w[1]) {
        stats.apart += 1;
    }
    Ok(settled)
}

/// What a set of changes did to the outputs.
struct Stats {
    outputs: Vec<OutputStats>,
    /// Whether each output is a status line, such as carry out, which may
    /// settle on a tick of its own.
    status: Vec<bool>,
    /// How many changes its data outputs settled on different ticks in.
    apart: u64,
    /// How many changes settled the whole result by each tick.
    settled: BTreeMap<i32, u64>,
    /// The slowest change's tick and input values.
    slowest: (i32, Vec<Value>),
    changes: u64,
}

struct OutputStats {
    bits: Vec<BitStats>,
    glitches: u64,
    staggered: u64,
    /// The first glitch from all off, by its case's number.
    glitch: Option<(u64, Glitch)>,
}

#[derive(Clone, Copy, Default)]
struct BitStats {
    first: Option<i32>,
    last: Option<i32>,
    glitches: u64,
}

impl BitStats {
    fn record(&mut self, first: i32, last: i32, glitched: bool) {
        self.first = Some(self.first.map_or(first, |f| f.min(first)));
        self.last = Some(self.last.map_or(last, |l| l.max(last)));
        self.glitches += u64::from(glitched);
    }
}

impl OutputStats {
    fn latency(&self) -> Option<i32> {
        self.bits.iter().filter_map(|bit| bit.last).max()
    }

    fn first_change(&self) -> Option<i32> {
        self.bits.iter().filter_map(|bit| bit.first).min()
    }
}

impl Stats {
    fn new(widths: &[usize], status: &[bool]) -> Self {
        Stats {
            status: status.to_vec(),
            apart: 0,
            outputs: widths
                .iter()
                .map(|&width| OutputStats {
                    bits: vec![BitStats::default(); width],
                    glitches: 0,
                    staggered: 0,
                    glitch: None,
                })
                .collect(),
            settled: BTreeMap::new(),
            slowest: (0, Vec::new()),
            changes: 0,
        }
    }

    /// Whether every output changed cleanly, each bit at most once, and
    /// every data output's bits, across them all, settled on one tick; a
    /// status line, such as carry out, may settle on a tick of its own.
    fn synced(&self) -> bool {
        self.apart == 0
            && self.outputs.iter().zip(&self.status).all(|(o, &status)| {
                o.glitches == 0 && (status || o.staggered == 0)
            })
    }

    /// Adds `other`'s findings, its ticks moved later by `offset`.
    fn merge(&mut self, other: &Stats, offset: i32) {
        self.apart += other.apart;
        for (mine, theirs) in self.outputs.iter_mut().zip(&other.outputs) {
            for (bit, their) in mine.bits.iter_mut().zip(&theirs.bits) {
                if let (Some(first), Some(last)) = (their.first, their.last) {
                    bit.record(first + offset, last + offset, false);
                }
                bit.glitches += their.glitches;
            }
            mine.glitches += theirs.glitches;
            mine.staggered += theirs.staggered;
            if let Some((number, glitch)) = &theirs.glitch
                && mine.glitch.as_ref().is_none_or(|(n, _)| number < n)
            {
                let mut glitch = glitch.clone();
                for step in &mut glitch.steps {
                    step.0 += offset;
                }
                mine.glitch = Some((*number, glitch));
            }
        }
        for (&tick, &count) in &other.settled {
            *self.settled.entry(tick + offset).or_default() += count;
        }
        if !other.slowest.1.is_empty()
            && (other.slowest.0 + offset > self.slowest.0
                || self.slowest.1.is_empty())
        {
            self.slowest = (other.slowest.0 + offset, other.slowest.1.clone());
        }
        self.changes += other.changes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What changing one input alone did: its one output's one bit settled
    /// `latency` ticks after it.
    fn alone(latency: i32) -> Stats {
        let mut stats = Stats::new(&[1], &[false]);
        stats.outputs[0].bits[0].record(latency, latency, false);
        stats
    }

    /// Latency is data in to data out: the slowest data input changes at
    /// t = 0, a faster one later, and a control line slower than the data,
    /// as a barrel shifter's amount, before it.
    #[test]
    fn a_slow_control_line_switches_before_the_data() {
        let inputs = [alone(5), alone(4), alone(8), alone(3)];
        let data = [true, true, false, false];
        assert_eq!(
            delays(&inputs, 1, &data),
            [Some(0), Some(1), Some(-3), Some(2)]
        );
        // An output only control lines reach waits on them as on data.
        assert_eq!(delays(&[alone(8)], 1, &[false]), [Some(0)]);
    }

    /// A build is synced when every output changes cleanly and its data
    /// outputs settle on one tick, all together; a status line, such as
    /// carry out, may settle on a tick of its own, but must not glitch.
    #[test]
    fn a_status_line_may_settle_on_a_tick_of_its_own() {
        let clean = || Stats::new(&[4, 1], &[false, true]);
        assert!(clean().synced());
        let mut stats = clean();
        stats.outputs[1].staggered = 1;
        assert!(stats.synced(), "a status line's own tick");
        let mut stats = clean();
        stats.outputs[1].glitches = 1;
        assert!(!stats.synced(), "a status line glitching");
        let mut stats = clean();
        stats.outputs[0].staggered = 1;
        assert!(!stats.synced(), "a data output's bits apart");
        let mut stats = clean();
        stats.apart = 1;
        assert!(!stats.synced(), "two data outputs apart");
    }
}
