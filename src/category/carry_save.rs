//! Testing a carry-save adder: three operand buses of one width, A, B, and
//! C, into two outputs as wide, its sum, each bit A xor B xor C, and its
//! carry, each bit the majority of A's, B's, and C's, worth twice its
//! place, so the sum plus twice the carry is A + B + C. Each bit is a full
//! adder of its own, so copies stacked side by side add bits.

use anyhow::Result;

use crate::{
    category::{Test, test::BySpec},
    interface::Interface,
    simulate::{
        Stacker,
        bench::{Bench, Case, Kind, Spec},
        broken,
    },
};

/// A carry-save adder's cases, its sum and its carry read together, in
/// the order its outputs come.
const SUM_CARRY: &str = "A xor B xor C, maj(A, B, C)";
const CARRY_SUM: &str = "maj(A, B, C), A xor B xor C";

/// A carry-save adder's every case, or with too many, each byte's and the
/// test patterns: three operand buses of one width into two outputs as
/// wide, one each bit's sum and the other its carry, whichever way round
/// they come. No line reads inverted, as an inverted operand makes
/// another circuit of it.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let inputs: Vec<usize> = (0..interface.inputs.len()).collect();
    let &[a, b, c] = inputs.as_slice() else {
        broken!(
            "A carry-save adder takes three operands and no lines, but this \
             has {} inputs.{}",
            inputs.len(),
            interface.lever_under_block(&inputs, 3)
        );
    };
    let &[first, second] =
        (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A carry-save adder has two outputs, its sum and its carry, but \
             this has {}.",
            interface.outputs.len()
        );
    };
    let width = interface.inputs[a].bits.len();
    let input = |port: usize| interface.inputs[port].bits.len();
    let output = |port: usize| interface.outputs[port].bits.len();
    if input(b) != width
        || input(c) != width
        || output(first) != width
        || output(second) != width
    {
        broken!(
            "A carry-save adder's three operands and two outputs have the \
             same number of bits, but this one's differ."
        );
    }
    let sum = |case: &Case| case[0] ^ case[1] ^ case[2];
    let carry =
        |case: &Case| case[0] & case[1] | case[0] & case[2] | case[1] & case[2];
    let mut bench = Bench::new(vec![a, b, c], vec![first, second], SUM_CARRY);
    bench.invertible = false;
    Ok(Spec {
        bench,
        kinds: vec![
            Kind::new(SUM_CARRY, move |case| sum(case) | carry(case) << width),
            Kind::new(CARRY_SUM, move |case| carry(case) | sum(case) << width),
        ],
    })
}

/// How copies of the carry-save adder in `data` stack into a wider one,
/// each bit a full adder of its own, as a gate's copies are.
pub fn stacker(data: &[u8]) -> Option<Stacker<'_>> {
    super::stacks::bitwise(
        data,
        Test::CarrySaveAdder,
        |interface| spec(interface).ok().map(|_| 0),
        "No pitch west or up makes copies whose bits together give the sum \
         and carry of A, B, and C.",
    )
}

/// A carry-save adder's test: its copies add bits, each a cell of its own.
pub(super) const CARRY_SAVE_ADDER: BySpec =
    BySpec::stacked(|_, interface| spec(interface), stacker);
