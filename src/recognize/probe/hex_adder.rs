//! Probing for a hex adder: its signal-strength inputs, comparators with
//! air behind, as A and B, a lone lever as its carry in, its bus of outputs
//! as its sum, and a lone output beside it as its carry out, each by its
//! shape as [`repeater_outputs::interface`] reads it; given those roles and
//! run through the adder's test, which reads each digit as four bits.

use super::{Fault, Order, Role, Roles, try_roles};
use crate::{
    recognize::{inspect, repeater_outputs},
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
    let (file, run) = match try_roles(data, &given(&operands), test, true)? {
        Ok(found) => found,
        Err(fault) if operands.len() >= 2 => {
            operands.swap(0, 1);
            try_roles(data, &given(&operands), test, true)?
                .map_err(|other| fault.nearer(other))?
        }
        Err(fault) => return Err(fault),
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
