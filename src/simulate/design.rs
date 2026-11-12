//! An adder's design, read from how each of its carries is made rather
//! than how it is laid out. Every component's on or off is tabled over the
//! cases, and the components whose table is a carry, or its complement,
//! are that carry. Each carry then gets a letter by what it is made from,
//! walking back along the circuit's links:
//!
//! - `~`: read as a strength, by a comparator reading dust from three or
//!   more distances, or held only in the halves of a sum a comparator
//!   holds, as a carry cancel's carry is; a carry held only in halves
//!   torches hold, as in a build from before comparators, is read from
//!   them as any other;
//! - `r`: from the carry just below, rippling;
//! - `L`: from the inputs alone, or the carry in, looking at every bit;
//! - `s`: from a carry further down, skipping;
//! - `?`: no component holds it;
//! - `-`: the carry out of an adder without one.
//!
//! Its bits fall into groups, each joined to the next by one carry made
//! from a run of strengths or ripples below it, as a 16-bit carry cancel's
//! two groups of 8. A lookahead term is a bit's generate and the transmits
//! above it within its group, or the carry in and the transmits above it in
//! the lowest group, and a group has a term when a component holds it, or
//! a comparator gives at least some strength just when it holds, as a
//! subtracting cell gives 15 for its term and a little otherwise, too
//! little to reach the sum it feeds.
//!
//! A 1-bit adder is a ripple carry. One with a carry no component holds is
//! other; one whose carries all ripple, a ripple carry. One whose every
//! group has every term of its own is a carry look everywhere, built of a
//! torch for each term; one with some groups so and the rest no terms, a
//! hybrid. One whose carries are mostly strengths, or with no terms at all
//! mostly strengths or lookahead, is a carry cancel, its carries a signal
//! strength, its towers making a few short terms at most; the rest
//! look ahead some other way, as two-bit groups skipping a carry do: carry
//! lookahead.

use crate::schem::lines::Role;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ops::RangeInclusive,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{
    Value,
    adder::Variant,
    circuit::{Circuit, compiled},
    rng::Rng,
};
use crate::{
    interface::{self, Interface, Port},
    schem::{self, Blocks, Direction, Pos},
};

/// How an adder makes its carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Design {
    /// The design's name, as stored: `ripple_carry`, `carry_cancel`,
    /// `carry_lookahead`, `carry_look_everywhere`, `hybrid`, or `other`.
    pub name: String,
    /// A letter per carry, from the carry into bit 1 up to the carry out.
    pub signature: String,
    /// How many components are lookahead terms.
    pub products: usize,
    /// How many bits each group holds, when it has more than one group
    /// and they are alike but for the lowest and highest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<usize>,
    /// Each group's lookahead terms: how many a component holds, of all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<Terms>,
}

impl Design {
    /// The design a build of it is given, as its category lists them;
    /// `None` for [`OTHER`], as its carries name none.
    pub fn given(&self) -> Option<&str> {
        (self.name != OTHER).then_some(self.name.as_str())
    }
}

/// A group's lookahead terms: how many have a component of their own, of
/// how many it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terms {
    pub found: usize,
    pub all: usize,
}

/// The most input bits every case of is run; past that, `SAMPLED` cases,
/// as many, which tell a carry from every other signal: the test runs
/// every case already.
const EVERY: usize = 14;
const SAMPLED: usize = 1 << EVERY;

