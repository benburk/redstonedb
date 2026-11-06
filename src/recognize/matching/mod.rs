//! Recognizing a build by matching what it does against what each category
//! computes. Its levers are driven once through a few patterns and
//! what each output shows is kept, see [`observe`]. Then each category's
//! slots, see [`Category`], are filled from the build's lines every way
//! they fit, see [`buses`], and each way is judged against the category's
//! operations with no more simulating. The closest is kept, the
//! conventions breaking ties: buses counted the way the site counts them,
//! fewer control lines read inverted, A where the site puts it, and the
//! kind probing tries first.
//!
//! A category says only what is its own: its slots, which may be optional
//! or take one of a few widths, and what each of its operations gives, each
//! output with the bits it decides, so a status line an operation leaves
//! meaningless is not checked. Finding which lines fill which slot, which
//! way round, and which control lines read inverted is the same for every
//! category, and done here.
//!
//! Only stateless builds driven by levers are matched so far, beside
//! probing, see [`crate::recognize::probe`], which the site still
//! recognizes builds by.

use std::{cmp::Reverse, collections::BTreeMap};

use anyhow::Result;

use crate::{
    category::{Kind, Probing},
    recognize::{
        inspect, interface,
        probe::{self, Order},
    },
    schem::{
        Blocks, Pos,
        lines::{self, Role, Roles},
    },
    simulate::{Value, alu_ops, mask},
};

pub mod buses;
mod categories;
mod observe;

pub use categories::CATEGORIES;
pub use observe::{Observation, observe};

use buses::{Bus, Conventional, Pool};

/// How wide a slot is, by the build's width, `n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    /// The build's width.
    N,
    /// A bit wider, as a sum holding its carry.
    NPlus1,
    /// One bit, as a carry in or a comparison's answer.
    Bit,
    /// A bit for each of an `n`-bit value's values, as a decoder's output.
    Pow2,
    /// Every input line no other slot takes, one or more, whose settings
    /// choose the operation, as an ALU's control lines do: a selector. Only
    /// a category's last input slot may be one.
    Lines,
}

impl Width {
    /// Its bits for a build `n` bits wide; `None` past what any build is.
    fn of(self, n: usize) -> Option<usize> {
        match self {
            Width::N => Some(n),
            Width::NPlus1 => Some(n + 1),
            Width::Bit => Some(1),
            Width::Pow2 => (n < 16).then(|| 1 << n),
            Width::Lines => None,
        }
    }
}

/// A line a category takes or gives: its name, its role, the widths it may
/// be, the first preferred, and whether a build may leave it out.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub name: &'static str,
    pub role: Role,
    pub widths: &'static [Width],
    pub optional: bool,
}

impl Slot {
    pub const fn new(
        name: &'static str,
        role: Role,
        widths: &'static [Width],
    ) -> Slot {
        Slot {
            name,
            role,
            widths,
            optional: false,
        }
    }

    /// A selector, see [`Width::Lines`]: control lines whose settings
    /// choose the operation.
    pub const fn selector(name: &'static str) -> Slot {
        Slot::new(name, Role::Control, &[Width::Lines])
    }

    /// Whether it is a selector.
    pub fn selects(&self) -> bool {
        self.widths.contains(&Width::Lines)
    }

    /// The same slot, which a build may leave out.
    #[must_use]
    pub const fn optional(self) -> Slot {
        Slot {
            optional: true,
            ..self
        }
    }

    /// Whether the build gives it rather than takes it.
    pub fn output(&self) -> bool {
        matches!(self.role, Role::Output(_) | Role::Status)
    }
}

/// What an output slot should show: `value` on the bits `care` holds, the
/// rest left to the build.
#[derive(Clone, Copy, Debug)]
pub struct Want {
    pub value: Value,
    pub care: Value,
}

impl Want {
    /// Anything, as a carry out shows for an operation that has none.
    pub const ANY: Want = Want { value: 0, care: 0 };
}

impl From<Value> for Want {
    fn from(value: Value) -> Want {
        Want {
            value,
            care: Value::MAX,
        }
    }
}

impl From<bool> for Want {
    fn from(on: bool) -> Want {
        Value::from(on).into()
    }
}

/// What an operation gives for one case: the value of each slot by its
/// place in the category's slots, 0 for an output or one left out; what
/// each output slot should show, in the order the slots list them.
type Eval = Box<dyn Fn(&[Value]) -> Vec<Want> + Sync>;

/// One function a category's builds may compute, as an adder's add or its
/// subtract, a subtractor's: the cases' name, such as `A + B + carry in`,
/// the kind a build that computes it is, if not the category's own, and
/// what it gives.
pub struct Op {
    pub label: String,
    pub kind: Option<Kind>,
    pub eval: Eval,
}

impl Op {
    pub fn new(
        label: impl Into<String>,
        eval: impl Fn(&[Value]) -> Vec<Want> + Sync + 'static,
    ) -> Op {
        Op {
            label: label.into(),
            kind: None,
            eval: Box::new(eval),
        }
    }

