//! Testing a monostable: input A turning on, or for a falling-edge one
//! turning off, gives a pulse of a set number of ticks on its output, and
//! the other edge gives nothing; a dual-edge monostable pulses on both.
//! Its output rests the same with A on as off: off, or on for an inverted
//! one, whose pulse turns it off. Each bit of A drives its own output bit, as copies
//! stacked side by side add bits: a bit of one output, or a pink status
//! line of its own, the one nearest it.
//!
//! From rest, each bit turns on alone and then off, and then every bit
//! together, while every output bit is watched tick by tick until nothing
//! changes. Its edge is the one most bits pulse on, unless it is tested as
//! dual-edge. A bit changing on its edge must leave its rest once and come
//! back, all for the same number of ticks, its pulse; every other bit must
//! stay put. The pulse is the length most bits give, so one bit that
//! differs counts as wrong rather than setting it.
//!
//! Its timing comes from the same changes: its latency is the tick its
//! pulse starts, from A changing on its edge, and it is synced when every
//! bit changed together starts on the same tick.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, bail};

use super::{
    Broken, Run, Signal, Test, Wrong,
    bench::mask,
    circuit::{Circuit, MAX_TICKS},
    stacking::{self, Stacking},
    timing::{
        BitTiming, Edge, InputTiming, OutputTiming, Pulse, Start, Takes, Timing,
    },
};
use crate::{
    interface::{self, Interface},
    lint,
    models::schematic::Orientation,
    schem::Blocks,
    stack,
};

/// The most wrong cases a run keeps to show.
const EXAMPLES: usize = 10;

/// One change of input A and what every output bit did after it.
struct Case {
    /// A after the change.
    a: u64,
    /// The bits of A that changed.
    flipped: u64,
    /// Whether they turned on rather than off.
    on: bool,
    /// Each output bit's change ticks, from A's.
    changes: Vec<Vec<i32>>,
}

impl Case {
    /// Bit `bit`'s pulse, its start and length in ticks, when it left its
    /// rest once and came back.
    fn pulse(&self, bit: usize) -> Option<(i32, i32)> {
        match self.changes[bit][..] {
            [start, end] => Some((start, end - start)),
            _ => None,
        }
    }
}

/// Every case a monostable was put through, with its interface's facts.
struct Cases {
    /// Input A's name.
    input: String,
    /// Its output's name, or `output` for status lines of their own.
    output: String,
    /// Whether its output is pink status lines rather than yellow.
    status: bool,
    width: usize,
    /// Whether its output rests on, so its pulse turns it off.
    inverted: bool,
    /// The ticks a repeater or comparator on its output takes to carry it
    /// out of the build.
    carried: i32,
    tried: Vec<Case>,
    /// Whether its output rests differently with A on than off, as a build
    /// that passes A along rather than pulsing does.
    follows: bool,
}

impl Cases {
    /// The edge it pulses on: both for a `dual` one, else the one more
    /// bits pulse on, rising for a tie.
    fn edge(&self, dual: bool) -> Edge {
        if dual {
            return Edge::Both;
        }
        let pulses = |on: bool| {
            self.tried
                .iter()
                .filter(|case| case.on == on)
                .flat_map(|case| {
                    bits(case.flipped, self.width)
                        .filter(|&bit| case.pulse(bit).is_some())
                })
                .count()
        };
        if pulses(false) > pulses(true) {
            Edge::Falling
        } else {
            Edge::Rising
        }
    }

    /// The pulse length most bits changed on `edge` give, the shortest of
    /// a tie.
    fn pulse(&self, edge: Edge) -> Option<i32> {
        let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
        for case in self.tried.iter().filter(|case| edge.triggers(case.on)) {
            for bit in bits(case.flipped, self.width) {
                if let Some((_, ticks)) = case.pulse(bit) {
                    *counts.entry(ticks).or_default() += 1;
                }
            }
        }
        let most = counts.values().copied().max()?;
        counts
            .into_iter()
            .find(|&(_, count)| count == most)
            .map(|(ticks, _)| ticks)
    }
}

/// The bits set in `value`, of `width`.
fn bits(value: u64, width: usize) -> impl Iterator<Item = usize> {
    (0..width).filter(move |&bit| value >> bit & 1 == 1)
}

/// Runs the monostable in `blocks` through its every case, as one that
/// pulses on both edges if `dual`.
pub fn check(blocks: &Blocks, dual: bool) -> Result<Run> {
    Ok(run(&cases(blocks)?, dual))
}