/// The design of the adder in `blocks`; `None` when it has no A and B of
/// one width, takes a signal strength, or has more than 64 input bits.
pub fn adder_design(
    blocks: &Blocks,
    variant: Variant,
) -> Result<Option<Design>> {
    let interface = interface::read(blocks);
    let Some(adder) = Adder::of(&interface, variant) else {
        return Ok(None);
    };
    if adder.width == 1 {
        return Ok(Some(Design {
            name: "ripple_carry".to_owned(),
            signature: "r".to_owned(),
            products: 0,
            group: None,
            terms: Vec::new(),
        }));
    }
    let circuit = Tabled::of(blocks, &interface, &adder)?;
    let signature = circuit.signature(&interface, &adder);
    let cases = adder.cases.len();
    let held = |term: &Table| {
        circuit
            .tables
            .iter()
            .chain(&circuit.levels)
            .any(|t| t.is(term, cases))
    };
    let groups = groups(&signature);
    let terms: Vec<Terms> = groups
        .iter()
        .map(|bits| {
            let all = adder.terms(bits.clone());
            Terms {
                found: all.iter().filter(|term| held(term)).count(),
                all: all.len(),
            }
        })
        .collect();
    let products = adder
        .terms(0..=adder.width - 1)
        .iter()
        .filter(|term| held(term))
        .count();
    Ok(Some(Design {
        name: design_of(&signature, &terms).to_owned(),
        group: group_size(&groups),
        signature,
        products,
        terms,
    }))
}

/// The groups of bits a carry signature shows, lowest first: a group ends
/// at a carry looked ahead or skipped to just above a run of strengths, or
/// skipped to above a ripple, and at the top.
pub fn groups(signature: &str) -> Vec<RangeInclusive<usize>> {
    let letters: Vec<char> = signature.chars().collect();
    let mut groups = Vec::new();
    let mut low = 0;
    for (i, &letter) in letters.iter().enumerate() {
        let below = i.checked_sub(1).map(|j| letters[j]);
        let ends = i + 1 == letters.len()
            || (matches!(letter, 'L' | 's') && below == Some('~'))
            || (letter == 's' && below == Some('r'));
        if ends {
            groups.push(low..=i);
            low = i + 1;
        }
    }
    groups
}

/// How many bits each of `groups` holds, when there are several and those
/// between the lowest and highest, or both when there are only two, are
/// alike; `None` for one group.
fn group_size(groups: &[RangeInclusive<usize>]) -> Option<usize> {
    let sizes: Vec<usize> =
        groups.iter().map(|g| g.end() - g.start() + 1).collect();
    match sizes.as_slice() {
        [] | [_] => None,
        [a, b] => (a == b).then_some(*a),
        [_, middle @ .., _] => middle
            .iter()
            .all(|size| *size == middle[0])
            .then_some(middle[0]),
    }
}

/// The name a report gives an adder whose carries name no design, as one
/// with a carry no component holds; a build of it is given none.
pub const OTHER: &str = "other";

/// The design a carry signature names, given each group's lookahead terms.
pub fn design_of(signature: &str, terms: &[Terms]) -> &'static str {
    if signature.contains('?') {
        return OTHER;
    }
    if signature.chars().all(|c| c == 'r') {
        return "ripple_carry";
    }
    let complete = |t: &Terms| t.all > 0 && t.found == t.all;
    if !terms.is_empty() && terms.iter().all(complete) {
        return "carry_look_everywhere";
    }
    if terms.iter().any(complete)
        && terms.iter().all(|t| complete(t) || t.found == 0)
    {
        return "hybrid";
    }
    let body: Vec<char> = signature.chars().take(signature.len() - 1).collect();
    let most = |count: usize| 2 * count > body.len();
    let strengths = body.iter().filter(|&&c| c == '~').count();
    let looked = body.iter().filter(|c| matches!(c, '~' | 'L')).count();
    // A carry cancel's towers may make a few short terms of their own.
    let none = terms.iter().all(|t| t.found == 0);
    if most(strengths) || (none && most(looked)) {
        "carry_cancel"
    } else {
        "carry_lookahead"
    }
}

/// An adder's ports, by their place in its interface, and the cases run:
/// each a word whose bits are A's, then B's, then the carry in's.
struct Adder {
    a: usize,
    b: usize,
    carry_in: Option<usize>,
    carry_out: Option<usize>,
    width: usize,
    cases: Vec<u64>,
    /// Whether its carries run on B inverted, as a subtractor's do.
    b_inverted: bool,
    /// Whether its carries run on its carry in inverted, as a subtractor's
    /// borrow in, or on a carry in of 1 with none, as a subtractor's
    /// without a borrow in.
    carry_flipped: bool,
}

