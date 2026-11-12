//! How weakly a stateless build's data inputs may be driven, and how weak
//! its data outputs then get. Every data input is driven at one strength,
//! as a comparator pointing into its block would drive it, its control
//! lines left at a lever's 15: a strength from 14 down is tried on the
//! telling cases until one changes what comes out, the weakest before it
//! confirmed on every case. Each data output bit's weakest strength while
//! on is read then, the inputs at their weakest, the worst it gets: what
//! arrives at the input of the repeater or comparator carrying it out, as
//! that gives 15 or its own strength whatever arrives, or dust's power.

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
    /// Each data output, by name, and its bits' weakest strength while on,
    /// least significant first, the data inputs at `input`: arriving at
    /// each one's repeater or comparator, see
    /// [`Circuit::arriving_strengths`]; 0 for a bit never on.
    pub outputs: Vec<(String, Vec<u8>)>,
}

/// What `bench` makes of `cases`, read together as its outputs are, and
/// each data output bit's weakest strength while on, with its data inputs
/// driven at `power`.
struct Look {
    got: Vec<Value>,
    weakest: Vec<Vec<u8>>,
}

/// How weakly `blocks`' data inputs may be driven, `bench` having got
/// every one of `cases` right, as `got`, with them at 15. `None` for a
/// bench with no data input, presses, or a signal-strength input, whose
/// strength is its value.
pub(super) fn measure(
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
    let outputs: Vec<usize> = (0..bench.outputs.len())
        .filter(|&o| !interface.outputs[bench.outputs[o]].status())
        .collect();
    let mut circuit = Circuit::new(blocks, interface)?;
    let mut look = |power: u8, cases: &[Case]| -> Result<Look> {
        for &port in &data {
            circuit.drive_at(port, power)?;
        }
        observe(&mut circuit, interface, bench, &outputs, cases)
    };
    let (telling, _) = paired(&bench.widths(interface));
    let full = look(15, &telling)?.got;
    let same = |looked: &Look, want: &[Value]| {
        looked
            .got
            .iter()
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
    loop {
        let looked = look(weakest, cases)?;
        if same(&looked, got) || weakest == 15 {
            return Ok(Some(Strength {
                input: weakest,
                outputs: outputs
                    .iter()
                    .zip(looked.weakest)
                    .map(|(&o, bits)| {
                        (interface.outputs[bench.outputs[o]].name.clone(), bits)
                    })
                    .collect(),
            }));
        }
        weakest += 1;
    }
}

/// Runs `cases` through `circuit` as `bench` drives them, reading what
/// comes out and the weakest strength each bit of `outputs`, by their
/// place in the bench, carries while on.
fn observe(
    circuit: &mut Circuit,
    interface: &Interface,
    bench: &Bench,
    outputs: &[usize],
    cases: &[Case],
) -> Result<Look> {
    for &(port, value) in &bench.held {
        circuit.set(port, value);
    }
    let widths: Vec<usize> = bench
        .outputs
        .iter()
        .map(|&port| output_bits(interface, &interface.outputs[port]))
        .collect();
    let mut weakest: Vec<Vec<u8>> =
        outputs.iter().map(|&o| vec![0; widths[o]]).collect();
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
        for (bits, &o) in weakest.iter_mut().zip(outputs) {
            for (bit, strength) in circuit
                .arriving_strengths(bench.outputs[o])
                .into_iter()
                .enumerate()
            {
                if strength > 0 && (bits[bit] == 0 || strength < bits[bit]) {
                    bits[bit] = strength;
                }
            }
        }
    }
    Ok(Look { got, weakest })
}
