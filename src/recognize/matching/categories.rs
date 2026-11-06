//! The categories matching knows: for each, its slots and what each of its
//! operations gives, and nothing else. Each operation's function takes
//! every slot's value by its place in the slots, its outputs' 0, and gives
//! what each output slot should show, in order, masked to its width when
//! judged.

use super::{Category, Op, Shape, Slot, Want, Width};
use crate::{category::Kind, schem::lines::Role, simulate::mask};

use Width::{Bit, N, NPlus1, Pow2};

/// Every category matching tries.
pub static CATEGORIES: &[&dyn Category] = &[
    &Gate::XOR,
    &Gate::XNOR,
    &Gate::AND,
    &Gate::NAND,
    &Gate::OR,
    &Gate::NOR,
    &NandXnor,
    &Equality,
    &GreaterThan,
    &CarryOut,
    &Adder,
    &Incrementer,
    &Inverter,
    &AbsoluteValue,
    &Parity,
    &Threshold,
    &Decoder,
];

const A: Slot = Slot::new("A", Role::Operand(0), &[N]);
const B: Slot = Slot::new("B", Role::Operand(1), &[N]);
const OUTPUT: Slot = Slot::new("output", Role::Output(0), &[N]);
const ANSWER: Slot = Slot::new("output", Role::Output(0), &[Bit]);

/// `value`, the low `n` bits of a two's complement number, as a signed
/// number.
fn signed(value: u128, n: usize) -> i128 {
    let shift = 128 - n;
    (value << shift).cast_signed() >> shift
}

/// A gate: A and B into one output as wide, each bit of it the gate of
/// theirs.
pub struct Gate {
    kind: Kind,
    name: &'static str,
    of: fn(u128, u128) -> u128,
}

impl Gate {
    const XOR: Gate = Gate {
        kind: Kind::XorGate,
        name: "xor",
        of: |a, b| a ^ b,
    };
    const XNOR: Gate = Gate {
        kind: Kind::XnorGate,
        name: "xnor",
        of: |a, b| !(a ^ b),
    };
    const AND: Gate = Gate {
        kind: Kind::AndGate,
        name: "and",
        of: |a, b| a & b,
    };
    const NAND: Gate = Gate {
        kind: Kind::NandGate,
        name: "nand",
        of: |a, b| !(a & b),
    };
    const OR: Gate = Gate {
        kind: Kind::OrGate,
        name: "or",
        of: |a, b| a | b,
    };
    const NOR: Gate = Gate {
        kind: Kind::NorGate,
        name: "nor",
        of: |a, b| !(a | b),
    };
}

impl Category for Gate {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn slots(&self) -> &'static [Slot] {
        &[A, B, OUTPUT]
    }

    fn ops(&self, _: &Shape) -> Vec<Op> {
        let of = self.of;
        vec![Op::new(format!("A {} B", self.name), move |v| {
            vec![of(v[0], v[1]).into()]
        })]
    }
}

/// A NAND+XNOR gate: A nand B and A xnor B, each on an output of its own.
pub struct NandXnor;

impl Category for NandXnor {
    fn kind(&self) -> Kind {
        Kind::NandXnorGate
    }

    fn slots(&self) -> &'static [Slot] {
        const { &[A, B, OUTPUT, Slot::new("output 2", Role::Output(1), &[N])] }
    }

    fn ops(&self, _: &Shape) -> Vec<Op> {
        vec![Op::new("A nand B, A xnor B", |v| {
            vec![(!(v[0] & v[1])).into(), (!(v[0] ^ v[1])).into()]
        })]
    }
}

/// An equality comparator: whether A equals B.
pub struct Equality;

impl Category for Equality {
    fn kind(&self) -> Kind {
        Kind::Equality
    }

    fn slots(&self) -> &'static [Slot] {
        &[A, B, ANSWER]
    }

    fn ops(&self, _: &Shape) -> Vec<Op> {
        vec![Op::new("A = B", |v| vec![(v[0] == v[1]).into()])]
    }
}

/// A greater-than comparator: whether A is greater than B, both unsigned,
/// or both in two's complement.
pub struct GreaterThan;

impl Category for GreaterThan {
    fn kind(&self) -> Kind {
        Kind::GreaterThan
    }

    fn slots(&self) -> &'static [Slot] {
        &[A, B, ANSWER]
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        let n = shape.n;
        vec![
            Op::new("A > B", |v| vec![(v[0] > v[1]).into()]),
            Op::new("A > B, signed", move |v| {
                vec![(signed(v[0], n) > signed(v[1], n)).into()]
            }),
        ]
    }
}

/// A carry out: the carry of A + B and a carry in, if it has one.
pub struct CarryOut;

impl Category for CarryOut {
    fn kind(&self) -> Kind {
        Kind::CarryOut
    }

    fn slots(&self) -> &'static [Slot] {
        const {
            &[
                A,
                B,
                Slot::new("carry in", Role::Control, &[Bit]).optional(),
                ANSWER,
            ]
        }
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        let n = shape.n;
        let label = if shape.has(2) {
            "carry of A + B + carry in"
        } else {
            "carry of A + B"
        };
        vec![Op::new(label, move |v| {
            vec![((v[0] + v[1] + v[2]) >> n & 1 == 1).into()]
        })]
    }
}

/// An adder: A + B and a carry in, if it has one, into its sum, which may
/// hold the carry as its top bit, and a carry out, if it has one; or, B
/// subtracted and its carries borrows, a subtractor.
pub struct Adder;

impl Category for Adder {
    fn kind(&self) -> Kind {
        Kind::Adder
    }

