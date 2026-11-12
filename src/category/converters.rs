//! Testing stateless builds of one input, A, that convert it: to binary-
//! coded decimal and back, to its absolute value, and to how many of its
//! bits are on, as a popcount and a threshold read it, and whether that
//! is odd, as a parity circuit does; and from hex digits, each a block's
//! signal strength, to binary.

use anyhow::{Result, bail};

use crate::{
    interface::{Interface, Port},
    simulate::{
        Broken, Reading, Value,
        bench::{
            Bench, Case, Cases, Distinct, EXHAUSTIVE_BITS, Kind, Spec, mask,
            patterns,
        },
        broken,
        circuit::output_bits,
    },
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
    let needed = (Value::BITS - to_bcd(largest).leading_zeros()) as usize;
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
    at_least_two(width, "An absolute value")?;
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: vec![Kind::new("|A|", move |case| absolute(case[0], width))],
        by_digit: false,
    })
}

/// The fewest bits a popcount counts.
const MIN_POPCOUNT: usize = 4;

/// Refuses `what`, a build of A alone, for an A of `width` bits under two:
/// one bit's absolute value, parity, or threshold is the bit itself, as a
/// buffer's is.
fn at_least_two(width: usize, what: &str) -> Result<()> {
    if width < 2 {
        broken!("{what} takes at least 2 bits, but A has {width}.");
    }
    Ok(())
}

/// A popcount's every value of A, or with too many, each byte's and the
/// test patterns: how many of A's bits are on, on outputs wide enough to
/// hold A's width, in binary, or as the strength of its one output block,
/// as one with too few blocks for the binary count shows it.
pub(super) fn popcount(interface: &mut Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "popcount")?;
    let width = bench.widths(interface)[0];
    // Fewer, and its count is little more than its inputs: one bit's is
    // the bit itself, as a buffer's.
    if width < MIN_POPCOUNT {
        broken!(
            "A popcount counts at least {MIN_POPCOUNT} bits, but A has \
             {width}."
        );
    }
    let needed = usize::BITS - width.leading_zeros();
    if let &[out] = bench.outputs.as_slice()
        && interface.outputs[out].bits.len() == 1
        && needed > 1
        && width <= 15
    {
        interface.outputs[out].signal_strength = true;
    }
    let has: usize = bench
        .outputs
        .iter()
        .map(|&i| output_bits(interface, &interface.outputs[i]))
        .sum();
    if has < needed as usize {
        broken!(
            "A has {width} bits, so its count takes {needed}, but the \
             outputs have {has}."
        );
    }
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: vec![Kind::new("ones in A", |case| {
            Value::from(case[0].count_ones())
        })],
        by_digit: false,
    })
}

/// A binary-to-hex converter's every value of A: A as the strength of
/// its one output block, from 0 to 15.
pub(super) fn binary_to_hex(interface: &mut Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "binary-to-hex converter")?;
    let width = bench.widths(interface)[0];
    let &[out] = bench.outputs.as_slice() else {
        broken!("A binary-to-hex converter has one output block, its digit.");
    };
    if interface.outputs[out].bits.len() != 1 || width > 4 {
        broken!(
            "A binary-to-hex converter shows A, of at most 4 bits, as the \
             strength of one output block, but this has {} bits and {} \
             output blocks.",
            width,
            interface.outputs[out].bits.len()
        );
    }
    interface.outputs[out].signal_strength = true;
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: vec![Kind::new("A as a strength", |case| case[0])],
        by_digit: false,
    })
}

/// A hex-to-binary converter's every value of A, its digit inputs, a hex
/// digit a block as each block's strength: A in binary, four bits a digit,
/// on outputs that give a bit a block.
pub(super) fn hex_to_binary(interface: &mut Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "hex-to-binary converter")?;
    if !interface.inputs[0].signal_strength {
        broken!(
            "A hex-to-binary converter takes a hex digit a block as its \
             strength, through a comparator with air behind, but A has \
             levers."
        );
    }
    interface.binary_outputs = true;
    let width = bench.widths(interface)[0];
    let has: usize = bench
        .outputs
        .iter()
        .map(|&i| output_bits(interface, &interface.outputs[i]))
        .sum();
    if has < width {
        broken!(
            "A has {} hex digits, so {width} bits, but the outputs have {has}.",
            width / 4
        );
    }
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: vec![Kind::new("A in binary", |case| case[0])],
        by_digit: false,
    })
}

/// A threshold's cases, at least `k` of A's bits on, as they are named.
pub(super) fn threshold_pattern(k: u32) -> String {
    format!("at least {k} of A on")
}

/// A threshold's every value of A, or with too many, each byte's and the
/// test patterns: whether at least some number of A's bits are on, on its
/// one output, the number whichever of 1 to A's width it gets right.
pub(super) fn threshold(interface: &Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "threshold")?;
    let width = bench.widths(interface)[0];
    at_least_two(width, "A threshold")?;
    if bench.outputs.len() != 1
        || interface.outputs[bench.outputs[0]].bits.len() != 1
    {
        broken!("A threshold has one output bit.");
    }
    let most = u32::try_from(width).unwrap_or(u32::MAX);
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: (1..=most)
            .map(|k| {
                Kind::new(&threshold_pattern(k), move |case| {
                    Value::from(case[0].count_ones() >= k)
                })
                .reading(Reading::Threshold { at_least: k })
            })
            .collect(),
        by_digit: false,
    })
}

