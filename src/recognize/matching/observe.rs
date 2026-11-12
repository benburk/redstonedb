//! Driving a build once, before anything is known of what its lines are
//! for, and keeping what each run of outputs shows. A build of few levers
//! is driven every way. Any other has its buses driven through a few
//! patterns of their widths, see [`patterns`], every pairing of them, under
//! each setting of its lone lines, which may choose an operation, as an
//! ALU's control lines do, and carry in. Which run is A and which way round is decided later,
//! by reading the values kept, so a build is compiled and run once however
//! many ways its lines could be read.

use anyhow::{Result, bail};

use crate::{
    interface::{self, Bit, Interface, Port, inspect},
    schem::{Blocks, Pos},
    simulate::{Circuit, Value, mask},
};

/// What a build did: its runs of inputs and of outputs, each run's bits in
/// its order, and each case, the value driven on each input run and the
/// value each output run showed, a run's first bit its value's lowest.
pub struct Observation {
    pub inputs: Vec<Vec<Pos>>,
    pub outputs: Vec<Vec<Pos>>,
    pub cases: Vec<(Vec<Value>, Vec<Value>)>,
}

/// The most levers a build may have to be driven in every combination of
/// them.
const EXHAUSTIVE_BITS: usize = 12;

/// The longest run of a build of more levers driven as lone lines, each
/// setting of them, rather than as a bus.
const SHORT_RUN: usize = 3;

/// The most lone lines driven in every setting of them; past it, each
/// setting with at most [`MOST_ON`] of them on.
pub(super) const EVERY_SETTING: usize = 6;

/// The most lone lines on together in a setting, past [`EVERY_SETTING`].
const MOST_ON: u32 = 3;

/// The most cases a build is observed in: past it, the pairings of its
/// buses' patterns are sampled evenly under each setting of its lone
/// lines.
const MOST_CASES: usize = 1 << 14;

/// The build in `blocks` driven as [`plan`] plans for `report`'s levers,
/// with what its outputs showed; or why it couldn't be, as for a build of
/// buttons, which are pressed in time rather than held.
pub fn observe(
    blocks: &Blocks,
    report: &inspect::Report,
) -> Result<Observation> {
    let switches = interface::switches(blocks);
    if report
        .inputs
        .iter()
        .flat_map(|run| &run.bits)
        .any(|pos| switches.get(pos).is_none_or(|switch| switch.button))
    {
        bail!("A button is pressed, not held, so it is observed in time.");
    }
    if !report.digits.is_empty() {
        bail!("A signal-strength input takes a digit, not a bit.");
    }
    let port = |i: usize, bits: Vec<Bit>| Port {
        name: format!("line {}", i + 1),
        role: None,
        labelled: false,
        bits,
        strength: 1,
        words: Vec::new(),
        signal_strength: false,
    };
    let inputs: Vec<Vec<Pos>> =
        report.inputs.iter().map(|run| run.bits.clone()).collect();
    let outputs: Vec<Vec<Pos>> =
        report.outputs.iter().map(|run| run.bits.clone()).collect();
    let interface = Interface {
        inputs: inputs
            .iter()
            .enumerate()
            .map(|(i, bits)| {
                let bits = bits
                    .iter()
                    .map(|&pos| Bit::input(blocks, &switches, pos))
                    .collect();
                port(i, bits)
            })
            .collect(),
        outputs: outputs
            .iter()
            .enumerate()
            .map(|(i, bits)| {
                port(i, bits.iter().copied().map(Bit::at).collect())
            })
            .collect(),
        binary_outputs: false,
    };
    let mut circuit = Circuit::new(blocks, &interface)?;
    let mut cases = Vec::new();
    for driven in plan(&inputs) {
        for (port, &value) in driven.iter().enumerate() {
            circuit.set(port, value);
        }
        circuit.settle()?;
        let shown = (0..outputs.len()).map(|port| circuit.get(port)).collect();
        cases.push((driven, shown));
    }
    Ok(Observation {
        inputs,
        outputs,
        cases,
    })
}

/// The cases a build of runs of levers `inputs` is driven in, each the
/// value of every run: every combination of its levers, when they number
/// at most [`EXHAUSTIVE_BITS`]; else, under each setting of its lone
/// lines, a run of at most [`SHORT_RUN`] counting as lone lines, every
/// pairing of its buses' patterns, or an even sample of them past
/// [`MOST_CASES`].
fn plan(inputs: &[Vec<Pos>]) -> Vec<Vec<Value>> {
    if inputs.iter().map(Vec::len).sum::<usize>() <= EXHAUSTIVE_BITS {
        let every: Vec<Vec<Value>> = inputs
            .iter()
            .map(|bits| (0..1 << bits.len()).collect())
            .collect();
        return (0..pairings(&every)).map(|i| pairing(&every, i)).collect();
    }
    let lone = |bits: &Vec<Pos>| bits.len() <= SHORT_RUN;
    // Each lone line, by its run and its place along it.
    let lines: Vec<(usize, usize)> = inputs
        .iter()
        .enumerate()
        .filter(|(_, bits)| lone(bits))
        .flat_map(|(run, bits)| (0..bits.len()).map(move |bit| (run, bit)))
        .collect();
    let settings: Vec<Value> = if lines.len() <= EVERY_SETTING {
        (0..1 << lines.len()).collect()
    } else {
        few_on(lines.len())
    };
    let buses: Vec<usize> = (0..inputs.len())
        .filter(|&run| !lone(&inputs[run]))
        .collect();
    let patterns: Vec<Vec<Value>> = buses
        .iter()
        .map(|&run| patterns(inputs[run].len()))
        .collect();
    let total = pairings(&patterns);
    let taken = total.min((MOST_CASES / settings.len()).max(1));
    let mut cases = Vec::new();
    for setting in settings {
        for i in 0..taken {
            let values = pairing(&patterns, i * total / taken);
            let mut driven = vec![0; inputs.len()];
            for (&run, value) in buses.iter().zip(values) {
                driven[run] = value;
            }
            for (k, &(run, bit)) in lines.iter().enumerate() {
                driven[run] |= (setting >> k & 1) << bit;
            }
            cases.push(driven);
        }
    }
    cases
}