/// The run its cases make, each right when every bit changed on its edge
/// gave one pulse of the length most give and every other bit stayed put.
fn run(found: &Cases, dual: bool) -> Run {
    let edge = found.edge(dual);
    let pulse = found.pulse(edge);
    let mut wrong_bits: BTreeMap<String, u64> = BTreeMap::new();
    let mut examples = Vec::new();
    let mut wrong = 0;
    for case in &found.tried {
        let mut faults = Vec::new();
        for bit in 0..found.width {
            let fault = fault(case, bit, edge, pulse);
            if let Some(fault) = fault {
                faults.push(bit);
                let name = if found.width == 1 {
                    format!("{}, {fault}", found.output)
                } else {
                    format!("{} bit {bit}, {fault}", found.output)
                };
                *wrong_bits.entry(name).or_default() += 1;
            }
        }
        if faults.is_empty() {
            continue;
        }
        wrong += 1;
        if examples.len() < EXAMPLES {
            let changed = (0..found.width)
                .filter(|&bit| !case.changes[bit].is_empty())
                .fold(0, |value, bit| value | 1 << bit);
            examples.push(Wrong {
                op: Some(
                    if case.on {
                        "A turning on"
                    } else {
                        "A turning off"
                    }
                    .to_owned(),
                ),
                inputs: vec![case.a],
                expected: vec![if edge.triggers(case.on) {
                    case.flipped
                } else {
                    0
                }],
                got: vec![changed],
            });
        }
    }
    let shape = match pulse {
        Some(ticks) => format!(
            "a {ticks}-tick {} pulse",
            if found.inverted { "low" } else { "high" }
        ),
        None => "a pulse".to_owned(),
    };
    let pattern = match edge {
        Edge::Rising => format!("A on gives {shape}, A off none"),
        Edge::Falling => format!("A off gives {shape}, A on none"),
        Edge::Both => format!("A on or off gives {shape}"),
    };
    let hint = (wrong > 0 && found.follows).then(|| {
        "Its output rests differently with A on than off, so it follows A \
         rather than giving a pulse."
            .to_owned()
    });
    Run {
        pattern,
        cases: found.tried.len() as u64,
        exhaustive: true,
        patterns: 0,
        by_digit: false,
        wrong,
        inputs: vec![Signal {
            name: found.input.clone(),
            bits: found.width,
        }],
        outputs: vec![Signal {
            name: format!("{} pulsed", found.output),
            bits: found.width,
        }],
        wrong_bits: wrong_bits.into_iter().collect(),
        examples,
        hint,
        cells: Vec::new(),
        faults: Vec::new(),
        timing: None,
        stacking: None,
    }
}

/// What is wrong with output bit `bit` in `case`, if anything, for a
/// monostable pulsing on `edge` for `pulse` ticks.
fn fault(
    case: &Case,
    bit: usize,
    edge: Edge,
    pulse: Option<i32>,
) -> Option<String> {
    let changes = &case.changes[bit];
    let flipped = case.flipped >> bit & 1 == 1;
    if !flipped || !edge.triggers(case.on) {
        (!changes.is_empty()).then(|| match (flipped, case.on) {
            (true, true) => "pulsing as A turns on".to_owned(),
            (true, false) => "pulsing as A turns off".to_owned(),
            (false, _) => "changing for another bit of A".to_owned(),
        })
    } else {
        match (case.pulse(bit), pulse) {
            (Some((_, ticks)), Some(pulse)) if ticks == pulse => None,
            (Some((_, ticks)), _) => Some(format!(
                "pulsing {ticks} {}",
                if ticks == 1 { "tick" } else { "ticks" }
            )),
            _ if changes.is_empty() => Some("not pulsing".to_owned()),
            _ if changes.len() % 2 == 1 => {
                Some("not coming back to rest".to_owned())
            }
            _ => Some("pulsing more than once".to_owned()),
        }
    }
}

