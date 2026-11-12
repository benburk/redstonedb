//! Testing a two-input gate: two operand buses of one width, A and B,
//! into one output as wide, each bit A xor B, A and B, or A or B, or that
//! inverted, as a torch on the output makes it: an XNOR, NAND, or NOR,
//! each a category of its own.
//! It has no lines, so copies stacked side by side add bits.

use anyhow::Result;

use crate::{
    category::{Test, adder::adder_cases, test::BySpec},
    interface::Interface,
    simulate::{
        Broken, Stacker, Value,
        bench::{Bench, Case, Kind, Spec, mask},
        broken,
    },
};

/// A two-input gate, its category's: one function, or that function
/// inverted, as a torch on the output makes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gate {
    Xor,
    Xnor,
    And,
    Nand,
    Or,
    Nor,
}

impl Gate {
    /// The gate whose category's test `test` is, if any.
    pub fn of(test: Test) -> Option<Gate> {
        match test {
            Test::Xor => Some(Gate::Xor),
            Test::Xnor => Some(Gate::Xnor),
            Test::And => Some(Gate::And),
            Test::Nand => Some(Gate::Nand),
            Test::Or => Some(Gate::Or),
            Test::Nor => Some(Gate::Nor),
            _ => None,
        }
    }

    /// Its category's test.
    pub fn test(self) -> Test {
        match self {
            Gate::Xor => Test::Xor,
            Gate::Xnor => Test::Xnor,
            Gate::And => Test::And,
            Gate::Nand => Test::Nand,
            Gate::Or => Test::Or,
            Gate::Nor => Test::Nor,
        }
    }

    /// Its cases' name, such as `A xor B`.
    pub fn pattern(self) -> &'static str {
        match self {
            Gate::Xor => "A xor B",
            Gate::Xnor => "A xnor B",
            Gate::And => "A and B",
            Gate::Nand => "A nand B",
            Gate::Or => "A or B",
            Gate::Nor => "A nor B",
        }
    }

    /// Its function of `a` and `b`, bit by bit, each `width` bits.
    fn apply(self, a: Value, b: Value, width: usize) -> Value {
        let all = mask(width);
        match self {
            Gate::Xor => a ^ b,
            Gate::Xnor => !(a ^ b) & all,
            Gate::And => a & b,
            Gate::Nand => !(a & b) & all,
            Gate::Or => a | b,
            Gate::Nor => !(a | b) & all,
        }
    }
}

/// A `gate`'s every case, or with too many, each byte's and the test
/// patterns: its function alone, so an XOR gate whose output is inverted
/// is no XOR gate but an XNOR gate. No line reads inverted, as an inverted
/// input makes another gate of it: a NOR is an AND of A and B inverted.
pub(super) fn spec(interface: &Interface, gate: Gate) -> Result<Spec> {
    let &[a, b] = (0..interface.inputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A gate takes two operands and no lines, but this has {} \
             inputs.",
            interface.inputs.len()
        );
    };
    let &[out] = (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A gate has one output, but this has {}.",
            interface.outputs.len()
        );
    };
    let width = interface.inputs[a].bits.len();
    let b_bits = interface.inputs[b].bits.len();
    let out_bits = interface.outputs[out].bits.len();
    if b_bits != width || out_bits != width {
        broken!(
            "A has {width} bits, B {b_bits}, and the output {out_bits}, but \
             a gate's operands and output have the same number."
        );
    }
    let pattern = gate.pattern();
    let mut bench = Bench::new(vec![a, b], vec![out], pattern);
    bench.invertible = false;
    Ok(Spec {
        bench,
        cases: Box::new(move || adder_cases(width, false, 8)),
        kinds: vec![Kind::new(pattern, move |case| {
            gate.apply(case[0], case[1], width)
        })],
        by_digit: false,
    })
}

/// A NAND+XNOR gate's cases, its NAND output and its XNOR output read
/// together, in the order its outputs come.
const NAND_XNOR: &str = "A nand B, A xnor B";
const XNOR_NAND: &str = "A xnor B, A nand B";

