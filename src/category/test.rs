//! The test each category's builds get. Most are stateless, and state
//! only what sets them apart as a spec, which the simulator's bench runs;
//! an ALU's operations, a register's or buffer's cells, and a monostable's
//! pulse run their own way.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::{Kind, alu, incrementer};
use crate::{
    interface::{self, Interface},
    schem::Blocks,
    simulate::{
        MAX_WIDTH, Reading, Report, Run, Stacker, Stacking, Step, Timing,
        bench, throughput,
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
pub(super) type SpecOf = fn(&Blocks, &mut Interface) -> Result<bench::Spec>;

/// How copies of a build, its file, whose own cases came out right, are
/// stacked and tested as its category's group asks; see `Test::stacking`.
pub(super) type StacksOf = for<'a> fn(&'a [u8]) -> Option<Stacker<'a>>;

/// How a category's test runs: its cases, and what is measured of a build
/// that gets them all right. Most are a spec's, which the bench runs, see
/// [`BySpec`]; an ALU's operations, a barrel shifter's shifts, a
/// register's cells, a monostable's pulse, and a counter's held value run
/// their own way. Each category's row names its own; see `Facts::runs`.
pub(super) trait Runs: Sync + std::fmt::Debug {
    /// The test's cases run on `blocks`, or with `quick` a few telling
    /// ones, quick enough for a guess at what a build is.
    fn check(&self, blocks: &Blocks, quick: bool) -> Result<Run>;

    /// The cases run through `interface`, as probing finds it rather than
    /// as the file marks it; `None` for a test that reads a build's lines
    /// its own way.
    fn check_with(
        &self,
        _blocks: &Blocks,
        _interface: Interface,
        _quick: bool,
    ) -> Option<Result<Run>> {
        None
    }

    /// Whether the build in `blocks`, whose cases all came out right,
    /// holds state; `None` where nothing tells.
    fn remembers(&self, blocks: &Blocks) -> Result<Option<bool>>;

    /// How copies of the build in `data` are stacked and tested, one step
    /// at a time; `None` for a test whose copies are never stacked.
    fn stacker<'a>(&self, _data: &'a [u8]) -> Option<Stacker<'a>> {
        None
    }

    /// Whether copies of the build in `data` stack into a wider one, tried
    /// at each pitch class.
    fn stacking(&self, data: &[u8]) -> Option<Stacking> {
        Some(self.stacker(data)?.stacking())
    }

    /// Whether `copies` of the build in `data` stacked `step` make a wider
    /// one: the step with the box that works and the cases run, or why
    /// not.
    fn stacks_at(
        &self,
        data: &[u8],
        step: Step,
        copies: usize,
    ) -> Option<Result<(Step, u64), String>> {
        Some(self.stacker(data)?.at(step, copies))
    }

    /// How often the build in `blocks`, its file `data`, timed as `timing`,
    /// takes a new case.
    fn throughput(
        &self,
        blocks: &Blocks,
        data: &[u8],
        timing: &Timing,
    ) -> Result<Option<i32>>;

    /// The lines of `blocks` named as its timing names them; as
    /// [`interface::read`] finds them for a test that names none.
    fn lines(&self, blocks: &Blocks) -> Result<Interface> {
        Ok(interface::read(blocks))
    }

    /// Measures when `blocks`' outputs settle, its inputs taking the test
    /// patterns whatever their width when `patterned`, stopping early once
    /// `cancel` is set.
    fn time(
        &self,
        blocks: &Blocks,
        patterned: bool,
        cancel: &AtomicBool,
    ) -> Result<Timing>;
}

/// A test driven by a spec, which the bench runs, as most stateless ones
/// are.
#[derive(Debug)]
pub(super) struct BySpec {
    spec: SpecOf,
    /// How its copies stack, when they are tried.
    stacks: Option<StacksOf>,
    /// What its cases, once judged, add to a run, as an adder's name the
    /// lines it reads inverted.
    reads: Option<fn(&mut Run)>,
    /// Whether it is always timed by the test patterns, as an adder is:
    /// its slowest changes are its longest carries, which the patterns
    /// drive, so every live adder times as it did trying every case, in
    /// half the time.
    patterned: bool,
}

impl BySpec {
    /// A test driven by `spec` whose copies are never tried stacked.
    pub(super) const fn spec(spec: SpecOf) -> Self {
        BySpec {
            spec,
            stacks: None,
            reads: None,
            patterned: false,
        }
    }

