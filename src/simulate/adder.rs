//! Testing an adder, and a carry out, which gives only an adder's carry:
//! two operand buses and an optional carry in, into the sum and an optional
//! carry out above it, or the carry alone.

use anyhow::Result;

use super::{
    Broken,
    bench::{Bench, Case, Cases, Distinct, Hint, Kind, Spec, mask, patterns},
};
use crate::{
    interface::{Interface, Port},
    lint,
};

/// An adder's every sum: A + B, plus its carry in when it has one, into its
/// output and its carry out. When every sum is wrong, carry in flipped is
/// tried, since a lever saved the wrong way round does that. A hex adder,
/// slower to run, takes each digit's every case when there are too many to
/// try every one, and a binary one each byte's.
pub(super) fn adder(interface: &mut Interface) -> Result<Spec> {
    let ports = adder_ports(interface)?;
    let mut inputs = vec![ports.a, ports.b];
    inputs.extend(ports.carry_in);
    let mut outputs = vec![ports.sum];
    outputs.extend(ports.carry_out);
    let pattern = match ports.carry_in {
        Some(port) => format!("A + B + {}", interface.inputs[port].name),
        None => "A + B".to_owned(),
    };
    let bench = Bench::new(inputs, outputs, &pattern);
    let width = bench.widths(interface)[0];
    let carry_in = ports.carry_in.is_some();
    let hex = interface.inputs.iter().any(|port| port.signal_strength);
    let piece = if hex { 4 } else { 8 };
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, carry_in, piece)
        })),
        kinds: vec![Kind::new(&pattern, |case| case[0] + case[1] + case[2])],
        hint: if carry_in {
            Hint::Flipped(2)
        } else {
            Hint::None
        },
        by_digit: hex,
    })
}

/// A carry out's every case: the carry out of A + B, plus its carry in
/// when it has one, without the sum. Its lone control line and its one
/// output need no signs, as an adder's do.
pub(super) fn carry_out(interface: &mut Interface) -> Result<Spec> {
    let carry_in = adder_line(
        &mut interface.inputs,
        lint::CONTROL,
        &lint::CARRY_IN,
        "control line is its carry in",
    )?;
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| Some(i) != carry_in)
        .collect();
    let &[a, b] = operands.as_slice() else {
        broken!(
            "A carry out takes two operands, but this has {}.",
            operands.len()
        );
    };
    let (a_bits, b_bits) = (
        interface.inputs[a].bits.len(),
        interface.inputs[b].bits.len(),
    );
    if a_bits != b_bits {
        broken!(
            "A has {a_bits} bits and B {b_bits}, but a carry out's operands \
             have the same number."
        );
    }
    let &[out] = (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A carry out has one output, its carry, but this has {}.",
            interface.outputs.len()
        );
    };
    if interface.outputs[out].bits.len() != 1 {
        broken!(
            "A carry out's output is one line, but this has {} bits.",
            interface.outputs[out].bits.len()
        );
    }
    if !interface.outputs[out].labelled {
        lint::CARRY_OUT[0].clone_into(&mut interface.outputs[out].name);
    }
    let mut inputs = vec![a, b];
    inputs.extend(carry_in);
    let pattern = match carry_in {
        Some(port) => {
            format!("carry of A + B + {}", interface.inputs[port].name)
        }
        None => "carry of A + B".to_owned(),
    };
    let width = a_bits;
    let carry_in = carry_in.is_some();
    Ok(Spec {
        bench: Bench::new(inputs, vec![out], &pattern),
        cases: Cases::Combinations(Box::new(move || {
            adder_cases(width, carry_in, 8)
        })),
        kinds: vec![Kind::new(&pattern, move |case| {
            (case[0] + case[1] + case[2]) >> width & 1
        })],
        hint: Hint::None,
        by_digit: false,
    })
}

/// The adder's ports, by their place in its interface.
pub(super) struct AdderPorts {
    pub a: usize,
    pub b: usize,
    pub carry_in: Option<usize>,
    pub sum: usize,
    pub carry_out: Option<usize>,
}

