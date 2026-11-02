//! Testing an incrementer, a decrementer, or an inc/dec: A, and a control
//! line that adds or subtracts it, or picks between adding and
//! subtracting 1.

use anyhow::Result;

use super::{
    Broken,
    bench::{Bench, Case, Cases, Distinct, Kind, Spec, mask, patterns},
};
use crate::recognize::{colour, interface::Interface};

/// Every value of A with its control line each way, checked against what
/// each kind of the category named `category` gives: an incrementer's A
/// plus 1 or plus its control line, a decrementer's A minus either, and an
/// inc/dec's plus 1 one way and minus 1 the other. With no category, every
/// kind is tried, as for a guess. The kind it gets most right names its
/// cases.
pub(super) fn spec(
    interface: &Interface,
    category: Option<&str>,
) -> Result<Spec> {
    let bench = incrementer_bench(interface)?;
    let line = bench
        .inputs
        .get(1)
        .map(|&i| interface.inputs[i].name.as_str());
    let kinds: Vec<Kind> = changes(line)
        .into_iter()
        .filter(|change| category.is_none_or(|c| change.category == c))
        .map(|change| {
            Kind::new(&change.name, move |case| {
                (change.apply)(case[0], case[1])
            })
        })
        .collect();
    if kinds.is_empty() {
        broken!(
            "An inc/dec has a control line to choose between adding and \
             subtracting 1, but this has none."
        );
    }
    let widths = bench.widths(interface);
    Ok(Spec {
        bench,
        cases: Cases::Combinations(Box::new(move || {
            incrementer_cases(&widths)
        })),
        kinds,
        by_digit: false,
    })
}

/// One kind of incrementer: its cases' name, such as `A + increment`, the
/// category it is, and what it gives for A and its control line.
pub(super) struct Change {
    pub name: String,
    pub category: &'static str,
    pub apply: fn(u64, u64) -> u64,
}

/// The kinds of incrementer with control line `line`, if any.
pub(super) fn changes(line: Option<&str>) -> Vec<Change> {
    let change = |name: String, category, apply| Change {
        name,
        category,
        apply,
    };
    match line {
        None => vec![
            change("A + 1".to_owned(), "incrementer", |a, _| a.wrapping_add(1)),
            change("A − 1".to_owned(), "decrementer", |a, _| {
                a.wrapping_sub(1)
            }),
        ],
        Some(line) => vec![
            change(format!("A + {line}"), "incrementer", u64::wrapping_add),
            change(format!("A − {line}"), "decrementer", u64::wrapping_sub),
            change(
                format!("A + 1, or − 1 with {line} on"),
                "inc_dec",
                |a, c| {
                    if c == 1 {
                        a.wrapping_sub(1)
                    } else {
                        a.wrapping_add(1)
                    }
                },
            ),
            change(
                format!("A − 1, or + 1 with {line} on"),
                "inc_dec",
                |a, c| {
                    if c == 1 {
                        a.wrapping_add(1)
                    } else {
                        a.wrapping_sub(1)
                    }
                },
            ),
        ],
    }
}

/// What an incrementer's test drives and reads: A and its control line, if
/// any; its output buses, then any status line, such as a carry out.
fn incrementer_bench(interface: &Interface) -> Result<Bench> {
    let control =
        |i: &usize| interface.inputs[*i].block.starts_with(colour::CONTROL);
    let (lines, operands): (Vec<usize>, Vec<usize>) =
        (0..interface.inputs.len()).partition(control);
    let &[a] = operands.as_slice() else {
        broken!(
            "An incrementer takes one operand, but this has {}.",
            operands.len()
        );
    };
    if lines.len() > 1 {
        broken!(
            "Too many control lines: {} (an incrementer takes one, such \
             as increment).",
            lines.len()
        );
    }
    let status =
        |i: &usize| interface.outputs[*i].block.starts_with(colour::STATUS);
    let (mut status_lines, mut outputs): (Vec<usize>, Vec<usize>) =
        (0..interface.outputs.len()).partition(status);
    outputs.append(&mut status_lines);
    let bits = interface.inputs[a].bits.len();
    let result: usize = outputs
        .iter()
        .map(|&i| interface.outputs[i].bits.len())
        .sum();
    if !(bits..=bits + 1).contains(&result) {
        broken!(
            "A has {bits} bits, so the result has {bits}, or one more for a \
             carry, but the outputs have {result}."
        );
    }
    let mut inputs = vec![a];
    inputs.extend(lines);
    Ok(Bench::new(inputs, outputs, "A"))
}

/// The values an incrementer too wide to try every one of is checked on:
/// every value of each byte of A, the bits below it all clear and all set,
/// so a carry or borrow runs into it, and the test patterns; each with its
/// control line, if any, each way.
fn incrementer_cases(widths: &[usize]) -> (Vec<Case>, u64) {
    let lines: &[u64] = if widths.len() > 1 { &[0, 1] } else { &[0] };
    let mut cases = Distinct::default();
    for low in (0..widths[0]).step_by(8) {
        let bits = (widths[0] - low).min(8);
        for below in [0, mask(low)] {
            for a in 0..1u64 << bits {
                for &line in lines {
                    cases.add([below | a << low, line, 0]);
                }
            }
        }
    }
    let before = cases.len();
    for a in patterns(widths[0]) {
        for &line in lines {
            cases.add([a, line, 0]);
        }
    }
    let paired = (cases.len() - before) as u64;
    (cases.cases, paired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incrementer_names_what_it_does_by_its_control_line() {
        let kinds: Vec<(String, &str)> = changes(Some("increment"))
            .into_iter()
            .map(|change| (change.name, change.category))
            .collect();
        assert_eq!(kinds[0], ("A + increment".to_owned(), "incrementer"));
        assert_eq!(kinds[1].1, "decrementer");
        // Plus 1 with the line off, minus 1 with it on.
        let inc_dec = &changes(Some("decrement"))[2];
        assert_eq!(((inc_dec.apply)(5, 0), (inc_dec.apply)(5, 1)), (6, 4));
    }
}
