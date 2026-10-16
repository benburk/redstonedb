//! Testing stateless builds of one input, A, that convert it: to binary-
//! coded decimal and back, and to its absolute value.

use anyhow::{Result, bail};

use super::{
    Broken,
    bench::{Bench, Case, Cases, Distinct, EXHAUSTIVE_BITS, Hint, Kind, Spec},
    bench::{mask, patterns},
};
use crate::{
    interface::{Interface, Port},
    lint,
};

/// A binary-to-BCD converter's every value of A, or with too many, each
/// byte's, the test patterns, and each power of ten and the number before
/// it, giving A in BCD, the ones digit lowest. Outputs too few for A's
/// largest value in BCD make it broken.
pub(super) fn binary_to_bcd(interface: &Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "converter")?;
    let width = bench.widths(interface)[0];
    let result: usize = bench.output_widths(interface).iter().sum();
    let largest = mask(width);
    let needed = 64 - to_bcd(largest).leading_zeros() as usize;
    if result < needed {
        broken!(
            "A has {width} bits, so up to {largest}, which takes {needed} \
             bits in BCD, but the outputs have {result}."
        );
    }
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || binary_cases(width))),
        kinds: vec![Kind::new("A in BCD", |case| to_bcd(case[0]))],
        hint: Hint::SavedOn,
        by_digit: false,
    })
}

/// A BCD-to-binary converter's every value of A whose digits are all
/// decimal and whose number the outputs can hold, or with too many, a
/// selection, giving A's number in binary.
pub(super) fn bcd_to_binary(interface: &Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "converter")?;
    let width = bench.widths(interface)[0];
    let result: usize = bench.output_widths(interface).iter().sum();
    Ok(Spec {
        bench,
        cases: Cases::Only(Box::new(move |quick| {
            bcd_values(width, result, quick)
        })),
        kinds: vec![Kind::new("A from BCD", |case| from_bcd(case[0]))],
        hint: Hint::SavedOn,
        by_digit: false,
    })
}

/// An absolute-value build's every value of A, or with too many, each
/// byte's and the test patterns: A taken as a signed number, two's
/// complement, and its magnitude, unsigned in as many bits as A, so the
/// most negative A, which has no positive, gives itself, as negating it
/// does; checked on as many low bits as the outputs hold.
pub(super) fn absolute_value(interface: &Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "absolute value")?;
    let width = bench.widths(interface)[0];
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: vec![Kind::new("|A|", move |case| absolute(case[0], width))],
        hint: Hint::SavedOn,
        by_digit: false,
    })
}

/// What a stateless build of one input, A, is tested on, named `what` in
/// what is wrong with it: A, and its outputs read together, status lines
/// left out, the first holding the lowest digits or bits. One with a
/// control line, such as a clock, is sequential, which these tests do not
/// cover.
pub(super) fn one_input_bench(
    interface: &Interface,
    what: &str,
) -> Result<Bench> {
    let control = |port: &&Port| port.block.starts_with(lint::CONTROL);
    if let Some(line) = interface.inputs.iter().find(control) {
        bail!(
            "It has a control line, {}, so it is stateful; only a stateless \
             {what} is tested.",
            line.name
        );
    }
    if interface.inputs.len() != 1 {
        broken!(
            "A {what} takes one input, A, but this has {}.",
            interface.inputs.len()
        );
    }
    let outputs: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| !interface.outputs[i].block.starts_with(lint::STATUS))
        .collect();
    if outputs.is_empty() {
        broken!("A {what} has an output, but this has none.");
    }
    Ok(Bench::new(vec![0], outputs, ""))
}

/// `value` in binary-coded decimal: each decimal digit in four bits, the
/// ones lowest.
fn to_bcd(mut value: u64) -> u64 {
    let mut bcd = 0;
    let mut shift = 0;
    while value > 0 && shift < 64 {
        bcd |= (value % 10) << shift;
        value /= 10;
        shift += 4;
    }
    bcd
}

/// The number binary-coded decimal `bcd` holds, its digits being decimal.
fn from_bcd(bcd: u64) -> u64 {
    (0..16)
        .rev()
        .fold(0, |value, digit| value * 10 + (bcd >> (4 * digit) & 0xf))
}

/// Whether `value` fits in `bits` bits.
fn fits(value: u64, bits: usize) -> bool {
    bits >= 64 || value >> bits == 0
}