/// The adder's ports. Its one magenta control line, if any, is its carry
/// in and its one pink status line its carry out, signed or not; a sign on
/// either must say so. An unsigned one is named for its role here.
pub(super) fn adder_ports(interface: &mut Interface) -> Result<AdderPorts> {
    let carry_in = adder_line(
        &mut interface.inputs,
        lint::CONTROL,
        &lint::CARRY_IN,
        "control line is its carry in",
    )?;
    let carry_out = adder_line(
        &mut interface.outputs,
        lint::STATUS,
        &lint::CARRY_OUT,
        "status line is its carry out",
    )?;
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| Some(i) != carry_in)
        .collect();
    let &[a, b] = operands.as_slice() else {
        broken!(
            "An adder takes two operands, but this has {}.",
            operands.len()
        );
    };
    let buses: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| Some(i) != carry_out)
        .collect();
    let &[sum] = buses.as_slice() else {
        broken!(
            "An adder has one output bus and a carry out, but this has {} \
             output buses.",
            buses.len()
        );
    };
    // The output may hold the carry as its top bit when there is no carry
    // out line.
    let bits = |port: &Port| port.bits.len();
    let (a_bits, b_bits) =
        (bits(&interface.inputs[a]), bits(&interface.inputs[b]));
    let sum_bits = bits(&interface.outputs[sum]);
    let carry_bit = usize::from(carry_out.is_none());
    if a_bits != b_bits || !(a_bits..=a_bits + carry_bit).contains(&sum_bits) {
        broken!(
            "A has {a_bits} bits, B {b_bits}, and the output {sum_bits}, but \
             an adder's operands and output have the same number."
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

/// The one line of `colour` among `ports`, if there is one: unsigned, when
/// it takes its role's first name, or signed with one of `names`. More
/// than one, or a sign naming something else, is broken; `role` says what
/// the line is, as in `control line is its carry in`.
fn adder_line(
    ports: &mut [Port],
    colour: &str,
    names: &[&str],
    role: &str,
) -> Result<Option<usize>> {
    let lines: Vec<usize> = (0..ports.len())
        .filter(|&i| ports[i].block.starts_with(colour))
        .collect();
    let count: usize = lines.iter().map(|&i| ports[i].bits.len()).sum();
    if count > 1 {
        broken!("An adder's {role}, but this has {count} such lines.");
    }
    let Some(&line) = lines.first() else {
        return Ok(None);
    };
    let port = &mut ports[line];
    if !port.labelled {
        names[0].clone_into(&mut port.name);
    } else if !names.contains(&port.name.as_str()) {
        broken!("An adder's {role}, but this one is signed {}.", port.name);
    }
    Ok(Some(line))
}

/// The sums an adder too wide to try every combination of is checked on,
/// as A, B, and carry in, with how many pair the test patterns. Each piece
/// of `piece` bits, a byte or a hex digit, takes every value of A and B,
/// with a carry into it and without: the bits below are all set in A, and
/// carry in, or B's lowest bit when there is no carry in line, carries
/// through them. Then every pairing of `patterns`, with carry in each way.
pub(super) fn adder_cases(
    width: usize,
    carry_in: bool,
    piece: usize,
) -> (Vec<Case>, u64) {
    let mut cases = Distinct::default();
    for low in (0..width).step_by(piece) {
        let bits = (width - low).min(piece);
        for carry in [false, true] {
            let below = match (low, carry, carry_in) {
                (0, true, false) => continue,
                (0, _, _) => (0, 0, carry),
                (_, false, _) => (mask(low), 0, false),
                (_, true, true) => (mask(low), 0, true),
                (_, true, false) => (mask(low), 1, false),
            };
            for a in 0..1 << bits {
                for b in 0..1 << bits {
                    cases.add([
                        below.0 | a << low,
                        below.1 | b << low,
                        u64::from(below.2),
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
    use crate::interface;

    #[test]
    fn a_wide_adder_takes_every_case_of_each_byte_and_the_patterns() {
        let (cases, paired) = adder_cases(16, true, 8);
        // Each byte with and without a carry, less two the high byte
        // repeats: A's low byte all set, with or without carry in.
        assert_eq!(cases.len() as u64 - paired, 4 * (1 << 16) - 2);
        // A carry from carry in through the low byte into the high one.
        assert!(cases.contains(&[0x00FF, 0x0F00, 1]));
        // The longest carry: all ones plus one.
        assert!(cases.contains(&[0xFFFF, 0x0001, 0]));
        // Without a carry in line, B's lowest bit makes the carry.
        let (cases, _) = adder_cases(16, false, 8);
        assert!(cases.contains(&[0x00FF, 0x0101, 0]));
        assert!(cases.iter().all(|case| case[2] == 0));
    }

    /// A port of `bits` bits named `name`, of `block`.
    fn port(name: &str, block: &str, labelled: bool, bits: usize) -> Port {
        Port {
            name: name.to_owned(),
            block: block.to_owned(),
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
        let mut outputs =
            vec![port("output", "yellow concrete", false, output)];
        if carry_out {
            outputs.push(port("carry out", "pink concrete", true, 1));
        }
        Interface {
            inputs: vec![
                port("A", "lime concrete", false, a),
                port("B", "light blue concrete", false, b),
            ],
            outputs,
        }
    }

    #[test]
    fn an_adder_whose_operands_and_output_differ_in_width_is_broken() {
        let broken = |mut interface: Interface| {
            adder_ports(&mut interface)
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
            let ports = adder_ports(&mut interface)?;
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
            port("unnamed control", "magenta concrete", false, 1),
            port("unnamed status", "pink concrete", false, 1),
        );
        assert_eq!(
            ports(unsigned).unwrap(),
            (Some("carry in".to_owned()), Some("carry out".to_owned()))
        );
        let signed = carries(
            port("cin", "magenta concrete", true, 1),
            port("cout", "pink concrete", true, 1),
        );
        assert_eq!(
            ports(signed).unwrap(),
            (Some("cin".to_owned()), Some("cout".to_owned()))
        );
        let subtract = carries(
            port("subtract", "magenta concrete", true, 1),
            port("cout", "pink concrete", true, 1),
        );
        assert!(ports(subtract).is_err());
        let two = carries(
            port("unnamed control", "magenta concrete", false, 2),
            port("cout", "pink concrete", true, 1),
        );
        assert!(ports(two).is_err());
    }
}
