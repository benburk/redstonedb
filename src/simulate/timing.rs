//! When a combinational build's outputs settle, and whether they settle
//! cleanly.
//!
//! Input A, the lime one, changes at tick 0, and every other time is told
//! from it. Each other input's timing is the latest tick at which it may
//! change without delaying any output: an output it reaches in fewer ticks
//! than A does lets it arrive that much later, and one it reaches in more
//! makes it arrive earlier, at a negative tick. Each output's latency is
//! the last tick it settles by, and the build is synced when every output
//! changes only once, with all its bits on the same tick. An output whose
//! every block has a repeater or comparator standing on it, carrying it
//! out of the build, is timed from when it reaches them, leaving out their
//! delay. An output whose
//! bit changes more than once glitches: it pulses when it should stay put,
//! or flips back and forth on its way to a new value.
//!
//! Three kinds of change are measured, each from a settled state:
//!
//! - each input alone, from 0 to every value and back, with the other
//!   inputs held at every combination of theirs, which gives each input's
//!   timing;
//! - every input together, each at its timing, from all off to every
//!   combination and back.
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

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{Circuit, MAX_TICKS};
use crate::{interface::Interface, lint, schem::Blocks};

/// When a build's inputs may change and its outputs settle, in ticks from
/// input A changing.
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
    pub slowest_case: Vec<u64>,
    /// How many changes were measured.
    pub changes: u64,
    /// Whether the inputs took the test patterns rather than every value.
    #[serde(default)]
    pub patterns: bool,
    /// Whether ticks count from a register's read lever turning on rather
    /// than from input A changing.
    #[serde(default)]
    pub from_read: bool,
    /// For a register, the most ticks from pressing a cell's write button,
    /// its read held on, until the bus shows the new value; or, for a cell
    /// that takes its data as its write closes, from it closing.
    #[serde(default)]
    pub write: Option<i32>,
    /// For a register, the most ticks for data to pass through a cell, its
    /// read on: from its data in changing, with its write held open, or for
    /// a cell that takes its data as its write closes, as it closes, until
    /// the bus shows it. `None` for one that takes its data only
    /// as its write opens.
    #[serde(default)]
    pub data: Option<i32>,
    /// When a register's cells take their data.
    #[serde(default)]
    pub takes: Takes,
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
    /// such as carry out: the build's latency. A register's is data in to
    /// data out, through a cell whose write is held open and read on, so a
    /// data loop's latency is its ALU's plus its registers'; or, for one
    /// that takes its data only as its write opens, from the write's button.
    pub fn latency(&self) -> Option<i32> {
        if self.from_read {
            return self.data.or(self.write).or_else(|| self.read());
        }
        self.read()
    }

    /// The slowest data output's ticks, from input A changing or, for a
    /// register, from a read lever turning on.
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
    /// The latest tick it may change, from A's; 0 for A. `None` for an
    /// input no output follows.
    pub tick: Option<i32>,
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
    /// to carry it out of the build, left out of its ticks.
    #[serde(default)]
    pub carried: i32,
    /// How many changes it glitched in: a bit of it changed more than once.
    #[serde(default)]
    pub glitches: u64,
    /// How many changes its bits changed on different ticks in, each once,
    /// passing through a value it should not show.
    #[serde(default)]
    pub staggered: u64,
    /// The first change from all off in which it glitched.
    #[serde(default)]
    pub glitch: Option<Glitch>,
    /// Each bit, least significant first.
    pub bits: Vec<BitTiming>,
}