    /// The same operation, a build computing it being a `kind`.
    #[must_use]
    pub fn kind(self, kind: Kind) -> Op {
        Op {
            kind: Some(kind),
            ..self
        }
    }
}

/// A category's slots as one build fills them: its width, `n`, and each
/// slot's bits, `None` for one left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub n: usize,
    pub widths: Vec<Option<usize>>,
}

impl Shape {
    /// Whether the build has slot `slot`.
    pub fn has(&self, slot: usize) -> bool {
        self.widths[slot].is_some()
    }
}

/// A kind of build matching recognizes: the slots it takes and gives, and
/// the operations a build of a shape may compute, each named. A new
/// category is a new implementation, see [`categories`], and a row in
/// [`CATEGORIES`].
pub trait Category: Sync {
    fn kind(&self) -> Kind;
    fn slots(&self) -> &'static [Slot];
    /// What a build of `shape` may compute; none for one it can't be, as a
    /// decoder too wide to test.
    fn ops(&self, shape: &Shape) -> Vec<Op>;
}

/// A line of a build as matching found it: the slot it fills, its bits,
/// least significant first, and whether it reads inverted: a control line
/// powered meaning 0, or a status line normally on.
#[derive(Clone, Debug)]
pub struct Line {
    pub name: &'static str,
    pub role: Role,
    pub bits: Vec<Pos>,
    pub inverted: bool,
}

/// What a build was matched as: its kind, the operation it computes, its
/// lines, how many of the cases observed it got wrong, how far its reading
/// strays from the conventions, and whether its lines keep them, every bus
/// a whole run, or a run but its top bit, rather than bent, as a build
/// whose top bits stand out of line bends them: one that doesn't is of
/// non-standard I/O.
#[derive(Clone, Debug)]
pub struct Match {
    pub kind: Kind,
    pub label: String,
    pub lines: Vec<Line>,
    pub cases: u64,
    pub wrong: u64,
    pub cost: u32,
    pub conventional: bool,
}

impl Match {
    /// Its lines' roles, by block, as the site keeps them.
    pub fn roles(&self) -> Roles {
        let mut roles = Roles::default();
        for line in &self.lines {
            roles.mark(line.role, &line.bits);
        }
        roles
    }

    /// The line filling the slot `name`, if any.
    pub fn line(&self, name: &str) -> Option<&Line> {
        self.lines.iter().find(|line| line.name == name)
    }

    /// What it is, in words, such as `an adder: 0 of 242 cases wrong, A +
    /// B + carry in`, naming any line read inverted.
    pub fn found(&self) -> String {
        let inverted: Vec<&str> = self
            .lines
            .iter()
            .filter(|line| line.inverted)
            .map(|line| line.name)
            .collect();
        let inverted = if inverted.is_empty() {
            String::new()
        } else {
            format!("; inverted: {}", inverted.join(", "))
        };
        format!(
            "{}: {} of {} cases wrong, {}{inverted}",
            self.kind.a(),
            self.wrong,
            self.cases,
            self.label
        )
    }
}

/// The file in `data`, made simulatable as probing makes it, matched as
/// the category it comes closest to; `None` when its inputs and outputs
/// break the conventions, or it can't be observed, or no category's slots
/// fit its lines. A build whose control lines' signs name ALU operations
/// is tested as an ALU as they name them, see [`signed_alu`], unless
/// another category gets its every case right.
pub fn recognize(data: &[u8]) -> Result<Option<Match>> {
    let simulated = probe::simulatable(&lines::without(data)?)?;
    let blocks = Blocks::read(&simulated)?;
    let report = inspect::inspect(&blocks);
    if report.faults().next().is_some() {
        return Ok(None);
    }
    let found = closest(&blocks, &report);
    let right = found
        .as_ref()
        .is_some_and(|found| found.kind != Kind::Alu && found.wrong == 0);
    if right || !names_operations(&blocks, &report) {
        return Ok(found);
    }
    Ok(signed_alu(&simulated, &report).or(found))
}