/// Puts the monostable in `blocks` through its cases: each bit of A on
/// alone, then off, and every bit together, each from rest.
fn cases(blocks: &Blocks) -> Result<Cases> {
    let mut interface = interface::read(blocks);
    interface::name_lines(Test::Monostable.name(), blocks, &mut interface);
    let [input] = interface.inputs.as_slice() else {
        broken!(
            "A monostable takes one input, A, but this has {}.",
            interface.inputs.len()
        );
    };
    if input.block.starts_with(lint::CONTROL) {
        broken!("A monostable's input is A, not a magenta control line.");
    }
    if input.signal_strength {
        bail!(
            "Input {} takes a signal strength, which the monostable test \
             does not drive.",
            input.name
        );
    }
    let width = input.bits.len();
    let outputs = output_bits(&interface)?;
    let mut circuit = Circuit::new(blocks, &interface)?;
    let carried_by = circuit.carried();
    let carried = outputs
        .iter()
        .map(|&(port, _)| carried_by[port])
        .min()
        .unwrap_or(0);
    circuit.set(0, 0);
    circuit.settle()?;
    let rest = read(&circuit, &outputs);
    let all = mask(width);
    let inverted = match rest {
        0 => false,
        r if r == all => true,
        _ => broken!(
            "Its output bits rest differently with A off, some on and some \
             off, where a monostable's all rest alike."
        ),
    };
    let changes = |circuit: &mut Circuit, flipped: u64, on: bool| {
        let a = if on { flipped } else { 0 };
        circuit.set(0, a);
        watch(circuit, &outputs).map(|changes| Case {
            a,
            flipped,
            on,
            changes,
        })
    };
    let mut cases = Vec::new();
    let mut follows = false;
    // Each bit alone, then every bit together.
    let mut turned_on: Vec<u64> = (0..width).map(|bit| 1 << bit).collect();
    if width > 1 {
        turned_on.push(all);
    }
    for on in turned_on {
        cases.push(changes(&mut circuit, on, true)?);
        follows |= read(&circuit, &outputs) != rest;
        cases.push(changes(&mut circuit, on, false)?);
    }
    let (output, status) = match interface.outputs.as_slice() {
        [port] => (port.name.clone(), port.block.starts_with(lint::STATUS)),
        _ => ("output".to_owned(), true),
    };
    Ok(Cases {
        input: input.name.clone(),
        output,
        status,
        width,
        inverted,
        carried,
        tried: cases,
        follows,
    })
}

/// Each bit of A's output bit, by its port and its place there: the bits
/// of one output as wide as A, or each a status line of its own, the one
/// nearest the bit, as copies stacked side by side give.
fn output_bits(interface: &Interface) -> Result<Vec<(usize, usize)>> {
    let a = &interface.inputs[0].bits;
    let outputs = &interface.outputs;
    if let [output] = outputs.as_slice()
        && output.bits.len() == a.len()
    {
        return Ok((0..a.len()).map(|bit| (0, bit)).collect());
    }
    if outputs.len() != a.len() || outputs.iter().any(|o| o.bits.len() != 1) {
        let bits: usize = outputs.iter().map(|o| o.bits.len()).sum();
        broken!(
            "A has {} bits, but its outputs have {bits}; each bit of A \
             drives its own.",
            a.len()
        );
    }
    let distance = |from: [i32; 3], to: [i32; 3]| -> i32 {
        from.iter().zip(&to).map(|(f, t)| (f - t).pow(2)).sum()
    };
    let nearest: Vec<usize> = a
        .iter()
        .map(|bit| {
            (0..outputs.len())
                .min_by_key(|&port| {
                    distance(bit.pos, outputs[port].bits[0].pos)
                })
                .unwrap_or(0)
        })
        .collect();
    let mut taken = nearest.clone();
    taken.sort_unstable();
    taken.dedup();
    if taken.len() != nearest.len() {
        broken!(
            "Its status lines do not each sit nearest a bit of A, to say \
             which bit each pulses for."
        );
    }
    Ok(nearest.into_iter().map(|port| (port, 0)).collect())
}

/// The value `outputs` show, a bit for each bit of A.
fn read(circuit: &Circuit, outputs: &[(usize, usize)]) -> u64 {
    outputs
        .iter()
        .enumerate()
        .fold(0, |value, (i, &(port, bit))| {
            value | (circuit.get(port) >> bit & 1) << i
        })
}

/// Runs the circuit until nothing changes, each of `outputs`' change
/// ticks, from the input's change.
fn watch(
    circuit: &mut Circuit,
    outputs: &[(usize, usize)],
) -> Result<Vec<Vec<i32>>> {
    let width = outputs.len();
    let mut before = read(circuit, outputs);
    let mut changes = vec![Vec::new(); width];
    let mut tick = 0;
    while circuit.step() {
        tick += 1;
        if tick > MAX_TICKS.cast_signed() {
            bail!(
                "It was still changing {MAX_TICKS} ticks after its input \
                 changed."
            );
        }
        let now = read(circuit, outputs);
        for bit in bits(now ^ before, width) {
            changes[bit].push(tick);
        }
        before = now;
    }
    Ok(changes)
}

