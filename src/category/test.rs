//! The test each category's builds get. Most are stateless, and state
//! only what sets them apart as a spec, which the simulator's bench runs;
//! an ALU's operations, a register's or buffer's cells, and a monostable's
//! pulse run their own way.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::{
    Kind, adder, alu, converters, decoder,
    gate::{self, Gate},
    incrementer, inverter, monostable, multiplier, negator, register,
    sequential, shifter, stacks,
};
use crate::{
    interface::{self, Interface},
    schem::Blocks,
    simulate::{
        Counts, MAX_WIDTH, Reading, Report, Run, Stacker, Stacking, Step,
        Timing, bench, throughput,
    },
};

/// The test a build of one category gets, which tries every case when they
/// number few enough, else every case of each byte and the test patterns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Test {
    /// The sums of A, B, and any carry in, or, B inverted, the differences
    /// of A and B, less any borrow in, which may read inverted.
    Adder,
    /// A's every value into one bit of an output, the one A names, off
    /// and the rest on.
    Decoder,
    /// A nand B and A xnor B, on outputs of their own.
    NandXnor,
    /// A shifted or rotated by an amount its lines give, the way its other
    /// lines choose.
    BarrelShifter,
    /// A plus 1, or plus its control line.
    Incrementer,
    /// A minus 1, or minus its control line.
    Decrementer,
    /// A plus 1 one way of its control line and minus 1 the other.
    IncDec,
    /// The products of A and B, plus C when there is one.
    Multiplier,
    /// A in binary-coded decimal: each decimal digit in four bits, the
    /// ones lowest.
    BinaryToBcd,
    /// A's number from binary-coded decimal, for each A whose digits are
    /// all decimal and whose number the outputs can hold.
    BcdToBinary,
    /// Each operation an ALU's control lines name, and any other they give.
    Alu,
    /// Each value written into every cell, then read.
    Register,
    /// Each value written into every cell, then shown with no read, as
    /// data in changes.
    Buffer,
    /// A's magnitude, A taken as a signed number.
    AbsoluteValue,
    /// −A in two's complement, or A until its control line turns on.
    Negator,
    /// The carry out of A + B, and a carry in when there is one.
    CarryOut,
    /// Whether A is greater than B, both unsigned or both in two's
    /// complement, whichever the build gets more right.
    GreaterThan,
    /// Whether A equals B.
    Equality,
    /// A pulse as each bit of A turns on, and none as it turns off, or the
    /// other way round.
    Monostable,
    /// A pulse as each bit of A turns on and as it turns off.
    DualEdgeMonostable,
    /// A, or with its control line on, every bit of A inverted.
    Inverter,
    /// How many of A's bits are on.
    Popcount,
    /// A as the strength of one output block.
    BinaryToHex,
    /// A, a hex digit a block as its strength, in binary.
    HexToBinary,
    /// Whether at least some number of A's bits are on.
    Threshold,
    /// Whether an odd number of A's bits are on, the xor of them all, or
    /// an even number, its inverse.
    Parity,
    /// A xor B.
    Xor,
    /// A xor B, every bit of it inverted.
    Xnor,
    /// A and B.
    And,
    /// A and B, every bit of it inverted.
    Nand,
    /// A or B.
    Or,
    /// A or B, every bit of it inverted.
    Nor,
    /// A value held, counted up or down as its control lines pulse.
    Counter,
    /// A value held, shifted toward its high or low bit as its control
    /// lines pulse.
    ShiftRegister,
    /// One bit held, toggled by each pulse of its one control line.
    TFlipFlop,
}

/// What a stateless test drives, reads, and expects of a build, from its
/// file and its interface, whose lines it names for their roles.
type SpecOf = fn(&Blocks, &mut Interface) -> Result<bench::Spec>;

/// How copies of a build, its file, whose own cases came out right, are
/// stacked and tested as its category's group asks; see `Test::stacking`.
type StacksOf = for<'a> fn(&'a [u8]) -> Option<Stacker<'a>>;

