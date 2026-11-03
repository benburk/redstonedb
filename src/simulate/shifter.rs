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
    Broken, Run, Timing, Value,
    bench::{self, Bench, Case, mask, observe, sweep},
    circuit::output_bits,
    rng::Rng,
    timing,
};
use crate::{
    recognize::{colour, interface::Interface},
    schem::Blocks,
};

/// A shift a barrel shifter may give, of A by the amount `n`, of `width`
/// bits. On a tie, the one first in `Shift::ALL` is taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shift {
    Left,
    Right,
    RotateLeft,
    RotateRight,
    /// Right, keeping the sign.
    Arithmetic,
    /// Left by the width less the amount.
    LeftRest,
    /// Right by the width less the amount, as a shifter that shifts right
    /// through its left shifter's wrapped bits does.
    RightRest,
    /// Nothing: every bit off.
    Off,
}

impl Shift {
    const ALL: [Shift; 8] = [
        Shift::Left,
        Shift::Right,
        Shift::RotateLeft,
        Shift::RotateRight,
        Shift::Arithmetic,
        Shift::LeftRest,
        Shift::RightRest,
        Shift::Off,
    ];

    /// How its cases read.
    fn pattern(self) -> &'static str {
        match self {
            Shift::Left => "A << n",
            Shift::Right => "A >> n",
            Shift::RotateLeft => "A rotated left n",
            Shift::RotateRight => "A rotated right n",
            Shift::Arithmetic => "A >> n, signed",
            Shift::LeftRest => "A << (width − n)",
            Shift::RightRest => "A >> (width − n)",
            Shift::Off => "0",
        }
    }

    /// What it gives for A and the amount `n`, of `width` bits.
    fn apply(self, a: Value, n: Value, width: usize) -> Value {
        let all = mask(width);
        let w = Value::try_from(width).unwrap_or(Value::BITS.into());
        let n = n.min(w);
        let left = |by: Value| if by >= w { 0 } else { a << by & all };
        let right = |by: Value| if by >= w { 0 } else { a >> by };
        let top = a >> (w - 1) & 1;
        match self {
            Shift::Left => left(n),
            Shift::Right => right(n),
            Shift::RotateLeft => (left(n % w) | right((w - n % w) % w)) & all,
            Shift::RotateRight => (right(n % w) | left((w - n % w) % w)) & all,
            Shift::Arithmetic => {
                let kept = right(n);
                if top == 1 {
                    kept | (all & !(all >> n.min(w)))
                } else {
                    kept
                }
            }
            Shift::LeftRest => left(w - n),
            Shift::RightRest => right(w - n),
            Shift::Off => 0,
        }
    }
}

/// A barrel shifter's ports, by their place in its interface.
struct Shifter {
    a: usize,
    width: usize,
    /// Each line of the amount, and what it is worth.
    amount: Vec<(usize, Value)>,
    /// The lines choosing the shift.
    modes: Vec<usize>,
    output: usize,
}

impl Shifter {
    /// The largest amount its lines give.
    fn most(&self) -> Value {
        self.amount.iter().map(|&(_, worth)| worth).sum()
    }

    /// Each amount line's value for amount `n`, which its lines must give.
    fn amount_lines(&self, n: Value) -> Vec<(usize, Value)> {
        let mut left = n;
        let mut lines: Vec<(usize, Value)> = Vec::new();
        let mut by_worth = self.amount.clone();
        by_worth.sort_by_key(|&(_, worth)| std::cmp::Reverse(worth));
        for (port, worth) in by_worth {
            let on = left >= worth;
            if on {
                left -= worth;
            }
            lines.push((port, Value::from(on)));
        }
        lines
    }