/// One change in which an output glitched, to try in-game.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Glitch {
    /// The inputs it changed to from all off, in the order of
    /// `Timing::inputs`.
    pub case: Vec<u64>,
    /// The output's value before the change.
    pub before: u64,
    /// Each tick the output changed on, from A's, and its value from then.
    pub steps: Vec<(i32, u64)>,
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
/// value, or the test patterns when that would be too many.
pub(super) fn measure(
    blocks: &Blocks,
    interface: &Interface,
    inputs: &[usize],
    held: &[(usize, u64)],
    chosen: Option<(Vec<Vec<u64>>, bool)>,
    cancel: &AtomicBool,
) -> Result<Timing> {
    let widths: Vec<usize> = inputs
        .iter()
        .map(|&port| super::input_bits(&interface.inputs[port]))
        .collect();
    let (values, patterns) = chosen.unwrap_or_else(|| values_to_try(&widths));
    let outputs: Vec<usize> = interface
        .outputs
        .iter()
        .map(|port| super::output_bits(interface, port))
        .collect();
    let carried = Circuit::new(blocks, interface)?.carried();
    let rig = Rig {
        blocks,
        interface,
        inputs,
        held,
        values: &values,
        outputs: &outputs,
        cancel,
    };

    // Each input alone, times from its own change.
    let mut alone = Vec::new();
    for input in 0..inputs.len() {
        alone.push(rig.parallel(|circuit, part, parts| {
            rig.alone(circuit, input, part, parts)
        })?);
    }
    let latency =
        |stats: &Stats, output: usize| stats.outputs[output].latency();
    let ticks: Vec<Option<i32>> = alone
        .iter()
        .map(|stats| {
            (0..outputs.len())
                .filter_map(|o| {
                    Some(latency(&alone[0], o)? - latency(stats, o)?)
                })
                .min()
        })
        .collect();

    // Every input together, each at its timing, times from A's.
    let offsets: Vec<i32> = ticks.iter().map(|t| t.unwrap_or(0)).collect();
    let mut together = rig.parallel(|circuit, part, parts| {
        rig.together(circuit, &offsets, part, parts)
    })?;
    for (stats, offset) in alone.iter().zip(&offsets) {
        together.merge(stats, *offset);
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
            .zip(&ticks)
            .map(|(&port, &tick)| InputTiming {
                name: interface.inputs[port].name.clone(),
                tick,
            })
            .collect(),
        outputs: output_timings(interface, &together, &carried),
        synced: together
            .outputs
            .iter()
            .all(|stats| stats.glitches == 0 && stats.staggered == 0),
        median: percentile(50),
        p99: percentile(99),
        slowest: together.slowest.0,
        slowest_case: together.slowest.1.clone(),
        changes: together.changes,
        patterns,
        from_read: false,
        takes: Takes::Open,
        write: None,
        data: None,
    })
}

