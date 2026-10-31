//! Testing a barrel shifter: A, the bits of an amount to shift it by, and
//! lines choosing how. A magenta line signed with a number, as `1`, `2`, or
//! `4`, is a bit of the amount, worth that number; the rest choose the
//! shift, so each combination of them is tried on a few telling cases at
//! every amount, and the shift it gives three cases in four, as A shifted
//! left or right, rotated either way, shifted right keeping its sign, or
//! shifted by the width less the amount, as a shifter that shifts right
//! through its left shifter's wrapped bits does, is checked on every case
//! at every amount. A combination that gives nothing is a shift too, off.
//!
//! Its flags say what its shifts are: bidirectional when it shifts both
//! ways, rotate when it rotates, arithmetic when it keeps the sign.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::{
    Broken, Run, Timing,
    bench::{self, Bench, Case, mask, observe, sweep},
    circuit::output_bits,
    timing,
};
use crate::{colour, interface::Interface, schem::Blocks};

/// The shifts a barrel shifter may give, each of A, by the amount `n`, of
/// `width` bits; and how its cases read.
const SHIFTS: [(&str, &str); 8] = [
    ("shl", "A << n"),
    ("shr", "A >> n"),
    ("rol", "A rotated left n"),
    ("ror", "A rotated right n"),
    ("asr", "A >> n, signed"),
    ("shl_rest", "A << (width − n)"),
    ("shr_rest", "A >> (width − n)"),
    ("off", "0"),
];

/// What `shift` gives for A and the amount `n`, of `width` bits.
fn apply(shift: &str, a: u64, n: u64, width: usize) -> u64 {
    let all = mask(width);
    let w = u64::try_from(width).unwrap_or(64);
    let n = n.min(w);
    let left = |by: u64| if by >= w { 0 } else { a << by & all };
    let right = |by: u64| if by >= w { 0 } else { a >> by };
    let top = a >> (w - 1) & 1;
    match shift {
        "shl" => left(n),
        "shr" => right(n),
        "rol" => (left(n % w) | right((w - n % w) % w)) & all,
        "ror" => (right(n % w) | left((w - n % w) % w)) & all,
        "asr" => {
            let kept = right(n);
            if top == 1 {
                kept | (all & !(all >> n.min(w)))
            } else {
                kept
            }
        }
        "shl_rest" => left(w - n),
        "shr_rest" => right(w - n),
        _ => 0,
    }
}

/// A barrel shifter's ports, by their place in its interface.
struct Shifter {
    a: usize,
    width: usize,
    /// Each line of the amount, and what it is worth.
    amount: Vec<(usize, u64)>,
    /// The lines choosing the shift.
    modes: Vec<usize>,
    output: usize,
}

impl Shifter {
    /// The largest amount its lines give.
    fn most(&self) -> u64 {
        self.amount.iter().map(|&(_, worth)| worth).sum()
    }

    /// Each amount line's value for amount `n`, which its lines must give.
    fn amount_lines(&self, n: u64) -> Vec<(usize, u64)> {
        let mut left = n;
        let mut lines: Vec<(usize, u64)> = Vec::new();
        let mut by_worth = self.amount.clone();
        by_worth.sort_by_key(|&(_, worth)| std::cmp::Reverse(worth));
        for (port, worth) in by_worth {
            let on = left >= worth;
            if on {
                left -= worth;
            }
            lines.push((port, u64::from(on)));
        }
        lines
    }

    /// The amounts its lines can give, each once.
    fn amounts(&self) -> Vec<u64> {
        (0..=self.most())
            .filter(|&n| {
                let lines = self.amount_lines(n);
                let worth: u64 = lines
                    .iter()
                    .filter(|&&(_, on)| on == 1)
                    .map(|&(port, _)| {
                        self.amount
                            .iter()
                            .find(|&&(p, _)| p == port)
                            .map_or(0, |&(_, w)| w)
                    })
                    .sum();
                worth == n
            })
            .collect()
    }
}

