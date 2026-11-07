//! Testing an adder, a subtractor, and a carry out, which gives only an
//! adder's carry: two operand buses and an optional carry or borrow in, into
//! the sum or difference and an optional carry or borrow out above it, or
//! the carry alone.

use crate::schem::lines::Role;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{
    Broken, Reading, Run, Value,
    bench::{Bench, Case, Cases, Distinct, Kind, Spec, mask, patterns},
};
use crate::recognize::{
    colour,
    interface::{Interface, Port},
};

/// How an adder's messages name it and its lone lines: the adder's test
/// reads a subtractor too, its B inverted, so its lines may go by a
/// subtractor's names as well.
pub(super) struct Arithmetic {
    /// The build, as its messages begin, such as `An adder`.
    a: &'static str,
    /// The names its control line may be signed with, its carry or borrow
    /// in; an unsigned one goes by the first.
    line_in: &'static [&'static str],
    /// The same for its status line, its carry or borrow out.
    line_out: &'static [&'static str],
}

pub(super) const ADD: Arithmetic = Arithmetic {
    a: "An adder",
    line_in: &CARRY_OR_BORROW_IN,
    line_out: &CARRY_OR_BORROW_OUT,
};

/// What an adder's lone lines may be signed: its carry in and out, or, as a
/// subtractor's, its borrows; unsigned, they go by the first.
const CARRY_OR_BORROW_IN: [&str; 4] = [
    colour::CARRY_IN[0],
    colour::CARRY_IN[1],
    colour::BORROW_IN[0],
    colour::BORROW_IN[1],
];
const CARRY_OR_BORROW_OUT: [&str; 4] = [
    colour::CARRY_OUT[0],
    colour::CARRY_OUT[1],
    colour::BORROW_OUT[0],
    colour::BORROW_OUT[1],
];

/// One way an adder's lines may read: as an add, or, B inverted and its
/// carries borrows, a subtract, A − B less its borrow in; and with its
/// carry in inverted, as a torch on that line gives. Inverting a line does
/// not change what a build is, so a subtractor is an adder, its B
/// inverted. An adder's sum inverted is an inverted adder, a category of
/// its own, whose carry out may read either way.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each a line read one way or the other"
)]
pub struct Variant {
    pub subtract: bool,
    pub carry_in: bool,
    /// Whether its sum reads inverted, as an inverted adder's does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sum: bool,
    /// Whether, its sum inverted, its carry out reads inverted too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub carry_out: bool,
}

impl Variant {
    /// Every way an adder with a carry in, if it has one, may read, the
    /// plain add first, then, its sum inverted, an inverted adder's add,
    /// its carry out, if it has one, either way.
    fn all(carry_in: bool, carry_out: bool) -> Vec<Variant> {
        let ins = [false, true].into_iter().take(1 + usize::from(carry_in));
        let mut all = Vec::new();
        for subtract in [false, true] {
            for carry_in in ins.clone() {
                all.push(Variant {
                    subtract,
                    carry_in,
                    ..Variant::default()
                });
            }
        }
        for carry_in in ins {
            for out in
                [false, true].into_iter().take(1 + usize::from(carry_out))
            {
                all.push(Variant {
                    carry_in,
                    sum: true,
                    carry_out: out,
                    ..Variant::default()
                });
            }
        }
        all
    }

    /// Its cases' name, such as `A − B − carry in` or `A + B + not carry
    /// in`, its carry in named `carry_in`.
    pub(super) fn name(self, carry_in: Option<&str>) -> String {
        let sign = if self.subtract { '−' } else { '+' };
        // A subtract's unsigned carry in is its borrow in.
        let carry_in = carry_in.map(|name| {
            if self.subtract && name == colour::CARRY_IN[0] {
                colour::BORROW_IN[0]
            } else {
                name
            }
        });
        let carry = carry_in.map_or(String::new(), |name| {
            let not = if self.carry_in { "not " } else { "" };
            format!(" {sign} {not}{name}")
        });
        match (self.sum, self.carry_out) {
            (false, _) => format!("A {sign} B{carry}"),
            (true, false) => format!("not (A {sign} B{carry})"),
            (true, true) => {
                format!("not (A {sign} B{carry}), carry out inverted")
            }
        }
    }