/// How a test runs: from a spec, as most stateless ones do, or its own
/// way.
#[derive(Clone, Copy)]
enum Way {
    /// Driven by `spec`, which `bench` runs, its copies stacking as
    /// `stacks` finds, when they are tried.
    Spec {
        spec: SpecOf,
        stacks: Option<StacksOf>,
    },
    /// Each operation an ALU's control lines name.
    Alu,
    /// Each shift a barrel shifter's lines give.
    Shifter,
    /// Each value written into a register's or buffer's every cell.
    Register(register::Kind),
    /// A pulse on one edge of A, or with `dual` on both.
    Monostable { dual: bool },
    /// A value held and changed as control lines pulse.
    Sequential(sequential::Kind),
}

impl Way {
    /// A test driven by `spec` whose copies are never tried stacked.
    const fn spec(spec: SpecOf) -> Way {
        Way::Spec { spec, stacks: None }
    }

    /// A test driven by `spec` whose copies stack as `stacks` finds.
    const fn stacked(spec: SpecOf, stacks: StacksOf) -> Way {
        Way::Spec {
            spec,
            stacks: Some(stacks),
        }
    }
}

/// The interface of the build in `blocks` with its lines named as `spec`
/// names them, and the spec.
fn bench(spec: SpecOf, blocks: &Blocks) -> Result<(Interface, bench::Spec)> {
    let mut interface = interface::read(blocks);
    let spec = spec(blocks, &mut interface)?;
    Ok((interface, spec))
}

/// An incrementer's kind of test, counting `counts`: its unsigned line is
/// named for its role, such as increment, so the cases read `A +
/// increment`.
fn counting(
    kind: Kind,
    counts: Counts,
    blocks: &Blocks,
    interface: &mut Interface,
) -> Result<bench::Spec> {
    interface::name_lines(kind.lines(), blocks, interface);
    incrementer::spec(interface, Some(counts))
}

