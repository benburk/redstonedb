//! Testing a multiplier: A times B, plus C for a fused multiply-add,
//! stateless, or sequential with one control line, its calculate, pressed
//! once its operands are set.

use crate::simulate::{Rule, untested};
use anyhow::Result;

use crate::{
    category::test::BySpec,
    interface::{self, Interface},
    simulate::{
        Value,
        bench::{Bench, Case, Distinct, Kind, Spec, mask, patterns},
        broken,
    },
};

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
        _ => untested!(
            Rule::TooManyControlLines;
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
        cases: Box::new(move || multiplier_cases(&widths)),
        kinds: vec![Kind::new(pattern, product)],
        by_digit: true,
    })
}

/// How many products of operands with no pattern a wide multiplier is
/// checked on, besides those [`multiplier_cases`] lays out.
const SCATTERED: usize = 1024;

/// The products a multiplier too wide to try every one of is checked on,
/// chosen for how one goes wrong: a partial product wired to the wrong
/// place, an adder in its tree, or a carry that stops short. Every value
/// of each hex digit of A with every value of each hex digit of B, the
/// other bits of both clear, so each partial product and the adders
/// summing it are tried alone, then all set, so the carries out of them
/// run the length of the tree; every value of each byte of C, if any,
/// with A and B clear or all set; every pairing of the test patterns,
/// each bit alone set and clear and runs of ones; and [`SCATTERED`]
/// products of operands with no pattern, the same every run. That is
/// some 24 thousand for 24 bits by 24, where every value of each byte of
/// A with each byte of B was some 590 thousand, and it grows with the
/// square of the width in digits, not bytes.
fn multiplier_cases(widths: &[usize]) -> (Vec<Case>, u64) {
    let mut cases = Distinct::default();
    let pieces = |width: usize, piece: usize| {
        (0..width)
            .step_by(piece)
            .map(move |low| (low, (width - low).min(piece)))
    };
    let full = [mask(widths[0]), mask(widths[1])];
    for rest in [[0, 0], full] {
        for (low_a, bits_a) in pieces(widths[0], 4) {
            for (low_b, bits_b) in pieces(widths[1], 4) {
                let around = [
                    rest[0] & !(mask(bits_a) << low_a),
                    rest[1] & !(mask(bits_b) << low_b),
                ];
                for a in 0..1 << bits_a {
                    for b in 0..1 << bits_b {
                        cases.add([
                            around[0] | a << low_a,
                            around[1] | b << low_b,
                            0,
                        ]);
                    }
                }
            }
        }
    }
    let addend = widths.get(2).copied();
    if let Some(width) = addend {
        for (low, bits) in pieces(width, 8) {
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
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..SCATTERED {
        let mut case = [0; 3];
        for (value, &width) in case.iter_mut().zip(widths) {
            *value = Value::from(splitmix(&mut seed)) & mask(width);
        }
        cases.add(case);
    }
    (cases.cases, paired)
}

/// The next of a fixed run of numbers with no pattern, from `seed`.
fn splitmix(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A multiplier's test: its lone control line is named for its role,
/// calculate.
pub(super) const MULTIPLIER: BySpec = BySpec::spec(|_, interface| {
    interface::name_lines(super::Kind::Multiplier.lines(), interface);
    spec(interface)
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_multiplier_pairs_every_digit_of_a_with_every_digit_of_b() {
        let (cases, paired) = multiplier_cases(&[16, 16]);
        // Each pair of digits, the rest clear, then the rest set.
        assert!(cases.contains(&[0x0A00, 0x00B0, 0]));
        assert!(cases.contains(&[0xFAFF, 0xFFBF, 0]));
        assert!(cases.contains(&[0xFFFF, 0xFFFF, 0]));
        // Each bit alone, set and clear, against all set.
        assert!(cases.contains(&[0x8000, 0xFFFF, 0]));
        assert!(cases.contains(&[0xFFFF, 0x7FFF, 0]));
        // Pairings the digits already tried are counted once, as theirs.
        assert!(paired > 0 && paired <= patterns(16).len().pow(2) as u64);
        // Far fewer than each byte of A with each byte of B.
        let (wide, _) = multiplier_cases(&[24, 24]);
        assert!(wide.len() < 30_000, "{}", wide.len());
        // The same cases every run, so a result can be run again.
        assert_eq!(wide, multiplier_cases(&[24, 24]).0);
        // A × B + C: each byte of C, with A and B clear and all set.
        let (cases, _) = multiplier_cases(&[8, 8, 16]);
        assert!(cases.contains(&[0, 0, 0xAB00]));
        assert!(cases.contains(&[0xFF, 0xFF, 0x00CD]));
    }
}