/// Whether a sign on any of `report`'s control lines, its lone levers and
/// short runs, names an operation an ALU may offer, as `and nand sub`.
fn names_operations(blocks: &Blocks, report: &inspect::Report) -> bool {
    let signs = interface::line_signs(blocks);
    report
        .inputs
        .iter()
        .filter(|run| run.bits.len() <= 3)
        .flat_map(|run| &run.bits)
        .filter_map(|pos| signs.get(pos))
        .flat_map(|lines| lines.iter())
        .flat_map(|line| {
            line.split(|c: char| c.is_whitespace() || c == ',')
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .any(|word| alu_ops::Op::named(&word).is_some())
}

/// The build in `data`, its lines as `report` finds them, tested as an ALU
/// as its control lines' signs name its operations, by the ALU test: each
/// operation a sign names checked on its every case, so one that gets a
/// few wrong is a fault, not a setting meaning nothing, as without signs it
/// would be, and a sign naming one its lines don't choose is a fault too.
/// Signs are needed so for an ALU of more control lines than matching
/// observes every setting of. `None` when its lines don't fit an ALU's.
fn signed_alu(data: &[u8], report: &inspect::Report) -> Option<Match> {
    let line = |name, role, bits: &[Pos]| {
        (!bits.is_empty()).then(|| Line {
            name,
            role,
            bits: bits.to_vec(),
            inverted: false,
        })
    };
    match probe::alu(data, report, Order::default()) {
        Ok(probed) => Some(Match {
            kind: Kind::Alu,
            label: probed.run.pattern.clone(),
            lines: [
                line("A", Role::Operand(0), &probed.a),
                line("B", Role::Operand(1), &probed.b),
                line("operation", Role::Control, &probed.controls),
                line("output", Role::Output(0), &probed.output),
                line("status", Role::Status, &probed.status),
            ]
            .into_iter()
            .flatten()
            .collect(),
            cases: probed.run.cases,
            wrong: 0,
            cost: 0,
            conventional: true,
        }),
        Err(probe::Fault::Wrong {
            wrong, cases, said, ..
        }) => Some(Match {
            kind: Kind::Alu,
            // What it says past its count, which `Match::found` gives.
            label: said
                .split_once("cases wrong: ")
                .map_or(said.clone(), |(_, evidence)| evidence.to_owned()),
            lines: Vec::new(),
            cases,
            wrong,
            cost: 0,
            conventional: true,
        }),
        Err(_) => None,
    }
}

/// Of every category whose slots `report`'s lines fill, the reading of the
/// build in `blocks` that comes closest; see [`Score`]. Buses laid as the
/// conventions lay them are tried first, and bent ones too only when no
/// category's slots fit those at all.
pub fn closest(blocks: &Blocks, report: &inspect::Report) -> Option<Match> {
    let observed = observe(blocks, report).ok()?;
    let mut best: Option<(Score, Match)> = None;
    for bent in [false, true] {
        for &category in CATEGORIES {
            if let Some((score, found)) =
                judge(category, blocks, &observed, bent)
                && best.as_ref().is_none_or(|(best, _)| score < *best)
            {
                best = Some((score, found));
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(_, found)| found)
}

/// How one reading ranks, lowest first: its share of cases wrong; how far
/// its buses stray from the site's count; the more of the cases it
/// explains and the more output bits it decides, see [`Reading::rank`];
/// how many control lines it reads inverted; whether A and B are
/// the other way round from where the site puts A; and the place probing
/// tries its kind at, as a 1-bit XOR gate's is a 1-bit adder's reading
/// too.
type Score = (u64, u32, Reverse<u64>, Reverse<u64>, u32, bool, u16);

/// The most control lines matching tries reading inverted, every way.
const MOST_INVERTIBLE: usize = 3;

/// The most ways of filling a category's input slots, or its output
/// slots, tried for one shape.
const MOST_FILLINGS: usize = 4096;

/// The most pairs of ways of filling a category's input slots and its
/// output slots read for one shape, so a build whose lines could be read
/// too many ways is soon given up on, its category left to its author.
const MOST_PAIRS: usize = 4096;

/// The cases of each setting a way of filling a category's slots is first
/// read on.
const SAMPLE: usize = 8;

/// The most ways of filling a category's slots read on every case, those
/// that come closest on the sample.
const MOST_READ: usize = 8;

/// The closest reading of the build `observed`, of `blocks`, as
/// `category`, its buses bent too when `bent`, and its score; `None` when
/// its slots don't fit the lines. Its input slots and its output slots are
/// filled apart, as they take different lines, and what each operation
/// expects is worked out once for each way of filling the input slots.
fn judge(
    category: &dyn Category,
    blocks: &Blocks,
    observed: &Observation,
    bent: bool,
) -> Option<(Score, Match)> {
    let slots = category.slots();
    let conventional_in =
        |bits: &[(usize, usize)]| conventional(blocks, &observed.inputs, bits);
    let conventional_out =
        |bits: &[(usize, usize)]| conventional(blocks, &observed.outputs, bits);
    let inputs = Pool::of(&observed.inputs);
    let outputs = Pool::of(&observed.outputs);
    let mut found: Option<(Score, Match)> = None;
    for shape in shapes(slots, inputs.bits(), outputs.bits()) {
        let ops = category.ops(&shape);
        if ops.is_empty() {
            continue;
        }
        let mut ins =
            fill(slots, &shape, false, &inputs, &conventional_in, bent);
        let outs = fill(slots, &shape, true, &outputs, &conventional_out, bent);
        ins.truncate(MOST_PAIRS / outs.len().max(1));
        // What each way of filling the output slots shows, case by case.
        let shown: Vec<Vec<Vec<Value>>> = outs
            .iter()
            .map(|filling| {
                observed
                    .cases
                    .iter()
                    .map(|(_, shown)| values(slots, filling, true, shown))
                    .collect()
            })
            .collect();
        let mut expected: Option<(usize, Expected)> = None;
        let kept = sample(slots, &shape, &ins, &outs, &shown, &ops, observed);
        for (i, o, sample) in kept {
            let (input, output, shown) = (&ins[i], &outs[o], &shown[o]);
            if expected.as_ref().is_none_or(|&(at, _)| at != i) {
                expected = Some((
                    i,
                    expect(slots, &shape, input, &ops, observed, false),
                ));
            }
            let Some((_, expected)) = &expected else {
                continue;
            };
            let selects = slots
                .iter()
                .zip(input)
                .any(|(slot, bus)| slot.selects() && bus.is_some());
            let Some(reading) = read(expected, shown, Some(&sample)) else {
                continue;
            };
            let filling: Vec<Option<Bus>> = input
                .iter()
                .zip(output)
                .map(|(i, o)| i.clone().or_else(|| o.clone()))
                .collect();
            let positions = positions(slots, &filling, observed);
            let cost: u32 = filling.iter().flatten().map(|bus| bus.cost).sum();
            // A selector's build is the category's; any other is what
            // its one operation says.
            let kind = match reading.chosen.as_slice() {
                [(_, k)] if !selects => ops[*k].kind.unwrap_or(category.kind()),
                _ => category.kind(),
            };
            let score = (
                reading.share(),
                cost,
                Reverse(reading.coverage()),
                Reverse(reading.decided),
                reading.inverted.count_ones(),
                swapped(slots, &positions),
                kind_order(kind),
            );
            if found.as_ref().is_some_and(|(kept, _)| *kept <= score) {
                continue;
            }
            let closer = Match {
                kind,
                label: label(&reading, &ops, selects),
                lines: lines(slots, positions, &reading),
                cases: reading.cases,
                wrong: reading.wrong,
                cost,
                conventional: !filling.iter().flatten().any(|bus| bus.bent),
            };
            found = Some((score, closer));
        }
    }
    found
}

/// Of each way `ins` fills the input slots of `slots`, sized by `shape`,
/// with each way `outs` fills its output slots, which showed `shown`, each
/// read as `ops` on a sample of the cases `observed`: the [`MOST_READ`]
/// that come closest on it, ranked as [`Reading::rank`] ranks them, the
/// least bent first on a tie, each with its sample's reading, to read on
/// every case.
fn sample(
    slots: &[Slot],
    shape: &Shape,
    ins: &[Vec<Option<Bus>>],
    outs: &[Vec<Option<Bus>>],
    shown: &[Vec<Vec<Value>>],
    ops: &[Op],
    observed: &Observation,
) -> Vec<(usize, usize, Reading)> {
    let cost = |filling: &[Option<Bus>]| -> u32 {
        filling.iter().flatten().map(|bus| bus.cost).sum()
    };
    let mut sampled: Vec<(usize, usize, Reading)> = Vec::new();
    for (i, input) in ins.iter().enumerate() {
        let expected = expect(slots, shape, input, ops, observed, true);
        for (o, shown) in shown.iter().enumerate() {
            if let Some(reading) = read(&expected, shown, None) {
                sampled.push((i, o, reading));
            }
        }
    }
    sampled.sort_by_cached_key(|(i, o, reading)| {
        (reading.rank(), cost(&ins[*i]) + cost(&outs[*o]), *i, *o)
    });
    sampled.truncate(MOST_READ);
    sampled.sort_unstable_by_key(|&(i, o, _)| (i, o));
    sampled
}

/// The lines of `slots` filled, at `positions`, by slot, each lone control
/// line read inverted as `reading` has it, a bit each in the slots' order,
/// and each status line normally on as it has it, a bit each output slot.
fn lines(
    slots: &[Slot],
    positions: Vec<Option<Vec<Pos>>>,
    reading: &Reading,
) -> Vec<Line> {
    let mut controls = 0;
    let mut outputs = 0;
    slots
        .iter()
        .zip(positions)
        .filter_map(|(slot, bits)| {
            let output = slot.output().then(|| {
                outputs += 1;
                outputs - 1
            });
            let bits = bits?;
            let inverted = match output {
                Some(o) => reading.flipped >> o & 1 == 1,
                None => {
                    invertible(slot, bits.len()) && {
                        controls += 1;
                        controls <= MOST_INVERTIBLE
                            && reading.inverted >> (controls - 1) & 1 == 1
                    }
                }
            };
            Some(Line {
                name: slot.name,
                role: slot.role,
                bits,
                inverted,
            })
        })
        .collect()
}

/// Where the bits of each slot `filling` fills are, by slot.
fn positions(
    slots: &[Slot],
    filling: &[Option<Bus>],
    observed: &Observation,
) -> Vec<Option<Vec<Pos>>> {
    slots
        .iter()
        .zip(filling)
        .map(|(slot, bus)| {
            let side = if slot.output() {
                &observed.outputs
            } else {
                &observed.inputs
            };
            bus.as_ref().map(|bus| {
                bus.bits.iter().map(|&(run, bit)| side[run][bit]).collect()
            })
        })
        .collect()
}

/// The value of each of `slots` on the side `output` says, as `filling`
/// fills them, in a case whose runs on that side were `runs`; 0 for a slot
/// left out or on the other side.
fn values(
    slots: &[Slot],
    filling: &[Option<Bus>],
    output: bool,
    runs: &[Value],
) -> Vec<Value> {
    slots
        .iter()
        .zip(filling)
        .map(|(slot, bus)| match bus {
            Some(bus) if slot.output() == output => bus.value(runs),
            _ => 0,
        })
        .collect()
}

/// The place probing tries `kind` at, as a tie between kinds goes to the
/// one it tries first.
fn kind_order(kind: Kind) -> u16 {
    let kind = match kind.facts().probe {
        Probing::As(other) => other,
        _ => kind,
    };
    match kind.facts().probe {
        Probing::Tried { order, .. } => order,
        _ => u16::MAX,
    }
}

/// Whether `bits`, of runs `side`, are counted the way the site counts a
/// bus, from the end nearest the box's south-east-bottom corner, or as an
/// `lsb` or `msb` sign says; see [`interface::lsb_first`].
fn conventional(
    blocks: &Blocks,
    side: &[Vec<Pos>],
    bits: &[(usize, usize)],
) -> bool {
    let positions: Vec<Pos> =
        bits.iter().map(|&(run, bit)| side[run][bit]).collect();
    let mut counted = positions.clone();
    interface::lsb_first(blocks, &mut counted);
    counted.first() == positions.first()
}

/// Whether operands A and B, when a category has both, are the other way
/// round from where the site puts A, see [`Order`].
fn swapped(slots: &[Slot], positions: &[Option<Vec<Pos>>]) -> bool {
    let operand = |n: u8| {
        slots
            .iter()
            .position(|slot| slot.role == Role::Operand(n))
            .and_then(|i| positions[i].as_deref())
    };
    match (operand(0), operand(1)) {
        (Some(a), Some(b)) => !Order::default().keeps(a, b),
        _ => false,
    }
}

/// Whether a line of `bits` bits filling `slot` may read inverted: a lone
/// control line, but a selector's, whose settings each choose what they
/// choose whichever way its lines read.
fn invertible(slot: &Slot, bits: usize) -> bool {
    slot.role == Role::Control && bits == 1 && !slot.selects()
}

/// The most control lines a selector may have: as many as are observed in
/// every setting, see [`observe`].
const MOST_SELECTOR_LINES: usize = observe::EVERY_SETTING;

/// Every shape `slots` may take with `inputs` bits in and `outputs` out:
/// each optional slot in or out, each slot one of its widths, at each
/// width `n` from 1 to 64 that adds up, a selector taking every input line
/// left over, at least one.
fn shapes(slots: &[Slot], inputs: usize, outputs: usize) -> Vec<Shape> {
    let mut choices: Vec<Vec<Option<Width>>> = vec![Vec::new()];
    for slot in slots {
        let options: Vec<Option<Width>> = slot
            .widths
            .iter()
            .map(|&w| Some(w))
            .chain(slot.optional.then_some(None))
            .collect();
        choices = choices
            .into_iter()
            .flat_map(|chosen| {
                options.iter().map(move |&option| {
                    let mut chosen = chosen.clone();
                    chosen.push(option);
                    chosen
                })
            })
            .collect();
    }
    let mut found = Vec::new();
    for chosen in choices {
        // A build whose width no slot depends on is 1 bit wide.
        let sized = chosen.iter().flatten().any(|&w| w != Width::Bit);
        for n in 1..=if sized { 64 } else { 1 } {
            let Some(mut widths) = chosen
                .iter()
                .map(|w| match w {
                    None => Some(None),
                    Some(Width::Lines) => Some(Some(0)),
                    Some(w) => w.of(n).map(Some),
                })
                .collect::<Option<Vec<Option<usize>>>>()
            else {
                continue;
            };
            let total = |widths: &[Option<usize>], output: bool| -> usize {
                slots
                    .iter()
                    .zip(widths)
                    .filter(|(slot, _)| slot.output() == output)
                    .filter_map(|(_, w)| *w)
                    .sum()
            };
            if let Some(i) =
                chosen.iter().position(|&w| w == Some(Width::Lines))
            {
                match inputs.checked_sub(total(&widths, false)) {
                    Some(left) if (1..=MOST_SELECTOR_LINES).contains(&left) => {
                        widths[i] = Some(left);
                    }
                    _ => continue,
                }
            }
            if total(&widths, false) == inputs
                && total(&widths, true) == outputs
            {
                let shape = Shape { n, widths };
                if !found.contains(&shape) {
                    found.push(shape);
                }
            }
        }
    }
    found
}

/// Every way of filling the slots of `slots` on the side `output` says,
/// as `shape` sizes them, from the lines of `pool`, every line taken, its
/// buses bent too when `bent`; each filling by slot, the other side's
/// slots left empty, up to [`MOST_FILLINGS`] of them.
fn fill(
    slots: &[Slot],
    shape: &Shape,
    output: bool,
    pool: &Pool,
    conventional: Conventional,
    bent: bool,
) -> Vec<Vec<Option<Bus>>> {
    let mut found = Vec::new();
    let side = Side {
        slots,
        shape,
        output,
        conventional,
        bent,
    };
    side.fill(0, &mut Vec::new(), pool, &mut found);
    found
}

/// The slots of one side of a category, as one shape sizes them, to fill.
struct Side<'s> {
    slots: &'s [Slot],
    shape: &'s Shape,
    output: bool,
    conventional: Conventional<'s>,
    bent: bool,
}

impl Side<'_> {
    /// Every way of filling the side's slots from `slot` on with what is
    /// left of `pool`, onto `chosen`, the buses of the slots before.
    fn fill(
        &self,
        slot: usize,
        chosen: &mut Vec<Option<Bus>>,
        pool: &Pool,
        found: &mut Vec<Vec<Option<Bus>>>,
    ) {
        if found.len() >= MOST_FILLINGS {
            return;
        }
        let Some(this) = self.slots.get(slot) else {
            if pool.bits() == 0 {
                found.push(chosen.clone());
            }
            return;
        };
        let width =
            self.shape.widths[slot].filter(|_| this.output() == self.output);
        let Some(width) = width else {
            chosen.push(None);
            self.fill(slot + 1, chosen, pool, found);
            chosen.pop();
            return;
        };
        let buses = if this.selects() {
            pool.rest(self.shape.n).into_iter().collect()
        } else {
            pool.take(width, self.conventional, self.bent)
        };
        for (bus, rest) in buses {
            chosen.push(Some(bus));
            self.fill(slot + 1, chosen, &rest, found);
            chosen.pop();
        }
    }
}

/// How a build fares read one way: which of its control lines read
/// inverted, and the operation it computes, or for a selector the one each
/// of its settings chooses, over the cases observed.
#[derive(Clone)]
struct Reading {
    /// The cases of the settings choosing an operation, of `observed`.
    cases: u64,
    observed: u64,
    /// Those of them whose operation decides any output bit.
    decisive: u64,
    wrong: u64,
    /// The control lines read inverted, a bit each, in the slots' order.
    inverted: u32,
    /// The status lines read normally on, a bit for each output slot, in
    /// the slots' order.
    flipped: u32,
    /// The output bits its operations decide, over every case.
    decided: u64,
    /// The operation each setting of its selector chooses, by its place
    /// among the operations, the settings choosing none left out; for a
    /// build with no selector, its one operation, at setting 0.
    chosen: Vec<(Value, usize)>,
}

/// What each operation expects of a build's outputs, its input slots
/// filled one way.
struct Expected {
    /// The output slots, each by its place and the bits of its width.
    outputs: Vec<(usize, Value)>,
    /// Which of them are lone status lines, which may be normally on.
    status: Vec<bool>,
    /// The cases of each setting of the selector, or every case at setting
    /// 0 for a build with none.
    settings: Vec<(Value, Vec<usize>)>,
    selects: bool,
    ops: usize,
    /// Each operation's name, two of one name being one operation read two
    /// ways.
    labels: Vec<String>,
    /// The cases it holds, by their place among those observed; the
    /// settings name cases by their place here.
    ids: Vec<usize>,
    /// For each way of reading its control lines inverted, a bit a line,
    /// each operation's wants for each case, one an output slot.
    wants: Vec<Want>,
}

impl Expected {
    /// What operation `op` wants of output `o` in case `case`, its
    /// control lines read as `inverted`.
    fn want(&self, inverted: usize, op: usize, case: usize, o: usize) -> Want {
        let at = ((inverted * self.ops + op) * self.ids.len() + case)
            * self.outputs.len()
            + o;
        self.wants[at]
    }

    /// How many ways its control lines may read inverted.
    fn inversions(&self) -> usize {
        self.wants.len()
            / (self.ops * self.ids.len() * self.outputs.len()).max(1)
    }
}

/// What each of `ops` expects of the build `observed`'s outputs, its
/// `slots` sized by `shape` and its input slots filled by `input`, each
/// way its control lines may read inverted: in every case observed, or
/// with `sample` in [`SAMPLE`] cases of each setting of its selector, or
/// of its one setting, spread over its cases.
fn expect(
    slots: &[Slot],
    shape: &Shape,
    input: &[Option<Bus>],
    ops: &[Op],
    observed: &Observation,
    sample: bool,
) -> Expected {
    let invertible: Vec<usize> = slots
        .iter()
        .zip(input)
        .enumerate()
        .filter(|(_, (slot, bus))| {
            bus.as_ref()
                .is_some_and(|bus| invertible(slot, bus.bits.len()))
        })
        .map(|(i, _)| i)
        .take(MOST_INVERTIBLE)
        .collect();
    let outputs: Vec<(usize, Value)> = slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.output())
        .map(|(i, _)| (i, shape.widths[i].map_or(0, mask)))
        .collect();
    let selector = slots
        .iter()
        .zip(input)
        .position(|(slot, bus)| slot.selects() && bus.is_some());
    // Each case by its selector's setting, the only slot's value needed
    // before the cases are sampled.
    let setting = |driven: &[Value]| {
        selector
            .and_then(|s| input[s].as_ref())
            .map_or(0, |bus| bus.value(driven))
    };
    let mut ids: BTreeMap<Value, Vec<usize>> = BTreeMap::new();
    for (i, (driven, _)) in observed.cases.iter().enumerate() {
        ids.entry(setting(driven)).or_default().push(i);
    }
    // Spread over each setting's cases, as its other lines, such as a
    // carry in, change slower than its buses.
    if sample {
        for members in ids.values_mut() {
            let len = members.len();
            let mut spread: Vec<usize> =
                (0..SAMPLE).map(|k| members[k * len / SAMPLE]).collect();
            spread.dedup();
            *members = spread;
        }
    }
    let mut cases = Vec::new();
    let mut kept = Vec::new();
    let mut settings = Vec::new();
    for (setting, members) in ids {
        let at: Vec<usize> = (kept.len()..kept.len() + members.len()).collect();
        for &i in &members {
            cases.push(values(slots, input, false, &observed.cases[i].0));
        }
        kept.extend(members);
        settings.push((setting, at));
    }
    let mut wants = Vec::new();
    for inverted in 0..1_u32 << invertible.len() {
        for op in ops {
            for case in &cases {
                let mut given = case.clone();
                for (k, &i) in invertible.iter().enumerate() {
                    given[i] ^= Value::from(inverted >> k & 1 == 1);
                }
                let mut each = (op.eval)(&given);
                each.resize(outputs.len(), Want::ANY);
                wants.extend(each);
            }
        }
    }
    let status = outputs
        .iter()
        .map(|&(slot, bits)| slots[slot].role == Role::Status && bits == 1)
        .collect();
    Expected {
        outputs,
        status,
        settings,
        selects: selector.is_some(),
        ops: ops.len(),
        labels: ops.iter().map(|op| op.label.clone()).collect(),
        ids: kept,
        wants,
    }
}

/// The reading of a build whose operations expect `expected` and whose
/// output slots showed `shown`, case by case, by slot, that comes closest,
/// see [`Reading::rank`], of each way of reading its control lines
/// inverted, each setting of a selector taking whichever operation it gets
/// most right. A setting chooses an operation only when it gets every case
/// right, as many an ALU's settings mean nothing, and one that gives an
/// operation all but a few cases is told from them by nothing; but a
/// selector must choose two operations or more, by name. Given `found`, a
/// reading of a sample of the same cases, its control lines are read as
/// that one's, and only the settings choosing an operation there are
/// tried. `None` when no reading fits.
fn read(
    expected: &Expected,
    shown: &[Vec<Value>],
    found: Option<&Reading>,
) -> Option<Reading> {
    let mut best: Option<Reading> = None;
    let inversions = match found {
        Some(found) => {
            let inverted = found.inverted as usize;
            inverted..inverted + 1
        }
        None => 0..expected.inversions(),
    };
    for inverted in inversions {
        // How many of `members`' cases `op` gets wrong, the output bits it
        // decides, and the cases it decides any of, its status lines read
        // as `flipped` has them, or with `core` left out.
        let tally = |op: usize, members: &[usize], flipped: u32, core: bool| {
            members.iter().fold(
                (0_u64, 0_u64, 0_u64),
                |(wrong, decided, decisive), &case| {
                    let mut right = true;
                    let mut decides = 0;
                    for (o, &(slot, bits)) in
                        expected.outputs.iter().enumerate()
                    {
                        if core && expected.status[o] {
                            continue;
                        }
                        let want = expected.want(inverted, op, case, o);
                        let care = want.care & bits;
                        let shown = shown[expected.ids[case]][slot]
                            ^ Value::from(flipped >> o & 1 == 1);
                        decides += u64::from(care.count_ones());
                        right &= (shown ^ want.value) & care == 0;
                    }
                    (
                        wrong + u64::from(!right),
                        decided + decides,
                        decisive + u64::from(decides > 0),
                    )
                },
            )
        };
        // Each setting's operation, chosen by what it gives on its
        // outputs but its status lines, whose polarity is not yet known.
        let mut chosen: Vec<(Value, usize, &[usize])> = Vec::new();
        for (setting, members) in &expected.settings {
            let counted = found.is_none_or(|found| {
                found.chosen.iter().any(|(chosen, _)| chosen == setting)
            });
            if !counted {
                continue;
            }
            let Some((k, (wrong, ..))) = (0..expected.ops)
                .map(|op| tally(op, members, 0, true))
                .enumerate()
                .min_by_key(|&(k, (wrong, decided, _))| {
                    (wrong, Reverse(decided), k)
                })
            else {
                continue;
            };
            if expected.selects && wrong > 0 {
                continue;
            }
            chosen.push((*setting, k, members));
        }
        let flipped = polarity(expected, shown, inverted, &chosen);
        let mut reading = Reading {
            cases: 0,
            observed: expected.ids.len() as u64,
            decisive: 0,
            wrong: 0,
            inverted: u32::try_from(inverted).unwrap_or(0),
            flipped,
            decided: 0,
            chosen: Vec::new(),
        };
        for &(setting, k, members) in &chosen {
            let (wrong, decided, decisive) = tally(k, members, flipped, false);
            reading.cases += members.len() as u64;
            reading.wrong += wrong;
            reading.decided += decided;
            reading.decisive += decisive;
            reading.chosen.push((setting, k));
        }
        let mut distinct: Vec<&str> = reading
            .chosen
            .iter()
            .map(|&(_, k)| expected.labels[k].as_str())
            .collect();
        distinct.sort_unstable();
        distinct.dedup();
        if reading.cases == 0 || expected.selects && distinct.len() < 2 {
            continue;
        }
        if best.as_ref().is_none_or(|b| reading.rank() < b.rank()) {
            best = Some(reading);
        }
    }
    best
}

/// Which of the status lines of a build whose operations expect `expected`
/// and whose outputs showed `shown` read normally on, a bit each output, its
/// control lines read as `inverted` has them and each setting computing the
/// operation `chosen` gives it: each where that gets fewer of its bits
/// wrong, on its own.
fn polarity(
    expected: &Expected,
    shown: &[Vec<Value>],
    inverted: usize,
    chosen: &[(Value, usize, &[usize])],
) -> u32 {
    let mut flipped = 0;
    for (o, _) in expected.status.iter().enumerate().filter(|(_, s)| **s) {
        let wrong = |flip: bool| -> usize {
            chosen
                .iter()
                .flat_map(|&(_, k, members)| {
                    members.iter().map(move |&case| (k, case))
                })
                .filter(|&(k, case)| {
                    let want = expected.want(inverted, k, case, o);
                    let shown = shown[expected.ids[case]]
                        [expected.outputs[o].0]
                        ^ Value::from(flip);
                    (shown ^ want.value) & want.care & 1 != 0
                })
                .count()
        };
        if wrong(true) < wrong(false) {
            flipped |= 1 << o;
        }
    }
    flipped
}

impl Reading {
    /// Its share of cases wrong, in millionths.
    fn share(&self) -> u64 {
        self.wrong * 1_000_000 / self.cases.max(1)
    }

    /// The share of the cases observed that it explains, its operation
    /// deciding some output bit, in millionths.
    fn coverage(&self) -> u64 {
        self.decisive * 1_000_000 / self.observed.max(1)
    }

    /// How it ranks against another reading of the same lines, lowest
    /// first: its share of cases wrong; then the more of the cases it
    /// explains, as a reading that takes a carry in for a selector's line
    /// leaves an add with it on unexplained; then the more output bits it
    /// decides, as an add's carry in read the wrong way round leaves every
    /// other operation's cases with it on undecided; then the fewer lines
    /// it reads inverted or normally on.
    fn rank(&self) -> (u64, Reverse<u64>, Reverse<u64>, u32) {
        (
            self.share(),
            Reverse(self.coverage()),
            Reverse(self.decided),
            self.inverted.count_ones() + self.flipped.count_ones(),
        )
    }
}

/// What `reading` computes, in words: its operation's name, or each
/// operation its selector chooses, by the settings choosing it, its lines
/// counted from 1, as `add (no lines on), and (line 2 on)`.
fn label(reading: &Reading, ops: &[Op], selects: bool) -> String {
    if !selects {
        return reading
            .chosen
            .first()
            .map_or_else(String::new, |&(_, k)| ops[k].label.clone());
    }
    let setting = |setting: Value| {
        let on: Vec<String> = (0..Value::BITS)
            .filter(|&line| setting >> line & 1 == 1)
            .map(|line| (line + 1).to_string())
            .collect();
        match on.as_slice() {
            [] => "no lines on".to_owned(),
            [one] => format!("line {one} on"),
            many => format!("lines {} on", many.join(", ")),
        }
    };
    // By operation, in the order the category lists them, two of one
    // name, as an operation read two ways, as one.
    let mut found: Vec<usize> =
        reading.chosen.iter().map(|&(_, k)| k).collect();
    found.sort_unstable();
    found.dedup_by_key(|k| &ops[*k].label);
    found
        .into_iter()
        .map(|k| {
            let label = &ops[k].label;
            let settings: Vec<String> = reading
                .chosen
                .iter()
                .filter(|&&(_, chosen)| ops[chosen].label == *label)
                .map(|&(s, _)| setting(s))
                .collect();
            format!("{label} ({})", settings.join(" or "))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
