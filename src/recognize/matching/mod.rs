//! Recognizing a build by matching what it does against what each category
//! computes. Its levers are driven once through a few telling values and
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

use std::cmp::Reverse;

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
    simulate::{Value, mask},
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
}

impl Width {
    /// Its bits for a build `n` bits wide; `None` past what any build is.
    fn of(self, n: usize) -> Option<usize> {
        match self {
            Width::N => Some(n),
            Width::NPlus1 => Some(n + 1),
            Width::Bit => Some(1),
            Width::Pow2 => (n < 16).then(|| 1 << n),
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
/// least significant first, and whether it reads inverted, powered
/// meaning 0, as only a control line may.
#[derive(Clone, Debug)]
pub struct Line {
    pub name: &'static str,
    pub role: Role,
    pub bits: Vec<Pos>,
    pub inverted: bool,
}

/// What a build was matched as: its kind, the operation it computes, its
/// lines, how many of the cases observed it got wrong, and how far its
/// reading strays from the conventions.
#[derive(Clone, Debug)]
pub struct Match {
    pub kind: Kind,
    pub label: String,
    pub lines: Vec<Line>,
    pub cases: u64,
    pub wrong: u64,
    pub cost: u32,
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
/// fit its lines.
pub fn recognize(data: &[u8]) -> Result<Option<Match>> {
    let simulated = probe::simulatable(&lines::without(data)?)?;
    let blocks = Blocks::read(&simulated)?;
    let report = inspect::inspect(&blocks);
    if report.faults().next().is_some() {
        return Ok(None);
    }
    Ok(closest(&blocks, &report))
}

/// Of every category whose slots `report`'s lines fill, the reading of the
/// build in `blocks` that comes closest; see [`Score`].
pub fn closest(blocks: &Blocks, report: &inspect::Report) -> Option<Match> {
    let observed = observe(blocks, report).ok()?;
    CATEGORIES
        .iter()
        .filter_map(|&category| judge(category, blocks, &observed))
        .min_by(|(x, _), (y, _)| x.cmp(y))
        .map(|(_, found)| found)
}

/// How one reading ranks, lowest first: its share of cases wrong; how far
/// its buses stray from the site's count; how many control lines it reads
/// inverted; whether A and B are the other way round from where the site
/// puts A; the more output bits it decides; and the place probing tries
/// its kind at, as a 1-bit XOR gate's is a 1-bit adder's reading too.
type Score = (u64, u32, u32, bool, Reverse<u64>, u16);

/// The most control lines matching tries reading inverted, every way.
const MOST_INVERTIBLE: usize = 3;

/// The most ways of filling a category's slots tried for one shape.
const MOST_FILLINGS: usize = 4096;

/// The closest reading of the build `observed`, of `blocks`, as
/// `category`, and its score; `None` when its slots don't fit the lines.
fn judge(
    category: &dyn Category,
    blocks: &Blocks,
    observed: &Observation,
) -> Option<(Score, Match)> {
    let slots = category.slots();
    let conventional_in =
        |bits: &[(usize, usize)]| conventional(blocks, &observed.inputs, bits);
    let conventional_out =
        |bits: &[(usize, usize)]| conventional(blocks, &observed.outputs, bits);
    let inputs = Pool::of(&observed.inputs);
    let outputs = Pool::of(&observed.outputs);
    let mut best: Option<(Score, Match)> = None;
    for shape in shapes(slots, inputs.bits(), outputs.bits()) {
        let ops = category.ops(&shape);
        if ops.is_empty() {
            continue;
        }
        let mut fillings = Vec::new();
        fill(
            slots,
            &shape,
            0,
            &mut Vec::new(),
            &inputs,
            &outputs,
            &conventional_in,
            &conventional_out,
            &mut fillings,
        );
        for filling in fillings {
            let positions: Vec<Option<Vec<Pos>>> = slots
                .iter()
                .zip(&filling)
                .map(|(slot, bus)| {
                    let side = if slot.output() {
                        &observed.outputs
                    } else {
                        &observed.inputs
                    };
                    bus.as_ref().map(|bus| {
                        bus.bits
                            .iter()
                            .map(|&(run, bit)| side[run][bit])
                            .collect()
                    })
                })
                .collect();
            let cost: u32 = filling.iter().flatten().map(|bus| bus.cost).sum();
            let swapped = swapped(slots, &positions);
            let order = kind_order(category.kind());
            let Some((reading, op)) =
                read(slots, &shape, &filling, &ops, observed)
            else {
                continue;
            };
            let score = (
                reading.wrong * 1_000_000 / reading.cases.max(1),
                cost,
                reading.inverted.count_ones(),
                swapped,
                Reverse(reading.decided),
                op.kind.map_or(order, kind_order),
            );
            if best.as_ref().is_some_and(|(best, _)| *best <= score) {
                continue;
            }
            let mut controls = 0;
            let lines = slots
                .iter()
                .zip(positions)
                .filter_map(|(slot, bits)| {
                    let bits = bits?;
                    let inverted = invertible(slot, bits.len()) && {
                        controls += 1;
                        controls <= MOST_INVERTIBLE
                            && reading.inverted >> (controls - 1) & 1 == 1
                    };
                    Some(Line {
                        name: slot.name,
                        role: slot.role,
                        bits,
                        inverted,
                    })
                })
                .collect();
            let found = Match {
                kind: op.kind.unwrap_or(category.kind()),
                label: op.label.clone(),
                lines,
                cases: reading.cases,
                wrong: reading.wrong,
                cost,
            };
            best = Some((score, found));
        }
    }
    best
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
/// control line.
fn invertible(slot: &Slot, bits: usize) -> bool {
    slot.role == Role::Control && bits == 1
}

/// Every shape `slots` may take with `inputs` bits in and `outputs` out:
/// each optional slot in or out, each slot one of its widths, at each
/// width `n` from 1 to 64 that adds up.
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
            let Some(widths) = chosen
                .iter()
                .map(|w| w.map_or(Some(None), |w| w.of(n).map(Some)))
                .collect::<Option<Vec<Option<usize>>>>()
            else {
                continue;
            };
            let total = |output: bool| -> usize {
                slots
                    .iter()
                    .zip(&widths)
                    .filter(|(slot, _)| slot.output() == output)
                    .filter_map(|(_, w)| *w)
                    .sum()
            };
            if total(false) == inputs && total(true) == outputs {
                let shape = Shape { n, widths };
                if !found.contains(&shape) {
                    found.push(shape);
                }
            }
        }
    }
    found
}

/// Every way of filling `slots` from `slot` on, as `shape` sizes them, with
/// buses of what is left of `inputs` and `outputs`, onto `chosen`, the
/// buses of the slots before; each complete filling pushed onto `found`,
/// up to [`MOST_FILLINGS`].
#[allow(clippy::too_many_arguments)]
fn fill(
    slots: &[Slot],
    shape: &Shape,
    slot: usize,
    chosen: &mut Vec<Option<Bus>>,
    inputs: &Pool,
    outputs: &Pool,
    conventional_in: Conventional,
    conventional_out: Conventional,
    found: &mut Vec<Vec<Option<Bus>>>,
) {
    if found.len() >= MOST_FILLINGS {
        return;
    }
    let Some(this) = slots.get(slot) else {
        found.push(chosen.clone());
        return;
    };
    let Some(width) = shape.widths[slot] else {
        chosen.push(None);
        fill(
            slots,
            shape,
            slot + 1,
            chosen,
            inputs,
            outputs,
            conventional_in,
            conventional_out,
            found,
        );
        chosen.pop();
        return;
    };
    let (pool, conventional) = if this.output() {
        (outputs, conventional_out)
    } else {
        (inputs, conventional_in)
    };
    for (bus, rest) in pool.take(width, conventional) {
        let (inputs, outputs) = if this.output() {
            (inputs, &rest)
        } else {
            (&rest, outputs)
        };
        chosen.push(Some(bus));
        fill(
            slots,
            shape,
            slot + 1,
            chosen,
            inputs,
            outputs,
            conventional_in,
            conventional_out,
            found,
        );
        chosen.pop();
    }
}

/// How one operation, with some control lines read inverted, fares over
/// the cases observed.
#[derive(Clone, Copy)]
struct Reading {
    cases: u64,
    wrong: u64,
    /// The control lines read inverted, a bit each, in the slots' order.
    inverted: u32,
    /// The output bits its operation decides, over every case.
    decided: u64,
}

/// The reading of the build `observed`, its `slots` filled by `filling`
/// at `shape`, that gets fewest cases wrong, of each of `ops` with each
/// way of reading its control lines inverted, and the operation it is of.
fn read<'o>(
    slots: &[Slot],
    shape: &Shape,
    filling: &[Option<Bus>],
    ops: &'o [Op],
    observed: &Observation,
) -> Option<(Reading, &'o Op)> {
    // Each case's value of every slot, inputs as driven, outputs as shown.
    let cases: Vec<Vec<Value>> = observed
        .cases
        .iter()
        .map(|(driven, shown)| {
            slots
                .iter()
                .zip(filling)
                .map(|(slot, bus)| {
                    let source = if slot.output() { shown } else { driven };
                    bus.as_ref().map_or(0, |bus| bus.value(source))
                })
                .collect()
        })
        .collect();
    let invertible: Vec<usize> = slots
        .iter()
        .zip(filling)
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
    let mut best: Option<(Reading, &Op)> = None;
    for op in ops {
        for inverted in 0..1_u32 << invertible.len() {
            let mut reading = Reading {
                cases: cases.len() as u64,
                wrong: 0,
                inverted,
                decided: 0,
            };
            for case in &cases {
                let mut given = case.clone();
                for (k, &i) in invertible.iter().enumerate() {
                    given[i] ^= Value::from(inverted >> k & 1 == 1);
                }
                for &(i, _) in &outputs {
                    given[i] = 0;
                }
                let wants = (op.eval)(&given);
                let mut right = true;
                for (&(i, bits), want) in outputs.iter().zip(wants) {
                    let care = want.care & bits;
                    reading.decided += u64::from(care.count_ones());
                    right &= (case[i] ^ want.value) & care == 0;
                }
                reading.wrong += u64::from(!right);
            }
            let rank = |r: &Reading| {
                (r.wrong, r.inverted.count_ones(), Reverse(r.decided))
            };
            if best.as_ref().is_none_or(|(b, _)| rank(&reading) < rank(b)) {
                best = Some((reading, op));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests;
