//! Driving a build once, before anything is known of what its lines are
//! for: each run of levers is driven through a few telling values of its
//! width, every pairing of them, and what each run of outputs shows is
//! kept. Which run is A and which way round is decided later, by reading
//! the values kept, so a build is compiled and run once however many ways
//! its lines could be read.

use anyhow::{Result, bail};

use crate::{
    recognize::{
        inspect,
        interface::{self, Bit, Interface, Port},
    },
    schem::{Blocks, Pos},
    simulate::{Circuit, Value, telling},
};

/// What a build did: its runs of inputs and of outputs, each run's bits in
/// its order, and each case, the value driven on each input run and the
/// value each output run showed, a run's first bit its value's lowest.
pub struct Observation {
    pub inputs: Vec<Vec<Pos>>,
    pub outputs: Vec<Vec<Pos>>,
    pub cases: Vec<(Vec<Value>, Vec<Value>)>,
}

/// The most levers a build may have to be driven in every combination of
/// them, rather than through telling values.
const EXHAUSTIVE_BITS: usize = 12;

/// The most cases a build is observed in: past it, the pairings are
/// sampled evenly.
const MOST_CASES: u128 = 1 << EXHAUSTIVE_BITS;

/// The build in `blocks` driven through every combination of `report`'s
/// levers, when they are few, else every pairing of a few telling values
/// of each run of them, a lone lever 0 and 1, as [`telling`] gives them,
/// or an even sample of those past [`MOST_CASES`], with what its outputs
/// showed; or why it couldn't be.
pub fn observe(
    blocks: &Blocks,
    report: &inspect::Report,
) -> Result<Observation> {
    let switches = interface::switches(blocks);
    if report
        .inputs
        .iter()
        .flat_map(|run| &run.bits)
        .any(|pos| switches.get(pos).is_none_or(|switch| switch.button))
    {
        bail!("A button is pressed, not held, so it is observed in time.");
    }
    if !report.digits.is_empty() {
        bail!("A signal-strength input takes a digit, not a bit.");
    }
    let port = |i: usize, bits: Vec<Bit>| Port {
        name: format!("line {}", i + 1),
        role: None,
        labelled: false,
        bits,
        strength: 1,
        words: Vec::new(),
        signal_strength: false,
    };
    let inputs: Vec<Vec<Pos>> =
        report.inputs.iter().map(|run| run.bits.clone()).collect();
    let outputs: Vec<Vec<Pos>> =
        report.outputs.iter().map(|run| run.bits.clone()).collect();
    let interface = Interface {
        inputs: inputs
            .iter()
            .enumerate()
            .map(|(i, bits)| {
                let bits = bits
                    .iter()
                    .map(|&pos| Bit::input(blocks, &switches, pos))
                    .collect();
                port(i, bits)
            })
            .collect(),
        outputs: outputs
            .iter()
            .enumerate()
            .map(|(i, bits)| {
                port(i, bits.iter().copied().map(Bit::at).collect())
            })
            .collect(),
        binary_outputs: false,
    };
    let mut circuit = Circuit::new(blocks, &interface)?;
    // Few enough levers are driven every way, as a threshold's count
    // shows only in values with that many bits on.
    let every = inputs.iter().map(Vec::len).sum::<usize>() <= EXHAUSTIVE_BITS;
    let choices: Vec<Vec<Value>> = inputs
        .iter()
        .map(|bits| match bits.len() {
            width if every => (0..1 << width).collect(),
            1 => vec![0, 1],
            width => telling(width),
        })
        .collect();
    let total: u128 =
        choices.iter().map(|values| values.len() as u128).product();
    let taken = total.min(MOST_CASES);
    let mut cases = Vec::new();
    for i in 0..taken {
        // The pairing at its place among them all, each run a digit.
        let mut place = i * total / taken;
        let driven: Vec<Value> = choices
            .iter()
            .map(|values| {
                let len = values.len() as u128;
                let value = values[usize::try_from(place % len).unwrap_or(0)];
                place /= len;
                value
            })
            .collect();
        for (port, &value) in driven.iter().enumerate() {
            circuit.set(port, value);
        }
        circuit.settle()?;
        let shown = (0..outputs.len()).map(|port| circuit.get(port)).collect();
        cases.push((driven, shown));
    }
    Ok(Observation {
        inputs,
        outputs,
        cases,
    })
}