    /// A test driven by `spec` whose copies stack as `stacks` finds.
    pub(super) const fn stacked(spec: SpecOf, stacks: StacksOf) -> Self {
        BySpec {
            stacks: Some(stacks),
            ..Self::spec(spec)
        }
    }

    /// It, its cases adding to a run what `reads` does.
    pub(super) const fn reading(self, reads: fn(&mut Run)) -> Self {
        BySpec {
            reads: Some(reads),
            ..self
        }
    }

    /// It, always timed by the test patterns.
    pub(super) const fn patterned(self) -> Self {
        BySpec {
            patterned: true,
            ..self
        }
    }

    /// The interface of the build in `blocks` with its lines named as the
    /// spec names them, and the spec.
    fn bench(&self, blocks: &Blocks) -> Result<(Interface, bench::Spec)> {
        let mut interface = interface::read(blocks);
        let spec = (self.spec)(blocks, &mut interface)?;
        Ok((interface, spec))
    }
}

impl Runs for BySpec {
    fn check(&self, blocks: &Blocks, quick: bool) -> Result<Run> {
        let (interface, spec) = self.bench(blocks)?;
        let mut run = spec.check(blocks, &interface, quick)?;
        if let Some(reads) = self.reads {
            reads(&mut run);
        }
        Ok(run)
    }

    fn check_with(
        &self,
        blocks: &Blocks,
        mut interface: Interface,
        quick: bool,
    ) -> Option<Result<Run>> {
        Some(
            (self.spec)(blocks, &mut interface)
                .and_then(|spec| spec.check(blocks, &interface, quick)),
        )
    }

    fn remembers(&self, blocks: &Blocks) -> Result<Option<bool>> {
        let (interface, spec) = self.bench(blocks)?;
        bench::remembers(blocks, &interface, &spec.bench).map(Some)
    }

    fn stacker<'a>(&self, data: &'a [u8]) -> Option<Stacker<'a>> {
        self.stacks.and_then(|stacks| stacks(data))
    }

    fn throughput(
        &self,
        blocks: &Blocks,
        _data: &[u8],
        timing: &Timing,
    ) -> Result<Option<i32>> {
        let (interface, _) = self.bench(blocks)?;
        throughput::throughput(blocks, &interface, timing)
    }

    fn lines(&self, blocks: &Blocks) -> Result<Interface> {
        Ok(self.bench(blocks)?.0)
    }

    fn time(
        &self,
        blocks: &Blocks,
        patterned: bool,
        cancel: &AtomicBool,
    ) -> Result<Timing> {
        let (interface, spec) = self.bench(blocks)?;
        spec.time(blocks, &interface, patterned || self.patterned, cancel)
    }
}

impl Test {
    /// The category it is made for, whose name its results are stored
    /// under and whose row says how it runs: an adder's for a hex adder's
    /// and a subtractor's too.
    pub const fn kind(self) -> Kind {
        match self {
            Test::Adder => Kind::Adder,
            Test::Decoder => Kind::Decoder,
            Test::NandXnor => Kind::NandXnorGate,
            Test::BarrelShifter => Kind::BarrelShifter,
            Test::Incrementer => Kind::Incrementer,
            Test::Decrementer => Kind::Decrementer,
            Test::IncDec => Kind::IncDec,
            Test::Multiplier => Kind::Multiplier,
            Test::Alu => Kind::Alu,
            Test::Register => Kind::Register,
            Test::Buffer => Kind::Buffer,
            Test::AbsoluteValue => Kind::AbsoluteValue,
            Test::Negator => Kind::Negator,
            Test::CarryOut => Kind::CarryOut,
            Test::GreaterThan => Kind::GreaterThan,
            Test::Equality => Kind::Equality,
            Test::Monostable => Kind::Monostable,
            Test::DualEdgeMonostable => Kind::DualEdgeMonostable,
            Test::Inverter => Kind::Inverter,
            Test::Popcount => Kind::Popcount,
            Test::BinaryToHex => Kind::BinaryToHex,
            Test::HexToBinary => Kind::HexToBinary,
            Test::Threshold => Kind::Threshold,
            Test::Parity => Kind::Parity,
            Test::Xor => Kind::XorGate,
            Test::Xnor => Kind::XnorGate,
            Test::And => Kind::AndGate,
            Test::Nand => Kind::NandGate,
            Test::Or => Kind::OrGate,
            Test::Nor => Kind::NorGate,
            Test::Counter => Kind::Counter,
            Test::ShiftRegister => Kind::ShiftRegister,
            Test::TFlipFlop => Kind::TFlipFlop,
        }
    }