impl Test {
    /// The category the test is made for, which its results are stored
    /// under the name of, and how it runs.
    #[allow(clippy::too_many_lines, reason = "a table, one arm per test")]
    fn made(self) -> (Kind, Way) {
        match self {
            Test::Adder => (
                Kind::Adder,
                Way::stacked(|_, i| adder::adder(i), stacks::adder),
            ),
            Test::Decoder => {
                (Kind::Decoder, Way::spec(|_, i| decoder::spec(i)))
            }
            Test::NandXnor => (
                Kind::NandXnorGate,
                Way::stacked(
                    |_, i| gate::nand_xnor(i),
                    gate::nand_xnor_stacker,
                ),
            ),
            Test::BarrelShifter => (Kind::BarrelShifter, Way::Shifter),
            Test::Incrementer => (
                Kind::Incrementer,
                Way::stacked(
                    |b, i| counting(Kind::Incrementer, Counts::Up, b, i),
                    |data| stacks::chained(data, Test::Incrementer),
                ),
            ),
            Test::Decrementer => (
                Kind::Decrementer,
                Way::stacked(
                    |b, i| counting(Kind::Decrementer, Counts::Down, b, i),
                    |data| stacks::chained(data, Test::Decrementer),
                ),
            ),
            Test::IncDec => (
                Kind::IncDec,
                Way::spec(|b, i| counting(Kind::IncDec, Counts::Either, b, i)),
            ),
            Test::Multiplier => (
                Kind::Multiplier,
                // Its lone control line is named for its role, calculate.
                Way::spec(|b, i| {
                    interface::name_lines(Kind::Multiplier.lines(), b, i);
                    multiplier::spec(i)
                }),
            ),
            Test::BinaryToBcd => (
                Kind::BinaryToBcd,
                Way::spec(|_, i| converters::binary_to_bcd(i)),
            ),
            Test::BcdToBinary => (
                Kind::BcdToBinary,
                Way::spec(|_, i| converters::bcd_to_binary(i)),
            ),
            Test::Alu => (Kind::Alu, Way::Alu),
            Test::Register => {
                (Kind::Register, Way::Register(register::Kind::Register))
            }
            Test::Buffer => {
                (Kind::Buffer, Way::Register(register::Kind::Buffer))
            }
            Test::AbsoluteValue => (
                Kind::AbsoluteValue,
                Way::spec(|_, i| converters::absolute_value(i)),
            ),
            Test::Negator => (
                Kind::Negator,
                // Its lone control line is named for its role, negate.
                Way::spec(|b, i| {
                    interface::name_lines(Kind::Negator.lines(), b, i);
                    negator::spec(i)
                }),
            ),
            Test::CarryOut => (
                Kind::CarryOut,
                Way::stacked(
                    |_, i| adder::carry_out(i),
                    |data| stacks::chained(data, Test::CarryOut),
                ),
            ),
            Test::GreaterThan => {
                (Kind::GreaterThan, Way::spec(|_, i| adder::greater_than(i)))
            }
            Test::Equality => {
                (Kind::Equality, Way::spec(|_, i| adder::equality(i)))
            }
            Test::Monostable => {
                (Kind::Monostable, Way::Monostable { dual: false })
            }
            Test::DualEdgeMonostable => {
                (Kind::DualEdgeMonostable, Way::Monostable { dual: true })
            }
            Test::Inverter => (
                Kind::Inverter,
                // Its lone control line is named for its role, invert.
                Way::stacked(
                    |b, i| {
                        interface::name_lines(Kind::Inverter.lines(), b, i);
                        inverter::spec(i)
                    },
                    inverter::stacker,
                ),
            ),
            Test::Popcount => {
                (Kind::Popcount, Way::spec(|_, i| converters::popcount(i)))
            }
            Test::BinaryToHex => (
                Kind::BinaryToHex,
                Way::spec(|_, i| converters::binary_to_hex(i)),
            ),
            Test::HexToBinary => (
                Kind::HexToBinary,
                Way::spec(|_, i| converters::hex_to_binary(i)),
            ),
            Test::Threshold => {
                (Kind::Threshold, Way::spec(|_, i| converters::threshold(i)))
            }
            Test::Parity => {
                (Kind::Parity, Way::spec(|_, i| converters::parity(i)))
            }
            Test::Xor => (
                Kind::XorGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Xor),
                    |data| gate::stacker(data, Gate::Xor),
                ),
            ),
            Test::Xnor => (
                Kind::XnorGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Xnor),
                    |data| gate::stacker(data, Gate::Xnor),
                ),
            ),
            Test::And => (
                Kind::AndGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::And),
                    |data| gate::stacker(data, Gate::And),
                ),
            ),
            Test::Nand => (
                Kind::NandGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Nand),
                    |data| gate::stacker(data, Gate::Nand),
                ),
            ),
            Test::Or => (
                Kind::OrGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Or),
                    |data| gate::stacker(data, Gate::Or),
                ),
            ),
            Test::Nor => (
                Kind::NorGate,
                Way::stacked(
                    |_, i| gate::spec(i, Gate::Nor),
                    |data| gate::stacker(data, Gate::Nor),
                ),
            ),
            Test::Counter => {
                (Kind::Counter, Way::Sequential(sequential::Kind::Counter))
            }
            Test::ShiftRegister => (
                Kind::ShiftRegister,
                Way::Sequential(sequential::Kind::ShiftRegister),
            ),
            Test::TFlipFlop => (
                Kind::TFlipFlop,
                Way::Sequential(sequential::Kind::TFlipFlop),
            ),
        }
    }

    /// The category it is made for, whose name its results are stored
    /// under: an adder's for a hex adder's and a subtractor's too.
    pub fn kind(self) -> Kind {
        self.made().0
    }

    fn way(self) -> Way {
        self.made().1
    }

    /// Every test, each its own category's, in the order of their
    /// categories.
    pub fn all() -> impl Iterator<Item = Test> {
        Kind::all().filter_map(|kind| kind.test().filter(|t| t.kind() == kind))
    }

    /// The test a build of `category` and `width` gets: its category's,
    /// when it has one and the build is no wider than `MAX_WIDTH`.
    pub fn of(category: &str, width: Option<i64>) -> Option<Self> {
        let tested = |w: i64| {
            usize::try_from(w).is_ok_and(|w| (1..=MAX_WIDTH).contains(&w))
        };
        if !width.is_some_and(tested) {
            return None;
        }
        super::behaviour(category).test
    }

    /// The name stored with its result, which is its category's.
    pub fn name(self) -> &'static str {
        self.kind().name()
    }

    /// Runs the test's cases on `blocks` and checks each result.
    pub fn run(self, blocks: &Blocks) -> Report {
        Report::of(self.check(blocks, false))
    }

    /// A few telling cases of the test run on `blocks`, as its colours mark
    /// its lines: enough to tell one reading of a build from another.
    pub fn quick(self, blocks: &Blocks) -> Result<Run> {
        self.check(blocks, true)
    }

    /// The test's cases run on `blocks` through `interface`, as probing
    /// finds it rather than as colours mark it, or with `quick` a few
    /// telling ones. Only a test driven by a spec, such as an adder's,
    /// runs so; any other fails.
    pub fn check_with(
        self,
        blocks: &Blocks,
        mut interface: Interface,
        quick: bool,
    ) -> Result<Run> {
        let Way::Spec { spec, .. } = self.way() else {
            anyhow::bail!("{} runs its own way", self.kind().a());
        };
        let spec = spec(blocks, &mut interface)?;
        spec.check(blocks, &interface, quick)
    }

    /// The test's cases run on `blocks`, or with `quick` a few telling
    /// ones, quick enough for a guess at what a build is.
    pub(super) fn check(self, blocks: &Blocks, quick: bool) -> Result<Run> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, spec) = bench(spec, blocks)?;
                let mut run = spec.check(blocks, &interface, quick)?;
                // An adder's kinds name the lines it reads inverted.
                if self == Test::Adder {
                    run.inverted = run.adder_variant().lines(&run);
                }
                Ok(run)
            }
            Way::Alu => alu::check(blocks, quick),
            Way::Shifter => shifter::check(blocks, quick),
            Way::Register(kind) => register::check(blocks, kind, quick),
            Way::Monostable { dual } => monostable::check(blocks, dual),
            Way::Sequential(kind) => sequential::check(blocks, kind),
        }
    }

    /// Whether the build in `blocks`, whose cases all came out right, holds
    /// state, as `bench::remembers` tells: what is left of each case once
    /// its presses are over, against a fresh copy given only the case's
    /// inputs. A register's or buffer's cases are what it holds once its
    /// data bus has changed, so passing them is holding state, as a
    /// counter's, shift register's, or T flip-flop's are; an ALU's is
    /// told of each of its operations; a monostable, a pulse generator,
    /// holds none, its pulse being its input's change, not a held value.
    pub fn remembers(self, blocks: &Blocks) -> Result<Option<bool>> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, spec) = bench(spec, blocks)?;
                bench::remembers(blocks, &interface, &spec.bench).map(Some)
            }
            Way::Register(_) | Way::Sequential(_) => Ok(Some(true)),
            Way::Alu => alu::remembers(blocks).map(Some),
            Way::Shifter => shifter::remembers(blocks).map(Some),
            Way::Monostable { .. } => Ok(Some(false)),
        }
    }

    /// Whether copies of the build in `data`, whose own cases came out
    /// right, stack into a wider one of its kind, tried at each pitch
    /// class as its category's group asks: an adder's, an incrementer's,
    /// and a carry out's copies must chain; a gate's, a monostable's, and a
    /// T flip-flop's each work alone beside the others; an inverter's, a register's, and
    /// a buffer's share lines, as many copies as work counted, and a
    /// register's copies may also add words on its shared bus. `None` for
    /// any other, and for a category whose builds never stack, as a
    /// decoder's, whose copies side by side are separate decoders.
    pub fn stacking(self, data: &[u8]) -> Option<Stacking> {
        if self.kind().facts().behaviour.stacking
            == crate::category::Stacking::Never
        {
            return None;
        }
        match self.way() {
            Way::Register(kind) => register::stacking(data, kind),
            _ => Some(self.stacker(data)?.stacking()),
        }
    }

    /// How copies of the build in `data` are stacked and tested, as
    /// [`Test::stacking`] tries each pitch, for one step at a time; see
    /// [`Stacker::at`]. `None` for a test whose copies are never stacked.
    pub fn stacker(self, data: &[u8]) -> Option<Stacker<'_>> {
        match self.way() {
            Way::Spec { stacks, .. } => stacks.and_then(|stacks| stacks(data)),
            Way::Register(kind) => register::stacker(data, kind),
            Way::Monostable { dual } => monostable::stacker(data, dual),
            Way::Sequential(sequential::Kind::TFlipFlop) => {
                sequential::toggle_stacker(data)
            }
            Way::Alu | Way::Shifter | Way::Sequential(_) => None,
        }
    }

    /// Whether `copies` of the build in `data`, whose own cases came out
    /// right, stacked `step` make a wider one of its kind, as its own
    /// `stacking` tests each step, the box rule's box first, then boxes
    /// leaving out more of the first copy: the step with the box that
    /// works and the cases run, or why not. Copies apart, as
    /// `stack::touches` has them, never stack. A register's copies are tried as more words on
    /// its shared bus, then as more bits. `None` for a test whose copies
    /// are never stacked.
    pub fn stacks_at(
        self,
        data: &[u8],
        step: Step,
        copies: usize,
    ) -> Option<Result<(Step, u64), String>> {
        if let Way::Register(kind) = self.way() {
            return register::stacks_at(data, kind, step, copies)
                .map(|found| found.map(|(step, cases, _)| (step, cases)));
        }
        Some(self.stacker(data)?.at(step, copies))
    }

    /// How often the build in `blocks`, its file `data`, timed as
    /// `timing`, takes a new case, as `throughput::throughput` streams
    /// them, through its lines named as its test names them, as its timing
    /// names them, so each goes in at its own tick; a register's cases are
    /// its accesses, as `register::throughput` streams them. Meant for a
    /// build whose cases all came out right.
    pub fn throughput(
        self,
        blocks: &Blocks,
        data: &[u8],
        timing: &Timing,
    ) -> Result<Option<i32>> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, _) = bench(spec, blocks)?;
                throughput::throughput(blocks, &interface, timing)
            }
            Way::Register(kind) => register::throughput(data, timing, kind),
            Way::Monostable { .. } => monostable::throughput(blocks, timing),
            Way::Alu => alu::throughput(blocks, timing),
            Way::Shifter => shifter::throughput(blocks, timing),
            Way::Sequential(kind) => sequential::throughput(blocks, kind),
        }
    }

    /// The lines of `blocks` named as its timing names them, such as an
    /// adder's A, B, and carry in; for a test that names none, as
    /// [`interface::read`] finds them.
    pub fn lines(self, blocks: &Blocks) -> Result<Interface> {
        match self.way() {
            Way::Spec { spec, .. } => Ok(bench(spec, blocks)?.0),
            _ => Ok(interface::read(blocks)),
        }
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        self.timed(blocks, false, cancel)
    }

    /// [`Test::time`], its inputs taking the test patterns whatever their
    /// width, as a stack of copies is timed: fewer cases, which still make
    /// its carries run furthest.
    pub fn time_by_patterns(
        self,
        blocks: &Blocks,
        cancel: &AtomicBool,
    ) -> Result<Timing> {
        self.timed(blocks, true, cancel)
    }

    /// [`Test::time`], by the test patterns when `patterned`.
    fn timed(
        self,
        blocks: &Blocks,
        patterned: bool,
        cancel: &AtomicBool,
    ) -> Result<Timing> {
        match self.way() {
            Way::Spec { spec, .. } => {
                let (interface, spec) = bench(spec, blocks)?;
                // An adder's slowest changes are its longest carries, which
                // the test patterns drive: timed by them, every live adder
                // times as it did trying every case, in half the time.
                let patterned = patterned || self == Test::Adder;
                spec.time(blocks, &interface, patterned, cancel)
            }
            Way::Alu => alu::time(blocks, cancel),
            Way::Shifter => shifter::time(blocks, cancel),
            Way::Register(kind) => register::time(blocks, kind, cancel),
            Way::Monostable { dual } => monostable::time(blocks, dual, cancel),
            Way::Sequential(kind) => sequential::time(blocks, kind, cancel),
        }
    }
}

