//! Running a stateless build through its cases: what a test drives and
//! reads, which cases it tries, and how what came out is judged. Each such
//! test states only what sets it apart, as a [`Spec`], and runs the same
//! way.

use std::{collections::HashSet, sync::atomic::AtomicBool};

use anyhow::{Context, Result, bail};

use super::{
    Reading, Run, Signal, Timing, Value, Wrong,
    circuit::{Circuit, input_bits, output_bits},
    timing,
};
use crate::{recognize::interface::Interface, schem::Blocks};

/// Up to this many input bits, every combination is tried; beyond it, every
/// case of each byte and every pairing of the test patterns.
pub(super) const EXHAUSTIVE_BITS: usize = 20;

/// The widest input whose every bit `patterns` sets and clears alone.
const WIDE: usize = 32;

/// One case's input values, in the order of its bench's inputs; those it
/// has no input for are 0.
pub(super) type Case = [Value; 3];

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
    pub held: Vec<(usize, Value)>,
    /// The bits of the outputs read together that the cases decide; the
    /// rest, such as a status line an operation leaves meaningless, are
    /// not checked.
    pub care: Value,
    /// A control line pressed once each case's inputs settle, as a
    /// sequential multiplier's calculate.
    pub press: Option<usize>,
    /// Whether its control lines may read inverted, powered meaning 0,
    /// which judging tries; not where the kinds name the lines that are,
    /// as an adder's do. A data input, such as an operand, never reads
    /// inverted, nor does an output, its every bit an answer.
    pub invertible: bool,
}

impl Bench {
    /// Fails for a bench too wide to run on `interface`, which the cases'
    /// values cannot hold: an input wider than `MAX_WIDTH`, whose sums or
    /// products would overflow one, or outputs of more bits together than
    /// one holds, as they are read as one. A test asked of a build by
    /// its category is held to its width already, see `Test::of`, but a
    /// guess at what a build is, and its copies stacked wider, are not.
    pub fn fits(&self, interface: &Interface) -> Result<()> {
        if let Some((&port, bits)) = self
            .inputs
            .iter()
            .zip(self.widths(interface))
            .find(|&(_, bits)| bits > super::MAX_WIDTH)
        {
            bail!(
                "Too wide to simulate: {} has {bits} bits, and the simulator \
                 takes at most {}.",
                interface.inputs[port].name,
                super::MAX_WIDTH
            );
        }
        let bits: usize = self.output_widths(interface).iter().sum();
        if bits > Value::BITS as usize {
            bail!(
                "Too wide to simulate: its outputs have {bits} bits, and the \
                 simulator reads at most {}.",
                Value::BITS
            );
        }
        Ok(())
    }