/// Reads a barrel shifter's ports: A, its one operand; its amount lines,
/// signed with what each is worth; its other lines, choosing the shift;
/// and its one output, as wide as A.
fn ports(interface: &Interface) -> Result<Shifter> {
    let control =
        |i: &usize| interface.inputs[*i].block.starts_with(colour::CONTROL);
    let (lines, operands): (Vec<usize>, Vec<usize>) =
        (0..interface.inputs.len()).partition(control);
    let &[a] = operands.as_slice() else {
        broken!(
            "A barrel shifter takes one operand, A, but this has {}.",
            operands.len()
        );
    };
    let worth = |i: usize| -> Option<u64> {
        let words = &interface.inputs[i].words;
        let [word] = words.as_slice() else {
            return None;
        };
        word.parse().ok().filter(|n: &u64| n.is_power_of_two())
    };
    let (amount, modes): (Vec<usize>, Vec<usize>) =
        lines.into_iter().partition(|&i| worth(i).is_some());
    if amount.is_empty() {
        broken!(
            "A barrel shifter's amount is lines signed with what each is \
             worth, as 1, 2, and 4, but this has none."
        );
    }
    if modes.len() > 6 {
        broken!(
            "A barrel shifter's lines choosing its shift are tried in every \
             combination, but this has {}, too many to try.",
            modes.len()
        );
    }
    let &[output] = (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A barrel shifter has one output, but this has {}.",
            interface.outputs.len()
        );
    };
    let width = interface.inputs[a].bits.len();
    if output_bits(interface, &interface.outputs[output]) != width {
        broken!(
            "A barrel shifter's output is as wide as A, but this one's is not."
        );
    }
    Ok(Shifter {
        a,
        width,
        amount: amount
            .into_iter()
            .map(|i| (i, worth(i).unwrap_or(0)))
            .collect(),
        modes,
        output,
    })
}

/// One way its lines are set, and the shift it gives.
struct Selection {
    shift: &'static str,
    /// Each mode line's value.
    modes: Vec<(usize, u64)>,
    /// How it reads in a result, such as `A << n (shift left on)`.
    label: String,
}

/// The shift each combination of its mode lines gives, as every value of
/// A, or with too many a few telling ones, shows at every amount: the one
/// most cases give, if three in four do. A combination that gives none of
/// them is left out.
fn selections(
    blocks: &Blocks,
    interface: &Interface,
    shifter: &Shifter,
) -> Result<Vec<Selection>> {
    // Every value of A, while there are few, so a combination that shifts
    // only some of them right is not taken for one that shifts them all.
    let values: Vec<u64> = if shifter.width <= 8 {
        (0..1u64 << shifter.width).collect()
    } else {
        let top = 1 << (shifter.width - 1);
        vec![0, 1, mask(shifter.width), 0x9696 & mask(shifter.width), top]
    };
    let telling: Vec<Case> = values.into_iter().map(|a| [a, 0, 0]).collect();
    let mut found = Vec::new();
    for combination in 0..1u64 << shifter.modes.len() {
        let modes: Vec<(usize, u64)> = shifter
            .modes
            .iter()
            .enumerate()
            .map(|(k, &port)| (port, combination >> k & 1))
            .collect();
        let mut right = [0usize; SHIFTS.len()];
        let mut tried = 0;
        for n in shifter.amounts() {
            let bench = Bench {
                inputs: vec![shifter.a],
                outputs: vec![shifter.output],
                pattern: String::new(),
                held: modes
                    .iter()
                    .copied()
                    .chain(shifter.amount_lines(n))
                    .collect(),
                care: u64::MAX,
                press: None,
                invertible: false,
            };
            let got = observe(blocks, interface, &bench, &telling, None)?;
            for (case, &got) in telling.iter().zip(&got) {
                tried += 1;
                for (k, (shift, _)) in SHIFTS.iter().enumerate() {
                    right[k] += usize::from(
                        apply(shift, case[0], n, shifter.width) == got,
                    );
                }
            }
        }
        let best = (0..SHIFTS.len())
            .filter(|&k| right[k] * 4 >= tried * 3)
            .max_by_key(|&k| (right[k], std::cmp::Reverse(k)));
        let Some(k) = best else {
            continue;
        };
        let on: Vec<&str> = modes
            .iter()
            .filter(|&&(_, on)| on == 1)
            .map(|&(port, _)| interface.inputs[port].name.as_str())
            .collect();
        let lines = if on.is_empty() {
            "no lines on".to_owned()
        } else {
            format!("{} on", on.join(" + "))
        };
        found.push(Selection {
            shift: SHIFTS[k].0,
            modes,
            label: format!("{} ({lines})", SHIFTS[k].1),
        });
    }
    if found.iter().all(|s| s.shift == "off") {
        broken!("No setting of its lines shifts A.");
    }
    Ok(found)
}

