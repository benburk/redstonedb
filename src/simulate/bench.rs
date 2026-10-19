//! Running a stateless build through its cases: what a test drives and
//! reads, which cases it tries, and how what came out is judged. Each such
//! test states only what sets it apart, as a [`Spec`], and runs the same
//! way.

use std::{collections::HashSet, sync::atomic::AtomicBool};

use anyhow::{Context, Result};

use super::{
    Run, Signal, Timing, Wrong,
    circuit::{Circuit, input_bits, output_bits},
    timing,
};
use crate::{interface::Interface, schem::Blocks};

/// Up to this many input bits, every combination is tried; beyond it, every
/// case of each byte and every pairing of the test patterns.
pub(super) const EXHAUSTIVE_BITS: usize = 20;

/// One case's input values, in the order of its bench's inputs; those it
/// has no input for are 0.
pub(super) type Case = [u64; 3];

/// What a test drives and reads.
pub(super) struct Bench {
    /// Input ports, by their place in the interface, in the order of each
    /// case's values.
    pub inputs: Vec<usize>,
    /// Output ports, least significant first, read together as one number.
    pub outputs: Vec<usize>,
    /// What each case is, such as `A + B + carry in`.
    pub pattern: String,
    /// Inputs set once and held through every case, by their place in the
    /// interface, such as the control lines choosing an ALU's operation.
    pub held: Vec<(usize, u64)>,
    /// The bits of the outputs read together that the cases decide; the
    /// rest, such as a status line an operation leaves meaningless, are
    /// not checked.
    pub care: u64,
    /// Control lines pressed in order once each case's inputs settle, each
    /// settling in turn, as a sequential multiplier's reset and calculate.
    pub presses: Vec<usize>,
}

impl Bench {
    /// A bench driving `inputs` and reading `outputs`, every bit of which
    /// the cases decide, with nothing held.
    pub fn new(inputs: Vec<usize>, outputs: Vec<usize>, pattern: &str) -> Self {
        Bench {
            inputs,
            outputs,
            pattern: pattern.to_owned(),
            held: Vec::new(),
            care: u64::MAX,
            presses: Vec::new(),
        }
    }

    /// Each input's width in bits, in the order of `inputs`.
    pub fn widths(&self, interface: &Interface) -> Vec<usize> {
        self.inputs
            .iter()
            .map(|&i| input_bits(&interface.inputs[i]))
            .collect()
    }

    /// Each output's width in bits, in the order of `outputs`.
    pub fn output_widths(&self, interface: &Interface) -> Vec<usize> {
        self.outputs
            .iter()
            .map(|&i| output_bits(interface, &interface.outputs[i]))
            .collect()
    }
}

/// A stateless test, as it differs from the others: what it drives and
/// reads, the cases it tries, and what each case should give.
pub(super) struct Spec {
    pub bench: Bench,
    pub cases: Cases,
    /// What the build may be. Most tests know, so this holds one; an
    /// incrementer may add or subtract, so it holds each kind, and the
    /// one the build gets most right names its cases.
    pub kinds: Vec<Kind>,
    pub hint: Hint,
    /// Whether the cases, when there are too many to try every one, take
    /// each hex digit's every value rather than each byte's, as a hex
    /// adder's do.
    pub by_digit: bool,
}

/// One thing a build may compute: its cases' name, such as `A + B`, and
/// what it gives for a case, before it is cut to the outputs' bits.
pub(super) struct Kind {
    pub pattern: String,
    pub expected: Box<dyn Fn(&Case) -> u64>,
}

impl Kind {
    pub fn new(
        pattern: &str,
        expected: impl Fn(&Case) -> u64 + 'static,
    ) -> Self {
        Kind {
            pattern: pattern.to_owned(),
            expected: Box::new(expected),
        }
    }
}

/// Which cases a test tries.
pub(super) enum Cases {
    /// Every combination of the inputs when they come to at most
    /// `EXHAUSTIVE_BITS`, else those the function gives, with how many of
    /// them pair the test patterns. A quick look pairs a few telling values
    /// of each input instead.
    Combinations(Box<dyn FnOnce() -> (Vec<Case>, u64)>),
    /// Only the values of A the function gives, quick or not, with whether
    /// they are every one, for an input that takes only some values, as a
    /// BCD input takes only decimal digits.
    Only(Box<dyn Fn(bool) -> (Vec<u64>, bool)>),
}

/// What a test tries when cases come out wrong, to name the likely cause.
#[derive(Clone, Copy)]
pub(super) enum Hint {
    None,
    /// When every case is wrong, the 1-bit input at this place in each case
    /// driven the other way round, as its lever saved the wrong way does.
    Flipped(usize),
    /// When some are wrong, each of A's bits whose lever is saved on driven
    /// as a plain input: a lever saved on reads as an inverted input, so one
    /// left on by mistake gets most cases wrong.
    SavedOn,
}

