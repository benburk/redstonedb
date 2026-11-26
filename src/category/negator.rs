//! Testing a negator: input A, and perhaps a control line, its negate,
//! giving −A in two's complement, or with a line, A while it is off and −A
//! while it is on, on an output as wide as A, or a bit wider, its top bit
//! the borrow out of 0 − A, on whenever A is not 0.

use crate::simulate::Rule;
use anyhow::Result;

use crate::{
    category::test::BySpec,
    interface::{self, Interface},
    simulate::{
        bench::{Bench, Kind, Spec, mask},
        broken,
        circuit::output_bits,
    },
};

/// A negator's every case, or with too many, each byte's with the bits
/// below it all clear and all set, as whether any lower bit is on decides
/// whether a bit flips, and the test patterns; each with its control line,
/// if any, each way: −A, or A until the line turns on.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let control = |i: &usize| interface.inputs[*i].control();
    let (lines, operands): (Vec<usize>, Vec<usize>) =
        (0..interface.inputs.len()).partition(control);
    let &[a] = operands.as_slice() else {
        broken!("Expected 1 operand, found {}", operands.len());
    };
    if lines.len() > 1 {
        broken!(
            rule: Rule::TooManyControlLines;
            "Expected at most 1 control line, found {}",
            lines.len()
        );
    }
    let width = interface.inputs[a].bits.len();
    let outputs: Vec<usize> = (0..interface.outputs.len()).collect();
    let bits: usize = outputs
        .iter()
        .map(|&port| output_bits(interface, &interface.outputs[port]))
        .sum();
    if !(width..=width + 1).contains(&bits) {
        broken!(
            "Expected output width {width} or {}, found {bits}",
            width + 1
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
    Ok(Spec {
        bench,
        kinds: vec![Kind::new(&pattern, move |case| {
            if negated || case[1] == 1 {
                case[0].wrapping_neg() & mask(bits)
            } else {
                case[0]
            }
        })],
    })
}

/// A negator's test: its lone control line is named for its role, negate.
pub(super) const NEGATOR: BySpec = BySpec::spec(|_, interface| {
    interface::name_lines(super::Kind::Negator.lines(), interface);
    spec(interface)
});