    /// What it gives for a case of A, B, and the carry in, A and B of
    /// `width` bits, as an output that may hold the carry as its top bit,
    /// and a carry out above it if it has one; its sum's bits inverted for
    /// an inverted adder, and its carry with them if it reads so.
    pub(super) fn apply(self, case: &Case, width: usize) -> Value {
        let (a, b) = (case[0], case[1]);
        let carry = case[2] ^ Value::from(self.carry_in);
        let total = if self.subtract {
            a.wrapping_sub(b).wrapping_sub(carry)
        } else {
            a + b + carry
        };
        if !self.sum {
            return total;
        }
        let sum = !total & super::mask(width);
        let carried = (total >> width & 1) ^ Value::from(self.carry_out);
        sum | carried << width
    }

    /// The lines of `run`, an adder's, that it reads inverted, by name: its
    /// carry in, if so. A subtract's B is read as it is, subtracting being
    /// its function.
    pub fn lines(self, run: &Run) -> Vec<String> {
        run.inputs
            .get(2)
            .filter(|_| self.carry_in)
            .map(|signal| signal.name.clone())
            .into_iter()
            .collect()
    }

    /// Its flags, by name: `subtract`, `inverted_carry_in`, and
    /// `inverted_carry_out`.
    pub fn flags(self) -> Vec<&'static str> {
        [
            ("subtract", self.subtract),
            ("inverted_carry_in", self.carry_in),
            ("inverted_carry_out", self.carry_out),
        ]
        .into_iter()
        .filter(|&(_, on)| on)
        .map(|(flag, _)| flag)
        .collect()
    }
}

/// An adder's every sum: A + B, plus its carry in when it has one, into its
/// output and its carry out, its carry in perhaps read inverted, and B
/// read inverted, so a subtractor's every difference too: a kind for each
/// way its lines may read. A hex build, slower to run, takes each digit's
/// every case when there are too many to try every one, and a binary one
/// each byte's.
pub(super) fn adder(interface: &mut Interface) -> Result<Spec> {
    let ports = adder_ports(interface, &ADD)?;
    let mut inputs = vec![ports.a, ports.b];
    inputs.extend(ports.carry_in);
    let mut outputs = vec![ports.sum];
    outputs.extend(ports.carry_out);
    let line = ports
        .carry_in
        .map(|port| interface.inputs[port].name.clone());
    let pattern = match &line {
        Some(name) => format!("A + B + {name}"),
        None => "A + B".to_owned(),
    };
    let mut bench = Bench::new(inputs, outputs, &pattern);
    // Its kinds name each way its carry in may read, so judging tries no
    // other.
    bench.invertible = false;
    let width = bench.widths(interface)[0];
    let carry_in = ports.carry_in.is_some();
    let hex = interface.inputs.iter().any(|port| port.signal_strength);
    let piece = if hex { 4 } else { 8 };
    let kinds = Variant::all(carry_in, ports.carry_out.is_some())
        .into_iter()
        .map(|variant| {
            Kind::new(&variant.name(line.as_deref()), move |case| {
                variant.apply(case, width)
            })
            .reading(Reading::Adder(variant))
        })
        .collect();
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || {
            cases(width, carry_in, piece)
        })),
        kinds,
        by_digit: hex,
    })
}

/// A carry out's every case: the carry out of A + B, plus its carry in
/// when it has one, without the sum. Its lone control line and its one
/// output need no signs, as an adder's do.
pub(super) fn carry_out(interface: &mut Interface) -> Result<Spec> {
    let carry_in =
        adder_line(&mut interface.inputs, Role::Control, &ADD, true)?;
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| Some(i) != carry_in)
        .collect();
    let (a, b, out) = one_line(interface, &operands, "carry out", "its carry")?;
    if !interface.outputs[out].labelled {
        colour::CARRY_OUT[0].clone_into(&mut interface.outputs[out].name);
    }
    let mut inputs = vec![a, b];
    inputs.extend(carry_in);
    let pattern = match carry_in {
        Some(port) => {
            format!("carry of A + B + {}", interface.inputs[port].name)
        }
        None => "carry of A + B".to_owned(),
    };
    let width = interface.inputs[a].bits.len();
    let carry_in = carry_in.is_some();
    Ok(Spec {
        bench: Bench::new(inputs, vec![out], &pattern),
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, carry_in, 8)
        })),
        kinds: vec![Kind::new(&pattern, move |case| {
            (case[0] + case[1] + case[2]) >> width & 1
        })],
        by_digit: false,
    })
}