    /// How it runs, as its category's row says.
    fn runs(self) -> &'static dyn Runs {
        self.kind()
            .facts()
            .runs
            .expect("the category a test is made for says how it runs")
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

    /// A few telling cases of the test run on `blocks`, as its file marks
    /// its lines: enough to tell one reading of a build from another.
    pub fn quick(self, blocks: &Blocks) -> Result<Run> {
        self.check(blocks, true)
    }

    /// The test's cases run on `blocks` through `interface`, as probing
    /// finds it rather than as the file marks it, or with `quick` a few
    /// telling ones. Only a test driven by a spec, such as an adder's,
    /// runs so; any other fails.
    pub fn check_with(
        self,
        blocks: &Blocks,
        interface: Interface,
        quick: bool,
    ) -> Result<Run> {
        self.runs()
            .check_with(blocks, interface, quick)
            .unwrap_or_else(|| {
                anyhow::bail!("{} runs its own way", self.kind().a())
            })
    }

    /// The test's cases run on `blocks`, or with `quick` a few telling
    /// ones, quick enough for a guess at what a build is.
    pub(crate) fn check(self, blocks: &Blocks, quick: bool) -> Result<Run> {
        self.runs().check(blocks, quick)
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
        self.runs().remembers(blocks)
    }

    /// Whether copies of the build in `data`, whose own cases came out
    /// right, stack into a wider one of its kind, tried at each pitch
    /// class as its category's group asks: an adder's, an incrementer's,
    /// and a carry out's copies must chain; a gate's, a monostable's, and
    /// a T flip-flop's each work alone beside the others; an inverter's, a
    /// register's, and a buffer's share lines, as many copies as work
    /// counted, and a register's copies may also add words on its shared
    /// bus. `None` for any other, and for a category whose builds never
    /// stack, as a decoder's, whose copies side by side are separate
    /// decoders.
    pub fn stacking(self, data: &[u8]) -> Option<Stacking> {
        if self.kind().facts().behaviour.stacking == super::Stacking::Never {
            return None;
        }
        self.runs().stacking(data)
    }

    /// How copies of the build in `data` are stacked and tested, as
    /// [`Test::stacking`] tries each pitch, for one step at a time; see
    /// [`Stacker::at`]. `None` for a test whose copies are never stacked.
    pub fn stacker(self, data: &[u8]) -> Option<Stacker<'_>> {
        self.runs().stacker(data)
    }

    /// Whether `copies` of the build in `data`, whose own cases came out
    /// right, stacked `step` make a wider one of its kind, as its own
    /// `stacking` tests each step, the box rule's box first, then boxes
    /// leaving out more of the first copy: the step with the box that
    /// works and the cases run, or why not. Copies apart, as
    /// `stack::touches` has them, never stack. A register's copies are
    /// tried as more words on its shared bus, then as more bits. `None`
    /// for a test whose copies are never stacked.
    pub fn stacks_at(
        self,
        data: &[u8],
        step: Step,
        copies: usize,
    ) -> Option<Result<(Step, u64), String>> {
        self.runs().stacks_at(data, step, copies)
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
        self.runs().throughput(blocks, data, timing)
    }

    /// The lines of `blocks` named as its timing names them, such as an
    /// adder's A, B, and carry in; for a test that names none, as
    /// [`interface::read`] finds them.
    pub fn lines(self, blocks: &Blocks) -> Result<Interface> {
        self.runs().lines(blocks)
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        self.runs().time(blocks, false, cancel)
    }

    /// [`Test::time`], its inputs taking the test patterns whatever their
    /// width, as a stack of copies is timed: fewer cases, which still make
    /// its carries run furthest.
    pub fn time_by_patterns(
        self,
        blocks: &Blocks,
        cancel: &AtomicBool,
    ) -> Result<Timing> {
        self.runs().time(blocks, true, cancel)
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
        assert_eq!(Test::of("multiplier", Some(65)), None);
    }
}