/// A setting of its lines to check: a shift found, at an amount.
struct Setting {
    /// The shift, by its place among those found.
    shift: usize,
    amount: u64,
    /// Every line's value.
    held: Vec<(usize, u64)>,
}

/// Each setting of its lines to check: every shift found, at every amount.
fn settings(shifter: &Shifter, found: &[Selection]) -> Vec<Setting> {
    let mut settings = Vec::new();
    for (shift, selection) in found.iter().enumerate() {
        for amount in shifter.amounts() {
            let held = selection
                .modes
                .iter()
                .copied()
                .chain(shifter.amount_lines(amount))
                .collect();
            settings.push(Setting {
                shift,
                amount,
                held,
            });
        }
    }
    settings
}

/// Runs a barrel shifter through every shift its lines give, at every
/// amount, on every value of A, or with `quick` a few telling ones.
pub fn check(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = crate::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let mut merged: Option<Run> = None;
    for Setting {
        shift: k,
        amount: n,
        held,
    } in settings(&shifter, &found)
    {
        let selection = &found[k];
        let bench = Bench {
            inputs: vec![shifter.a],
            outputs: vec![shifter.output],
            pattern: selection.label.clone(),
            held,
            care: u64::MAX,
            press: None,
            invertible: false,
        };
        let widths = [shifter.width];
        let (cases, exhaustive, patterns) = if quick {
            let (telling, _) = bench::paired(&widths);
            let count = telling.len() as u64;
            (telling, false, count)
        } else {
            bench::cases(&widths, || {
                (
                    (0..1u64 << shifter.width.min(16))
                        .map(|a| [a, 0, 0])
                        .collect(),
                    0,
                )
            })
        };
        let (shift, width) = (selection.shift, shifter.width);
        let mut run =
            sweep(blocks, &interface, &bench, &cases, None, move |case| {
                apply(shift, case[0], n, width)
            })?
            .covering(exhaustive, patterns);
        for wrong in &mut run.examples {
            wrong.op = Some(format!("{}, n = {n}", selection.label));
        }
        match &mut merged {
            Some(all) => all.absorb(run),
            None => merged = Some(run),
        }
    }
    let mut merged =
        merged.ok_or_else(|| Broken("no shift to check".to_owned()))?;
    merged.pattern = found
        .iter()
        .map(|s| s.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(merged)
}

/// The flags the barrel shifter in `blocks` shows: `bidirectional` when it
/// shifts both ways, `rotate` when it rotates, and `arithmetic` when it
/// shifts right keeping the sign.
pub fn flags(blocks: &Blocks) -> Result<Vec<String>> {
    let interface = crate::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let has = |shifts: &[&str]| found.iter().any(|s| shifts.contains(&s.shift));
    let left = has(&["shl", "rol", "shl_rest"]);
    let right = has(&["shr", "ror", "asr", "shr_rest"]);
    let mut flags = Vec::new();
    if left && right {
        flags.push("bidirectional".to_owned());
    }
    if has(&["rol", "ror"]) {
        flags.push("rotate".to_owned());
    }
    if has(&["asr"]) {
        flags.push("arithmetic".to_owned());
    }
    Ok(flags)
}

/// Whether the barrel shifter in `blocks` holds state, as
/// `bench::remembers` tells of each setting of its lines.
pub fn remembers(blocks: &Blocks) -> Result<bool> {
    let interface = crate::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    for Setting { held, .. } in settings(&shifter, &found) {
        let bench = Bench {
            inputs: vec![shifter.a],
            outputs: vec![shifter.output],
            pattern: String::new(),
            held,
            care: u64::MAX,
            press: None,
            invertible: false,
        };
        if bench::remembers(blocks, &interface, &bench)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Measures each setting of its lines, every shift at every amount, and
/// returns the slowest's timing, synced only when every setting's is.
pub fn time(blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
    let interface = crate::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let mut slowest: Option<Timing> = None;
    let mut synced = true;
    for Setting { shift: k, held, .. } in settings(&shifter, &found) {
        // An off shift's output never changes, so it has nothing to time.
        if found[k].shift == "off" {
            continue;
        }
        let timing = timing::measure(
            blocks,
            &interface,
            &[shifter.a],
            &held,
            None,
            cancel,
        )?;
        synced &= timing.synced;
        if slowest
            .as_ref()
            .is_none_or(|s| timing.latency() > s.latency())
        {
            slowest = Some(timing);
        }
    }
    let mut timing =
        slowest.ok_or_else(|| Broken("no shift to time".to_owned()))?;
    timing.synced = synced;
    Ok(timing)
}

/// How often the barrel shifter in `blocks`, timed as `timing`, takes a new
/// case, as `throughput::shortest` streams them: each case A, an amount,
/// and a shift, its lines set as a decoder does, timed against A as works
/// best, as a control line is. `None` when no period up to twice its
/// slowest output keeps every result right.
pub fn throughput(blocks: &Blocks, timing: &Timing) -> Result<Option<i32>> {
    let interface = crate::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let settings = settings(&shifter, &found);
    let latency: Vec<i32> = timing
        .outputs
        .iter()
        .map(|o| o.latency + o.carried)
        .collect();
    if latency.len() != interface.outputs.len() {
        return Ok(None);
    }
    let slowest = latency.iter().copied().max().unwrap_or(1).max(1);
    let stream = |count: usize| {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..count)
            .map(|_| {
                let setting =
                    &settings[usize::try_from(next() % settings.len() as u64)
                        .unwrap_or(0)];
                let mut case = vec![0; interface.inputs.len()];
                case[shifter.a] = next() & mask(shifter.width);
                for &(port, value) in &setting.held {
                    case[port] = value;
                }
                case
            })
            .collect::<Vec<_>>()
    };
    let (short, long) = (
        stream(super::throughput::CASES),
        stream(super::throughput::LONG),
    );
    let mut best: Option<i32> = None;
    for offset in -slowest..=slowest {
        let offsets: Vec<i32> = (0..interface.inputs.len())
            .map(|port| if port == shifter.a { 0 } else { offset })
            .collect();
        let most = best.map_or(2 * slowest, |b| b - 1);
        if most < 1 {
            break;
        }
        let found = super::throughput::shortest(
            blocks,
            (&short, &long),
            &offsets,
            &latency,
            most,
        )?;
        if found.is_some() {
            best = found;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shift_gives_its_result_on_eight_bits() {
        assert_eq!(apply("shl", 0b1001_0110, 1, 8), 0b0010_1100);
        assert_eq!(apply("shr", 0b1001_0110, 1, 8), 0b0100_1011);
        assert_eq!(apply("rol", 0b1001_0110, 1, 8), 0b0010_1101);
        assert_eq!(apply("ror", 0b1001_0110, 1, 8), 0b0100_1011);
        assert_eq!(apply("asr", 0b1001_0110, 2, 8), 0b1110_0101);
        assert_eq!(apply("shr_rest", 0x80, 1, 8), 0x01);
        assert_eq!(apply("shl_rest", 0x01, 7, 8), 0x02);
        assert_eq!(apply("rol", 0x96, 0, 8), 0x96);
        assert_eq!(apply("shr_rest", 0x96, 0, 8), 0);
    }
}
