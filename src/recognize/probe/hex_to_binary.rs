//! Probing for a hex-to-binary converter: its signal-strength input, a
//! comparator with air behind a digit, as A, and its outputs as the bits
//! of A, each line by its shape as [`repeater_outputs::interface`] reads
//! it, a lone one beside a bus taken as the bus's top bit or its bottom,
//! whichever holds; given those roles and run through its test.

use super::{Fault, Role, Roles, try_roles};
use crate::interface::problem::Rule;
use crate::{
    category::Test,
    interface::{inspect, repeater_outputs},
    schem::{Blocks, Pos},
    simulate,
};

/// A hex-to-binary converter as probing found it: its digit inputs, each
/// by the block under its comparator, the file carrying its lines' roles,
/// as its test reads them, and that test's run.
pub struct HexToBinaryProbed {
    pub digits: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The hex-to-binary converter the file in `data`, of `blocks`, is, run
/// through `test`: its digits A and its output lines A's bits, the lowest
/// first, as their shapes give them, see [`repeater_outputs::interface`],
/// then the other way round; or why it isn't one.
pub fn hex_to_binary(
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    test: Test,
) -> Result<HexToBinaryProbed, Fault> {
    if report.digits.is_empty() {
        return Err(Fault::Breaks(
            Rule::HexLines,
            "Inputs: no comparator with air behind to take a hex digit"
                .to_owned(),
        ));
    }
    let lines = repeater_outputs::interface(blocks);
    let [a] = lines.inputs.as_slice() else {
        return Err(Fault::Unfit(format!(
            "Inputs: {} lines, a hex-to-binary converter takes its digits",
            lines.inputs.len()
        )));
    };
    if lines.outputs.is_empty() {
        return Err(Fault::Unfit("Outputs: none".to_owned()));
    }
    let digits: Vec<Pos> = a.bits.iter().map(|bit| bit.pos).collect();
    let given = |outputs: &mut dyn Iterator<Item = &[Pos]>| {
        let mut roles = Roles::default();
        roles.mark(Role::Operand(0), &digits);
        roles.outputs(outputs);
        roles
    };
    let outputs: Vec<Vec<Pos>> = lines
        .outputs
        .iter()
        .map(|port| port.bits.iter().map(|bit| bit.pos).collect())
        .collect();
    let forward = given(&mut outputs.iter().map(Vec::as_slice));
    let (file, run) = match try_roles(data, &forward, test, true)? {
        Ok(found) => found,
        Err(fault) if outputs.len() >= 2 => {
            let backward = given(&mut outputs.iter().rev().map(Vec::as_slice));
            try_roles(data, &backward, test, true)?
                .map_err(|other| fault.nearer(other))?
        }
        Err(fault) => return Err(fault),
    };
    Ok(HexToBinaryProbed { digits, file, run })
}