/// The values of `width` bits a bus is driven through: none set; each bit's
/// place written in binary down the bits, `1010…`, `1100…`, `11110000…`,
/// each place's bit `k` a pattern of runs `2^k` long; and each of those
/// inverted. Every bit reads its own column across them, its place and
/// its place inverted, so whichever order a bus's bits are in shows, and
/// together with a carry in, all of them set and a carry ripple the bus's
/// length.
pub fn patterns(width: usize) -> Vec<Value> {
    let all = mask(width);
    let mut values = vec![0, all];
    let mut run = 1;
    while run < width {
        let pattern = (0..width)
            .filter(|bit| bit / run % 2 == 1)
            .fold(0, |value, bit| value | 1 << bit);
        values.extend([pattern, !pattern & all]);
        run *= 2;
    }
    values.sort_unstable();
    values.dedup();
    values
}

/// Every setting of `lines` lone lines with at most [`MOST_ON`] of them on,
/// a bit a line, the fewest on first.
fn few_on(lines: usize) -> Vec<Value> {
    let mut settings: Vec<Value> = vec![0];
    for _ in 0..MOST_ON {
        let more: Vec<Value> = settings
            .iter()
            .flat_map(|&setting| {
                // Each line above the highest already on, so each setting
                // comes once.
                let from = Value::BITS - setting.leading_zeros();
                (from as usize..lines).map(move |line| setting | 1 << line)
            })
            .filter(|setting| !settings.contains(setting))
            .collect();
        settings.extend(more);
    }
    settings.sort_by_key(|setting| (setting.count_ones(), *setting));
    settings.dedup();
    settings
}

/// How many pairings there are of `choices`, one value of each.
fn pairings(choices: &[Vec<Value>]) -> usize {
    choices.iter().map(Vec::len).product()
}

/// The pairing at place `i` among every pairing of `choices`, one value of
/// each, the first changing fastest.
fn pairing(choices: &[Vec<Value>], mut i: usize) -> Vec<Value> {
    choices
        .iter()
        .map(|values| {
            let value = values[i % values.len()];
            i /= values.len();
            value
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(lengths: &[usize]) -> Vec<Vec<Pos>> {
        lengths
            .iter()
            .enumerate()
            .map(|(run, &len)| {
                (0..len)
                    .map(|bit| {
                        let at = |n: usize| i32::try_from(n).unwrap();
                        [at(run), at(bit), 0]
                    })
                    .collect()
            })
            .collect()
    }

    /// A bus's patterns: none set, each bit's place down the bits, and
    /// each inverted, so no two bits read alike across them.
    #[test]
    fn each_bit_reads_its_own_column() {
        assert_eq!(
            patterns(8),
            [0x00, 0x0F, 0x33, 0x55, 0xAA, 0xCC, 0xF0, 0xFF]
        );
        assert_eq!(patterns(1), [0, 1]);
        let wide = patterns(16);
        let column = |bit: usize| -> Vec<bool> {
            wide.iter().map(|v| v >> bit & 1 == 1).collect()
        };
        for x in 0..16 {
            for y in x + 1..16 {
                assert_ne!(column(x), column(y), "bits {x} and {y}");
            }
        }
    }

    /// Few levers are driven every way.
    #[test]
    fn few_levers_are_driven_every_way() {
        let cases = plan(&runs(&[4, 4, 1]));
        assert_eq!(cases.len(), 512);
        assert!(cases.contains(&vec![15, 0, 1]));
    }

    /// An ALU's buses take every pairing of their patterns under
    /// each setting of its lone lines, a short run's bits lone lines too.
    #[test]
    fn buses_pair_under_each_setting_of_the_lone_lines() {
        let cases = plan(&runs(&[8, 8, 2, 1, 1, 1]));
        let patterns = patterns(8).len();
        assert_eq!(cases.len(), patterns * patterns * 32);
        assert!(cases.contains(&vec![0, 0, 0b11, 1, 0, 1]));
    }

    /// Past every setting of a few lone lines, each with at most three on,
    /// the pairings sampled to stay within the cases a build gets.
    #[test]
    fn many_lone_lines_take_the_settings_with_few_on() {
        let cases = plan(&runs(&[8, 8, 3, 2, 1, 1, 1, 1, 1]));
        assert!(cases.len() <= MOST_CASES);
        let on = |case: &Vec<Value>| {
            case[2..].iter().map(|v| v.count_ones()).sum::<u32>()
        };
        assert!(cases.iter().all(|case| on(case) <= MOST_ON));
        assert!(cases.iter().any(|case| on(case) == MOST_ON));
    }
}