/// Each output's timing, from what every change together found, with the
/// ticks `carried` its repeaters or comparators take to carry it out.
fn output_timings(
    interface: &Interface,
    together: &Stats,
    carried: &[i32],
) -> Vec<OutputTiming> {
    interface
        .outputs
        .iter()
        .zip(&together.outputs)
        .zip(carried)
        .map(|((port, stats), &carried)| OutputTiming {
            name: port.name.clone(),
            status: port.block.starts_with(lint::STATUS),
            latency: stats.latency().unwrap_or(0),
            first_change: stats.first_change().unwrap_or(0),
            carried,
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
fn values_to_try(widths: &[usize]) -> (Vec<Vec<u64>>, bool) {
    let patterns = widths.iter().sum::<usize>() > super::EXHAUSTIVE_BITS;
    let values = widths
        .iter()
        .map(|&width| {
            if patterns {
                super::patterns(width)
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
    inputs: &'a [usize],
    /// Inputs set before anything is measured and left alone.
    held: &'a [(usize, u64)],
    /// The values each input takes, 0 among them.
    values: &'a [Vec<u64>],
    /// Each output port's width.
    outputs: &'a [usize],
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
        let mut merged = Stats::new(self.outputs);
        for stats in found {
            merged.merge(&stats?, 0);
        }
        Ok(merged)
    }

    /// Input `input` alone from 0 to each of its values and back, with the
    /// others held at each combination of theirs: the `part`th of `parts`
    /// shares of those combinations.
    fn alone(
        &self,
        circuit: &mut Circuit,
        input: usize,
        part: u64,
        parts: u64,
    ) -> Result<Stats> {
        let mut stats = Stats::new(self.outputs);
        let driven = self.inputs[input];
        let others: Vec<usize> =
            (0..self.inputs.len()).filter(|&i| i != input).collect();
        let combinations: u64 = others
            .iter()
            .map(|&i| self.values[i].len() as u64)
            .product();
        for rest in (part..combinations).step_by(usize::try_from(parts)?) {
            self.check_cancel()?;
            for (&other, value) in others.iter().zip(self.pick(&others, rest)) {
                circuit.set(self.inputs[other], value);
            }
            circuit.set(driven, 0);
            circuit.settle()?;
            for &value in self.values[input].iter().filter(|&&v| v != 0) {
                transition(circuit, &[(0, driven, value)], None, &mut stats)?;
                transition(circuit, &[(0, driven, 0)], None, &mut stats)?;
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
        let mut stats = Stats::new(self.outputs);
        for &port in self.inputs {
            circuit.set(port, 0);
        }
        circuit.settle()?;
        let every: Vec<usize> = (0..self.inputs.len()).collect();
        let combinations: u64 =
            self.values.iter().map(|v| v.len() as u64).product();
        for case in (part..combinations).step_by(usize::try_from(parts)?) {
            if case % 256 == 0 {
                self.check_cancel()?;
            }
            let values = self.pick(&every, case);
            let changes = |off: bool| -> Vec<(i32, usize, u64)> {
                values
                    .iter()
                    .enumerate()
                    .filter(|&(_, &value)| value != 0)
                    .map(|(i, &value)| {
                        (
                            offsets[i],
                            self.inputs[i],
                            if off { 0 } else { value },
                        )
                    })
                    .collect()
            };
            let up = transition(
                circuit,
                &changes(false),
                Some((case, &values)),
                &mut stats,
            )?;
            if let Some(tick) = up {
                *stats.settled.entry(tick).or_default() += 1;
                if tick > stats.slowest.0 || stats.slowest.1.is_empty() {
                    stats.slowest = (tick, values.clone());
                }
            }
            transition(circuit, &changes(true), None, &mut stats)?;
        }
        Ok(stats)
    }

    /// The values of `inputs` in their `index`th combination, the first
    /// changing fastest.
    fn pick(&self, inputs: &[usize], mut index: u64) -> Vec<u64> {
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

/// Makes each change at its tick, given as (tick, input port, value), and
/// runs until nothing is left to update, recording how each output bit
/// changed, less the ticks the components standing on its blocks take to
/// carry it out. A change from all off gives its `case`, its number and
/// input values, to keep as an example of a glitch. Returns the last tick
/// any output changed on, if one did.
fn transition(
    circuit: &mut Circuit,
    changes: &[(i32, usize, u64)],
    case: Option<(u64, &[u64])>,
    stats: &mut Stats,
) -> Result<Option<i32>> {
    let mut changes = changes.to_vec();
    changes.sort_unstable_by_key(|&(tick, ..)| tick);
    let start = changes.first().map_or(0, |&(tick, ..)| tick);
    let mut shown: Vec<u64> =
        (0..stats.outputs.len()).map(|o| circuit.get(o)).collect();
    let before = shown.clone();
    let carried = circuit.carried();
    // Per output, each tick it changed on and its value from then.
    let mut steps: Vec<Vec<(i32, u64)>> = vec![Vec::new(); shown.len()];
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
            let tick = tick - carried[output];
            if flipped != 0 {
                steps[output].push((tick, now));
            }
            for (bit, (count, first, last)) in bits.iter_mut().enumerate() {
                if flipped >> bit & 1 == 1 {
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
            if let Some((number, values)) = case
                && output.glitch.as_ref().is_none_or(|(n, _)| number < *n)
            {
                let glitch = Glitch {
                    case: values.to_vec(),
                    before: before[o],
                    steps: steps[o].clone(),
                };
                output.glitch = Some((number, glitch));
            }
        } else if moved.iter().any(|b| b.2 != last) {
            output.staggered += 1;
        }
        settled = settled.max(Some(last));
    }
    Ok(settled)
}

/// What a set of changes did to the outputs.
struct Stats {
    outputs: Vec<OutputStats>,
    /// How many changes settled the whole result by each tick.
    settled: BTreeMap<i32, u64>,
    /// The slowest change's tick and input values.
    slowest: (i32, Vec<u64>),
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
    fn new(widths: &[usize]) -> Self {
        Stats {
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

    /// Adds `other`'s findings, its ticks moved later by `offset`.
    fn merge(&mut self, other: &Stats, offset: i32) {
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