impl Spec {
    /// Runs the cases through `blocks`, whose interface is `interface`, or
    /// with `quick` a few telling ones, and judges what came out against
    /// the kind it gets most right.
    pub fn check(
        self,
        blocks: &Blocks,
        interface: &Interface,
        quick: bool,
    ) -> Result<Run> {
        let widths = self.bench.widths(interface);
        let (cases, exhaustive, patterns) = match self.cases {
            Cases::Only(values) => {
                let (values, every) = values(quick);
                let cases: Vec<Case> =
                    values.into_iter().map(|a| [a, 0, 0]).collect();
                let selected = if every { 0 } else { cases.len() as u64 };
                (cases, every, selected)
            }
            Cases::Combinations(_) if quick => {
                let (cases, count) = paired(&widths);
                (cases, false, count)
            }
            Cases::Combinations(structured) => cases(&widths, structured),
        };
        let got = observe(blocks, interface, &self.bench, &cases, None)?;
        let (kind, run) = self
            .kinds
            .iter()
            .map(|kind| {
                let mut run =
                    judge(interface, &self.bench, &cases, &got, &kind.expected);
                run.pattern.clone_from(&kind.pattern);
                (kind, run)
            })
            .min_by_key(|(_, run)| run.wrong)
            .context("a test has something to check")?;
        let mut run = run.covering(exhaustive, patterns);
        run.hint = self.hint.find(
            blocks,
            interface,
            &self.bench,
            &cases,
            &run,
            &kind.expected,
        )?;
        run.by_digit = self.by_digit && !run.exhaustive;
        Ok(run)
    }

    /// Measures when the outputs settle, on every core, stopping early once
    /// `cancel` is set; an input that takes only some values takes only
    /// those.
    pub fn time(
        self,
        blocks: &Blocks,
        interface: &Interface,
        cancel: &AtomicBool,
    ) -> Result<Timing> {
        if !self.bench.presses.is_empty() {
            let every = match self.cases {
                Cases::Combinations(structured) => {
                    cases(&self.bench.widths(interface), structured).0
                }
                Cases::Only(values) => {
                    values(false).0.into_iter().map(|a| [a, 0, 0]).collect()
                }
            };
            return timing::pressed(
                blocks,
                interface,
                &self.bench,
                &every,
                cancel,
            );
        }
        let values = match &self.cases {
            Cases::Only(values) => {
                let (valid, every) = values(false);
                Some((vec![valid], !every))
            }
            Cases::Combinations(_) => None,
        };
        timing::measure(
            blocks,
            interface,
            &self.bench.inputs,
            &self.bench.held,
            values,
            cancel,
        )
    }
}

