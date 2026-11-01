//! Testing a two-input gate: two operand buses of one width, A and B,
//! into one output as wide, each bit A xor B, A and B, or A or B, or that
//! inverted, as a torch on the output makes it: an XNOR, NAND, or NOR.
//! It has no lines, so copies stacked side by side add bits.

use anyhow::Result;

use super::{
    Broken, Report, Stacking, Test,
    adder::adder_cases,
    bench::{Bench, Cases, Kind, Spec, mask},
    stacking,
};
use crate::{
    interface::{self, Interface},
    models::schematic::Orientation,
    schem::Blocks,
    stack,
};

/// A family of two-input gates, its category's: one function and its
/// inverse, which a torch on the output makes of it, the inverse taking
/// the category's flag.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gate {
    Xor,
    And,
    Or,
}

impl Gate {
    /// The gate whose category's test `test` is, if any.
    pub fn of(test: Test) -> Option<Gate> {
        match test {
            Test::XorXnor => Some(Gate::Xor),
            Test::AndNand => Some(Gate::And),
            Test::OrNor => Some(Gate::Or),
            _ => None,
        }
    }

    /// Its category's test.
    pub fn test(self) -> Test {
        match self {
            Gate::Xor => Test::XorXnor,
            Gate::And => Test::AndNand,
            Gate::Or => Test::OrNor,
        }
    }

    /// Its cases' names, as its function and as its inverse.
    pub fn patterns(self) -> [&'static str; 2] {
        match self {
            Gate::Xor => ["A xor B", XNOR],
            Gate::And => ["A and B", "A nand B"],
            Gate::Or => ["A or B", "A nor B"],
        }
    }

    /// The flag of a gate that gives its inverse, such as `xnor`.
    pub fn flag(self) -> &'static str {
        match self {
            Gate::Xor => "xnor",
            Gate::And => "nand",
            Gate::Or => "nor",
        }
    }

    /// Its function of `a` and `b`, bit by bit.
    fn apply(self, a: u64, b: u64) -> u64 {
        match self {
            Gate::Xor => a ^ b,
            Gate::And => a & b,
            Gate::Or => a | b,
        }
    }
}

/// An XNOR gate's cases, as they are named.
pub const XNOR: &str = "A xnor B";

/// A `gate`'s every case, or with too many, each byte's and the test
/// patterns: its function, or every bit of it inverted, whichever the
/// build gives. No line reads inverted, as an inverted input makes
/// another gate of it: a NOR is an AND of A and B inverted.
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
    let [plain, inverse] = gate.patterns();
    let mut bench = Bench::new(vec![a, b], vec![out], plain);
    bench.invertible = false;
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, false, 8)
        })),
        kinds: vec![
            Kind::new(plain, move |case| gate.apply(case[0], case[1])),
            Kind::new(inverse, move |case| {
                !gate.apply(case[0], case[1]) & mask(width)
            }),
        ],
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
    let nand = move |case: &[u64; 3]| !(case[0] & case[1]) & all;
    let xnor = move |case: &[u64; 3]| !(case[0] ^ case[1]) & all;
    Ok(Spec {
        bench: Bench::new(vec![a, b], vec![first, second], NAND_XNOR),
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, false, 8)
        })),
        kinds: vec![
            Kind::new(NAND_XNOR, move |case| nand(case) | xnor(case) << width),
            Kind::new(XNOR_NAND, move |case| xnor(case) | nand(case) << width),
        ],
        by_digit: false,
    })
}

/// How copies of the NAND+XNOR gate in `data` stack west into a wider one,
/// as an XOR gate's do.
pub fn nand_xnor_stacking(data: &[u8]) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    nand_xnor(&interface).ok()?;
    let width = interface.inputs[0].bits.len();
    let reach = [0, 1].map(|axis| i64::from(blocks.size[axis]));
    let buses =
        stack::Buses::of(Test::NandXnor.name(), Orientation::Horizontal);
    let works = |blocks: &Blocks, copies: usize| {
        let Report::Ran(run) = Test::NandXnor.run(blocks) else {
            return None;
        };
        let bits = interface::read(blocks).inputs.first()?.bits.len();
        (run.wrong == 0 && bits == width * copies).then_some(run.cases)
    };
    Some(stacking::into_bits(
        data,
        reach,
        width,
        buses,
        works,
        "No step west or up, as far as it reaches, makes copies whose bits \
         together give A nand B and A xnor B.",
    ))
}

/// How copies of the `gate` in `data` stack west into a wider one, each
/// adding its bits to A, B, and the output, as the inverter's do; `None`
/// when it is not one.
pub fn stacking(data: &[u8], gate: Gate) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    spec(&interface, gate).ok()?;
    let width = interface.inputs[0].bits.len();
    let reach = [0, 1].map(|axis| i64::from(blocks.size[axis]));
    let test = gate.test();
    let buses = stack::Buses::of(test.name(), Orientation::Horizontal);
    let works = |blocks: &Blocks, copies: usize| {
        let Report::Ran(run) = test.run(blocks) else {
            return None;
        };
        let bits = interface::read(blocks).inputs.first()?.bits.len();
        (run.wrong == 0 && bits == width * copies).then_some(run.cases)
    };
    let [plain, inverse] = gate.patterns();
    Some(stacking::into_bits(
        data,
        reach,
        width,
        buses,
        works,
        &format!(
            "No step west or up, as far as it reaches, makes copies whose \
             bits together give {plain}, or {inverse}."
        ),
    ))
}
