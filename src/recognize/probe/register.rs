//! Probing for a register or a buffer: its one output line the data bus
//! out, an input line as wide the data bus in, and every other switch a
//! control, each way of choosing the bus in given those roles and run
//! through the register test.

use super::{Fault, Role, Roles, lengths, try_roles};
use crate::interface::problem::Rule;
use crate::{
    category::Test,
    interface::inspect::{self, Run},
    schem::{Blocks, Pos},
    simulate,
};

/// A register as probing found it: its data bus in, its controls, writes on
/// buttons and reads on levers, its data bus out, the file carrying their
/// roles, as the register test reads them, and that test's run.
pub struct RegisterProbed {
    pub data_in: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub data_out: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The register the file in `data` is, from `report`'s levers and outputs:
/// its one output line the data bus out, an input line as wide the data bus
/// in, and every other switch a control; each way of choosing the bus in
/// given those roles and run through the register test, quick cases first.
/// Or why none holds.
pub fn register(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
) -> Result<RegisterProbed, Fault> {
    let [out] = report.outputs.as_slice() else {
        return Err(Fault::Unfit(format!(
            "Output bus: line lengths {}",
            lengths(&report.outputs)
        )));
    };
    let width = out.bits.len();
    let blocks =
        Blocks::read(data).map_err(|err| Fault::Unfit(err.to_string()))?;
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
        return Err(Fault::Breaks(
            Rule::Buses,
            format!("Data bus: no line of {width} levers"),
        ));
    }
    let mut why = Fault::Unfit(String::new());
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
            why = Fault::Unfit("Controls: no write button".to_owned());
            continue;
        }
        let mut roles = Roles::default();
        roles.mark(Role::Operand(0), &bus.bits);
        roles.mark(Role::Control, &controls);
        roles.mark(Role::Output(0), &out.bits);
        match try_roles(data, &roles, test, true)? {
            Ok((file, run)) => {
                return Ok(RegisterProbed {
                    data_in: bus.bits.clone(),
                    controls,
                    data_out: out.bits.clone(),
                    file,
                    run,
                });
            }
            Err(fault) => why = why.nearer(fault),
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
