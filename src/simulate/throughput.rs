//! A stateless build's throughput: the fewest ticks between one input and
//! the next at which every result still comes out right. Cases go in every
//! `k` ticks through a fixed stream, each input at its own tick from A's,
//! as the site's timing gives them, and each output is read for each case
//! at its own latency from A, as the site times it, or just before the next
//! case comes in, whichever is later. A build that only takes a new input
//! once the last has settled has a throughput near its latency; a
//! pipelined one, or one whose paths all take about as long, takes them
//! faster. A sequential build's button, as its calculate, is pressed in
//! the stream at its own tick, so its throughput is how often a new
//! calculation can start. The simulator does not model torch burnout, so `evaluate` holds
//! a build with torches to three ticks or more, and one without to two.

use anyhow::Result;

use super::{
    circuit::{Circuit, input_bits},
    timing::Timing,
};
use crate::{recognize::interface, schem::Blocks};

/// The fewest random cases the stream a period is found on runs through,
/// and the long one it is checked on.
pub(super) const CASES: usize = 64;
pub(super) const LONG: usize = 4096;

/// The most mixes of each input's telling values a stream begins with.
const MIXES: usize = 4096;

/// The throughput of the build in `blocks`, its lines `interface` as its
/// test names them, timed as `timing`; `None` when no period up to twice
/// its slowest output keeps every result right.
pub fn throughput(
    blocks: &Blocks,
    interface: &interface::Interface,
    timing: &Timing,
) -> Result<Option<i32>> {
    let widths: Vec<usize> = interface.inputs.iter().map(input_bits).collect();
    let cases = stream(&widths, CASES);
    // Each input's tick from A's, as the timing found it.
    let offset: Vec<i32> = interface
        .inputs
        .iter()
        .map(|port| {
            let found = timing.inputs.iter().find(|i| i.name == port.name);
            found.and_then(|i| i.tick).unwrap_or(0)
        })
        .collect();
    // Each output's tick from A's, past the repeaters or comparators that
    // carry it out, where the site reads it.
    let latency: Vec<i32> = timing
        .outputs
        .iter()
        .map(|o| o.latency + o.carried)
        .collect();
    if latency.len() != interface.outputs.len() {
        return Ok(None);
    }
    let circuit = Circuit::new(blocks, interface)?;
    let button: Vec<bool> = (0..interface.inputs.len())
        .map(|p| circuit.is_button(p, 0))
        .collect();
    let timed = |port: usize| {
        let name = &interface.inputs[port].name;
        timing
            .inputs
            .iter()
            .any(|i| &i.name == name && i.tick.is_some())
    };
    // A button with no tick can't be streamed.
    if (0..button.len()).any(|p| button[p] && !timed(p)) {
        return Ok(None);
    }
    let slowest = latency.iter().copied().max().unwrap_or(1).max(1);
    let long = stream(&widths, LONG);
    shortest(blocks, (&cases, &long), &offset, &latency, 2 * slowest)
}