/// The cases of a comparison that reads its operands in two's complement.
pub(super) const SIGNED_GREATER_THAN: &str = "A > B, signed";

/// A greater-than comparator's every case: whether A is greater than B,
/// both unsigned or both in two's complement, whichever it gets more
/// right, unsigned for a tie, on its one output line. The two differ only
/// in each operand's top bit being inverted. It has no control line.
pub(super) fn greater_than(interface: &mut Interface) -> Result<Spec> {
    let (a, b, out) =
        comparison(interface, "greater-than comparator", "A > B")?;
    let width = interface.inputs[a].bits.len();
    Ok(Spec {
        bench: Bench::new(vec![a, b], vec![out], "A > B"),
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, false, 8)
        })),
        kinds: vec![
            Kind::new("A > B", |case| Value::from(case[0] > case[1]))
                .reading(Reading::GreaterThan { signed: false }),
            Kind::new(SIGNED_GREATER_THAN, move |case| {
                Value::from(
                    sign_extended(case[0], width)
                        > sign_extended(case[1], width),
                )
            })
            .reading(Reading::GreaterThan { signed: true }),
        ],
        by_digit: false,
    })
}

/// An equality comparator's every case: whether A equals B, on its one
/// output line. It has no control line.
pub(super) fn equality(interface: &mut Interface) -> Result<Spec> {
    let (a, b, out) = comparison(interface, "equality comparator", "A = B")?;
    let width = interface.inputs[a].bits.len();
    Ok(Spec {
        bench: Bench::new(vec![a, b], vec![out], "A = B"),
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, false, 8)
        })),
        kinds: vec![Kind::new("A = B", |case| Value::from(case[0] == case[1]))],
        by_digit: false,
    })
}

/// A comparison's operands and one output, which an unsigned output shows
/// as `result`, such as `A > B`, named so.
fn comparison(
    interface: &mut Interface,
    built: &str,
    result: &str,
) -> Result<(usize, usize, usize)> {
    let operands: Vec<usize> = (0..interface.inputs.len()).collect();
    let (a, b, out) = one_line(interface, &operands, built, result)?;
    if !interface.outputs[out].labelled {
        result.clone_into(&mut interface.outputs[out].name);
    }
    Ok((a, b, out))
}

/// `n`, of `width` bits, read in two's complement.
fn sign_extended(n: Value, width: usize) -> i128 {
    let unused = Value::BITS as usize - width;
    (n << unused).cast_signed() >> unused
}

/// The ports of a build of two operands, `operands` by their place in its
/// interface, of one width, and one output of one line, which shows `what`:
/// A, B, and that output. A build of another shape is broken as `built`.
fn one_line(
    interface: &Interface,
    operands: &[usize],
    built: &str,
    what: &str,
) -> Result<(usize, usize, usize)> {
    let &[a, b] = operands else {
        broken!(
            "A {built} takes two operands, but this has {}.",
            operands.len()
        );
    };
    let (a_bits, b_bits) = (
        interface.inputs[a].bits.len(),
        interface.inputs[b].bits.len(),
    );
    if a_bits != b_bits {
        broken!(
            "A has {a_bits} bits and B {b_bits}, but a {built}'s operands \
             have the same number."
        );
    }
    let &[out] = (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A {built} has one output, {what}, but this has {}.",
            interface.outputs.len()
        );
    };
    if interface.outputs[out].bits.len() != 1 {
        broken!(
            "A {built}'s output is one line, but this has {} bits.",
            interface.outputs[out].bits.len()
        );
    }
    Ok((a, b, out))
}