    /// The amounts its lines can give, each once.
    fn amounts(&self) -> Vec<Value> {
        (0..=self.most())
            .filter(|&n| {
                let lines = self.amount_lines(n);
                let worth: Value = lines
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
    let worth = |i: usize| -> Option<Value> {
        let words = &interface.inputs[i].words;
        let [word] = words.as_slice() else {
            return None;
        };
        word.parse().ok().filter(|n: &Value| n.is_power_of_two())
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
    shift: Shift,
    /// Each mode line's value.
    modes: Vec<(usize, Value)>,
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
    let values: Vec<Value> = if shifter.width <= 8 {
        (0..1 << shifter.width).collect()
    } else {
        let top = 1 << (shifter.width - 1);
        vec![0, 1, mask(shifter.width), 0x9696 & mask(shifter.width), top]
    };
    let telling: Vec<Case> = values.into_iter().map(|a| [a, 0, 0]).collect();
    let mut found = Vec::new();
    for combination in 0..1 << shifter.modes.len() {
        let modes: Vec<(usize, Value)> = shifter
            .modes
            .iter()
            .enumerate()
            .map(|(k, &port)| (port, combination >> k & 1))
            .collect();
        let mut right = [0usize; Shift::ALL.len()];
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
                care: Value::MAX,
                press: None,
                invertible: false,
            };
            let got = observe(blocks, interface, &bench, &telling, None)?;
            for (case, &got) in telling.iter().zip(&got) {
                tried += 1;
                for (k, shift) in Shift::ALL.iter().enumerate() {
                    right[k] += usize::from(
                        shift.apply(case[0], n, shifter.width) == got,
                    );
                }
            }
        }
        let best = (0..Shift::ALL.len())
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
        let shift = Shift::ALL[k];
        found.push(Selection {
            shift,
            modes,
            label: format!("{} ({lines})", shift.pattern()),
        });
    }
    if found.iter().all(|s| s.shift == Shift::Off) {
        broken!("No setting of its lines shifts A.");
    }
    Ok(found)
}

/// A setting of its lines to check: a shift found, at an amount.
struct Setting {
    /// The shift, by its place among those found.
    shift: usize,
    amount: Value,
    /// Every line's value.
    held: Vec<(usize, Value)>,
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
    let interface = crate::recognize::interface::read(blocks);
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
            care: Value::MAX,
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
                    (0..1 << shifter.width.min(16))
                        .map(|a| [a, 0, 0])
                        .collect(),
                    0,
                )
            })
        };
        let (shift, width) = (selection.shift, shifter.width);
        let mut run =
            sweep(blocks, &interface, &bench, &cases, None, move |case| {
                shift.apply(case[0], n, width)
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
    let interface = crate::recognize::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let has =
        |shifts: &[Shift]| found.iter().any(|s| shifts.contains(&s.shift));
    let left = has(&[Shift::Left, Shift::RotateLeft, Shift::LeftRest]);
    let right = has(&[
        Shift::Right,
        Shift::RotateRight,
        Shift::Arithmetic,
        Shift::RightRest,
    ]);
    let mut flags = Vec::new();
    if left && right {
        flags.push("bidirectional".to_owned());
    }
    if has(&[Shift::RotateLeft, Shift::RotateRight]) {
        flags.push("rotate".to_owned());
    }
    if has(&[Shift::Arithmetic]) {
        flags.push("arithmetic".to_owned());
    }
    Ok(flags)
}

/// Whether the barrel shifter in `blocks` holds state, as
/// `bench::remembers` tells of each setting of its lines.
pub fn remembers(blocks: &Blocks) -> Result<bool> {
    let interface = crate::recognize::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    bench::remembers_any(
        blocks,
        &interface,
        (&[shifter.a], &[shifter.output]),
        settings(&shifter, &found)
            .into_iter()
            .map(|setting| setting.held),
    )
}

/// Measures each setting of its lines, every shift at every amount, and
/// returns the slowest's timing, synced only when every setting's is.
pub fn time(blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
    let interface = crate::recognize::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let timings = settings(&shifter, &found)
        .into_iter()
        // An off shift's output never changes, so it has nothing to time.
        .filter(|setting| found[setting.shift].shift != Shift::Off)
        .map(|Setting { held, .. }| {
            timing::measure(
                blocks,
                &interface,
                &[shifter.a],
                &held,
                None,
                cancel,
            )
        });
    timing::slowest(timings, |timing| timing.synced, "no shift to time")
}

/// How often the barrel shifter in `blocks`, timed as `timing`, takes a new
/// case, as `throughput::shortest` streams them: each case A, an amount,
/// and a shift, its lines set as a decoder does, timed against A as works
/// best, as a control line is. `None` when no period up to twice its
/// slowest output keeps every result right.
pub fn throughput(blocks: &Blocks, timing: &Timing) -> Result<Option<i32>> {
    let interface = crate::recognize::interface::read(blocks);
    let shifter = ports(&interface)?;
    let found = selections(blocks, &interface, &shifter)?;
    let settings = settings(&shifter, &found);
    let stream = |count: usize| {
        let mut rng = Rng::default();
        (0..count)
            .map(|_| {
                let setting = &settings[rng.below(settings.len())];
                let mut case = vec![0; interface.inputs.len()];
                case[shifter.a] = Value::from(rng.draw()) & mask(shifter.width);
                for &(port, value) in &setting.held {
                    case[port] = value;
                }
                case
            })
            .collect()
    };
    super::throughput::controlled(
        blocks,
        &interface,
        timing,
        |port| port != shifter.a,
        stream,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shift_gives_its_result_on_eight_bits() {
        assert_eq!(Shift::Left.apply(0b1001_0110, 1, 8), 0b0010_1100);
        assert_eq!(Shift::Right.apply(0b1001_0110, 1, 8), 0b0100_1011);
        assert_eq!(Shift::RotateLeft.apply(0b1001_0110, 1, 8), 0b0010_1101);
        assert_eq!(Shift::RotateRight.apply(0b1001_0110, 1, 8), 0b0100_1011);
        assert_eq!(Shift::Arithmetic.apply(0b1001_0110, 2, 8), 0b1110_0101);
        assert_eq!(Shift::RightRest.apply(0x80, 1, 8), 0x01);
        assert_eq!(Shift::LeftRest.apply(0x01, 7, 8), 0x02);
        assert_eq!(Shift::RotateLeft.apply(0x96, 0, 8), 0x96);
        assert_eq!(Shift::RightRest.apply(0x96, 0, 8), 0);
    }
}
