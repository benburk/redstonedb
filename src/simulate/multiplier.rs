//! Testing a multiplier: A times B, plus C for a fused multiply-add,
//! stateless, or sequential with one control line, its calculate, pressed
//! once its operands are set.

use anyhow::{Result, bail};

use super::{
    Broken,
    bench::{Bench, Case, Cases, Distinct, Kind, Spec, mask, patterns},
};
use crate::interface::Interface;

/// A multiplier's every product, A × B, plus C when there is one, checked
/// on as many low bits as its outputs hold together, the first holding the
/// lowest; status lines are left out. A sequential one has one control
/// line, its calculate, pressed once the operands are set, whatever its
/// sign says; one with more, such as a reset, isn't tested.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let control = |i: &usize| interface.inputs[*i].control();
    let lines: Vec<usize> =
        (0..interface.inputs.len()).filter(control).collect();
    let calculate = match lines.as_slice() {
        [] => None,
        [line] if interface.inputs[*line].bits.len() == 1 => Some(*line),
        _ => bail!(
            "Too many control lines: {} (a multiplier takes one, \
             calculate).",
            lines
                .iter()
                .map(|&i| interface.inputs[i].name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let inputs: Vec<usize> = (0..interface.inputs.len())
        .filter(|i| !control(i))
        .collect();
    let outputs: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| !interface.outputs[i].status())
        .collect();
    let widths: Vec<usize> = inputs
        .iter()
        .map(|&i| interface.inputs[i].bits.len())
        .collect();
    let pattern = match widths.len() {
        2 => "A × B",
        3 => "A × B + C",
        n => broken!(
            "A multiplier takes two operands, or three for A × B + C, but \
             this has {n}."
        ),
    };
    let result: usize = outputs
        .iter()
        .map(|&i| interface.outputs[i].bits.len())
        .sum();
    let (a, b) = (widths[0], widths[1]);
    if result == 0 || result < a.max(b) || result > a + b {
        broken!(
            "A and B have {a} and {b} bits, so the product has from {} to {} \
             bits, but the outputs have {result}.",
            a.max(b),
            a + b
        );
    }
    // Exact for operands of up to 64 bits each, and the low bits are all
    // that is compared.
    let product =
        |case: &Case| case[0].wrapping_mul(case[1]).wrapping_add(case[2]);
    let mut bench = Bench::new(inputs, outputs, pattern);
    bench.press = calculate;
    let widths = bench.widths(interface);
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || multiplier_cases(&widths))),
        kinds: vec![Kind::new(pattern, product)],
        by_digit: false,
    })
}

/// The products a multiplier too wide to try every one of is checked on:
/// every value of each byte of A with every value of each byte of B, the
/// other bits clear; every value of each byte of C, if any, with A and B
/// clear or all set; and every pairing of the test patterns.
fn multiplier_cases(widths: &[usize]) -> (Vec<Case>, u64) {
    let mut cases = Distinct::default();
    let bytes = |width: usize| {
        (0..width)
            .step_by(8)
            .map(move |low| (low, (width - low).min(8)))
    };
    for (low_a, bits_a) in bytes(widths[0]) {
        for (low_b, bits_b) in bytes(widths[1]) {
            for a in 0..1 << bits_a {
                for b in 0..1 << bits_b {
                    cases.add([a << low_a, b << low_b, 0]);
                }
            }
        }
    }
    let addend = widths.get(2).copied();
    if let Some(width) = addend {
        let full = [mask(widths[0]), mask(widths[1])];
        for (low, bits) in bytes(width) {
            for c in 0..1 << bits {
                cases.add([0, 0, c << low]);
                cases.add([full[0], full[1], c << low]);
            }
        }
    }
    let before = cases.len();
    let addends = addend.map_or_else(|| vec![0], patterns);
    for &a in &patterns(widths[0]) {
        for &b in &patterns(widths[1]) {
            for &c in &addends {
                cases.add([a, b, c]);
            }
        }
    }
    let paired = (cases.len() - before) as u64;
    (cases.cases, paired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_multiplier_pairs_every_byte_of_a_with_every_byte_of_b() {
        let (cases, paired) = multiplier_cases(&[16, 16]);
        let bytes = cases.len() as u64 - paired;
        // Four pairs of bytes, less the cases they share, such as 0 × 0.
        assert!(bytes > 4 * (1 << 16) - 1024 && bytes <= 4 * (1 << 16));
        assert!(cases.contains(&[0xFF00, 0x00FF, 0]));
        assert!(cases.contains(&[0xFFFF, 0xFFFF, 0]));
        // A × B + C: each byte of C, with A and B clear and all set.
        let (cases, _) = multiplier_cases(&[8, 8, 16]);
        assert!(cases.contains(&[0, 0, 0xAB00]));
        assert!(cases.contains(&[0xFF, 0xFF, 0x00CD]));
    }
}