/// An adder's or subtractor's ports, by their place in its interface: its
/// carry in and carry out are a subtractor's borrow in and borrow out.
pub(super) struct AdderPorts {
    pub a: usize,
    pub b: usize,
    pub carry_in: Option<usize>,
    pub sum: usize,
    pub carry_out: Option<usize>,
}

/// An adder's or subtractor's ports, as `op` says. Its one control
/// line, if any, is its carry or borrow in and its one status line its
/// carry or borrow out, signed or not; a sign on either must say so. An
/// unsigned one is named for its role here.
pub(super) fn adder_ports(
    interface: &mut Interface,
    op: &Arithmetic,
) -> Result<AdderPorts> {
    let carry_in = adder_line(&mut interface.inputs, Role::Control, op, true)?;
    let carry_out =
        adder_line(&mut interface.outputs, Role::Status, op, false)?;
    let built = op.a;
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| Some(i) != carry_in)
        .collect();
    let &[a, b] = operands.as_slice() else {
        broken!(
            "{built} takes two operands, but this has {}.",
            operands.len()
        );
    };
    let buses: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| Some(i) != carry_out)
        .collect();
    let &[sum] = buses.as_slice() else {
        broken!(
            "{built} has one output bus and a {}, but this has {} output \
             buses.",
            op.line_out[0],
            buses.len()
        );
    };
    // The output may hold the carry or borrow as its top bit when there is
    // no line for it.
    let bits = |port: &Port| port.bits.len();
    let (a_bits, b_bits) =
        (bits(&interface.inputs[a]), bits(&interface.inputs[b]));
    // One bit is a slice of a wider build, which its carry in chains;
    // without it, its one output bit is A xor B, an XOR gate's. Its carry
    // out, as a wider build's, may be left out. One hex digit is four bits,
    // a whole adder without it.
    let digits = interface.inputs[a].signal_strength;
    if a_bits == 1 && !digits && carry_in.is_none() {
        let (line_in, line_out) = (op.line_in[0], op.line_out[0]);
        let lacks = if carry_out.is_none() {
            "without either it is an XOR gate".to_owned()
        } else {
            format!("this has no {line_in}")
        };
        broken!(
            "{built} of one bit has a {line_in}, and may have a {line_out}, \
             as a slice of a wider one does; {lacks}."
        );
    }
    let sum_bits = bits(&interface.outputs[sum]);
    let carry_bit = usize::from(carry_out.is_none());
    if a_bits != b_bits || !(a_bits..=a_bits + carry_bit).contains(&sum_bits) {
        broken!(
            "A has {a_bits} bits, B {b_bits}, and the output {sum_bits}, but \
             {} operands and output have the same number.",
            possessive(built).to_lowercase()
        );
    }
    Ok(AdderPorts {
        a,
        b,
        carry_in,
        sum,
        carry_out,
    })
}

/// The one line of `role` among `ports`, if there is one, `op`'s line in
/// when `into` holds, else its line out: unsigned, when it takes its role's
/// first name, or signed with one of its names. More than one, or a sign
/// naming something else, is broken.
fn adder_line(
    ports: &mut [Port],
    line_role: Role,
    op: &Arithmetic,
    into: bool,
) -> Result<Option<usize>> {
    let (names, what) = if into {
        (op.line_in, "control")
    } else {
        (op.line_out, "status")
    };
    let role = format!("{} {what} line is its {}", possessive(op.a), names[0]);
    let lines: Vec<usize> = (0..ports.len())
        .filter(|&i| ports[i].role == Some(line_role))
        .collect();
    let count: usize = lines.iter().map(|&i| ports[i].bits.len()).sum();
    if count > 1 {
        broken!("{role}, but this has {count} such lines.");
    }
    let Some(&line) = lines.first() else {
        return Ok(None);
    };
    let port = &mut ports[line];
    if !port.labelled {
        names[0].clone_into(&mut port.name);
    } else if !names.contains(&port.name.as_str()) {
        broken!("{role}, but this one is signed {}.", port.name);
    }
    Ok(Some(line))
}