    /// A bench driving `inputs` and reading `outputs`, every bit of which
    /// the cases decide, with nothing held.
    pub fn new(inputs: Vec<usize>, outputs: Vec<usize>, pattern: &str) -> Self {
        Bench {
            inputs,
            outputs,
            pattern: pattern.to_owned(),
            held: Vec::new(),
            care: Value::MAX,
            press: None,
            invertible: true,
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
    /// Whether the cases, when there are too many to try every one, take
    /// each hex digit's every value rather than each byte's, as a hex
    /// adder's do.
    pub by_digit: bool,
}

/// One thing a build may compute: its cases' name, such as `A + B`, what
/// sets it apart from the test's other kinds where more depends on it,
/// and what it gives for a case, before it is cut to the outputs' bits.
pub(super) struct Kind {
    pub pattern: String,
    pub reading: Option<Reading>,
    pub expected: Box<dyn Fn(&Case) -> Value>,
}

impl Kind {
    pub fn new(
        pattern: &str,
        expected: impl Fn(&Case) -> Value + 'static,
    ) -> Self {
        Kind {
            pattern: pattern.to_owned(),
            reading: None,
            expected: Box::new(expected),
        }
    }

    /// The kind, set apart from the test's others by `reading`, which a run
    /// it wins keeps.
    pub fn reading(self, reading: Reading) -> Self {
        Kind {
            reading: Some(reading),
            ..self
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
    Only(Box<dyn Fn(bool) -> (Vec<Value>, bool)>),
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
        self.bench.fits(interface)?;
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
        let run = best(interface, &self.bench, &self.kinds, &cases, &got)
            .context("a test has something to check")?;
        let mut run = run.covering(exhaustive, patterns);
        // How weakly its data inputs may be driven, once it gets every case
        // right; a build that can't be measured so says nothing of it.
        if !quick && run.wrong == 0 {
            run.strength = super::strength::measure(
                blocks,
                interface,
                &self.bench,
                &cases,
                &got,
            )
            .ok()
            .flatten();
        }
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
        self.bench.fits(interface)?;
        if self.bench.press.is_some() {
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

/// `bits` ones, the lowest bits.
pub(super) fn mask(bits: usize) -> Value {
    match bits {
        0 => 0,
        bits => Value::MAX >> (Value::BITS as usize).saturating_sub(bits),
    }
}

/// Values of `width` bits that make an adder carry far and across its
/// groups: none and all set; ones and zeros alternating in runs of one,
/// two, and four, at every shift; and each bit alone set or clear. Past
/// `WIDE` bits, only each hex digit's lowest and highest bit is set or
/// cleared alone, a carry into it and out of it, so a 64-bit build pairs
/// as many as a 32-bit one.
pub(super) fn patterns(width: usize) -> Vec<Value> {
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
    let alone = |bit: usize| width <= WIDE || matches!(bit % 4, 0 | 3);
    for bit in (0..width).filter(|&bit| alone(bit)) {
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
                *value = Value::from(packed) >> shift & mask(width);
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
    let choices: Vec<Vec<Value>> = widths
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
fn telling(width: usize) -> Vec<Value> {
    let all = mask(width);
    let mut values: Vec<Value> = [
        0,
        1,
        Value::MAX,
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
type Flip = Option<(usize, Value)>;

/// Which of a bench's inputs read inverted, a bit per input in the order
/// of its inputs.
type Inversion = u64;

/// Each way a bench's lines may read inverted, the fewest inverted first:
/// each of its control lines, an input such as a carry in or a
/// clock, inverted or not. Its data inputs and its outputs always read as
/// they are, an inverted output being a wrong answer.
fn inversions(bench: &Bench, interface: &Interface) -> Vec<Inversion> {
    if !bench.invertible {
        return vec![0];
    }
    let lines: Vec<u64> = (0..bench.inputs.len())
        .filter(|&i| {
            let port = &interface.inputs[bench.inputs[i]];
            port.control() && !port.signal_strength
        })
        .map(|i| 1 << i)
        .collect();
    let mut all: Vec<(u32, Inversion)> = (0..1u64 << lines.len())
        .map(|chosen| {
            let inversion = lines
                .iter()
                .enumerate()
                .filter(|&(i, _)| chosen >> i & 1 == 1)
                .fold(0, |inversion, (_, &bit)| inversion | bit);
            (chosen.count_ones(), inversion)
        })
        .collect();
    all.sort_by_key(|&(count, _)| count);
    all.into_iter().map(|(_, inversion)| inversion).collect()
}

/// Judging enough cases to rule most readings out before judging all.
const FIRST_LOOK: usize = 256;

/// The kind, and the way its control lines read inverted, that `got` gets
/// most right over `cases`, the fewest lines inverted winning a tie,
/// judged: each reading first on a few cases, then those that do best
/// there on all. Reading lines inverted needs no more simulating, as an
/// inverted input only changes the case a run of the circuit was.
fn best(
    interface: &Interface,
    bench: &Bench,
    kinds: &[Kind],
    cases: &[Case],
    got: &[Value],
) -> Option<Run> {
    let widths = bench.widths(interface);
    let readings: Vec<(&Kind, Inversion)> = kinds
        .iter()
        .flat_map(|kind| {
            inversions(bench, interface)
                .into_iter()
                .map(move |inversion| (kind, inversion))
        })
        .collect();
    let expected = |kind, inversion| reading(kind, &widths, inversion);
    let first = cases.len().min(FIRST_LOOK);
    let looks: Vec<u64> = readings
        .iter()
        .map(|&(kind, inversion)| {
            judge(
                interface,
                bench,
                &cases[..first],
                &got[..first],
                expected(kind, inversion),
            )
            .wrong
        })
        .collect();
    let fewest = *looks.iter().min()?;
    readings
        .iter()
        .zip(&looks)
        .filter(|&(_, &wrong)| wrong == fewest)
        .map(|(&(kind, inversion), _)| {
            let mut run =
                judge(interface, bench, cases, got, expected(kind, inversion));
            run.pattern.clone_from(&kind.pattern);
            run.reading = kind.reading;
            run.inverted = bench
                .inputs
                .iter()
                .enumerate()
                .filter(|&(i, _)| inversion >> i & 1 == 1)
                .map(|(_, &port)| interface.inputs[port].name.clone())
                .collect();
            run
        })
        // The fewest lines inverted wins a tie.
        .min_by_key(|run| (run.wrong, run.inverted.len()))
}

/// What `kind` gives for a case, its inputs `inversion` names inverted,
/// each `widths` bits.
fn reading<'k>(
    kind: &'k Kind,
    widths: &'k [usize],
    inversion: Inversion,
) -> impl Fn(&Case) -> Value + 'k {
    move |case: &Case| {
        let mut case = *case;
        for (i, &width) in widths.iter().enumerate() {
            if inversion >> i & 1 == 1 {
                case[i] ^= mask(width);
            }
        }
        (kind.expected)(&case)
    }
}

/// Runs `cases` through the bench's inputs, driving the bits `flip` gives,
/// if any, the other way round, and checks each result against `expected`,
/// cut to as many bits as the outputs hold.
pub(super) fn sweep(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    flip: Flip,
    expected: impl Fn(&Case) -> Value,
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
) -> Result<Vec<Value>> {
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
        if let Some(port) = bench.press {
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

/// Whether the build holds state: a copy run through the telling cases in
/// turn, making the press the test makes, against a fresh copy for each
/// case given only its inputs, nothing pressed and every control at rest.
/// An output that differs was left behind by what came before.
pub(super) fn remembers(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
) -> Result<bool> {
    let (mut cases, _) = paired(&bench.widths(interface));
    cases.truncate(32);
    let history = observe(blocks, interface, bench, &cases, None)?;
    let rest = Bench {
        inputs: bench.inputs.clone(),
        outputs: bench.outputs.clone(),
        pattern: String::new(),
        held: bench.held.clone(),
        care: bench.care,
        press: None,
        invertible: false,
    };
    for (case, &was) in cases.iter().zip(&history) {
        let fresh = observe(blocks, interface, &rest, &[*case], None)?;
        if fresh[0] & bench.care != was & bench.care {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the build holds state with any of `settings` held, each a
/// value for the lines that choose what it does, as an ALU's operation, as
/// [`remembers`] tells of a bench driving `inputs` and reading `outputs`.
pub(super) fn remembers_any(
    blocks: &Blocks,
    interface: &Interface,
    (inputs, outputs): (&[usize], &[usize]),
    settings: impl IntoIterator<Item = Vec<(usize, Value)>>,
) -> Result<bool> {
    for held in settings {
        let bench = Bench {
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
            pattern: String::new(),
            held,
            care: Value::MAX,
            press: None,
            invertible: false,
        };
        if remembers(blocks, interface, &bench)? {
            return Ok(true);
        }
    }
    Ok(false)
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
pub(super) fn judge(
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    got: &[Value],
    expected: impl Fn(&Case) -> Value,
) -> Run {
    let widths = bench.output_widths(interface);
    let result_bits: usize = widths.iter().sum();
    // Each output's value from a result, the first holding the lowest bits.
    let split = |result: Value| {
        let mut shift = 0;
        widths
            .iter()
            .map(|&width| {
                let value = result >> shift & mask(width);
                shift += width;
                value
            })
            .collect::<Vec<Value>>()
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
                *count += u64::from((want ^ got) >> bit & 1 == 1);
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
        inverted: Vec::new(),
        reading: None,
        strength: None,
        cells: Vec::new(),
        faults: Vec::new(),
        timing: None,
        stacking: None,
        design: None,
        by_digit: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        recognize::interface::{Bit, Port},
        schem::lines::Role,
    };

    /// A port of `bits` bits named `name`, of `role`.
    fn port(name: &str, role: Role, bits: usize) -> Port {
        Port {
            name: name.to_owned(),
            role: Some(role),
            labelled: false,
            bits: (0..bits)
                .map(|_| Bit {
                    pos: [0, 0, 0],
                    entry: None,
                    hard: false,
                    inverted: false,
                })
                .collect(),
            strength: 1,
            words: Vec::new(),
            signal_strength: false,
        }
    }

    /// The run `kinds` best explain of what a build of `inputs` and
    /// `outputs` showed, `got`, for every case of its inputs.
    fn judged(
        inputs: Vec<Port>,
        outputs: Vec<Port>,
        kinds: &[Kind],
        got: impl Fn(&Case) -> Value,
    ) -> Run {
        let interface = Interface {
            inputs,
            outputs,
            binary_outputs: false,
        };
        let bench = Bench::new(
            (0..interface.inputs.len()).collect(),
            (0..interface.outputs.len()).collect(),
            "",
        );
        let (cases, _, _) = cases(&bench.widths(&interface), || {
            unreachable!("few enough bits to try every case")
        });
        let got: Vec<Value> = cases.iter().map(got).collect();
        best(&interface, &bench, kinds, &cases, &got).unwrap()
    }

    /// A control line, such as an incrementer's, may read inverted: on, it
    /// means 0. The test finds it, and names it.
    #[test]
    fn a_control_line_may_read_inverted() {
        let run = judged(
            vec![
                port("A", Role::Operand(0), 4),
                port("increment", Role::Control, 1),
            ],
            vec![port("output", Role::Output(0), 4)],
            &[Kind::new("A + increment", |case| case[0] + case[1])],
            |case| (case[0] + (case[1] ^ 1)) & 0xF,
        );
        assert_eq!(run.wrong, 0);
        assert_eq!(run.inverted, ["increment"]);
    }

    /// A data input never reads inverted: a build that gives A + 1 for
    /// every bit of A inverted is no incrementer.
    #[test]
    fn a_data_input_never_reads_inverted() {
        let run = judged(
            vec![port("A", Role::Operand(0), 4)],
            vec![port("output", Role::Output(0), 4)],
            &[Kind::new("A + 1", |case| case[0] + 1)],
            |case| ((!case[0] & 0xF) + 1) & 0xF,
        );
        assert!(run.wrong > 0);
        assert!(run.inverted.is_empty(), "{:?}", run.inverted);
    }

    /// An output never reads inverted, a status line no more than a bus:
    /// an equality comparator whose output is on when A and B differ gets
    /// every case wrong.
    #[test]
    fn an_output_never_reads_inverted() {
        let run = judged(
            vec![
                port("A", Role::Operand(0), 2),
                port("B", Role::Operand(1), 2),
            ],
            vec![port("A = B", Role::Status, 1)],
            &[Kind::new("A = B", |case| Value::from(case[0] == case[1]))],
            |case| Value::from(case[0] != case[1]),
        );
        assert_eq!(run.wrong, run.cases);
    }

    /// A decoder's outputs are inverted, its selected bit off and the rest
    /// on; one whose selected bit is on gets every case wrong.
    #[test]
    fn a_decoders_selected_bit_is_off() {
        let interface = || {
            (
                vec![port("A", Role::Operand(0), 2)],
                vec![port("output", Role::Output(0), 4)],
            )
        };
        let (inputs, outputs) = interface();
        let spec = super::super::decoder::spec(&Interface {
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            binary_outputs: false,
        })
        .unwrap();
        let off =
            judged(inputs, outputs, &spec.kinds, |case| !(1 << case[0]) & 0xF);
        assert_eq!((off.wrong, off.pattern.as_str()), (0, "bit A off"));
        let (inputs, outputs) = interface();
        let on = judged(inputs, outputs, &spec.kinds, |case| 1 << case[0]);
        assert_eq!(on.wrong, on.cases);
    }

    /// A parity circuit may be on for an odd number of A's bits on, or for
    /// an even number, its inverse, each found as its own kind.
    #[test]
    fn a_parity_circuit_is_odd_or_even() {
        let interface = || {
            (
                vec![port("A", Role::Operand(0), 4)],
                vec![port("output", Role::Output(0), 1)],
            )
        };
        let (inputs, outputs) = interface();
        let spec = super::super::converters::parity(&Interface {
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
        assert_eq!((run.wrong, run.cases), (0, 16));
        assert_eq!(run.pattern, super::super::converters::EVEN_PARITY);
        assert!(run.inverted.is_empty());
    }

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
