//! Probing for a multiplier: its operands, A and B, and a fused
//! multiply-add's addend, C, its calculate, if any, and its product's
//! lines, tried in every order, each way coloured and run through the
//! multiplier test, the closest that ran kept when none holds.

use std::collections::HashMap;

use super::{lengths, paint, recognize::doesnt_hold, split};
use crate::{
    recognize::{
        colour,
        inspect::{self, Run},
    },
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// A multiplier as probing found it: its operands A and B, its control
/// lines, its product's lines, lowest first, the file with them coloured
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
/// whichever is which, as A × B is B × A, and a third line, if any, a fused
/// multiply-add's addend, C, each line tried as it; a lone lever or button,
/// if any, a sequential one's calculate; and its output lines, up to three,
/// its product, tried in each order, lowest first, each coloured and run
/// through the multiplier test, quick cases first. Or why none holds.
pub fn multiplier(
    data: &[u8],
    report: &inspect::Report,
) -> Result<MultiplierProbed, String> {
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
        .collect();
    if operands.is_empty() {
        return Err(format!(
            "Operands: line lengths {}, need A and B as wide, and perhaps C",
            lengths(&report.inputs)
        ));
    }
    let lines: Vec<&Run> = report.outputs.iter().collect();
    if lines.is_empty() || lines.len() > colour::OUTPUT_COLOURS.len() {
        return Err(format!(
            "Product: line lengths {}, need one to three lines",
            lengths(&report.outputs)
        ));
    }
    let mut why = String::new();
    // Of the orders that ran, the share of cases the closest got wrong,
    // which `why` tells, rather than one that couldn't run.
    let mut closest = f64::INFINITY;
    let mut wrong = |run: &simulate::Run, file: &[u8], why: &mut String| {
        #[allow(clippy::cast_precision_loss)]
        let share = run.wrong as f64 / run.cases.max(1) as f64;
        if share < closest {
            closest = share;
            *why = doesnt_hold(run, file);
        }
    };
    let tries = operands.iter().flat_map(|&(a, b, c)| {
        orders(lines.len())
            .into_iter()
            .map(move |order| (a, b, c, order))
    });
    for (a, b, c, order) in tries {
        let product: Vec<Vec<Pos>> =
            order.iter().map(|&i| lines[i].bits.clone()).collect();
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(a.bits.iter().map(|&p| (p, "lime")));
        roles.extend(b.bits.iter().map(|&p| (p, "light_blue")));
        roles.extend(c.iter().flat_map(|c| &c.bits).map(|&p| (p, "cyan")));
        roles.extend(controls.iter().map(|&p| (p, colour::CONTROL)));
        for (bits, colour) in product.iter().zip(colour::OUTPUT_COLOURS) {
            roles.extend(bits.iter().map(|&p| (p, colour)));
        }
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match Test::Multiplier.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => {
                wrong(&run, &file, &mut why);
                continue;
            }
            Err(err) => {
                if !why.starts_with("Doesn't hold") {
                    why = format!("Test couldn't run: {err}");
                }
                continue;
            }
        }
        match Test::Multiplier.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(MultiplierProbed {
                    a: a.bits.clone(),
                    b: b.bits.clone(),
                    c: c.map(|c| c.bits.clone()).unwrap_or_default(),
                    controls,
                    product,
                    file,
                    run: *run,
                });
            }
            simulate::Report::Ran(run) => wrong(&run, &file, &mut why),
            simulate::Report::Broken { reason }
            | simulate::Report::Skipped { reason } => {
                if !why.starts_with("Doesn't hold") {
                    why = format!("Test couldn't run: {reason}");
                }
            }
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
        let standard = crate::schem::standardize::standardize(&raw).unwrap();
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
