//! How weakly a stateless build's data inputs may be driven. Every data
//! input is driven at one strength, as a comparator pointing into its
//! block would drive it, its control lines left at a lever's 15: a
//! strength from 14 down is tried on the telling cases until one changes
//! what comes out, the weakest before it confirmed on every case. What
//! arrives at its output repeaters is measured with the cases, at 15; see
//! [`OutputStrength`](super::OutputStrength).

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{
    Value,
    bench::{Bench, Case, paired},
    circuit::{Circuit, output_bits},
};
use crate::{interface::Interface, schem::Blocks};

/// What weak inputs do to a build.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Strength {
    /// The weakest strength every data input may be driven at together,
    /// with every case still right.
    pub input: u8,
}

/// How weakly `blocks`' data inputs may be driven, `bench` having got
/// every one of `cases` right, as `got`, with them at 15. `None` for a
/// bench with no data input, presses, or a signal-strength input, whose
/// strength is its value.
pub(crate) fn measure(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    got: &[Value],
) -> Result<Option<Strength>> {
    let data: Vec<usize> = bench
        .inputs
        .iter()
        .copied()
        .filter(|&port| !interface.inputs[port].control())
        .collect();
    let digits = interface.inputs.iter().any(|port| port.signal_strength);
    if data.is_empty() || digits || bench.press.is_some() {
        return Ok(None);
    }
    let mut circuit = Circuit::new(blocks, interface)?;
    let mut look = |power: u8, cases: &[Case]| -> Result<Vec<Value>> {
        for &port in &data {
            circuit.drive_at(port, power)?;
        }
        observe(&mut circuit, interface, bench, cases)
    };
    let (telling, _) = paired(&bench.widths(interface));
    let full = look(15, &telling)?;
    let same = |got: &[Value], want: &[Value]| {
        got.iter()
            .zip(want)
            .all(|(got, want)| got & bench.care == want & bench.care)
    };
    let mut weakest = 15;
    for power in (1..15).rev() {
        if !same(&look(power, &telling)?, &full) {
            break;
        }
        weakest = power;
    }
    // Every case, the weakest made stronger until each is right.
    while weakest < 15 && !same(&look(weakest, cases)?, got) {
        weakest += 1;
    }
    Ok(Some(Strength { input: weakest }))
}

/// Runs `cases` through `circuit` as `bench` drives them, reading what
/// comes out.
fn observe(
    circuit: &mut Circuit,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
) -> Result<Vec<Value>> {
    for &(port, value) in &bench.held {
        circuit.set(port, value);
    }
    let widths: Vec<usize> = bench
        .outputs
        .iter()
        .map(|&port| output_bits(interface, &interface.outputs[port]))
        .collect();
    let mut got = Vec::with_capacity(cases.len());
    for case in cases {
        for (i, &port) in bench.inputs.iter().enumerate() {
            circuit.set(port, case[i]);
        }
        circuit.settle()?;
        let mut value = 0;
        let mut shift = 0;
        for (&port, &width) in bench.outputs.iter().zip(&widths) {
            value |= circuit.get(port) << shift;
            shift += width;
        }
        got.push(value);
    }
    Ok(got)
}

/// What arrives at an output's repeaters through a test's cases, its data
/// inputs at 15: the strength from the dust, block, or comparator behind
/// each, see [`Circuit::arriving_strengths`], not the 15 it gives out.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OutputStrength {
    /// The output, by name.
    pub name: String,
    /// Each bit's weakest and strongest strength arriving while on, least
    /// significant first; `(0, 0)` for a bit never on, or carried out by a
    /// comparator rather than a repeater.
    pub bits: Vec<(u8, u8)>,
}

impl OutputStrength {
    /// The weakest and strongest strength arriving at any of its bits.
    pub fn range(&self) -> Option<(u8, u8)> {
        let on = self.bits.iter().filter(|&&(weakest, _)| weakest > 0);
        let weakest = on.clone().map(|&(weakest, _)| weakest).min()?;
        let strongest = on.map(|&(_, strongest)| strongest).max()?;
        Some((weakest, strongest))
    }
}

/// The weakest and strongest strength arriving at each of a bench's
/// output repeaters, read case by case.
pub(crate) struct Arriving {
    /// Each output port read and what arrived at its bits so far.
    outputs: Vec<(usize, Vec<(u8, u8)>)>,
}

impl Arriving {
    pub(crate) fn new(bench: &Bench) -> Self {
        Arriving {
            outputs: bench
                .outputs
                .iter()
                .map(|&port| (port, Vec::new()))
                .collect(),
        }
    }

    /// Reads what arrives at each bit as `circuit` stands.
    pub(crate) fn read(&mut self, circuit: &Circuit) {
        for (port, bits) in &mut self.outputs {
            let arriving = circuit.arriving_strengths(*port);
            bits.resize(arriving.len(), (0, 0));
            for (bit, strength) in bits.iter_mut().zip(arriving) {
                if strength > 0 {
                    *bit = match *bit {
                        (0, _) => (strength, strength),
                        (weakest, strongest) => {
                            (weakest.min(strength), strongest.max(strength))
                        }
                    };
                }
            }
        }
    }

    /// Each output with a repeater that was on, by name.
    pub(crate) fn measured(self, interface: &Interface) -> Vec<OutputStrength> {
        self.outputs
            .into_iter()
            .filter(|(_, bits)| bits.iter().any(|&(weakest, _)| weakest > 0))
            .map(|(port, bits)| OutputStrength {
                name: interface.outputs[port].name.clone(),
                bits,
            })
            .collect()
    }
}
