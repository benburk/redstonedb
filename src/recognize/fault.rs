//! What to do about each reason a build isn't verified, as a good compiler
//! error does: the reason says what is wrong and where, and its advice how
//! to fix it, pointing to the rule the Build Analysis page pictures. The
//! reasons come from the check by function, the simulator, and
//! `details::unrecognized`, each written in one of a few fixed shapes,
//! which are matched here.

/// How to fix one reason a build isn't verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advice {
    /// What to change, in a sentence or two.
    pub fix: &'static str,
    /// The section of the Build Analysis page that pictures the rule, by
    /// its id, such as `data-inputs`.
    pub rule: Option<&'static str>,
}

/// One shape of reason: what a reason of that shape is, as words it is,
/// starts with, or holds.
#[derive(Clone, Copy)]
enum Shape {
    Is(&'static str),
    Starts(&'static str),
    Has(&'static str),
}

impl Shape {
    fn fits(self, reason: &str) -> bool {
        match self {
            Shape::Is(words) => reason == words,
            Shape::Starts(words) => reason.starts_with(words),
            Shape::Has(words) => reason.contains(words),
        }
    }
}

/// The reasons of any of shapes `when`, and how to fix them: what to
/// change, and the rule's section on the Build Analysis page, if any. The
/// first case a reason fits gives its advice, so a narrower shape comes
/// before a wider one.
struct Case {
    when: &'static [Shape],
    fix: &'static str,
    rule: Option<&'static str>,
}

use Shape::{Has, Is, Starts};

const CASES: &[Case] = &[
    Case {
        when: &[Is("No inputs"), Starts("Inputs: none found")],
        fix: "Put a lever on the side of each input bit's block, one line \
              of levers for each input, or for a hex digit a comparator with \
              air behind it.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Is("No outputs"), Starts("Outputs: none found")],
        fix: "Give each output bit a repeater, or a comparator, that powers \
              nothing, the block in front of it air, on a block of any kind.",
        rule: Some("outputs"),
    },
    Case {
        when: &[Has("nothing under it in the file")],
        fix: "Save the build with the block under it, so the file holds \
              what it stands on.",
        rule: Some("outputs"),
    },
    Case {
        when: &[
            Starts("Inputs: no comparator"),
            Has("a hex adder takes"),
            Has("a hex adder gives"),
        ],
        fix: "Give a hex adder two buses of comparators with air behind \
              them, A and B, one for each digit, and its sum a bus of \
              repeaters or comparators facing air.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Has(": on top of block"), Has(": on bottom of block")],
        fix: "Put it on the side of its block.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Has(": on non-solid")],
        fix: "Put it on a full solid block, such as concrete, wool, or a \
              lamp.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Has(": touches ")],
        fix: "Move it so it touches only its own block, and no other \
              redstone.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Has("drives nothing")],
        fix: "Wire the input's block into the circuit, or take the input out.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Has("below blocks repeater"), Has("behind blocks repeater")],
        fix: "Clear that block, so a repeater could drive the input in place \
              of its lever, as a bus would.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Has("not bus-drivable")],
        fix: "Leave room for a repeater where the lever is, so a bus could \
              drive the input.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Starts("Input at ")],
        fix: "Wire each input's block into the circuit from the side, with no \
              redstone above or below it.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Starts("Output bus")],
        fix: "Put one output for each bit in one evenly spaced line, straight \
              or diagonal.",
        rule: Some("buses"),
    },
    Case {
        when: &[Starts("Input bus"), Has(": no line of ")],
        fix: "Put each input's levers in one evenly spaced line, one for each \
              bit, every input as wide as A.",
        rule: Some("buses"),
    },
    Case {
        when: &[Is("Couldn't count its bits")],
        fix: "Put input A's levers in one evenly spaced line, one for each \
              bit.",
        rule: Some("buses"),
    },
    Case {
        when: &[Starts("Couldn't recognize input")],
        fix: "Put a lever on each input bit's block. If its inputs take a hex \
              digit each, a comparator with air behind it, upload it as a \
              Hex Adder.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Is(crate::details::SUBTRACTS)],
        fix: "Upload it as a Subtractor, which ranks apart from adders.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Is(crate::details::ADDS)],
        fix: "Upload it as an Adder.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Is(crate::details::NO_HEX_INPUTS)],
        fix: "A hex adder's or hex-to-binary converter's inputs each take a \
              hex digit, a signal strength through a comparator with air \
              behind it. An adder with levers is an Adder.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Is("Couldn't read its inputs and outputs")],
        fix: "Give each input bit a lever, and each output bit a repeater or \
              comparator facing air.",
        rule: Some("data-inputs"),
    },
    Case {
        when: &[Starts(
            "Control lines: expected a button writing each cell and",
        )],
        fix: "Give each cell a button that writes it and a lever that reads \
              it onto the bus. One that shows what it holds all the time is \
              a Buffer.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Starts("Control lines: expected only writes")],
        fix: "Take out its read levers, or upload it as a Register, which \
              reads each cell with a lever.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Starts("Control lines: expected a button writing")],
        fix: "Give each cell a button that writes it.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Starts("Too many control lines")],
        fix: "Take out every control line but the one its category takes, so \
              a multiplier keeps only calculate.",
        rule: Some("per-category"),
    },
    Case {
        when: &[Is(crate::details::WRONG_OUTPUTS), Has("cases wrong")],
        fix: "See which cases fail under Simulation, fix the circuit, and \
              upload it as a new version.",
        rule: None,
    },
    Case {
        when: &[Is("Couldn't read its file")],
        fix: "Save it again with //schem save, and upload that file.",
        rule: None,
    },
    Case {
        when: &[Starts("Couldn't test "), Has("tested range")],
        fix: "Upload a version 32 bits wide or narrower, or a slice whose \
              copies stack to this width.",
        rule: None,
    },
    Case {
        when: &[Starts("Couldn't simulate its ")],
        fix: "The simulator doesn't model it yet, so the build can't be \
              tested. Build it from parts the simulator models to verify it.",
        rule: None,
    },
    Case {
        when: &[Has("still changing")],
        fix: "Make sure it comes to rest once its inputs stop: take out any \
              clock or loop that keeps it changing.",
        rule: None,
    },
];

/// Whether `reason` says a build gives wrong outputs, so it fails
/// verification, rather than that its inputs, outputs, or parts don't fit
/// its category's, which leaves it non-standard.
pub fn wrong_outputs(reason: &str) -> bool {
    let reason = reason.trim();
    reason == crate::details::WRONG_OUTPUTS || reason.contains("cases wrong")
}

/// The advice for `reason`, one line of a build's reasons it isn't
/// verified;
/// `None` for a reason with nothing more to say.
pub fn advice(reason: &str) -> Option<Advice> {
    let reason = reason.trim();
    // A test that couldn't run says why after its own words.
    if let Some(why) = reason.strip_prefix("Test couldn't run: ") {
        return advice(why);
    }
    CASES
        .iter()
        .find(|case| case.when.iter().any(|shape| shape.fits(reason)))
        .map(|case| Advice {
            fix: case.fix,
            rule: case.rule,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fault a pictured example that breaks a rule shows has advice
    /// pointing to that rule's section.
    #[test]
    fn each_pictured_fault_has_advice() {
        for example in crate::recognize::guide::examples() {
            if example.breaks.is_none() {
                continue;
            }
            let report =
                crate::recognize::inspect::inspect(&example.schematic());
            for problem in &report.errors {
                let advice = advice(&problem.message);
                assert!(
                    advice.as_ref().is_some_and(|a| a.rule.is_some()),
                    "{}: {}",
                    example.name,
                    problem.message
                );
            }
        }
    }

    /// The reasons a build is found unverified for each have advice.
    #[test]
    fn each_reason_for_an_unverified_build_has_advice() {
        for reason in [
            crate::details::WITHOUT_LEVERS,
            crate::details::NO_HEX_INPUTS,
            crate::details::SUBTRACTS,
            crate::details::ADDS,
            "Couldn't read its inputs and outputs",
            "Couldn't read its file",
            "Couldn't count its bits",
            "Couldn't test adders of 128 bits yet",
            "Gives wrong outputs for some inputs",
            "Couldn't simulate its piston head",
            "It was still changing 1000 ticks after it was loaded",
            "Too many control lines: calculate, reset (a multiplier takes \
             one, calculate)",
            "Test couldn't run: Couldn't recognize input A without levers.",
            "Output bus: 7 outputs, need 8",
            "A: no line of 8 levers",
            "Test couldn't run: Control lines: expected a button writing \
             each cell and a lever reading it, found 2 buttons, no levers.",
            "Control lines: expected only writes, as a buffer shows what it \
             holds, found 2 buttons, 2 levers.",
            "Control lines: expected a button writing each cell, found no \
             buttons, 3 levers.",
        ] {
            assert!(advice(reason).is_some(), "{reason}");
        }
        assert_eq!(advice("Something else entirely"), None);
    }
}
