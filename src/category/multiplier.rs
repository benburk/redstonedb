//! Testing a multiplier: A times B, plus C for a fused multiply-add,
//! stateless, or sequential with one control line, its calculate, pressed
//! once its operands are set.

use crate::simulate::{Rule, untested};
use anyhow::Result;

use crate::{
    category::test::BySpec,
    interface::{self, Interface},
    simulate::{
        bench::{Bench, Case, Kind, Spec},
        broken,
    },
};

/// A multiplier's every product, A × B, plus C when there is one, checked
/// on as many low bits as its outputs hold together, the first holding the
/// lowest; status lines are left out. A sequential one has one control
/// line, its calculate, pressed once the operands are set, whatever its
/// sign says; one with more, such as a reset, isn't tested.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let control = |i: &usize| interface.inputs[*i].control();
    let lines: Vec<usize> =
        (0..interface.inputs.len()).filter(control).collect();
    let calculate = match lines.as_slice() {
        [] => None,
        [line] if interface.inputs[*line].bits.len() == 1 => Some(*line),
        _ => untested!(
            Rule::TooManyControlLines;
            "Expected at most 1 control line, found {}",
            lines
                .iter()
                .map(|&i| interface.inputs[i].name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let inputs: Vec<usize> = (0..interface.inputs.len())
        .filter(|i| !control(i))
        .collect();
    let outputs: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| !interface.outputs[i].status())
        .collect();
    let widths: Vec<usize> = inputs
        .iter()
        .map(|&i| interface.inputs[i].bits.len())
        .collect();
    let pattern = match widths.len() {
        2 => "A × B",
        3 => "A × B + C",
        n => broken!("Expected 2 or 3 operands, found {n}"),
    };
    let result: usize = outputs
        .iter()
        .map(|&i| interface.outputs[i].bits.len())
        .sum();
    let (a, b) = (widths[0], widths[1]);
    if result == 0 || result < a.max(b) || result > a + b {
        broken!(
            "Expected output width {} to {}, found {result}",
            a.max(b),
            a + b
        );
    }
    // Exact for operands of up to 64 bits each, and the low bits are all
    // that is compared.
    let product =
        |case: &Case| case[0].wrapping_mul(case[1]).wrapping_add(case[2]);
    let mut bench = Bench::new(inputs, outputs, pattern);
    bench.press = calculate;
    Ok(Spec {
        bench,
        kinds: vec![Kind::new(pattern, product)],
    })
}

/// A multiplier's test: its lone control line is named for its role,
/// calculate.
pub(super) const MULTIPLIER: BySpec = BySpec::spec(|_, interface| {
    interface::name_lines(super::Kind::Multiplier.lines(), interface);
    spec(interface)
});
