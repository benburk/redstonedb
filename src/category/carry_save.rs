//! Testing a carry-save adder: three operand buses of one width, A, B, and
//! C, into two outputs as wide, its sum, each bit A xor B xor C, and its
//! carry, each bit the majority of A's, B's, and C's, worth twice its
//! place, so the sum plus twice the carry is A + B + C. Each bit is a full
//! adder of its own, so copies stacked side by side add bits.

use anyhow::Result;

use crate::{
    category::{Test, test::BySpec},
    interface::Interface,
    schem::lines::Role,
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
    let &[a, b, c] = (0..interface.inputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!("Expected 3 inputs, found {}", interface.inputs.len());
    };
    let &[first, second] =
        (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!("Expected 2 outputs, found {}", interface.outputs.len());
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
            "Expected width {width} on each input and output, found A {width}, \
             B {}, C {}, outputs {} and {}",
            input(b),
            input(c),
            output(first),
            output(second)
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

/// Gives `interface`'s lines a carry-save adder's roles: its inputs A, B,
/// and C, and its outputs the first and second, so neither its sum nor its
/// carry is a status line its latency leaves out. A 1-bit one's lone lines
/// read as a full adder's carry in and carry out until then. A line its
/// own sign names keeps that name.
fn as_operands(interface: &mut Interface) {
    for (place, port) in (0u8..).zip(&mut interface.inputs) {
        if port.control() {
            port.role = Some(Role::Operand(place));
            if !port.labelled {
                port.name = Role::Operand(place).key();
            }
        }
    }
    let count = interface.outputs.len();
    for (place, port) in (0u8..).zip(&mut interface.outputs) {
        if port.status() {
            port.role = Some(Role::Output(place));
            if !port.labelled {
                port.name = if count == 1 {
                    "output".to_owned()
                } else {
                    format!("output {}", place + 1)
                };
            }
        }
    }
}

/// How copies of the carry-save adder in `data` stack into a wider one,
/// each bit a full adder of its own, as a gate's copies are.
pub fn stacker(data: &[u8]) -> Option<Stacker<'_>> {
    super::stacks::bitwise(
        data,
        Test::CarrySaveAdder,
        |interface| spec(interface).ok().map(|_| 0),
        "No pitch makes copies that together give the sum and carry",
    )
}

/// A carry-save adder's test, its lines given their roles: its copies add
/// bits, each a cell of its own.
pub(super) const CARRY_SAVE_ADDER: BySpec = BySpec::stacked(
    |_, interface| {
        as_operands(interface);
        spec(interface)
    },
    stacker,
);