impl Adder {
    fn of(interface: &Interface, variant: Variant) -> Option<Self> {
        let find = |ports: &[Port], role: Role| {
            ports.iter().position(|p| p.role == Some(role))
        };
        let a = find(&interface.inputs, Role::Operand(0))?;
        let b = find(&interface.inputs, Role::Operand(1))?;
        let width = interface.inputs[a].bits.len();
        if interface.inputs.iter().any(|p| p.signal_strength)
            || interface.inputs[b].bits.len() != width
        {
            return None;
        }
        let carry_in = find(&interface.inputs, Role::Control);
        let vars = 2 * width + usize::from(carry_in.is_some());
        if vars > 64 {
            return None;
        }
        let cases = if vars <= EVERY {
            (0..1u64 << vars).collect()
        } else {
            let mut rng = Rng::default();
            (0..SAMPLED)
                .map(|_| rng.draw() & (u64::MAX >> (64 - vars)))
                .collect()
        };
        Some(Adder {
            a,
            b,
            carry_in,
            carry_out: find(&interface.outputs, Role::Status),
            width,
            cases,
            b_inverted: variant.subtract,
            carry_flipped: variant.subtract != variant.carry_in,
        })
    }

    /// B's bit `i`, as its carries take it.
    fn bit_b(&self, case: u64, i: usize) -> bool {
        (case >> (self.width + i) & 1 == 1) != self.b_inverted
    }

    /// The carry in's lever in `case`.
    fn lever_carry_in(&self, case: u64) -> bool {
        self.carry_in.is_some() && case >> (2 * self.width) & 1 == 1
    }

    /// The carry in, as its carries take it.
    fn bit_carry_in(&self, case: u64) -> bool {
        self.lever_carry_in(case) != self.carry_flipped
    }

    /// The carry into bit `i`; the carry out at the width.
    fn carry(&self, case: u64, i: usize) -> bool {
        (0..i).fold(self.bit_carry_in(case), |carry, j| {
            let (x, y) = (bit(case, j), self.bit_b(case, j));
            (x && y) || (carry && (x || y))
        })
    }

    /// Whether every bit from `from` up to `to` transmits a carry.
    fn transmits(&self, case: u64, from: usize, to: usize) -> bool {
        (from..=to).all(|j| bit(case, j) || self.bit_b(case, j))
    }

    fn table(&self, f: impl Fn(u64) -> bool) -> Table {
        Table::from_bools(&self.cases.iter().map(|&c| f(c)).collect::<Vec<_>>())
    }

    /// The value each of A, B, and the carry in takes in `case`.
    fn values(&self, case: u64) -> Vec<(usize, Value)> {
        let mask = (1u64 << self.width) - 1;
        let mut values = vec![
            (self.a, Value::from(case & mask)),
            (self.b, Value::from(case >> self.width & mask)),
        ];
        if let Some(port) = self.carry_in {
            values.push((port, Value::from(self.lever_carry_in(case))));
        }
        values
    }

    /// The lookahead terms of the group of `bits`: a bit's generate and
    /// the transmits of the bits above it in the group up to some bit, or,
    /// in the lowest group, the carry in and every transmit up to one.
    fn terms(&self, bits: RangeInclusive<usize>) -> Vec<Table> {
        let (low, high) = (*bits.start(), *bits.end());
        let mut terms = Vec::new();
        for j in low..=high {
            for i in j + 1..=high {
                terms.push(self.table(|c| {
                    bit(c, j) && self.bit_b(c, j) && self.transmits(c, j + 1, i)
                }));
            }
        }
        if self.carry_in.is_some() && low == 0 {
            for i in 0..=high {
                terms.push(self.table(|c| {
                    self.bit_carry_in(c) && self.transmits(c, 0, i)
                }));
            }
        }
        terms
    }
}

