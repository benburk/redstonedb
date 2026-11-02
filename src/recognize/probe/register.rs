//! Probing for a register or a buffer: its one output line the data bus
//! out, an input line as wide the data bus in, and every other switch a
//! control, each way of choosing the bus in coloured and run through the
//! register test.

use std::collections::HashMap;

use super::{lengths, paint, recognize::doesnt_hold};
use crate::{
    recognize::inspect::{self, Run},
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// A register as probing found it: its data bus in, its controls, writes
/// on buttons and reads on levers, its data bus out, the file with them
/// coloured as the register test reads them, and that test's run.
pub struct RegisterProbed {
    pub data_in: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub data_out: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The register the file in `data` is, from `report`'s levers and outputs:
/// its one output line the data bus out, an input line as wide the data
/// bus in, and every other switch a control; each way of choosing the bus
/// in coloured and run through the register test, quick cases first. Or
/// why none holds.
pub fn register(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
) -> Result<RegisterProbed, String> {
    let [out] = report.outputs.as_slice() else {
        return Err(format!(
            "Output bus: line lengths {}",
            lengths(&report.outputs)
        ));
    };
    let width = out.bits.len();
    let blocks = Blocks::read(data).map_err(|err| err.to_string())?;
    let buttons: Vec<Pos> = blocks
        .switches()
        .into_iter()
        .filter(|switch| switch.button)
        .map(|switch| switch.block)
        .collect();
    let every: Vec<Pos> = report
        .inputs
        .iter()
        .flat_map(|run| run.bits.clone())
        .collect();
    // A data line is levers; a lone button is a write.
    let buses: Vec<&Run> = report
        .inputs
        .iter()
        .filter(|run| {
            run.bits.len() == width
                && run.bits.iter().all(|bit| !buttons.contains(bit))
        })
        .collect();
    if buses.is_empty() {
        return Err(format!("Data bus: no line of {width} levers"));
    }
    let mut why = String::new();
    for bus in buses {
        let controls: Vec<Pos> = every
            .iter()
            .filter(|bit| !bus.bits.contains(bit))
            .copied()
            .collect();
        // A buffer's writes may all be levers, as it has no reads.
        if test == Test::Register
            && !controls.iter().any(|bit| buttons.contains(bit))
        {
            "Controls: no write button".clone_into(&mut why);
            continue;
        }
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(bus.bits.iter().map(|&p| (p, "lime")));
        roles.extend(controls.iter().map(|&p| (p, "magenta")));
        roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match test.quick(&coloured) {
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
        match test.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(RegisterProbed {
                    data_in: bus.bits.clone(),
                    controls,
                    data_out: out.bits.clone(),
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
    use crate::recognize::probe::{recognize, recognize_as, tests::uncoloured};

    #[test]
    fn a_register_is_found_from_its_levers_and_buttons() {
        let data = uncoloured("register-8-2-cells");
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("a register")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
        let as_adder = recognize_as(&data, Some("adder")).unwrap();
        assert!(as_adder.found.is_none());
    }
}
