//! Probing for a barrel shifter: its one output line A shifted, an input
//! line as wide its A, and every other lever a line of the shift, coloured
//! and run through its test.

use std::collections::HashMap;

use super::{Fault, OneInputProbed, lengths, try_roles};
use crate::{
    recognize::{colour, inspect},
    schem::Pos,
    simulate::Test,
};

/// The barrel shifter the file in `data` is, from `report`'s levers and
/// outputs: its one output line, A shifted, an input line as wide its A,
/// and every other lever a line of the shift, its amount's signed with
/// what each is worth, as `1`, `2`, and `4`, coloured and run through its
/// test. Or why it isn't one.
pub fn shifter(
    data: &[u8],
    report: &inspect::Report,
) -> Result<OneInputProbed, Fault> {
    let [out] = report.outputs.as_slice() else {
        return Err(Fault::Unfit(format!(
            "Output: line lengths {}, need one line",
            lengths(&report.outputs)
        )));
    };
    let width = out.bits.len();
    let mut why = Fault::Unfit(format!("A: no line of {width} levers"));
    for a in report.inputs.iter().filter(|run| run.bits.len() == width) {
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(
            report
                .inputs
                .iter()
                .flat_map(|run| run.bits.clone())
                .map(|p| (p, colour::CONTROL)),
        );
        roles.extend(a.bits.iter().map(|&p| (p, "lime")));
        roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
        match try_roles(data, &roles, Test::BarrelShifter, false)? {
            Ok((file, run)) => {
                return Ok(OneInputProbed {
                    a: a.bits.clone(),
                    file,
                    run,
                });
            }
            Err(fault) => why = fault,
        }
    }
    Err(why)
}