/// A compiled circuit's components, what feeds each, and each one's table.
struct Tabled {
    components: Vec<Pos>,
    tables: Vec<Table>,
    /// Each comparator's table at each strength above 1: whether it gives
    /// at least that, as a subtracting cell gives 15 only for its term and
    /// a little otherwise, which dies on the dust before it is read.
    levels: Vec<Table>,
    /// Each part's links in: from where, into the side, and the strength
    /// lost on the way.
    fanin: BTreeMap<Pos, Vec<(Pos, bool, u8)>>,
    /// The kind of block at each part.
    kinds: BTreeMap<Pos, String>,
}

impl Tabled {
    fn of(
        blocks: &Blocks,
        interface: &Interface,
        adder: &Adder,
    ) -> Result<Self> {
        let mut fanin: BTreeMap<Pos, Vec<(Pos, bool, u8)>> = BTreeMap::new();
        let mut nodes = BTreeSet::new();
        for (&(from, to, side), &distance) in &compiled(blocks)?.links {
            fanin.entry(to).or_default().push((from, side, distance));
            nodes.extend([from, to]);
        }
        let kinds: BTreeMap<Pos, String> = nodes
            .iter()
            .map(|&p| (p, schem::short(blocks.get(p)).to_owned()))
            .collect();
        let components: Vec<Pos> = nodes
            .into_iter()
            .filter(|p| {
                let kind = kinds[p].as_str();
                kind.ends_with("torch")
                    || matches!(kind, "repeater" | "comparator")
            })
            .collect();
        let mut circuit = Circuit::new(blocks, interface)?;
        let comparators: Vec<Pos> = components
            .iter()
            .copied()
            .filter(|p| kinds[p] == "comparator")
            .collect();
        let mut on =
            vec![Vec::with_capacity(adder.cases.len()); components.len()];
        let mut strengths =
            vec![Vec::with_capacity(adder.cases.len()); comparators.len()];
        for &case in &adder.cases {
            for (port, value) in adder.values(case) {
                circuit.set(port, value);
            }
            circuit.settle()?;
            for (seen, &p) in on.iter_mut().zip(&components) {
                seen.push(circuit.on(p));
            }
            for (seen, &p) in strengths.iter_mut().zip(&comparators) {
                seen.push(circuit.strength(p));
            }
        }
        let levels = strengths
            .iter()
            .flat_map(|seen| {
                (2..=15).map(move |level| {
                    let at: Vec<bool> =
                        seen.iter().map(|&s| s >= level).collect();
                    Table::from_bools(&at)
                })
            })
            .collect();
        Ok(Tabled {
            tables: on.iter().map(|seen| Table::from_bools(seen)).collect(),
            levels,
            components,
            fanin,
            kinds,
        })
    }

    /// The components whose table is `want` or its complement.
    fn matching(&self, want: &Table, cases: usize) -> Vec<Pos> {
        let found = self.components.iter().zip(&self.tables);
        found
            .filter(|(_, t)| t.is(want, cases))
            .map(|(&p, _)| p)
            .collect()
    }

    /// Whether any of `nodes` is a comparator reading dust from three or
    /// more distances, as a carry cancel's carry is read.
    fn strength(&self, nodes: &[Pos]) -> bool {
        nodes.iter().any(|p| {
            let distances: BTreeSet<u8> = self.fanin[p]
                .iter()
                .filter(|&&(_, side, d)| !side && d > 0)
                .map(|&(_, _, d)| d)
                .collect();
            self.kinds[p] == "comparator" && distances.len() >= 3
        })
    }

    /// The components holding bit `k`'s carry in halves of its sum: its
    /// propagate and carry, either inverted, both on.
    fn halves(&self, adder: &Adder, k: usize) -> Vec<Pos> {
        [(false, false), (false, true), (true, false), (true, true)]
            .into_iter()
            .flat_map(|(np, nc)| {
                let half = adder.table(|c| {
                    let p = bit(c, k) != adder.bit_b(c, k);
                    (p != np) && (adder.carry(c, k) != nc)
                });
                self.matching(&half, adder.cases.len())
            })
            .collect()
    }