/// A NAND+XNOR gate's every case, or with too many, each byte's and the
/// test patterns: two operand buses of one width into two outputs as wide,
/// one each bit A nand B and the other A xnor B, whichever way round they
/// come.
pub(super) fn nand_xnor(interface: &Interface) -> Result<Spec> {
    let &[a, b] = (0..interface.inputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A NAND+XNOR gate takes two operands and no lines, but this has \
             {} inputs.",
            interface.inputs.len()
        );
    };
    let &[first, second] =
        (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A NAND+XNOR gate has two outputs, but this has {}.",
            interface.outputs.len()
        );
    };
    let width = interface.inputs[a].bits.len();
    let bits = |port: usize| interface.outputs[port].bits.len();
    if interface.inputs[b].bits.len() != width
        || bits(first) != width
        || bits(second) != width
    {
        broken!(
            "A NAND+XNOR gate's operands and two outputs have the same \
             number of bits, but this one's differ."
        );
    }
    let all = mask(width);
    let nand = move |case: &Case| !(case[0] & case[1]) & all;
    let xnor = move |case: &Case| !(case[0] ^ case[1]) & all;
    Ok(Spec {
        bench: Bench::new(vec![a, b], vec![first, second], NAND_XNOR),
        cases: Box::new(move || adder_cases(width, false, 8)),
        kinds: vec![
            Kind::new(NAND_XNOR, move |case| nand(case) | xnor(case) << width),
            Kind::new(XNOR_NAND, move |case| xnor(case) | nand(case) << width),
        ],
        by_digit: false,
    })
}

/// How copies of the NAND+XNOR gate in `data` stack into a wider one, as
/// an XOR gate's do, each a gate of its own.
pub fn nand_xnor_stacker(data: &[u8]) -> Option<Stacker<'_>> {
    super::stacks::bitwise(
        data,
        Test::NandXnor,
        |interface| nand_xnor(interface).ok().map(|_| 0),
        "No pitch west or up makes copies whose bits together give A nand \
         B and A xnor B.",
    )
}

/// How copies of the `gate` in `data` stack into a wider one, each a gate
/// of its own adding its bits to A, B, and the output, the stack tested
/// as the wider gate, which shows each copy still computes alone and its
/// neighbours don't interfere; `None` when it is not one.
pub fn stacker(data: &[u8], gate: Gate) -> Option<Stacker<'_>> {
    let pattern = gate.pattern();
    super::stacks::bitwise(
        data,
        gate.test(),
        move |interface| spec(interface, gate).ok().map(|_| 0),
        &format!(
            "No pitch west or up makes copies whose bits together give \
             {pattern}."
        ),
    )
}

/// Each gate's test: its copies add bits, each a cell of its own.
pub(super) const XOR: BySpec = BySpec::stacked(
    |_, interface| spec(interface, Gate::Xor),
    |data| stacker(data, Gate::Xor),
);
pub(super) const XNOR: BySpec = BySpec::stacked(
    |_, interface| spec(interface, Gate::Xnor),
    |data| stacker(data, Gate::Xnor),
);
pub(super) const AND: BySpec = BySpec::stacked(
    |_, interface| spec(interface, Gate::And),
    |data| stacker(data, Gate::And),
);
pub(super) const NAND: BySpec = BySpec::stacked(
    |_, interface| spec(interface, Gate::Nand),
    |data| stacker(data, Gate::Nand),
);
pub(super) const OR: BySpec = BySpec::stacked(
    |_, interface| spec(interface, Gate::Or),
    |data| stacker(data, Gate::Or),
);
pub(super) const NOR: BySpec = BySpec::stacked(
    |_, interface| spec(interface, Gate::Nor),
    |data| stacker(data, Gate::Nor),
);

pub(super) const NAND_XNOR_GATE: BySpec =
    BySpec::stacked(|_, interface| nand_xnor(interface), nand_xnor_stacker);
