//! Testing an incrementer, a decrementer, or an inc/dec: A, and a control
//! line that adds or subtracts it, or picks between adding and
//! subtracting 1.

use crate::simulate::Rule;
use anyhow::Result;

use crate::{
    category::{self, Test, test::BySpec},
    interface::{self, Interface},
    simulate::{
        Counts, Reading, Value,
        bench::{Bench, Kind, Spec},
        broken,
    },
};

/// Every value of A with its control line each way, checked against what
/// each kind of incrementer that `counts` gives: an incrementer's A plus 1
/// or plus its control line, a decrementer's A minus either, and an
/// inc/dec's plus 1 with its line on and minus 1 with it off, a line that
/// means the other the test reads inverted. With no way of counting, every
/// kind is tried, as for a guess. The kind it gets most right names its
/// cases.
pub(super) fn spec(
    interface: &Interface,
    counts: Option<Counts>,
) -> Result<Spec> {
    let bench = incrementer_bench(interface)?;
    let line = bench
        .inputs
        .get(1)
        .map(|&i| interface.inputs[i].name.as_str());
    let kinds: Vec<Kind> = changes(line)
        .into_iter()
        .filter(|change| counts.is_none_or(|c| change.counts == c))
        .map(|change| {
            Kind::new(&change.name, move |case| {
                (change.apply)(case[0], case[1])
            })
            .reading(Reading::Incrementer {
                counts: change.counts,
            })
        })
        .collect();
    if kinds.is_empty() {
        broken!(
            "An inc/dec has a control line to choose between adding and \
             subtracting 1, but this has none."
        );
    }
    Ok(Spec { bench, kinds })
}

impl Counts {
    /// The category of a build that counts so.
    pub fn kind(self) -> category::Kind {
        match self {
            Counts::Up => category::Kind::Incrementer,
            Counts::Down => category::Kind::Decrementer,
            Counts::Either => category::Kind::IncDec,
        }
    }
}

/// One kind of incrementer: its cases' name, such as `A + increment`, the
/// way it counts, and what it gives for A and its control line.
pub(super) struct Change {
    pub name: String,
    pub counts: Counts,
    pub apply: fn(Value, Value) -> Value,
}

/// The kinds of incrementer with control line `line`, if any.
pub(super) fn changes(line: Option<&str>) -> Vec<Change> {
    let change = |name: String, counts, apply| Change {
        name,
        counts,
        apply,
    };
    match line {
        None => vec![
            change("A + 1".to_owned(), Counts::Up, |a, _| a.wrapping_add(1)),
            change("A − 1".to_owned(), Counts::Down, |a, _| {
                a.wrapping_sub(1)
            }),
        ],
        Some(line) => vec![
            change(format!("A + {line}"), Counts::Up, Value::wrapping_add),
            change(format!("A − {line}"), Counts::Down, Value::wrapping_sub),
            change(
                format!("A − 1, or + 1 with {line} on"),
                Counts::Either,
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
    let control = |i: &usize| interface.inputs[*i].control();
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
            rule: Rule::TooManyControlLines;
            "Too many control lines: {} (an incrementer takes one, such \
             as increment).",
            lines.len()
        );
    }
    let status = |i: &usize| interface.outputs[*i].status();
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

/// An incrementer's kind of test, counting `counts`: its unsigned line is
/// named for its role, such as increment, so the cases read `A +
/// increment`.
fn counting(
    kind: category::Kind,
    counts: Counts,
    interface: &mut Interface,
) -> Result<Spec> {
    interface::name_lines(kind.lines(), interface);
    spec(interface, Some(counts))
}

/// An incrementer's test: its copies chain, its carry out the next's
/// increment.
pub(super) const INCREMENTER: BySpec = BySpec::stacked(
    |_, interface| counting(category::Kind::Incrementer, Counts::Up, interface),
    |data| super::stacks::chained(data, Test::Incrementer),
);

pub(super) const DECREMENTER: BySpec = BySpec::stacked(
    |_, interface| {
        counting(category::Kind::Decrementer, Counts::Down, interface)
    },
    |data| super::stacks::chained(data, Test::Decrementer),
);

pub(super) const INC_DEC: BySpec = BySpec::spec(|_, interface| {
    counting(category::Kind::IncDec, Counts::Either, interface)
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incrementer_names_what_it_does_by_its_control_line() {
        let kinds: Vec<(String, &str)> = changes(Some("increment"))
            .into_iter()
            .map(|change| (change.name, change.counts.kind().name()))
            .collect();
        assert_eq!(kinds[0], ("A + increment".to_owned(), "incrementer"));
        assert_eq!(kinds[1].1, "decrementer");
        // Minus 1 with the line off, plus 1 with it on.
        let inc_dec = &changes(Some("increment"))[2];
        assert_eq!(((inc_dec.apply)(5, 0), (inc_dec.apply)(5, 1)), (4, 6));
        assert_eq!(kinds.len(), 3);
    }
}
