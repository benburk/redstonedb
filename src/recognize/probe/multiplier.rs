//! Probing for a multiplier: its operands, A and B, and a fused
//! multiply-add's addend, C, its calculate, if any, and its product's
//! lines, tried in every order, each way given those roles and run through
//! the multiplier test, the closest that ran kept when none holds.

use super::{Fault, Order, Role, Roles, lengths, split, try_roles};
use crate::{
    interface::inspect::{self, Run},
    schem::Pos,
    simulate::{self, Test},
};

/// A multiplier as probing found it: its operands A and B, its control
/// lines, its product's lines, lowest first, the file carrying their roles,
/// as the multiplier test reads them, and that test's run.
pub struct MultiplierProbed {
    pub a: Vec<Pos>,
    pub b: Vec<Pos>,
    /// A fused multiply-add's addend, C; empty for any other.
    pub c: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub product: Vec<Vec<Pos>>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The multiplier the file in `data` is, from `report`'s levers and
/// outputs: its two input lines of one width its operands, A and B,
/// whichever is which, as A × B is B × A, so A is the one `order` prefers,
/// and a third line, if any, a fused multiply-add's addend, C, each line
/// tried as it; a lone lever or button, if any, a sequential one's
/// calculate; and its output lines, up to three, its product, tried in
/// each order, lowest first, each given those roles and run through the
/// multiplier test, quick cases first. Or why none holds.
pub fn multiplier(
    data: &[u8],
    report: &inspect::Report,
    order: Order,
) -> Result<MultiplierProbed, Fault> {
    let (buses, controls) = split(&report.inputs);
    // A and B, and for a fused multiply-add its addend, C, each line tried
    // as C; A and B are as wide.
    let operands: Vec<(&Run, &Run, Option<&Run>)> = match buses.as_slice() {
        [a, b] => vec![(*a, *b, None)],
        [x, y, z] => [(x, y, z), (x, z, y), (y, z, x)]
            .into_iter()
            .map(|(a, b, c)| (*a, *b, Some(*c)))
            .collect(),
        _ => Vec::new(),
    };
    let operands: Vec<_> = operands
        .into_iter()
        .filter(|(a, b, _)| a.bits.len() == b.bits.len())
        .map(|(a, b, c)| {
            if order.keeps(&a.bits, &b.bits) {
                (a, b, c)
            } else {
                (b, a, c)
            }
        })
        .collect();
    if operands.is_empty() {
        return Err(Fault::Unfit(format!(
            "Operands: line lengths {}, need A and B as wide, and perhaps C",
            lengths(&report.inputs)
        )));
    }
    let lines: Vec<&Run> = report.outputs.iter().collect();
    if lines.is_empty() || lines.len() > 3 {
        return Err(Fault::Unfit(format!(
            "Product: line lengths {}, need one to three lines",
            lengths(&report.outputs)
        )));
    }
    // Of the orders that ran, the closest, rather than one that couldn't
    // run.
    let mut why = Fault::Unfit(String::new());
    let tries = operands.iter().flat_map(|&(a, b, c)| {
        orders(lines.len())
            .into_iter()
            .map(move |order| (a, b, c, order))
    });
    for (a, b, c, order) in tries {
        let product: Vec<Vec<Pos>> =
            order.iter().map(|&i| lines[i].bits.clone()).collect();
        let mut roles = Roles::default();
        roles.mark(Role::Operand(0), &a.bits);
        roles.mark(Role::Operand(1), &b.bits);
        roles.mark(Role::Operand(2), c.iter().flat_map(|c| &c.bits));
        roles.mark(Role::Control, &controls);
        roles.outputs(product.iter().map(Vec::as_slice));
        match try_roles(data, &roles, Test::Multiplier, true)? {
            Ok((file, run)) => {
                return Ok(MultiplierProbed {
                    a: a.bits.clone(),
                    b: b.bits.clone(),
                    c: c.map(|c| c.bits.clone()).unwrap_or_default(),
                    controls,
                    product,
                    file,
                    run,
                });
            }
            Err(fault) => why = why.nearer(fault),
        }
    }
    Err(why)
}

/// Every order of `n` things, by their places, as `[1, 0]` for two the
/// other way round.
fn orders(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for rest in orders(n - 1) {
        for at in 0..=rest.len() {
            let mut order = rest.clone();
            order.insert(at, n - 1);
            all.push(order);
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use crate::recognize::probe::{recognize_as, tests::uncoloured};

    /// A sequential multiplier, its one control line its calculate.
    #[test]
    fn a_multiplier_is_found_from_its_operands_and_product() {
        let data = uncoloured("mult-8-calculate");
        let found = recognize_as(&data, Some("multiplier")).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("a multiplier")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A multiplier has one control line, its calculate, so one with a
    /// reset button too isn't one.
    #[test]
    fn a_multiplier_with_a_reset_is_not_one() {
        // Raw, with blocks the simulator doesn't model until they are
        // standardized.
        let raw = std::fs::read("tests/fixtures/mult-8-buttons.schem").unwrap();
        let standard = crate::ingest::standardize::standardize(&raw).unwrap();
        let found = recognize_as(&standard, Some("multiplier")).unwrap();
        assert!(found.found.is_none(), "{:?}", found.found);
        assert!(
            found
                .problems
                .iter()
                .any(|p| p.contains("Too many control lines")),
            "{:?}",
            found.problems
        );
    }
}
