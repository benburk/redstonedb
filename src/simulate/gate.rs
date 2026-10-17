//! Testing an XOR or XNOR gate: two operand buses of one width, A and B,
//! into one output as wide, each bit A xor B, or its inverse for an XNOR.
//! It has no lines.

use anyhow::Result;

use super::{
    Broken,
    adder::adder_cases,
    bench::{Bench, Cases, Hint, Kind, Spec, mask},
};
use crate::interface::Interface;

/// An XOR gate's cases, as they are named.
const XOR: &str = "A xor B";

/// An XNOR gate's cases, as they are named, which its flag follows.
pub const XNOR: &str = "A xnor B";

/// An XOR or XNOR gate's every case, or with too many, each byte's and the
/// test patterns: A xor B, or every bit of it inverted, whichever the build
/// gives.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let &[a, b] = (0..interface.inputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "An XOR or XNOR gate takes two operands and no lines, but this \
             has {} inputs.",
            interface.inputs.len()
        );
    };
    let &[out] = (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "An XOR or XNOR gate has one output, but this has {}.",
            interface.outputs.len()
        );
    };
    let width = interface.inputs[a].bits.len();
    let b_bits = interface.inputs[b].bits.len();
    let out_bits = interface.outputs[out].bits.len();
    if b_bits != width || out_bits != width {
        broken!(
            "A has {width} bits, B {b_bits}, and the output {out_bits}, but \
             an XOR or XNOR gate's operands and output have the same number."
        );
    }
    Ok(Spec {
        bench: Bench::new(vec![a, b], vec![out], XOR),
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, false, 8)
        })),
        kinds: vec![
            Kind::new(XOR, |case| case[0] ^ case[1]),
            Kind::new(XNOR, move |case| !(case[0] ^ case[1]) & mask(width)),
        ],
        hint: Hint::None,
        by_digit: false,
    })
}