/// When the monostable in `blocks` pulses, from A changing on its edge,
/// and how long, as one that pulses on both edges if `dual`. Meant for one
/// whose cases all came out right.
pub fn time(
    blocks: &Blocks,
    dual: bool,
    cancel: &AtomicBool,
) -> Result<Timing> {
    if cancel.load(Ordering::Relaxed) {
        bail!("The timing was cancelled.");
    }
    let found = cases(blocks)?;
    let edge = found.edge(dual);
    let Some(ticks) = found.pulse(edge) else {
        bail!("It gives no pulse to time.");
    };
    let triggered: Vec<&Case> = found
        .tried
        .iter()
        .filter(|case| edge.triggers(case.on))
        .collect();
    let carried = found.carried;
    let starts = |case: &Case| -> Vec<i32> {
        bits(case.flipped, found.width)
            .filter_map(|bit| case.pulse(bit))
            .map(|(start, _)| start - carried)
            .collect()
    };
    let staggered = triggered
        .iter()
        .filter(|case| {
            let starts = starts(case);
            starts.iter().any(|&start| start != starts[0])
        })
        .count() as u64;
    let (slowest, slowest_case) = triggered
        .iter()
        .flat_map(|case| starts(case).into_iter().map(|start| (start, case.a)))
        .max_by_key(|&(start, _)| start)
        .unwrap_or_default();
    let mut ticks_seen: Vec<i32> =
        triggered.iter().flat_map(|case| starts(case)).collect();
    ticks_seen.sort_unstable();
    let at = |share: usize| {
        ticks_seen[(ticks_seen.len() * share / 100).min(ticks_seen.len() - 1)]
    };
    let bit_timing = |bit: usize| {
        let pulses: Vec<(i32, i32)> = triggered
            .iter()
            .filter(|case| case.flipped >> bit & 1 == 1)
            .filter_map(|case| case.pulse(bit))
            .collect();
        BitTiming {
            first_change: pulses
                .iter()
                .map(|&(start, _)| start - carried)
                .max()
                .unwrap_or(0),
            latency: pulses
                .iter()
                .map(|&(start, ticks)| start + ticks - carried)
                .max()
                .unwrap_or(0),
            glitches: 0,
        }
    };
    let bits: Vec<BitTiming> = (0..found.width).map(bit_timing).collect();
    Ok(Timing {
        inputs: vec![InputTiming {
            name: found.input.clone(),
            tick: Some(0),
            earliest: None,
        }],
        outputs: vec![OutputTiming {
            name: found.output.clone(),
            status: found.status,
            latency: bits.iter().map(|bit| bit.latency).max().unwrap_or(0),
            first_change: slowest,
            carried,
            glitches: 0,
            staggered,
            glitch: None,
            bits,
        }],
        synced: staggered == 0,
        median: at(50),
        p99: at(99),
        slowest,
        slowest_case: vec![slowest_case],
        changes: triggered.len() as u64,
        patterns: false,
        start: Start::Input,
        write: None,
        data: None,
        takes: Takes::Open,
        pulse: Some(Pulse {
            ticks,
            inverted: found.inverted,
            edge,
        }),
    })
}

/// How many times a monostable is triggered to find its throughput, and
/// to check it.
const TRIGGERS: usize = 32;
const LONG_TRIGGERS: usize = 256;

/// How often the monostable in `blocks`, timed as `timing`, can be
/// triggered: A turns on and off over and over, a trigger every period, on
/// for as long of it as works best, every bit together. Each trigger must
/// give each bit exactly one pulse of its length, starting no later than
/// its timing found, and the other edge nothing; a dual-edge one, whose
/// every edge triggers, is triggered every period on each edge in turn.
/// The shortest period that holds for a few triggers and then for many;
/// `None` when none up to twice its latency does.
pub fn throughput(blocks: &Blocks, timing: &Timing) -> Result<Option<i32>> {
    let Some(pulse) = timing.pulse else {
        return Ok(None);
    };
    let Some(output) = timing.outputs.first() else {
        return Ok(None);
    };
    let mut interface = interface::read(blocks);
    interface::name_lines(Test::Monostable.name(), blocks, &mut interface);
    let outputs = output_bits(&interface)?;
    let width = interface.inputs[0].bits.len();
    let all = mask(width);
    // Each bit's latest start, from A changing, where the site reads it.
    let latest: Vec<i32> = output
        .bits
        .iter()
        .map(|bit| bit.first_change + output.carried)
        .collect();
    let rest = if pulse.inverted { all } else { 0 };
    let dual = pulse.edge == Edge::Both;
    let most = 2 * output.latency.max(1) + 2;
    for period in 1..=most {
        // A dual-edge one is triggered by each edge, a period apart; another
        // by one edge a period, on for some of it.
        let ons: Vec<i32> = if dual {
            vec![period]
        } else {
            (1..period).collect()
        };
        for on in ons {
            let right = |triggers: usize| -> Result<bool> {
                let stream = Retriggered {
                    period,
                    on,
                    dual,
                    edge: pulse.edge,
                    ticks: pulse.ticks,
                    latest: &latest,
                    rest,
                };
                stream.right(blocks, &interface, &outputs, triggers)
            };
            if right(TRIGGERS)? && right(LONG_TRIGGERS)? {
                return Ok(Some(period));
            }
        }
    }
    Ok(None)
}

