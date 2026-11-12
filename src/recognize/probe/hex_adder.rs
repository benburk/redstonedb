//! Probing for a hex adder: its signal-strength inputs, comparators with
//! air behind, as A and B, a lone lever as its carry in, its bus of outputs
//! as its sum, and a lone output beside it as its carry out, each by its
//! shape as [`repeater_outputs::interface`] reads it; given those roles and
//! run through the adder's test, which reads each digit as four bits.

use super::{Fault, Order, Role, Roles, try_roles};
use crate::{
    interface::{inspect, repeater_outputs},
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// A hex adder as probing found it: its digit inputs, each by the block
/// under its comparator, the file carrying its lines' roles, as its test
/// reads them, and that test's run.
pub struct HexAdderProbed {
    pub digits: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The hex adder the file in `data`, of `blocks`, is, run through `test`:
/// its lines as their shapes give them, see
/// [`repeater_outputs::interface`], given those roles and tested, its
/// digits A and B as `order` prefers first and the other way round after;
/// or why it isn't one.
pub fn hex_adder(
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    test: Test,
    order: Order,
) -> Result<HexAdderProbed, Fault> {
    if report.digits.is_empty() {
        return Err(Fault::Unfit(
            "Inputs: no comparator with air behind to take a hex digit"
                .to_owned(),
        ));
    }
    let lines = repeater_outputs::interface(blocks);
    let mut roles = Roles::default();
    let mut operands: Vec<Vec<Pos>> = Vec::new();
    for port in &lines.inputs {
        let bits = port.bits.iter().map(|bit| bit.pos);
        if port.name == "control line" {
            roles.mark(Role::Control, &bits.collect::<Vec<_>>());
        } else {
            operands.push(bits.collect());
        }
    }
    if operands.len() > 3 {
        return Err(Fault::Unfit(format!(
            "Inputs: {} buses, a hex adder takes A and B",
            lines.inputs.iter().filter(|p| p.bits.len() > 1).count()
        )));
    }
    if let [a, b, ..] = operands.as_mut_slice()
        && !order.keeps(a, b)
    {
        std::mem::swap(a, b);
    }
    // At most three output buses, the sum's lowest bits first.
    let mut places = 0..3;
    for port in &lines.outputs {
        let role = if port.name == "status line" {
            Role::Status
        } else {
            places.next().map(Role::Output).ok_or_else(|| {
                Fault::Unfit(format!(
                    "Outputs: {} lines, a hex adder gives a sum and a carry \
                     out",
                    lines.outputs.len()
                ))
            })?
        };
        roles.mark(role, port.bits.iter().map(|bit| &bit.pos));
    }
    let given = |operands: &[Vec<Pos>]| {
        let mut roles = roles.clone();
        for (n, bits) in (0..).zip(operands) {
            roles.mark(Role::Operand(n), bits);
        }
        roles
    };
    let tries = splits(operands);
    let mut nearest: Option<Fault> = None;
    let mut found = None;
    'tries: for mut operands in tries {
        // A and B either way round.
        for _ in 0..2.min(operands.len()) {
            match try_roles(data, &given(&operands), test, true)? {
                Ok(read) => {
                    found = Some(read);
                    break 'tries;
                }
                Err(fault) => {
                    nearest = Some(match nearest {
                        Some(near) => near.nearer(fault),
                        None => fault,
                    });
                }
            }
            if operands.len() >= 2 {
                operands.swap(0, 1);
            }
        }
    }
    let Some((file, run)) = found else {
        return Err(nearest.unwrap_or_else(|| {
            Fault::Unfit("Inputs: no digits to add".to_owned())
        }));
    };
    Ok(HexAdderProbed {
        digits: report
            .digits
            .iter()
            .flat_map(|run| run.bits.clone())
            .collect(),
        file,
        run,
    })
}

/// The ways to try `operands`, each a hex adder's operand runs: as found,
/// or, for one run of digits of even length, both operands laid along it,
/// one after the other or alternating, as two digit inputs one above the
/// other are.
fn splits(operands: Vec<Vec<Pos>>) -> Vec<Vec<Vec<Pos>>> {
    let [run] = operands.as_slice() else {
        return vec![operands];
    };
    if run.len() < 2 || run.len() % 2 == 1 {
        return vec![operands];
    }
    let (low, high) = run.split_at(run.len() / 2);
    let every = |first: usize| -> Vec<Pos> {
        run.iter().skip(first).step_by(2).copied().collect()
    };
    vec![vec![low.to_vec(), high.to_vec()], vec![every(0), every(1)]]
}
