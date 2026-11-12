//! Probing for a build of two operands, a gate or a comparison: its input
//! lines split into A and B however they run, and its output line as wide
//! as each, or one bit, each way given those roles and run through its
//! test.

use super::{
    Fault, Order, Role, Roles, lengths, ordered, split, stray_operands,
    try_roles,
};
use crate::{
    interface::inspect::{self, Run},
    schem::Pos,
    simulate::{self, Gate, Test},
};

/// A build of two operands as probing found it, a gate or a comparison: its
/// inputs A and B, the file carrying its lines' roles, as its test reads
/// them, and that test's run.
pub struct OperandsProbed {
    pub inputs: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The build of two operands, run through `test`, the file in `data` is,
/// from `report`'s levers and outputs: its input lines, half its levers
/// each, A and B, found however they run, and its one output line, as wide
/// as each, a gate's, or one bit, a comparison's or a carry out's; each way
/// given those roles and run through the test. A carry out's lone lever
/// left over, if any, is its carry in. A greater-than comparator's operands
/// are tried both ways round, as `order` prefers first; any other's give
/// the same either way, so A is the one `order` prefers. With
/// `loose`, for a build kept as built, A and B may each take one of the
/// bits left over beside two runs as wide as each other, as
/// [`super::adder_from`] reads them loosely, where no other split fits. Or why
/// it isn't one.
pub fn operands_probe(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
    loose: bool,
    order: Order,
) -> Result<OperandsProbed, Fault> {
    // A NAND+XNOR gate has two outputs, its NAND and its XNOR, either way
    // round; any other one.
    let outs: Vec<&Run> = report.outputs.iter().collect();
    let wanted = if test == Test::NandXnor { 2 } else { 1 };
    if outs.len() != wanted {
        return Err(Fault::Unfit(format!(
            "Output: line lengths {}, need {wanted} line{}",
            lengths(&report.outputs),
            if wanted == 1 { "" } else { "s" }
        )));
    }
    let out = outs[0];
    if outs.iter().any(|run| run.bits.len() != out.bits.len()) {
        return Err(Fault::Unfit(format!(
            "Outputs: line lengths {}, need them as wide",
            lengths(&report.outputs)
        )));
    }
    let gate = Gate::of(test).is_some() || test == Test::NandXnor;
    // A carry out's carry in, a lone lever, is set aside from A and B.
    let (_, lones) = split(&report.inputs);
    let carries: Vec<Option<Pos>> = if test == Test::CarryOut {
        std::iter::once(None)
            .chain(lones.into_iter().map(Some))
            .collect()
    } else {
        vec![None]
    };
    let levers: usize = report.inputs.iter().map(|run| run.bits.len()).sum();
    let mut splits: Vec<(Vec<Pos>, Vec<Pos>, Option<Pos>)> = Vec::new();
    for carry in carries {
        let runs: Vec<Run> = report
            .inputs
            .iter()
            .filter(|run| carry.is_none_or(|c| run.bits != [c]))
            .cloned()
            .collect();
        let width = (levers - usize::from(carry.is_some())) / 2;
        if out.bits.len() == if gate { width } else { 1 } {
            splits.extend(
                operand_splits(&runs, width)
                    .into_iter()
                    .map(|(a, b)| (a, b, carry)),
            );
        }
    }
    if loose && splits.is_empty() {
        let width = levers / 2;
        if out.bits.len() == if gate { width } else { 1 } {
            splits.extend(
                stray_splits(&report.inputs, levers)
                    .into_iter()
                    .map(|(a, b)| (a, b, None)),
            );
        }
    }
    if splits.is_empty() {
        return Err(Fault::Unfit(format!(
            "Lines: inputs {}, output {}, need A and B of one width and \
             an output of {}",
            lengths(&report.inputs),
            out.bits.len(),
            if gate { "that width" } else { "one bit" }
        )));
    }
    if test == Test::GreaterThan {
        let swapped: Vec<_> = splits
            .iter()
            .map(|(a, b, carry)| (b.clone(), a.clone(), *carry))
            .collect();
        splits.extend(swapped);
        splits.sort_by_key(|(a, b, _)| !order.keeps(a, b));
    } else {
        for (a, b, _) in &mut splits {
            if !order.keeps(a, b) {
                std::mem::swap(a, b);
            }
        }
    }
    let mut why = Fault::Unfit(String::new());
    for (a, b, carry) in splits {
        let mut roles = Roles::default();
        roles.mark(Role::Operand(0), &a);
        roles.mark(Role::Operand(1), &b);
        roles.mark(Role::Control, &carry);
        roles.outputs(outs.iter().map(|run| run.bits.as_slice()));
        match try_roles(data, &roles, test, false)? {
            Ok((file, run)) => {
                return Ok(OperandsProbed {
                    inputs: [a, b].concat(),
                    file,
                    run,
                });
            }
            Err(fault) => why = why.nearer(fault),
        }
    }
    Err(why)
}

/// Each way the input `runs` could be two operands of `levers` bits in all,
/// each a run with one of the bits left over, as [`stray_operands`] finds
/// them, once whichever is which.
fn stray_splits(runs: &[Run], levers: usize) -> Vec<(Vec<Pos>, Vec<Pos>)> {
    let mut splits: Vec<(Vec<Pos>, Vec<Pos>)> = stray_operands(runs)
        .into_iter()
        .filter(|(a, _, carry)| carry.is_none() && a.len() * 2 == levers)
        .map(|(mut a, mut b, _)| {
            a.sort_unstable();
            b.sort_unstable();
            if b < a {
                std::mem::swap(&mut a, &mut b);
            }
            (a, b)
        })
        .collect();
    splits.sort();
    splits.dedup();
    splits
}

/// Each way the input `runs` could be two operands, A and B, `width` bits each,
/// once whichever is which: two runs of that width; one, and the rest of
/// the bits; or one run of both, as its halves or its bits alternating, as
/// a 1-bit gate's two levers, side by side, read as one run.
fn operand_splits(runs: &[Run], width: usize) -> Vec<(Vec<Pos>, Vec<Pos>)> {
    let every: Vec<Pos> =
        runs.iter().flat_map(|run| run.bits.clone()).collect();
    if every.len() != 2 * width {
        return Vec::new();
    }
    let mut splits: Vec<(Vec<Pos>, Vec<Pos>)> = Vec::new();
    for run in runs.iter().filter(|run| run.bits.len() == width) {
        let rest = every
            .iter()
            .filter(|bit| !run.bits.contains(bit))
            .copied()
            .collect();
        splits.push((run.bits.clone(), rest));
    }
    if let [whole] = runs {
        let bits = ordered(whole);
        splits.push((bits[..width].to_vec(), bits[width..].to_vec()));
        splits.push((
            bits.iter().step_by(2).copied().collect(),
            bits.iter().skip(1).step_by(2).copied().collect(),
        ));
    }
    // A and B the other way round make the same gate.
    for (a, b) in &mut splits {
        if b < a {
            std::mem::swap(a, b);
        }
    }
    splits.sort();
    splits.dedup();
    splits
}

#[cfg(test)]
mod tests {
    use crate::recognize::probe::recognize;

    /// A 1-bit XNOR of wool, one input carried up through dust on its block,
    /// and a lone block holding up a repeater that powers a block: the dust
    /// is wiring, as on any input, and the block under the repeater no
    /// output, so its one output is read and it is found an XNOR.
    #[test]
    fn dust_on_an_input_and_a_repeater_powering_a_block_are_wiring() {
        let data = std::fs::read("tests/fixtures/xnor-1-wool.schem").unwrap();
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.contains("A xnor B")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A 1-bit XOR gate, its two levers side by side read as one run, is
    /// found as a gate, not as a 1-bit adder without a carry.
    #[test]
    fn an_xor_gate_is_found_as_a_gate() {
        // Raw, with blocks the simulator doesn't model until they are
        // standardized.
        let raw = std::fs::read("tests/fixtures/xor-1.schem").unwrap();
        let standard = crate::ingest::standardize::standardize(&raw).unwrap();
        let found = recognize(&standard).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("an XOR gate")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }
}