/// A parity build's every value of A, or with too many, each byte's and
/// the test patterns: whether an odd number of A's bits are on, the xor of
/// them all, on its one output. One on for an even number is not one.
pub(super) fn parity(interface: &Interface) -> Result<Spec> {
    let bench = one_input_bench(interface, "parity circuit")?;
    let width = bench.widths(interface)[0];
    at_least_two(width, "A parity circuit")?;
    if bench.outputs.len() != 1
        || interface.outputs[bench.outputs[0]].bits.len() != 1
    {
        broken!("A parity circuit has one output bit.");
    }
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || byte_cases(width))),
        kinds: vec![Kind::new("xor of A's bits", |case| {
            Value::from(case[0].count_ones() % 2 == 1)
        })],
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
    let control = |port: &&Port| port.control();
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
        .filter(|&i| !interface.outputs[i].status())
        .collect();
    if outputs.is_empty() {
        broken!("A {what} has an output, but this has none.");
    }
    Ok(Bench::new(vec![0], outputs, ""))
}

/// `value` in binary-coded decimal: each decimal digit in four bits, the
/// ones lowest.
fn to_bcd(mut value: Value) -> Value {
    let mut bcd = 0;
    let mut shift = 0;
    while value > 0 && shift < Value::BITS {
        bcd |= (value % 10) << shift;
        value /= 10;
        shift += 4;
    }
    bcd
}

/// The number binary-coded decimal `bcd` holds, its digits being decimal.
fn from_bcd(bcd: Value) -> Value {
    (0..Value::BITS / 4)
        .rev()
        .fold(0, |value, digit| value * 10 + (bcd >> (4 * digit) & 0xf))
}

/// Whether `value` fits in `bits` bits.
fn fits(value: Value, bits: usize) -> bool {
    bits >= Value::BITS as usize || value >> bits == 0
}

/// The binary-coded decimal values of `width` bits whose numbers fit in
/// `result` bits, in order, with whether they are every one: every one
/// when there are few enough and not `quick`, else every number below
/// 1,000, each digit's every value alone, each run of nines, and the
/// largest.
fn bcd_values(width: usize, result: usize, quick: bool) -> (Vec<Value>, bool) {
    let valid = |n: Value| fits(to_bcd(n), width) && fits(n, result);
    if !quick {
        // Both tests rise with the number, so the valid ones run from 0.
        let every: Vec<Value> = (0..=1 << EXHAUSTIVE_BITS)
            .take_while(|&n| valid(n))
            .map(to_bcd)
            .collect();
        if every.len() <= 1 << EXHAUSTIVE_BITS {
            return (every, true);
        }
    }
    let mut numbers: Vec<Value> = (0..1000).collect();
    let mut power: Value = 1;
    while let Some(next) = power.checked_mul(10) {
        numbers.extend((1..10).map(|d| d * power));
        numbers.push(next - 1);
        power = next;
    }
    // The largest valid number, found digit by digit from the top.
    let mut largest: Value = 0;
    for digit in (0..power.ilog10()).rev() {
        for d in (0..10).rev() {
            let n = largest + d * Value::pow(10, digit);
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
fn absolute(a: Value, width: usize) -> Value {
    if width > 0 && a >> (width - 1) & 1 == 1 {
        a.wrapping_neg() & mask(width)
    } else {
        a
    }
}

/// The values of A a one-input build too wide to try every one of is
/// checked on: every value of each byte, the other bits clear, and the test
/// patterns, with how many are patterns.
pub(super) fn byte_cases(width: usize) -> (Vec<Case>, u64) {
    let mut cases = Distinct::default();
    for low in (0..width).step_by(8) {
        for v in 0..1 << (width - low).min(8) {
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
    let mut power: Value = 1;
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
    use crate::{
        schem::lines::Role,
        simulate::bench::testing::{judged, port},
    };

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

    /// A popcount counts at least four bits: one of fewer is refused, as
    /// a one-bit one's count is the bit itself, as a buffer's is.
    #[test]
    fn a_popcount_counts_at_least_four_bits() {
        let counted = |bits: usize| {
            popcount(&mut Interface {
                inputs: vec![port("A", Role::Operand(0), bits)],
                outputs: vec![port("output", Role::Output(0), 3)],
                binary_outputs: false,
            })
        };
        for bits in 1..4 {
            assert!(counted(bits).is_err(), "{bits}");
        }
        assert!(counted(4).is_ok());
    }

    /// A parity circuit is on for an odd number of A's bits on; its
    /// inverse, on for an even number, gets every case wrong.
    #[test]
    fn a_parity_circuit_is_odd() {
        let interface = || {
            (
                vec![port("A", Role::Operand(0), 4)],
                vec![port("output", Role::Output(0), 1)],
            )
        };
        let (inputs, outputs) = interface();
        let spec = parity(&Interface {
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            binary_outputs: false,
        })
        .unwrap();
        let odd = |case: &Case| Value::from(case[0].count_ones() % 2 == 1);
        let run = judged(inputs, outputs, &spec.kinds, odd);
        assert_eq!((run.wrong, run.cases), (0, 16));
        assert_eq!(run.pattern, "xor of A's bits");
        let (inputs, outputs) = interface();
        let run = judged(inputs, outputs, &spec.kinds, |case| odd(case) ^ 1);
        assert_eq!((run.wrong, run.cases), (16, 16));
    }
}