    /// A letter for each carry from bit 1's up to the carry out.
    fn signature(&self, interface: &Interface, adder: &Adder) -> String {
        let mut carries: Vec<Vec<Pos>> = vec![Vec::new(); adder.width + 1];
        if let Some(port) = adder.carry_in {
            let lever = |b: &interface::Bit| {
                b.entry.map(|side| schem::step(b.pos, side))
            };
            carries[0] = interface.inputs[port]
                .bits
                .iter()
                .filter_map(lever)
                .collect();
        }
        for (k, found) in carries.iter_mut().enumerate().skip(1) {
            let carry = adder.table(|c| adder.carry(c, k));
            *found = self.matching(&carry, adder.cases.len());
        }
        (1..=adder.width)
            .map(|k| {
                let start: Vec<Pos> =
                    match (carries[k].first(), adder.carry_out) {
                        (Some(&p), _) => vec![p],
                        (None, Some(port)) if k == adder.width => interface
                            .outputs[port]
                            .bits
                            .iter()
                            .map(|b| schem::step(b.pos, Direction::Up))
                            .filter(|p| self.fanin.contains_key(p))
                            .collect(),
                        _ => Vec::new(),
                    };
                if start.is_empty()
                    && k == adder.width
                    && adder.carry_out.is_none()
                {
                    return '-';
                }
                // A carry no component holds is read from the halves of its
                // sum: a strength where a comparator holds one, as a carry
                // cancel's sum reads its carry, and otherwise by what feeds
                // them, as torches do in a build from before comparators.
                let start = if start.is_empty() {
                    let halves = self.halves(adder, k);
                    if halves.iter().any(|p| self.kinds[p] == "comparator") {
                        return '~';
                    }
                    halves
                } else {
                    start
                };
                if start.is_empty() {
                    return '?';
                }
                if self.strength(&start) {
                    return '~';
                }
                match self.below(start, &carries[..k]).into_iter().max() {
                    Some(j) if j + 1 == k => 'r',
                    Some(0) | None => 'L',
                    Some(_) => 's',
                }
            })
            .collect()
    }

    /// The carries among `below` reached walking back from `start` before
    /// any other carry.
    fn below(&self, start: Vec<Pos>, below: &[Vec<Pos>]) -> Vec<usize> {
        let mut found = Vec::new();
        let mut seen: HashSet<Pos> = HashSet::new();
        let mut stack = start;
        while let Some(p) = stack.pop() {
            for &(from, _, _) in self.fanin.get(&p).into_iter().flatten() {
                if !seen.insert(from) {
                    continue;
                }
                match below.iter().position(|carry| carry.contains(&from)) {
                    Some(j) => found.push(j),
                    None => stack.push(from),
                }
            }
        }
        found
    }
}

/// Whether bit `k` of `case` is on: bit `i` of A is bit `i`.
fn bit(case: u64, k: usize) -> bool {
    case >> k & 1 == 1
}

/// Whether something is on in each case, a bit per case.
#[derive(Clone, PartialEq, Eq)]
struct Table(Vec<u64>);

impl Table {
    fn from_bools(on: &[bool]) -> Self {
        let mut words = vec![0u64; on.len().div_ceil(64)];
        for (i, _) in on.iter().enumerate().filter(|&(_, &on)| on) {
            words[i / 64] |= 1 << (i % 64);
        }
        Table(words)
    }

    /// Whether this is `want`, or off wherever it is on, over `cases`.
    fn is(&self, want: &Table, cases: usize) -> bool {
        if self == want {
            return true;
        }
        let last = self.0.len().saturating_sub(1);
        let tail = if cases.is_multiple_of(64) {
            u64::MAX
        } else {
            (1 << (cases % 64)) - 1
        };
        self.0.iter().zip(&want.0).enumerate().all(|(i, (a, b))| {
            let mask = if i == last { tail } else { u64::MAX };
            (a ^ b) & mask == mask
        })
    }
}
