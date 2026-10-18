//! A stateless build's throughput: the fewest ticks between one input and
//! the next at which every result still comes out right. Cases go in every
//! `k` ticks through a fixed stream, each input at its own tick from A's,
//! as the site's timing gives them, and each output is read for each case
//! at its own latency from A, as the site times it, or just before the next
//! case comes in, whichever is later. A build that only takes a new input
//! once the last has settled has a throughput near its latency; a
//! pipelined one, or one whose paths all take about as long, takes them
//! faster. The simulator does not model torch burnout, so `evaluate` holds
//! a build with torches to three ticks or more, and one without to two.

use anyhow::Result;

use super::{
    circuit::{Circuit, input_bits},
    timing::Timing,
};
use crate::{interface, schem::Blocks};

/// The fewest random cases the stream runs through.
const CASES: usize = 64;

/// The throughput of the build in `blocks`, timed as `timing`; `None` when
/// no period up to twice its slowest output keeps every result right.
pub fn throughput(blocks: &Blocks, timing: &Timing) -> Result<Option<i32>> {
    let interface = interface::read(blocks);
    let widths: Vec<usize> = interface.inputs.iter().map(input_bits).collect();
    let cases = stream(&widths);
    // Each input's tick from A's, as the timing found it.
    let offset: Vec<i32> = interface
        .inputs
        .iter()
        .map(|port| {
            let found = timing.inputs.iter().find(|i| i.name == port.name);
            found.and_then(|i| i.tick).unwrap_or(0)
        })
        .collect();
    let earliest = offset.iter().copied().min().unwrap_or(0).min(0);
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
    // What each case settles to.
    let mut circuit = Circuit::new(blocks, &interface)?;
    let mut expected = Vec::with_capacity(cases.len());
    for case in &cases {
        for (port, &value) in case.iter().enumerate() {
            circuit.set(port, value);
        }
        circuit.settle()?;
        expected.push(
            (0..latency.len())
                .map(|o| circuit.get(o))
                .collect::<Vec<_>>(),
        );
    }
    let slowest = latency.iter().copied().max().unwrap_or(1).max(1);
    for period in 1..=2 * slowest {
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
        let mut circuit = Circuit::new(blocks, &interface)?;
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
                circuit.set(port, value);
                next_set += 1;
            }
            circuit.tick();
        }
        if right {
            return Ok(Some(period));
        }
    }
    Ok(None)
}

/// A fixed stream of cases: every mix of the values that make the longest
/// carries and the most bits change, as zero, all ones, one, and
/// alternating bits, then as many from a simple generator, so every build
/// is measured on the same stream.
fn stream(widths: &[usize]) -> Vec<Vec<u64>> {
    let mask = |w: usize| if w >= 64 { u64::MAX } else { (1 << w) - 1 };
    let special = |w: usize| {
        [0, u64::MAX, 1, 0x5555_5555_5555_5555, 0xaaaa_aaaa_aaaa_aaaa]
            .map(|v| v & mask(w))
    };
    let mut cases: Vec<Vec<u64>> = vec![Vec::new()];
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
    let random = cases.len().max(CASES);
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