/// The binary-coded decimal values of `width` bits whose numbers fit in
/// `result` bits, in order, with whether they are every one: every one
/// when there are few enough and not `quick`, else every number below
/// 1,000, each digit's every value alone, each run of nines, and the
/// largest.
fn bcd_values(width: usize, result: usize, quick: bool) -> (Vec<u64>, bool) {
    let valid = |n: u64| fits(to_bcd(n), width) && fits(n, result);
    if !quick {
        // Both tests rise with the number, so the valid ones run from 0.
        let every: Vec<u64> = (0..=1u64 << EXHAUSTIVE_BITS)
            .take_while(|&n| valid(n))
            .map(to_bcd)
            .collect();
        if every.len() <= 1 << EXHAUSTIVE_BITS {
            return (every, true);
        }
    }
    let mut numbers: Vec<u64> = (0..1000).collect();
    let mut power = 1u64;
    while let Some(next) = power.checked_mul(10) {
        numbers.extend((1..10).map(|d| d * power));
        numbers.push(next - 1);
        power = next;
    }
    // The largest valid number, found digit by digit from the top.
    let mut largest = 0u64;
    for digit in (0..16).rev() {
        for d in (0..10).rev() {
            let n = largest + d * 10u64.pow(digit);
            if valid(n) {
                largest = n;
                break;
            }
        }
    }
    numbers.push(largest);
    numbers.retain(|&n| valid(n));
    numbers.sort_unstable();
    numbers.dedup();
    (numbers.into_iter().map(to_bcd).collect(), false)
}

/// The magnitude of `a`, a signed number of `width` bits, unsigned in as
/// many bits: `a` when its top bit is clear, else its negative, which for
/// the most negative is itself.
fn absolute(a: u64, width: usize) -> u64 {
    if width > 0 && a >> (width - 1) & 1 == 1 {
        a.wrapping_neg() & mask(width)
    } else {
        a
    }
}

/// The values of A a one-input build too wide to try every one of is
/// checked on: every value of each byte, the other bits clear, and the test
/// patterns, with how many are patterns.
fn byte_cases(width: usize) -> (Vec<Case>, u64) {
    let mut cases = Distinct::default();
    for low in (0..width).step_by(8) {
        for v in 0..1u64 << (width - low).min(8) {
            cases.add([v << low, 0, 0]);
        }
    }
    let bytes = cases.len();
    for a in patterns(width) {
        cases.add([a, 0, 0]);
    }
    let paired = (cases.len() - bytes) as u64;
    (cases.cases, paired)
}

/// The values a binary-to-BCD converter too wide to try every one of is
/// checked on: `byte_cases`, and each power of ten and the number before
/// it, as far as A holds them.
fn binary_cases(width: usize) -> (Vec<Case>, u64) {
    let (bytes, patterns) = byte_cases(width);
    let mut cases = Distinct::default();
    for case in bytes {
        cases.add(case);
    }
    let before = cases.len();
    let mut power = 1u64;
    while fits(power, width) {
        cases.add([power, 0, 0]);
        cases.add([power - 1, 0, 0]);
        match power.checked_mul(10) {
            Some(next) => power = next,
            None => break,
        }
    }
    let rest = patterns + (cases.len() - before) as u64;
    (cases.cases, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_negative_input_is_its_own_absolute_value() {
        assert_eq!(absolute(5, 8), 5);
        assert_eq!(absolute(0xff, 8), 1);
        assert_eq!(absolute(0x81, 8), 0x7f);
        assert_eq!(absolute(0x80, 8), 0x80);
    }

    #[test]
    fn bcd_holds_each_decimal_digit_in_four_bits() {
        assert_eq!(to_bcd(0), 0);
        assert_eq!(to_bcd(255), 0x255);
        assert_eq!(from_bcd(0x255), 255);
        assert_eq!(from_bcd(to_bcd(4_294_967_295)), 4_294_967_295);
    }

    #[test]
    fn a_bcd_input_takes_only_decimal_digits_that_its_outputs_hold() {
        // Ten bits of BCD hold up to 399, but eight output bits only 255.
        let (values, every) = bcd_values(10, 8, false);
        assert!(every);
        assert_eq!(values.len(), 256);
        assert_eq!((values[0], values[255]), (0, 0x255));
        assert!(values.contains(&0x199) && !values.contains(&0x1a0));
        // Too many to try every one: a selection, the largest of the eight
        // digits among them.
        let (values, every) = bcd_values(32, 27, false);
        assert!(!every);
        assert_eq!(values.last(), Some(&to_bcd(99_999_999)));
        assert!(values.iter().all(|&v| from_bcd(v) < 1 << 27));
    }
}