/// The shortest period, up to `most`, at which the `short` stream of cases
/// and then the `long` one each come out as they settle in `blocks`: each
/// input set, or its button pressed, at its `offset` from A's, and each
/// output read at its `latency`. Found on the short stream, then checked
/// on the long one, a period kept only once every case of that comes out
/// right too.
pub(super) fn shortest(
    blocks: &Blocks,
    (short, long): (&[Vec<u64>], &[Vec<u64>]),
    offset: &[i32],
    latency: &[i32],
    most: i32,
) -> Result<Option<i32>> {
    let interface = interface::read(blocks);
    let mut circuit = Circuit::new(blocks, &interface)?;
    let button: Vec<bool> = (0..interface.inputs.len())
        .map(|p| circuit.is_button(p, 0))
        .collect();
    // What each case settles to: its levers set, then its buttons, as a
    // sequential build's calculate, pressed in the order of their ticks,
    // each settling in turn.
    let mut presses: Vec<usize> =
        (0..button.len()).filter(|&p| button[p]).collect();
    presses.sort_by_key(|&p| offset[p]);
    let expected =
        settled_cases(&mut circuit, short, &button, &presses, latency.len())?;
    let quick = Stream {
        blocks,
        interface: &interface,
        cases: short,
        expected: &expected,
        offset,
        latency,
        button: &button,
    };
    let long_expected =
        settled_cases(&mut circuit, long, &button, &presses, latency.len())?;
    let checked = Stream {
        cases: long,
        expected: &long_expected,
        ..quick
    };
    for period in 1..=most {
        if quick.right(period)? {
            for period in period..=most {
                if checked.right(period)? {
                    return Ok(Some(period));
                }
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// What each of `cases` settles to: its levers set, then its buttons,
/// `presses`, pressed in turn, each settling, read from its `outputs`.
fn settled_cases(
    circuit: &mut Circuit,
    cases: &[Vec<u64>],
    button: &[bool],
    presses: &[usize],
    outputs: usize,
) -> Result<Vec<Vec<u64>>> {
    let mut expected = Vec::with_capacity(cases.len());
    for case in cases {
        for (port, &value) in case.iter().enumerate() {
            if !button[port] {
                circuit.set(port, value);
            }
        }
        circuit.settle()?;
        for &port in presses {
            circuit.press(port, 0);
            circuit.settle()?;
        }
        expected.push((0..outputs).map(|o| circuit.get(o)).collect::<Vec<_>>());
    }
    Ok(expected)
}

/// A stream of cases, each input set or pressed at its tick from A's, and
/// each output read at its latency: what `throughput` tries at each period.
struct Stream<'a> {
    blocks: &'a Blocks,
    interface: &'a crate::recognize::interface::Interface,
    cases: &'a [Vec<u64>],
    expected: &'a [Vec<u64>],
    offset: &'a [i32],
    latency: &'a [i32],
    button: &'a [bool],
}

impl Stream<'_> {
    /// Whether every case comes out right, a case going in every `period`
    /// ticks.
    fn right(&self, period: i32) -> Result<bool> {
        let (blocks, interface, cases, expected) =
            (self.blocks, self.interface, self.cases, self.expected);
        let (offset, latency, button) =
            (self.offset, self.latency, self.button);
        let earliest = offset.iter().copied().min().unwrap_or(0).min(0);

        // Ticks from the stream's start: case `i`'s A goes in at
        // `start + i * period`.
        let start = -earliest;
        let mut sets: Vec<(i32, usize, u64)> = Vec::new();
        let mut reads: Vec<(i32, usize, usize)> = Vec::new();
        for ((i, case), n) in cases.iter().enumerate().zip(0..) {
            let a = start + n * period;
            for (port, &value) in case.iter().enumerate() {
                sets.push((a + offset[port], port, value));
            }
            for (o, &l) in latency.iter().enumerate() {
                reads.push((a + l.max(period + earliest), o, i));
            }
        }
        sets.sort_unstable();
        reads.sort_unstable();
        let end = reads.last().map_or(0, |r| r.0);
        let mut circuit = Circuit::new(blocks, interface)?;
        circuit.settle()?;
        let (mut next_set, mut next_read, mut right) = (0, 0, true);
        for t in 0..=end {
            // Read before setting, so an output read as the next case goes
            // in still shows its own.
            while let Some(&(at, o, i)) = reads.get(next_read)
                && at == t
            {
                next_read += 1;
                right &= circuit.get(o) == expected[i][o];
            }
            if !right {
                break;
            }
            while let Some(&(at, port, value)) = sets.get(next_set)
                && at == t
            {
                if button[port] {
                    circuit.press(port, 0);
                } else {
                    circuit.set(port, value);
                }
                next_set += 1;
            }
            circuit.tick();
        }
        Ok(right)
    }
}

/// A fixed stream of cases: every mix of the values that make the longest
/// carries and the most bits change, as zero, all ones, one, and
/// alternating bits, then as many from a simple generator, so every build
/// is measured on the same stream.
fn stream(widths: &[usize], random: usize) -> Vec<Vec<u64>> {
    let mask = |w: usize| if w >= 64 { u64::MAX } else { (1 << w) - 1 };
    let special = |w: usize| {
        [0, u64::MAX, 1, 0x5555_5555_5555_5555, 0xaaaa_aaaa_aaaa_aaaa]
            .map(|v| v & mask(w))
    };
    // Every mix only while there are few: past `MIXES`, as for many
    // control lines, the generator alone.
    let few = u32::try_from(widths.len())
        .ok()
        .and_then(|n| 5_usize.checked_pow(n))
        .is_some_and(|mixes| mixes <= MIXES);
    let mut cases: Vec<Vec<u64>> =
        if few { vec![Vec::new()] } else { Vec::new() };
    for &w in widths {
        cases = cases
            .into_iter()
            .flat_map(|case| {
                special(w).into_iter().map(move |v| {
                    let mut case = case.clone();
                    case.push(v);
                    case
                })
            })
            .collect();
    }
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let random = cases.len().max(random);
    // Each special case beside random ones, so every case follows others
    // of every kind.
    let mut stream = Vec::new();
    for case in cases {
        stream.push(case);
        stream.push(widths.iter().map(|&w| next() & mask(w)).collect());
    }
    while stream.len() < 2 * random {
        stream.push(widths.iter().map(|&w| next() & mask(w)).collect());
    }
    stream
}