/// `built`, such as `An adder`, as an owner: `An adder's`.
fn possessive(built: &str) -> String {
    format!("{built}'s")
}

/// The sums an adder too wide to try every combination of is checked on;
/// see `cases`.
pub(super) fn adder_cases(
    width: usize,
    carry_in: bool,
    piece: usize,
) -> (Vec<Case>, u64) {
    cases(width, carry_in, piece)
}

/// The cases an adder or subtractor too wide to try every combination of
/// is checked on, as A, B, and carry or borrow in, with how many pair the
/// test patterns. Each piece of `piece` bits, a byte or a hex digit, takes
/// every value of A and B, with a carry or borrow into it and without: the
/// bits below are A's all set or all clear, B's clear, so a carry in, or
/// B's lowest bit when there is no line for it, carries through them, or a
/// borrow borrows through them. Then every pairing of `patterns`, with
/// carry or borrow in each way.
fn cases(width: usize, carry_in: bool, piece: usize) -> (Vec<Case>, u64) {
    let mut cases = Distinct::default();
    for low in (0..width).step_by(piece) {
        let bits = (width - low).min(piece);
        // A's bits below all set, which B's clear bits carry the carry in
        // through, and all clear, which a subtract borrows through.
        for (carry, a_below) in [(false, 0), (true, mask(low)), (true, 0)] {
            let below = match (low, carry, carry_in) {
                (0, true, false) => continue,
                (0, _, _) if a_below != 0 => continue,
                (0, _, _) => (0, 0, carry),
                (_, false, _) => (mask(low), 0, false),
                (_, true, true) => (a_below, 0, true),
                (_, true, false) => (a_below, 1, false),
            };
            for a in 0..1 << bits {
                for b in 0..1 << bits {
                    cases.add([
                        below.0 | a << low,
                        below.1 | b << low,
                        Value::from(below.2),
                    ]);
                }
            }
        }
    }
    let bytes = cases.len();
    let values = patterns(width);
    for &a in &values {
        for &b in &values {
            cases.add([a, b, 0]);
            if carry_in {
                cases.add([a, b, 1]);
            }
        }
    }
    let paired = (cases.len() - bytes) as u64;
    (cases.cases, paired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recognize::interface;

    #[test]
    fn a_wide_adder_takes_every_case_of_each_byte_and_the_patterns() {
        let (cases, paired) = adder_cases(16, true, 8);
        // Each byte with and without a carry, and the high byte with a
        // borrow through the low, less three it repeats: A's low byte all
        // set or all clear, with or without carry in.
        assert_eq!(cases.len() as u64 - paired, 5 * (1 << 16) - 3);
        // A carry from carry in through the low byte into the high one.
        assert!(cases.contains(&[0x00FF, 0x0F00, 1]));
        // The longest carry: all ones plus one.
        assert!(cases.contains(&[0xFFFF, 0x0001, 0]));
        // Without a carry in line, B's lowest bit makes the carry.
        let (cases, _) = adder_cases(16, false, 8);
        assert!(cases.contains(&[0x00FF, 0x0101, 0]));
        assert!(cases.iter().all(|case| case[2] == 0));
    }

    /// A 64-bit hex adder takes each digit's every case and pairs as many
    /// patterns as a 32-bit one, sampled rather than every sum.
    #[test]
    fn a_64_bit_hex_adder_takes_as_many_cases_as_a_32_bit_one_and_some() {
        let (narrow, narrow_paired) = adder_cases(32, true, 4);
        let (wide, wide_paired) = adder_cases(64, true, 4);
        assert_eq!((narrow.len(), narrow_paired), (18_399, 12_532));
        assert_eq!((wide.len(), wide_paired), (24_631, 12_644));
        // A carry out of the top digit, and all ones plus one across it.
        assert!(wide.contains(&[0x8 << 60, 0x8 << 60, 1]));
        assert!(wide.contains(&[u64::MAX.into(), 1, 0]));
    }

    #[test]
    fn a_wide_adder_borrows_through_each_byte_as_a_subtractor() {
        // The longest borrow: nothing less borrow in.
        let (tried, _) = cases(16, true, 8);
        assert!(tried.contains(&[0x0000, 0x0000, 1]));
        // From borrow in through the low byte into the high one.
        assert!(tried.contains(&[0x0F00, 0x0000, 1]));
        // Without a borrow in line, B's lowest bit makes the borrow.
        let (tried, _) = cases(16, false, 8);
        assert!(tried.contains(&[0x0F00, 0x0001, 0]));
    }

    /// A port of `bits` bits named `name`, of `role`.
    fn port(name: &str, role: Role, labelled: bool, bits: usize) -> Port {
        Port {
            name: name.to_owned(),
            role: Some(role),
            labelled,
            bits: (0..bits)
                .map(|_| interface::Bit {
                    pos: [0, 0, 0],
                    entry: Some(crate::schem::Direction::South),
                    hard: false,
                    inverted: false,
                })
                .collect(),
            strength: 1,
            words: Vec::new(),
            signal_strength: false,
        }
    }

    fn adder(a: usize, b: usize, output: usize, carry_out: bool) -> Interface {
        let mut outputs = vec![port("output", Role::Output(0), false, output)];
        if carry_out {
            outputs.push(port("carry out", Role::Status, true, 1));
        }
        Interface {
            inputs: vec![
                port("A", Role::Operand(0), false, a),
                port("B", Role::Operand(1), false, b),
            ],
            outputs,
            binary_outputs: false,
        }
    }

    #[test]
    fn an_adder_whose_operands_and_output_differ_in_width_is_broken() {
        let broken = |mut interface: Interface| {
            adder_ports(&mut interface, &ADD)
                .err()
                .is_some_and(|err| err.downcast_ref::<Broken>().is_some())
        };
        assert!(!broken(adder(8, 8, 8, true)));
        assert!(broken(adder(8, 7, 8, true)));
        assert!(broken(adder(8, 8, 9, true)));
        // Without a carry out line, the output holds the carry on top.
        assert!(!broken(adder(8, 8, 9, false)));
    }

    #[test]
    fn an_adders_lone_lines_are_its_carries_signed_or_not() {
        let ports = |mut interface: Interface| {
            let ports = adder_ports(&mut interface, &ADD)?;
            let names = (
                ports.carry_in.map(|i| interface.inputs[i].name.clone()),
                ports.carry_out.map(|o| interface.outputs[o].name.clone()),
            );
            anyhow::Ok(names)
        };
        let carries = |carry_in: Port, carry_out: Port| {
            let mut interface = adder(8, 8, 8, false);
            interface.inputs.push(carry_in);
            interface.outputs.push(carry_out);
            interface
        };
        let unsigned = carries(
            port("unnamed control", Role::Control, false, 1),
            port("unnamed status", Role::Status, false, 1),
        );
        assert_eq!(
            ports(unsigned).unwrap(),
            (Some("carry in".to_owned()), Some("carry out".to_owned()))
        );
        let signed = carries(
            port("cin", Role::Control, true, 1),
            port("cout", Role::Status, true, 1),
        );
        assert_eq!(
            ports(signed).unwrap(),
            (Some("cin".to_owned()), Some("cout".to_owned()))
        );
        let subtract = carries(
            port("subtract", Role::Control, true, 1),
            port("cout", Role::Status, true, 1),
        );
        assert!(ports(subtract).is_err());
        // A subtractor's are its borrows, signed so or not.
        let mut borrows = carries(
            port("unnamed control", Role::Control, false, 1),
            port("bout", Role::Status, true, 1),
        );
        let found = adder_ports(&mut borrows, &ADD).unwrap();
        assert_eq!(
            (
                borrows.inputs[found.carry_in.unwrap()].name.as_str(),
                borrows.outputs[found.carry_out.unwrap()].name.as_str()
            ),
            ("carry in", "bout")
        );
        let two = carries(
            port("unnamed control", Role::Control, false, 2),
            port("cout", Role::Status, true, 1),
        );
        assert!(ports(two).is_err());
    }
}
