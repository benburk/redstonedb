//! Probing for a barrel shifter: its one output line A shifted, an input
//! line as wide its A, and every other lever a line of the shift, coloured
//! and run through its test.

use std::collections::HashMap;

use super::{OneInputProbed, lengths, paint, recognize::doesnt_hold};
use crate::{
    recognize::{colour, inspect},
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// The barrel shifter the file in `data` is, from `report`'s levers and
/// outputs: its one output line, A shifted, an input line as wide its A,
/// and every other lever a line of the shift, its amount's signed with
/// what each is worth, as `1`, `2`, and `4`, coloured and run through its
/// test. Or why it isn't one.
pub fn shifter(
    data: &[u8],
    report: &inspect::Report,
) -> Result<OneInputProbed, String> {
    let [out] = report.outputs.as_slice() else {
        return Err(format!(
            "Output: line lengths {}, need one line",
            lengths(&report.outputs)
        ));
    };
    let width = out.bits.len();
    let mut why = format!("A: no line of {width} levers");
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
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match Test::BarrelShifter.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(OneInputProbed {
                    a: a.bits.clone(),
                    file,
                    run: *run,
                });
            }
            simulate::Report::Ran(run) => {
                why = doesnt_hold(&run, &file);
            }
            simulate::Report::Broken { reason }
            | simulate::Report::Skipped { reason } => {
                why = format!("Test couldn't run: {reason}");
            }
        }
    }
    Err(why)
}
