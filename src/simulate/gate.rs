//! Testing an XOR or XNOR gate: two operand buses of one width, A and B,
//! into one output as wide, each bit A xor B, or its inverse for an XNOR.
//! It has no lines, so copies stacked side by side add bits.

use anyhow::Result;

use super::{
    Broken, Report, Stacking, Test,
    adder::adder_cases,
    bench::{Bench, Cases, Hint, Kind, Spec, mask},
    stacking,
};
use crate::{
    interface::{self, Interface},
    models::schematic::Orientation,
    schem::Blocks,
    stack,
};

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
        hint: Hint::None,
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
    let (low, high) = interface::measured(&blocks);
    let reach = i64::from(high[0] - low[0] + 1) + 2;
    let buses =
        stack::Buses::of(Test::NandXnor.name(), Orientation::Horizontal);
    let works = |blocks: &Blocks, copies: usize| {
        let Report::Ran(run) = Test::NandXnor.run(blocks) else {
            return None;
        };
        let bits = interface::read(blocks).inputs.first()?.bits.len();
        (run.wrong == 0 && bits == width * copies).then_some(run.cases)
    };
    Some(stacking::west_into_bits(
        data,
        reach,
        width,
        buses,
        works,
        "No step west, up to two past its width, makes copies whose bits \
         together give A nand B and A xnor B.",
    ))
}

/// How copies of the gate in `data` stack west into a wider one, each
/// adding its bits to A, B, and the output, as the inverter's do; `None`
/// when it is not a gate.
pub fn stacking(data: &[u8]) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    spec(&interface).ok()?;
    let width = interface.inputs[0].bits.len();
    let (low, high) = interface::measured(&blocks);
    let reach = i64::from(high[0] - low[0] + 1) + 2;
    let buses = stack::Buses::of(Test::XorXnor.name(), Orientation::Horizontal);
    let works = |blocks: &Blocks, copies: usize| {
        let Report::Ran(run) = Test::XorXnor.run(blocks) else {
            return None;
        };
        let bits = interface::read(blocks).inputs.first()?.bits.len();
        (run.wrong == 0 && bits == width * copies).then_some(run.cases)
    };
    Some(stacking::west_into_bits(
        data,
        reach,
        width,
        buses,
        works,
        "No step west, up to two past its width, makes copies whose bits \
         together give A xor B, or A xnor B.",
    ))
}
