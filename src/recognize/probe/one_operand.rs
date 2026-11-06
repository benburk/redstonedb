//! Probing for a build of one operand, A: an inverter, an absolute value,
//! a negator, an incrementer, a monostable, a converter, and the like, its
//! control line, if it has one, tried among its levers and the rest A,
//! each way given those roles and run through its test.

use super::{Fault, Role, Roles, lengths, split, try_roles};
use crate::{
    recognize::inspect,
    schem::Pos,
    simulate::{self, Test},
};

/// Whether a build of one operand, A, has a control line: an inverter's,
/// its invert, always; an incrementer's, its increment, perhaps; an
/// absolute value's or a monostable's, never.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line {
    Always,
    Maybe,
    Never,
}

/// A build of one operand, A, as probing found it: A, the file carrying its
/// lines' roles, as its test reads them, and that test's run.
pub struct OneInputProbed {
    pub a: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The build of one operand, run through `test`, the file in `data` is,
/// from `report`'s levers and outputs: its widest output its result and
/// each lone one a status line, such as a carry out; of its levers, its
/// control line, if `line` gives it one, each tried, lone ones first, and
/// the rest A; each way given those roles and run through the test. Or why
/// it isn't one.
pub fn one_operand(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
    line: Line,
) -> Result<OneInputProbed, Fault> {
    let Some(out) = report.outputs.iter().max_by_key(|run| run.bits.len())
    else {
        return Err(Fault::Unfit("Outputs: none".to_owned()));
    };
    let status: Vec<Pos> = report
        .outputs
        .iter()
        .filter(|run| run.bits != out.bits)
        .flat_map(|run| run.bits.clone())
        .collect();
    if report
        .outputs
        .iter()
        .any(|run| run.bits != out.bits && run.bits.len() > 1)
    {
        return Err(Fault::Unfit(format!(
            "Outputs: line lengths {}, need one line and lone status lines",
            lengths(&report.outputs)
        )));
    }
    let mut levers: Vec<Pos> = report
        .inputs
        .iter()
        .flat_map(|run| run.bits.clone())
        .collect();
    let (_, lones) = split(&report.inputs);
    levers.sort_by_key(|lever| !lones.contains(lever));
    let tried: Vec<Option<Pos>> = match line {
        Line::Always => levers.iter().copied().map(Some).collect(),
        Line::Maybe => std::iter::once(None)
            .chain(levers.iter().copied().map(Some))
            .collect(),
        Line::Never => vec![None],
    };
    let mut why = Fault::Unfit("Inputs: none".to_owned());
    for line in tried {
        let a: Vec<Pos> = levers
            .iter()
            .copied()
            .filter(|&p| Some(p) != line)
            .collect();
        if a.is_empty() {
            continue;
        }
        let mut roles = Roles::default();
        roles.mark(Role::Operand(0), &a);
        roles.mark(Role::Control, &line);
        roles.mark(Role::Output(0), &out.bits);
        roles.mark(Role::Status, &status);
        match try_roles(data, &roles, test, false)? {
            Ok((file, run)) => return Ok(OneInputProbed { a, file, run }),
            Err(fault) => why = why.nearer(fault),
        }
    }
    Err(why)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        recognize::{colour, probe::recognize_as},
        schem::{self, Blocks, Pos},
    };

    /// Each build of one or two operands is found as what it is from its
    /// levers and outputs alone: an equality comparator, a carry out, an
    /// inverter, an absolute value, a negator, an incrementer, a barrel
    /// shifter, a buffer, a monostable, a threshold, a popcount, its count
    /// one output's strength, a parity circuit, one vertical on levers on
    /// lamps too, and a binary-to-hex converter.
    #[test]
    fn comparisons_and_one_operand_builds_are_found() {
        for (name, kind) in [
            ("eq-8", "equality"),
            ("cout-8", "carry_out"),
            ("inverter-1", "inverter"),
            ("abs-8", "absolute_value"),
            ("negator-8", "negator"),
            ("inc-8", "incrementer"),
            ("barrel-shifter-8", "barrel_shifter"),
            ("buffer-2-button", "buffer"),
            ("monostable-3-inverted", "monostable"),
            ("threshold-3-raw", "threshold"),
            ("popcount-8-strength", "popcount"),
            ("parity-8", "parity"),
            ("parity-8-vertical", "parity"),
            ("bin-hex-4", "binary_to_hex"),
        ] {
            let raw =
                std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
            let standard =
                crate::schem::standardize::standardize(&raw).unwrap();
            let changes: BTreeMap<Pos, String> = Blocks::read(&standard)
                .unwrap()
                .iter()
                .filter(|&(_, state)| {
                    [
                        colour::INPUT_COLOURS[0],
                        colour::INPUT_COLOURS[1],
                        colour::CONTROL,
                    ]
                    .iter()
                    .any(|c| schem::short(state) == format!("{c}_concrete"))
                })
                .map(|(pos, _)| (pos, "minecraft:white_concrete".to_owned()))
                .collect();
            let data =
                crate::schem::standardize::edit(&standard, &changes).unwrap();
            let found = recognize_as(&data, Some(kind)).unwrap();
            assert!(found.found.is_some(), "{name}: {:?}", found.problems);
        }
    }
}
