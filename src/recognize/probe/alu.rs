//! Probing for an ALU: its widest output line its output and each lone one
//! a status line, two input lines as wide its operands, either way round,
//! and every other lever a control line, each way coloured and run through
//! the ALU test.

use std::collections::HashMap;

use super::{lengths, paint, recognize::doesnt_hold};
use crate::{
    recognize::inspect::{self, Run},
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// An ALU as probing found it: its operands A and B, its control lines,
/// its output and status lines, the file with them coloured as the ALU
/// test reads them, and that test's run.
pub struct AluProbed {
    pub a: Vec<Pos>,
    pub b: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub output: Vec<Pos>,
    pub status: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The ALU the file in `data` is, from `report`'s levers and outputs: its
/// widest output line its output and each lone output a status line, named
/// by its sign; two input lines as wide its operands, A first either way
/// round; and every other lever a control line, each coloured and run
/// through the ALU test, quick cases first. Or why none holds.
pub fn alu(data: &[u8], report: &inspect::Report) -> Result<AluProbed, String> {
    let Some(out) = report.outputs.iter().max_by_key(|run| run.bits.len())
    else {
        return Err("Outputs: none".to_owned());
    };
    let width = out.bits.len();
    let status: Vec<Pos> = report
        .outputs
        .iter()
        .filter(|run| run.bits.len() == 1 && run.bits != out.bits)
        .flat_map(|run| run.bits.clone())
        .collect();
    if report
        .outputs
        .iter()
        .any(|run| run.bits != out.bits && run.bits.len() > 1)
    {
        return Err(format!(
            "Outputs: line lengths {}, need one bus and lone status lines",
            lengths(&report.outputs)
        ));
    }
    let operands: Vec<&Run> = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() == width)
        .collect();
    let [first, second] = operands.as_slice() else {
        return Err(format!(
            "Operands: {} lines of {width} levers, need A and B",
            operands.len()
        ));
    };
    let controls: Vec<Pos> = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() != width)
        .flat_map(|run| run.bits.clone())
        .collect();
    let mut why = String::new();
    for (a, b) in [(first, second), (second, first)] {
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(a.bits.iter().map(|&p| (p, "lime")));
        roles.extend(b.bits.iter().map(|&p| (p, "light_blue")));
        roles.extend(controls.iter().map(|&p| (p, "magenta")));
        roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
        roles.extend(status.iter().map(|&p| (p, "pink")));
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match Test::Alu.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => {
                why = doesnt_hold(&run, &file);
                continue;
            }
            Err(err) => {
                why = format!("Test couldn't run: {err}");
                continue;
            }
        }
        match Test::Alu.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(AluProbed {
                    a: a.bits.clone(),
                    b: b.bits.clone(),
                    controls,
                    output: out.bits.clone(),
                    status,
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

#[cfg(test)]
mod tests {
    use crate::recognize::probe::{recognize, tests::uncoloured};

    #[test]
    fn an_alu_is_found_from_its_levers_and_control_lines() {
        let data = uncoloured("alu-8-and-xor");
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("an ALU")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }
}
