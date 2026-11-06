//! Testing a negator: input A, and perhaps a control line, its negate,
//! giving −A in two's complement, or with a line, A while it is off and −A
//! while it is on, on an output as wide as A.

use anyhow::Result;

use super::{
    Broken,
    bench::{Bench, Cases, Kind, Spec, mask},
    circuit::output_bits,
    incrementer::incrementer_cases,
};
use crate::recognize::interface::Interface;

/// A negator's every case, or with too many, each byte's with the bits
/// below it all clear and all set, as whether any lower bit is on decides
/// whether a bit flips, and the test patterns; each with its control line,
/// if any, each way: −A, or A until the line turns on.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let control = |i: &usize| interface.inputs[*i].control();
    let (lines, operands): (Vec<usize>, Vec<usize>) =
        (0..interface.inputs.len()).partition(control);
    let &[a] = operands.as_slice() else {
        broken!(
            "A negator takes one operand, A, but this has {}.",
            operands.len()
        );
    };
    if lines.len() > 1 {
        broken!(
            "Too many control lines: {} (a negator takes at most one, its \
             negate).",
            lines.len()
        );
    }
    let width = interface.inputs[a].bits.len();
    let outputs: Vec<usize> = (0..interface.outputs.len()).collect();
    let bits: usize = outputs
        .iter()
        .map(|&port| output_bits(interface, &interface.outputs[port]))
        .sum();
    if bits != width {
        broken!(
            "A has {width} bits, but its outputs have {bits}; a negator \
             gives −A in as many bits as A."
        );
    }
    let pattern = match lines.first() {
        Some(&line) => {
            format!("A, or −A with {} on", interface.inputs[line].name)
        }
        None => "−A".to_owned(),
    };
    let negated = lines.is_empty();
    let mut inputs = vec![a];
    inputs.extend(lines);
    let bench = Bench::new(inputs, outputs, &pattern);
    let widths = bench.widths(interface);
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || {
            incrementer_cases(&widths)
        })),
        kinds: vec![Kind::new(&pattern, move |case| {
            if negated || case[1] == 1 {
                case[0].wrapping_neg() & mask(width)
            } else {
                case[0]
            }
        })],
        by_digit: false,
    })
}