impl Run {
    /// The category of a build the adder's test ran as this: an inverted
    /// adder when its sum reads inverted, a subtractor when it subtracts
    /// B, else a hex adder when its inputs take `hex` digits, else an
    /// adder.
    pub fn adder_category(&self, hex: bool) -> Kind {
        let variant = self.adder_variant();
        if variant.sum {
            Kind::InvertedAdder
        } else if variant.subtract {
            Kind::Subtractor
        } else if hex {
            Kind::HexAdder
        } else {
            Kind::Adder
        }
    }
}

/// What a build seems to be, from a few telling values run through each
/// test its inputs and outputs fit.
#[derive(Debug, Clone)]
pub struct Guessed {
    /// Its category, such as `adder`.
    pub category: &'static str,
    /// What its cases are, such as `A + B + carry in`.
    pub pattern: String,
    pub cases: u64,
    pub wrong: u64,
    /// How many of its lines it reads inverted.
    pub inverted: usize,
}

impl Guessed {
    /// The share of its cases it got wrong, in thousandths, which ranks one
    /// guess against another.
    pub fn wrong_share(&self) -> u64 {
        self.wrong * 1000 / self.cases.max(1)
    }
}

/// What the build in `blocks` seems to be: of the tests its inputs and
/// outputs fit, the one whose telling values come out right most often, if
/// three in four do, reading the fewest control lines inverted on a tie.
/// Quick enough to run before anyone says what it is, even for a hex
/// build, which runs slower.
pub fn guess(blocks: &Blocks) -> Option<Guessed> {
    // An ALU's lines are tried only when their signs name operations, as
    // any build with a control line might otherwise pass for one.
    let names_ops = interface::read(blocks)
        .inputs
        .iter()
        .filter(|port| port.control())
        .flat_map(|port| port.words.iter())
        .any(|word| alu::Op::named(word).is_some());
    // Inputs taking a hex digit per block, a signal strength, are a hex
    // adder's.
    let hex = interface::takes_digits(blocks);
    Kind::guessed()
        .into_iter()
        .filter(|&(_, test)| test != Test::Alu || names_ops)
        .filter_map(|(kind, test)| {
            // One run tries every kind of incrementer, and the kind it gets
            // most right names its category.
            let run = if test == Test::Incrementer {
                let interface = interface::read(blocks);
                incrementer::spec(&interface, None)
                    .and_then(|spec| spec.check(blocks, &interface, true))
            } else {
                test.check(blocks, true)
            }
            .ok()?;
            let kind = match run.reading {
                Some(Reading::Incrementer { counts }) => counts.kind(),
                _ if test == Test::Adder => run.adder_category(hex),
                _ => kind,
            };
            Some(Guessed {
                category: kind.name(),
                pattern: run.pattern,
                cases: run.cases,
                wrong: run.wrong,
                inverted: run.inverted.len(),
            })
        })
        // Three in four right, so a build with a few wrong bits is still
        // known for what it is.
        .filter(|guess| (guess.cases - guess.wrong) * 4 >= guess.cases * 3)
        .min_by_key(|guess| (guess.wrong_share(), guess.inverted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_up_to_64_bits_are_tested() {
        assert_eq!(Test::of("adder", Some(16)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(64)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(65)), None);
        assert_eq!(Test::of("adder", None), None);
        assert_eq!(Test::of("register", Some(8)), Some(Test::Register));
        assert_eq!(Test::of("ram", Some(8)), None);
        assert_eq!(Test::of("multiplier", Some(65)), None);
    }
}
