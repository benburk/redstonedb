//! Probing for a counter, a shift register, or a T flip-flop: its widest
//! output the value it holds, an input line as wide its A, and every other
//! lever or button a control line of its own, coloured and run through
//! its test, which finds what each line does.

use std::collections::HashMap;

use super::{lengths, paint, recognize::doesnt_hold};
use crate::{
    recognize::{colour, inspect},
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// A counter or shift register as probing found it: its input A, if any,
/// the file with its lines coloured as its test reads them, and that
/// test's run.
pub struct SequentialProbed {
    pub data: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The counter or shift register the file in `data` is, as `test` checks
/// it, from `report`'s levers and outputs: its widest output the value it
/// holds and each lone output a status line, such as a shift out; an input
/// line as wide as that value its input A, to load; and every other lever
/// or button a control line of its own, each coloured and run through the
/// test, which finds what each control line does. Or why it isn't one.
pub fn sequential(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
) -> Result<SequentialProbed, String> {
    let Some(held) = report.outputs.iter().max_by_key(|run| run.bits.len())
    else {
        return Err("Outputs: none".to_owned());
    };
    let width = held.bits.len();
    if report
        .outputs
        .iter()
        .any(|run| run.bits != held.bits && run.bits.len() > 1)
    {
        return Err(format!(
            "Outputs: line lengths {}, need one line and lone status lines",
            lengths(&report.outputs)
        ));
    }
    let status: Vec<Pos> = report
        .outputs
        .iter()
        .filter(|run| run.bits != held.bits)
        .flat_map(|run| run.bits.clone())
        .collect();
    let a: Vec<Pos> = report
        .inputs
        .iter()
        .find(|run| run.bits.len() == width && width > 1)
        .map(|run| run.bits.clone())
        .unwrap_or_default();
    let controls: Vec<Pos> = report
        .inputs
        .iter()
        .filter(|run| run.bits != a)
        .flat_map(|run| run.bits.clone())
        .collect();
    let mut roles: HashMap<Pos, &str> = HashMap::new();
    roles.extend(a.iter().map(|&p| (p, "lime")));
    roles.extend(controls.iter().map(|&p| (p, colour::CONTROL)));
    roles.extend(held.bits.iter().map(|&p| (p, "yellow")));
    roles.extend(status.iter().map(|&p| (p, colour::STATUS)));
    let file = paint(data, &roles).map_err(|err| err.to_string())?;
    let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
    match test.run(&coloured) {
        simulate::Report::Ran(run) if run.wrong == 0 => Ok(SequentialProbed {
            data: a,
            file,
            run: *run,
        }),
        simulate::Report::Ran(run) => Err(doesnt_hold(&run, &file)),
        simulate::Report::Broken { reason }
        | simulate::Report::Skipped { reason } => {
            Err(format!("Test couldn't run: {reason}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::recognize::probe::{recognize, recognize_as, tests::uncoloured};

    /// A counter whose clock button counts and loads A with its write enable
    /// lever on, and a shift register that loads A and shifts either way,
    /// found by what its lines do.
    #[test]
    fn a_counter_and_a_shift_register_are_found_from_their_lines() {
        for (name, kind) in [
            ("counter-8", "a counter"),
            ("shift-register-8", "a shift register"),
        ] {
            let found = recognize(&uncoloured(name)).unwrap();
            assert!(
                found.found.as_deref().is_some_and(|f| f.starts_with(kind)),
                "{name}: {:?} {:?}",
                found.found,
                found.problems
            );
        }
    }

    /// A T flip-flop, one input and one output, is found by each pulse of
    /// its input toggling its output, whatever else it might pass for.
    #[test]
    fn a_t_flip_flop_is_found_from_its_toggle() {
        let data = std::fs::read("tests/fixtures/t-flip-flop.schem").unwrap();
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("a T flip-flop")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A counter has one clock button and one write enable lever, so one
    /// loading A on a button of its own isn't one.
    #[test]
    fn a_counter_loading_on_a_second_button_is_not_one() {
        let found =
            recognize_as(&uncoloured("counter-8-two-buttons"), Some("counter"))
                .unwrap();
        assert!(found.found.is_none(), "{:?}", found.found);
        assert!(
            found
                .problems
                .iter()
                .any(|p| p.contains("write enable lever")),
            "{:?}",
            found.problems
        );
    }
}
