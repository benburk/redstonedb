//! Probing for a counter, a shift register, or a T flip-flop: its widest
//! output the value it holds, an input line as wide its A, and every other
//! lever or button a control line of its own, given those roles and run
//! through its test, which finds what each line does.

use super::{Fault, Role, Roles, lengths, try_roles};
use crate::{
    recognize::inspect,
    schem::Pos,
    simulate::{self, Test},
};

/// A counter or shift register as probing found it: its input A, if any,
/// the file carrying its lines' roles, as its test reads them, and that
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
/// or button a control line of its own, each given those roles and run
/// through the test, which finds what each control line does. Or why it
/// isn't one.
pub fn sequential(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
) -> Result<SequentialProbed, Fault> {
    let Some(held) = report.outputs.iter().max_by_key(|run| run.bits.len())
    else {
        return Err(Fault::Unfit("Outputs: none".to_owned()));
    };
    let width = held.bits.len();
    if report
        .outputs
        .iter()
        .any(|run| run.bits != held.bits && run.bits.len() > 1)
    {
        return Err(Fault::Unfit(format!(
            "Outputs: line lengths {}, need one line and lone status lines",
            lengths(&report.outputs)
        )));
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
    let mut roles = Roles::default();
    roles.mark(Role::Operand(0), &a);
    roles.mark(Role::Control, &controls);
    roles.mark(Role::Output(0), &held.bits);
    roles.mark(Role::Status, &status);
    let (file, run) = try_roles(data, &roles, test, false)??;
    Ok(SequentialProbed { data: a, file, run })
}

#[cfg(test)]
mod tests {
    use crate::recognize::probe::{recognize, recognize_as, tests::uncoloured};

    /// A shift register that loads A and shifts either way, found by what
    /// its lines do.
    #[test]
    fn a_shift_register_is_found_from_its_lines() {
        let found = recognize(&uncoloured("shift-register-8")).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("a shift register")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
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

    /// A counter's clock is a lever, pulsed for 2 ticks, so one clocked by
    /// a button, as counters once were, isn't one, and is told why.
    #[test]
    fn a_counter_clocked_by_a_button_is_not_one() {
        let found =
            recognize_as(&uncoloured("counter-8"), Some("counter")).unwrap();
        assert!(found.found.is_none(), "{:?}", found.found);
        assert!(
            found.problems.iter().any(|p| p.contains("are levers")),
            "{:?}",
            found.problems
        );
    }
}