impl Hint {
    /// The likely cause of `run`'s wrong cases, when one change puts every
    /// case right, as a sentence.
    fn find(
        self,
        blocks: &Blocks,
        interface: &Interface,
        bench: &Bench,
        cases: &[Case],
        run: &Run,
        expected: &dyn Fn(&Case) -> u64,
    ) -> Result<Option<String>> {
        let right = |flip| {
            sweep(blocks, interface, bench, cases, Some(flip), expected)
                .map(|run| run.wrong == 0)
        };
        match self {
            Hint::Flipped(input) if run.wrong == run.cases => {
                let name = &interface.inputs[bench.inputs[input]].name;
                Ok(right((input, 1))?.then(|| {
                    format!(
                        "Every case is right with {name} flipped, so its \
                         lever is saved the wrong way round."
                    )
                }))
            }
            Hint::SavedOn if run.wrong > 0 => {
                let port = &interface.inputs[bench.inputs[0]];
                for (bit, _) in
                    port.bits.iter().enumerate().filter(|(_, b)| b.inverted)
                {
                    if right((0, 1 << bit))? {
                        return Ok(Some(format!(
                            "Every case is right with bit {bit} of {} as a \
                             plain input, so its lever is saved on by \
                             mistake: a lever saved on marks an inverted \
                             input.",
                            port.name
                        )));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

/// `bits` ones, the lowest bits.
pub(super) fn mask(bits: usize) -> u64 {
    match bits {
        0 => 0,
        bits => u64::MAX >> 64usize.saturating_sub(bits),
    }
}

/// Values of `width` bits that make an adder carry far and across its
/// groups: none and all set; ones and zeros alternating in runs of one,
/// two, and four, at every shift; and each bit alone set or clear.
pub(super) fn patterns(width: usize) -> Vec<u64> {
    let all = mask(width);
    let mut found = vec![0, all];
    for period in [2, 4, 8] {
        for shift in 0..period {
            let runs = (0..width)
                .filter(|bit| (bit + shift) % period < period / 2)
                .fold(0, |value, bit| value | 1 << bit);
            found.push(runs);
        }
    }
    for bit in 0..width {
        found.extend([1 << bit, all & !(1 << bit)]);
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// Cases without repeats, in the order they are first given.
#[derive(Default)]
pub(super) struct Distinct {
    pub cases: Vec<Case>,
    seen: HashSet<Case>,
}

impl Distinct {
    /// Adds `case` unless it is already in.
    pub fn add(&mut self, case: Case) {
        if self.seen.insert(case) {
            self.cases.push(case);
        }
    }

    pub fn len(&self) -> usize {
        self.cases.len()
    }
}

/// Every combination of inputs of `widths` bits when they come to at most
/// `EXHAUSTIVE_BITS`, else what `structured` gives, with whether they were
/// every one and how many pair the test patterns.
pub(super) fn cases(
    widths: &[usize],
    structured: impl FnOnce() -> (Vec<Case>, u64),
) -> (Vec<Case>, bool, u64) {
    let bits: usize = widths.iter().sum();
    if bits > EXHAUSTIVE_BITS {
        let (cases, patterns) = structured();
        return (cases, false, patterns);
    }
    let every = (0..1u64 << bits)
        .map(|packed| {
            let mut case = [0; 3];
            let mut shift = 0;
            for (value, &width) in case.iter_mut().zip(widths) {
                *value = packed >> shift & mask(width);
                shift += width;
            }
            case
        })
        .collect();
    (every, true, 0)
}

/// Every pairing of a few telling values of each input's width, a 1-bit one
/// taking 0 and 1, with how many there are: a quick look, enough to tell
/// an adder from a multiplier, even for a build slow to run.
pub(super) fn paired(widths: &[usize]) -> (Vec<Case>, u64) {
    let choices: Vec<Vec<u64>> = widths
        .iter()
        .map(|&width| {
            if width == 1 {
                vec![0, 1]
            } else {
                telling(width)
            }
        })
        .collect();
    let mut cases = vec![[0; 3]];
    for (i, values) in choices.iter().enumerate() {
        cases = cases
            .iter()
            .flat_map(|case| {
                values.iter().map(move |&value| {
                    let mut case = *case;
                    case[i] = value;
                    case
                })
            })
            .collect();
    }
    let count = cases.len() as u64;
    (cases, count)
}

/// About a dozen values of `width` bits that tell functions apart: none,
/// one, and all set; alternate bits and pairs of bits, each way; the top
/// bit alone; the low half set; and two with no pattern.
fn telling(width: usize) -> Vec<u64> {
    let all = mask(width);
    let mut values: Vec<u64> = [
        0,
        1,
        u64::MAX,
        0x5555_5555_5555_5555,
        0xAAAA_AAAA_AAAA_AAAA,
        0x3333_3333_3333_3333,
        0xCCCC_CCCC_CCCC_CCCC,
        1 << (width - 1),
        mask(width / 2),
        0x9E37_79B9_7F4A_7C15,
        0x6A09_E667_F3BC_C908,
    ]
    .iter()
    .map(|value| value & all)
    .collect();
    values.sort_unstable();
    values.dedup();
    values
}

impl Run {
    /// Takes in `other`'s cases, as another operation of the same build,
    /// keeping the first five wrong cases of all.
    pub(super) fn absorb(&mut self, other: Run) {
        self.cases += other.cases;
        self.wrong += other.wrong;
        self.patterns += other.patterns;
        self.exhaustive &= other.exhaustive;
        for (label, count) in other.wrong_bits {
            match self.wrong_bits.iter_mut().find(|(l, _)| *l == label) {
                Some(bit) => bit.1 += count,
                None => self.wrong_bits.push((label, count)),
            }
        }
        let room = 5usize.saturating_sub(self.examples.len());
        self.examples.extend(other.examples.into_iter().take(room));
    }

    /// Notes whether its cases were every combination, and how many paired
    /// the test patterns.
    pub(super) fn covering(mut self, exhaustive: bool, patterns: u64) -> Self {
        self.exhaustive = exhaustive;
        self.patterns = patterns;
        self
    }
}

/// The bits of one of a bench's inputs, by its place in each case, to
/// drive the other way round, as a lever saved the wrong way does.
type Flip = Option<(usize, u64)>;

/// Runs `cases` through the bench's inputs, driving the bits `flip` gives,
/// if any, the other way round, and checks each result against `expected`,
/// cut to as many bits as the outputs hold.
pub(super) fn sweep(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    flip: Flip,
    expected: impl Fn(&Case) -> u64,
) -> Result<Run> {
    let got = observe(blocks, interface, bench, cases, flip)?;
    Ok(judge(interface, bench, cases, &got, expected))
}

/// What the bench's outputs show, read together as one number, for each
/// of `cases` run through its inputs, the bits `flip` gives, if any, driven
/// the other way round.
pub(super) fn observe(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    flip: Flip,
) -> Result<Vec<u64>> {
    let mut circuit = Circuit::new(blocks, interface)?;
    for &(port, value) in &bench.held {
        circuit.set(port, value);
    }
    let widths = bench.output_widths(interface);
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        for (i, &port) in bench.inputs.iter().enumerate() {
            let flipped = flip.filter(|&(input, _)| input == i);
            circuit.set(port, case[i] ^ flipped.map_or(0, |(_, bits)| bits));
        }
        circuit.settle()?;
        for &port in &bench.presses {
            pulse(&mut circuit, port)?;
            circuit.settle()?;
        }
        let mut got = 0;
        let mut shift = 0;
        for (&port, &width) in bench.outputs.iter().zip(&widths) {
            got |= circuit.get(port) << shift;
            shift += width;
        }
        results.push(got);
    }
    Ok(results)
}

/// Presses control line `port`: its button, or its lever turned on and,
/// once that settles, off.
pub(super) fn pulse(circuit: &mut Circuit, port: usize) -> Result<()> {
    if circuit.is_button(port, 0) {
        circuit.press(port, 0);
    } else {
        circuit.set(port, 1);
        circuit.settle()?;
        circuit.set(port, 0);
    }
    Ok(())
}

/// How `got`, what the outputs showed for each of `cases`, compares with
/// `expected`, cut to as many bits as the outputs hold.
fn judge(
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    got: &[u64],
    expected: impl Fn(&Case) -> u64,
) -> Run {
    let widths = bench.output_widths(interface);
    let result_bits: usize = widths.iter().sum();
    // Each output's value from a result, the first holding the lowest bits.
    let split = |result: u64| {
        let mut shift = 0;
        widths
            .iter()
            .map(|&width| {
                let value = result >> shift & mask(width);
                shift += width;
                value
            })
            .collect::<Vec<u64>>()
    };
    let mut wrong = 0;
    let mut wrong_by_bit = vec![0; result_bits];
    let mut examples = Vec::new();
    for (case, &got) in cases.iter().zip(got) {
        // The bits the cases leave open are taken as they came out.
        let want = (expected(case) & bench.care | got & !bench.care)
            & mask(result_bits);
        if want != got {
            wrong += 1;
            for (bit, count) in wrong_by_bit.iter_mut().enumerate() {
                *count += (want ^ got) >> bit & 1;
            }
            if examples.len() < 5 {
                examples.push(Wrong {
                    op: None,
                    inputs: case[..bench.inputs.len()].to_vec(),
                    expected: split(want),
                    got: split(got),
                });
            }
        }
    }
    // Each result bit as its output's name, and its place in it when it has
    // more than one.
    let labels: Vec<String> = bench
        .outputs
        .iter()
        .flat_map(|&i| {
            let port = &interface.outputs[i];
            let bits = output_bits(interface, port);
            (0..bits).map(move |bit| match bits {
                1 => port.name.clone(),
                _ => format!("{} bit {bit}", port.name),
            })
        })
        .collect();
    let wrong_bits = wrong_by_bit
        .into_iter()
        .zip(labels)
        .filter(|&(count, _)| count > 0)
        .map(|(count, label)| (label, count))
        .collect();
    let signals = |ports: &[usize], outputs: bool| {
        ports
            .iter()
            .map(|&i| {
                let (name, bits) = if outputs {
                    let port = &interface.outputs[i];
                    (&port.name, output_bits(interface, port))
                } else {
                    let port = &interface.inputs[i];
                    (&port.name, input_bits(port))
                };
                Signal {
                    name: name.clone(),
                    bits,
                }
            })
            .collect()
    };
    Run {
        pattern: bench.pattern.clone(),
        cases: cases.len() as u64,
        exhaustive: true,
        patterns: 0,
        wrong,
        inputs: signals(&bench.inputs, false),
        outputs: signals(&bench.outputs, true),
        wrong_bits,
        examples,
        hint: None,
        cells: Vec::new(),
        faults: Vec::new(),
        timing: None,
        stacking: None,
        by_digit: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_patterns_take_every_shift_and_each_bit_alone() {
        let found = patterns(16);
        assert_eq!(found.len(), 48);
        for value in [
            0, 0xFFFF, 0x5555, 0xAAAA, 0x3333, 0x6666, 0x0F0F, 0xF0F0, 0x8000,
            0xFFFE,
        ] {
            assert!(found.contains(&value), "{value:#x}");
        }
    }
}
