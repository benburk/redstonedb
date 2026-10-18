//! An adder's design, read from how each of its carries is made rather
//! than how it is laid out. Every component's on or off is tabled over the
//! cases, and the components whose table is a carry, or its complement,
//! are that carry. Each carry then gets a letter by what it is made from,
//! walking back along the circuit's links:
//!
//! - `~`: read as a strength, by a comparator reading dust from three or
//!   more distances, or held only in the halves of a sum, as a carry
//!   cancel's carry is;
//! - `r`: from the carry just below, rippling;
//! - `L`: from the inputs alone, or the carry in, looking at every bit;
//! - `s`: from a carry further down, skipping;
//! - `?`: no component holds it.
//!
//! A 1-bit adder is a ripple carry. One whose carries below the top are all
//! strengths is a carry cancel; one whose carries all ripple, a ripple
//! carry; one mixing strengths and lookahead, a hybrid. The rest look
//! ahead: a carry look everywhere when every carry has a lookahead term of
//! its own for each bit below, a generate and the transmits above it, else
//! a carry lookahead.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;

use super::circuit::{Circuit, links};
use crate::{
    interface::{self, Interface, Port},
    schem::{self, Blocks, Direction, Pos},
};

/// How an adder makes its carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Design {
    /// The design's name, as stored: `ripple_carry`, `carry_cancel`,
    /// `carry_lookahead`, `carry_look_everywhere`, or `hybrid`.
    pub name: &'static str,
    /// A letter per carry, from the carry into bit 1 up to the carry out.
    pub signature: String,
    /// How many components are lookahead terms.
    pub products: usize,
}

/// The most input bits every case of is run; past that, `SAMPLED` cases.
const EVERY: usize = 20;
const SAMPLED: usize = 16_384;

/// The design of the adder in `blocks`; `None` when it has no A and B of
/// one width, takes a signal strength, or has more than 64 input bits.
pub fn adder_design(blocks: &Blocks) -> Result<Option<Design>> {
    let interface = interface::read(blocks);
    let Some(adder) = Adder::of(&interface) else {
        return Ok(None);
    };
    if adder.width == 1 {
        let signature = "r".to_owned();
        return Ok(Some(Design {
            name: "ripple_carry",
            signature,
            products: 0,
        }));
    }
    let circuit = Tabled::of(blocks, &interface, &adder)?;
    let signature = circuit.signature(&interface, &adder);
    let terms = adder.terms();
    let cases = adder.cases.len();
    let products = circuit
        .tables
        .iter()
        .filter(|t| terms.iter().any(|w| t.is(w, cases)))
        .count();
    Ok(Some(Design {
        name: name(&signature, products >= terms.len()),
        signature,
        products,
    }))
}

/// The design a carry signature names, given whether every lookahead term
/// has a component of its own.
fn name(signature: &str, every_term: bool) -> &'static str {
    let body: Vec<char> = signature.chars().take(signature.len() - 1).collect();
    let strengths = body.contains(&'~');
    let lookahead = body.iter().any(|&c| c == 'L' || c == 's');
    if strengths && !lookahead {
        "carry_cancel"
    } else if signature.chars().all(|c| c == 'r') {
        "ripple_carry"
    } else if strengths {
        "hybrid"
    } else if every_term {
        "carry_look_everywhere"
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
}

impl Adder {
    fn of(interface: &Interface) -> Option<Self> {
        let find = |ports: &[Port], colour: &str| {
            ports.iter().position(|p| p.block.starts_with(colour))
        };
        let a = find(&interface.inputs, "lime")?;
        let b = find(&interface.inputs, "light blue")?;
        let width = interface.inputs[a].bits.len();
        if interface.inputs.iter().any(|p| p.signal_strength)
            || interface.inputs[b].bits.len() != width
        {
            return None;
        }
        let carry_in = find(&interface.inputs, "magenta");
        let vars = 2 * width + usize::from(carry_in.is_some());
        if vars > 64 {
            return None;
        }
        let cases = if vars <= EVERY {
            (0..1u64 << vars).collect()
        } else {
            let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
            (0..SAMPLED)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state & (u64::MAX >> (64 - vars))
                })
                .collect()
        };
        Some(Adder {
            a,
            b,
            carry_in,
            carry_out: find(&interface.outputs, "pink"),
            width,
            cases,
        })
    }

    fn bit_b(&self, case: u64, i: usize) -> bool {
        case >> (self.width + i) & 1 == 1
    }

    fn bit_carry_in(&self, case: u64) -> bool {
        self.carry_in.is_some() && case >> (2 * self.width) & 1 == 1
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
    fn values(&self, case: u64) -> Vec<(usize, u64)> {
        let mask = (1u64 << self.width) - 1;
        let mut values =
            vec![(self.a, case & mask), (self.b, case >> self.width & mask)];
        if let Some(port) = self.carry_in {
            values.push((port, u64::from(self.bit_carry_in(case))));
        }
        values
    }

    /// The lookahead terms: a bit's generate and the transmits of the bits
    /// above it up to some bit, or the carry in and every transmit up to
    /// one.
    fn terms(&self) -> Vec<Table> {
        let mut terms = Vec::new();
        for j in 0..self.width {
            for i in j + 1..self.width {
                terms.push(self.table(|c| {
                    bit(c, j) && self.bit_b(c, j) && self.transmits(c, j + 1, i)
                }));
            }
        }
        if self.carry_in.is_some() {
            for i in 0..self.width {
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
        for (&(from, to, side), &distance) in &links(blocks)? {
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
        let mut on =
            vec![Vec::with_capacity(adder.cases.len()); components.len()];
        for &case in &adder.cases {
            for (port, value) in adder.values(case) {
                circuit.set(port, value);
            }
            circuit.settle()?;
            for (seen, &p) in on.iter_mut().zip(&components) {
                seen.push(circuit.on(p));
            }
        }
        Ok(Tabled {
            tables: on.iter().map(|seen| Table::from_bools(seen)).collect(),
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

    /// Whether bit `k`'s carry is held only in halves of its sum: its
    /// propagate and carry, either inverted, both on.
    fn halves(&self, adder: &Adder, k: usize) -> bool {
        [(false, false), (false, true), (true, false), (true, true)]
            .into_iter()
            .any(|(np, nc)| {
                let half = adder.table(|c| {
                    let p = bit(c, k) != adder.bit_b(c, k);
                    (p != np) && (adder.carry(c, k) != nc)
                });
                !self.matching(&half, adder.cases.len()).is_empty()
            })
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
                if start.is_empty() {
                    return if self.halves(adder, k) { '~' } else { '?' };
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
