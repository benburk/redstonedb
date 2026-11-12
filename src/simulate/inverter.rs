//! Testing an inverter: a bus, input A, and one control line that inverts
//! every bit of it, A XOR the line, on an output as wide as A. It is a row
//! of XOR gates sharing one input, so copies stacked side by side add
//! bits, sharing the line.

use anyhow::{Result, bail};

use super::{
    Broken, Stacker, Test,
    bench::{Bench, Case, Cases, Kind, Spec, mask},
    circuit::output_bits,
    converters::byte_cases,
    stacking,
};
use crate::interface::Interface;

/// An inverter's every case, or with too many, each byte's and the test
/// patterns, each with its line off and on: A, or every bit of it
/// inverted.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let (a, line) = ports(interface)?;
    let width = interface.inputs[a].bits.len();
    let outputs: Vec<usize> = (0..interface.outputs.len()).collect();
    let bits: usize = outputs
        .iter()
        .map(|&port| output_bits(interface, &interface.outputs[port]))
        .sum();
    if bits != width {
        broken!(
            "A has {width} bits, but its outputs have {bits}; an inverter \
             gives each bit of A, inverted or not."
        );
    }
    let pattern = format!("A xor {}", interface.inputs[line].name);
    Ok(Spec {
        bench: Bench::new(vec![a, line], outputs, &pattern),
        cases: Cases::Combinations(Box::new(move || {
            let (values, paired) = byte_cases(width);
            let cases = [0, 1]
                .into_iter()
                .flat_map(|on| {
                    values.iter().map(move |&[a, ..]| -> Case { [a, on, 0] })
                })
                .collect();
            (cases, 2 * paired)
        })),
        kinds: vec![Kind::new(&pattern, move |case| {
            case[0] ^ if case[1] == 1 { mask(width) } else { 0 }
        })],
        by_digit: false,
    })
}

/// Input A and the control line, by their place in `interface`.
fn ports(interface: &Interface) -> Result<(usize, usize)> {
    let lines: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| interface.inputs[i].control())
        .collect();
    let &[line] = lines.as_slice() else {
        broken!(
            "An inverter has one control line, which inverts every bit of \
             A, but this has {}.",
            lines.len()
        );
    };
    if interface.inputs.len() != 2 {
        broken!(
            "An inverter takes A and its control line, but this has {} \
             inputs.",
            interface.inputs.len()
        );
    }
    let a = 1 - line;
    if interface.inputs[a].signal_strength {
        bail!(
            "Input {} takes a signal strength, which the inverter test does \
             not drive.",
            interface.inputs[a].name
        );
    }
    Ok((a, line))
}

/// How copies of the inverter in `data`, whose own cases came out right,
/// stack into a wider one sharing its control line: two copies inverting
/// every bit with the one line, then as many more as do, as the line runs
/// only so far. `None` when it is no inverter.
pub fn stacker(data: &[u8]) -> Option<Stacker<'_>> {
    stacking::bitwise(
        data,
        Test::Inverter,
        |interface| ports(interface).ok().map(|(a, _)| a),
        "No pitch west or up makes copies that one control line inverts \
         together.",
    )
}