/// A monostable triggered over and over: a trigger every `period`, A on
/// for `on` ticks of each cycle.
struct Retriggered<'a> {
    period: i32,
    on: i32,
    dual: bool,
    edge: Edge,
    ticks: i32,
    latest: &'a [i32],
    rest: u64,
}

impl Retriggered<'_> {
    /// Whether `triggers` triggers each give each bit one pulse of its
    /// length, no later than its latest start, and nothing else.
    fn right(
        &self,
        blocks: &Blocks,
        interface: &Interface,
        outputs: &[(usize, usize)],
        triggers: usize,
    ) -> Result<bool> {
        let mut circuit = Circuit::new(blocks, interface)?;
        let all = mask(outputs.len());
        circuit.set(0, 0);
        circuit.settle()?;
        // Each tick A changes, and whether it turns on.
        let cycle = if self.dual {
            2 * self.period
        } else {
            self.period
        };
        let cycles = if self.dual {
            triggers.div_ceil(2)
        } else {
            triggers
        };
        let mut changes: Vec<(i32, bool)> = Vec::new();
        for i in 0..i32::try_from(cycles)? {
            let start = i * cycle;
            changes.push((start, true));
            changes.push((
                start + if self.dual { self.period } else { self.on },
                false,
            ));
        }
        let edges: Vec<i32> = changes
            .iter()
            .filter(|&&(_, on)| self.edge.triggers(on))
            .map(|&(at, _)| at)
            .collect();
        let latest = self.latest.iter().copied().max().unwrap_or(0);
        let end =
            changes.last().map_or(0, |&(at, _)| at) + latest + self.ticks + 2;
        let mut shown = Vec::new();
        let mut next = 0;
        for tick in 0..=end {
            while let Some(&(at, on)) = changes.get(next)
                && at == tick
            {
                circuit.set(0, if on { all } else { 0 });
                next += 1;
            }
            shown.push(read(&circuit, outputs) ^ self.rest);
            circuit.tick();
        }
        // Each bit's pulses, as when it leaves its rest and how long for.
        for (bit, &latest) in self.latest.iter().enumerate() {
            let mut pulses: Vec<(i32, i32)> = Vec::new();
            let mut from = None;
            for (tick, &value) in (0..).zip(&shown) {
                let away = value >> bit & 1 == 1;
                match (away, from) {
                    (true, None) => from = Some(tick),
                    (false, Some(start)) => {
                        pulses.push((start, tick - start));
                        from = None;
                    }
                    _ => {}
                }
            }
            if from.is_some() || pulses.len() != edges.len() {
                return Ok(false);
            }
            let each =
                pulses.iter().zip(&edges).all(|(&(start, ticks), &edge)| {
                    ticks == self.ticks
                        && (edge..=edge + latest).contains(&start)
                });
            if !each {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Whether copies of the monostable in `data`, whose own cases came out
/// right, stack west into a wider one: the shortest step west, up to two
/// past its width, at which two copies give the same pulse on each bit,
/// then the most copies that do, up to `MOST_COPIES`, as one that pulses
/// on both edges if `dual`. `None` when it is no monostable.
pub fn stacking(data: &[u8], dual: bool) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let single = cases(&blocks).ok()?;
    let edge = single.edge(dual);
    let pulse = single.pulse(edge)?;
    let (low, high) = interface::measured(&blocks);
    let reach = i64::from(high[0] - low[0] + 1) + 2;
    let buses = stack::Buses::of("monostable", Orientation::Horizontal);
    let works = |blocks: &Blocks, copies: usize| {
        let found = cases(blocks).ok()?;
        let run = run(&found, dual);
        (run.wrong == 0
            && found.width == single.width * copies
            && found.edge(dual) == edge
            && found.pulse(edge) == Some(pulse)
            && found.inverted == single.inverted)
            .then_some(run.cases)
    };
    Some(stacking::west_into_bits(
        data,
        reach,
        single.width,
        buses,
        works,
        "No step west, up to two past its width, makes copies that each \
         give its pulse alone.",
    ))
}