    fn slots(&self) -> &'static [Slot] {
        const {
            &[
                A,
                B,
                Slot::new("carry in", Role::Control, &[Bit]).optional(),
                Slot::new("output", Role::Output(0), &[N, NPlus1]),
                Slot::new("carry out", Role::Status, &[Bit]).optional(),
            ]
        }
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        let n = shape.n;
        let carry = move |r: u128| -> Vec<Want> {
            vec![r.into(), (r >> n & 1 == 1).into()]
        };
        let (add, subtract) = if shape.has(2) {
            ("A + B + carry in", "A − B − borrow in")
        } else {
            ("A + B", "A − B")
        };
        vec![
            Op::new(add, move |v| carry(v[0] + v[1] + v[2])),
            Op::new(subtract, move |v| {
                carry(v[0].wrapping_sub(v[1]).wrapping_sub(v[2]))
            })
            .kind(Kind::Subtractor),
        ]
    }
}

/// An incrementer: A plus 1, or plus its control line if it has one, into
/// an output that may hold the carry as its top bit, and a carry out, if
/// it has one; or minus, a decrementer; or, by its control line, either,
/// an inc/dec.
pub struct Incrementer;

impl Category for Incrementer {
    fn kind(&self) -> Kind {
        Kind::Incrementer
    }

    fn slots(&self) -> &'static [Slot] {
        const {
            &[
                A,
                Slot::new("increment", Role::Control, &[Bit]).optional(),
                Slot::new("output", Role::Output(0), &[N, NPlus1]),
                Slot::new("carry out", Role::Status, &[Bit]).optional(),
            ]
        }
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        let n = shape.n;
        let carry = move |r: u128| -> Vec<Want> {
            vec![r.into(), (r >> n & 1 == 1).into()]
        };
        if !shape.has(1) {
            return vec![
                Op::new("A + 1", move |v| carry(v[0] + 1)),
                Op::new("A − 1", move |v| carry(v[0].wrapping_sub(1)))
                    .kind(Kind::Decrementer),
            ];
        }
        vec![
            Op::new("A + increment", move |v| carry(v[0] + v[1])),
            Op::new("A − decrement", move |v| carry(v[0].wrapping_sub(v[1])))
                .kind(Kind::Decrementer),
            Op::new("A + 1 or A − 1", move |v| {
                carry(if v[1] == 1 {
                    v[0] + 1
                } else {
                    v[0].wrapping_sub(1)
                })
            })
            .kind(Kind::IncDec),
        ]
    }
}

/// An inverter: A, every bit inverted while its control line is on.
pub struct Inverter;

impl Category for Inverter {
    fn kind(&self) -> Kind {
        Kind::Inverter
    }

    fn slots(&self) -> &'static [Slot] {
        const { &[A, Slot::new("invert", Role::Control, &[Bit]), OUTPUT] }
    }

    fn ops(&self, _: &Shape) -> Vec<Op> {
        vec![Op::new("A xor invert", |v| {
            vec![(v[0] ^ 0_u128.wrapping_sub(v[1])).into()]
        })]
    }
}

/// An absolute value: A's magnitude, A taken as a signed number.
pub struct AbsoluteValue;

impl Category for AbsoluteValue {
    fn kind(&self) -> Kind {
        Kind::AbsoluteValue
    }

    fn slots(&self) -> &'static [Slot] {
        &[A, OUTPUT]
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        let n = shape.n;
        vec![Op::new("|A|", move |v| {
            vec![(signed(v[0], n).unsigned_abs()).into()]
        })]
    }
}

/// A parity circuit: whether an odd number of A's bits are on, or an even
/// number.
pub struct Parity;

impl Category for Parity {
    fn kind(&self) -> Kind {
        Kind::Parity
    }

    fn slots(&self) -> &'static [Slot] {
        &[A, ANSWER]
    }

    fn ops(&self, _: &Shape) -> Vec<Op> {
        vec![
            Op::new("odd parity of A", |v| {
                vec![(v[0].count_ones() % 2 == 1).into()]
            }),
            Op::new("even parity of A", |v| {
                vec![(v[0].count_ones() % 2 == 0).into()]
            }),
        ]
    }
}

/// A threshold: whether at least `k` of A's bits are on, for each `k` it
/// might be.
pub struct Threshold;

impl Category for Threshold {
    fn kind(&self) -> Kind {
        Kind::Threshold
    }

    fn slots(&self) -> &'static [Slot] {
        &[A, ANSWER]
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        (1..=shape.n)
            .filter_map(|k| u32::try_from(k).ok())
            .map(|k| {
                Op::new(format!("at least {k} of A"), move |v| {
                    vec![(v[0].count_ones() >= k).into()]
                })
            })
            .collect()
    }
}

/// A decoder: A into one output with a bit for each of A's values, the one
/// A names off and the rest on, as a row of torches gives them.
pub struct Decoder;

/// The widest A a decoder is matched at.
const WIDEST_DECODER: usize = 6;

impl Category for Decoder {
    fn kind(&self) -> Kind {
        Kind::Decoder
    }

    fn slots(&self) -> &'static [Slot] {
        const { &[A, Slot::new("output", Role::Output(0), &[Pow2])] }
    }

    fn ops(&self, shape: &Shape) -> Vec<Op> {
        if shape.n > WIDEST_DECODER {
            return Vec::new();
        }
        let all = mask(1 << shape.n);
        vec![Op::new("bit A off", move |v| {
            vec![(!(1 << v[0]) & all).into()]
        })]
    }
}
