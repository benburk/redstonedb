//! Why a build isn't verified, one problem at a time: the rule it breaks,
//! what is wrong in a sentence, and where. The rule says how to fix it, as
//! a good compiler error does, and which section of the Build Analysis
//! page pictures it, so nothing reads a sentence to tell what it means.

use serde::{Deserialize, Serialize};

use crate::schem::Pos;

/// A rule a build breaks, or a way it falls short of its category.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    NoInputs,
    NoOutputs,
    /// A signal-strength input with nothing under it in the file.
    InputAtBottom,
    /// An output with nothing under it in the file.
    OutputAtBottom,
    SwitchOnTopOrBottom,
    InputBlockNotSolid,
    SwitchTouchesRedstone,
    InputBlockWiring,
    InputDrivesNothing,
    /// A block under a lever, where a repeater would stand to let a bus
    /// drive the input.
    NoRoomForRepeater,
    /// A block behind a lever, where a bus's driver would feed the
    /// repeater put in its place: fine on a control line, which a lever
    /// or button drives, but not on a data input, which a bus may drive;
    /// no fault until its line's role is known.
    BlockBehindLever,
    NotBusDrivable,
    /// Lines that aren't a hex adder's or a hex-to-binary converter's.
    HexLines,
    /// No line of levers as wide as the build, for an operand.
    Buses,
    /// An input that takes a signal strength where its category takes
    /// levers.
    WithoutLevers,
    /// A register's cells without a button writing each and a lever
    /// reading it.
    RegisterControls,
    /// A buffer with read levers.
    BufferControls,
    /// Cells without a button writing each.
    WriteControls,
    TooManyControlLines,
    /// Its test ran and got cases wrong.
    WrongCases,
    /// A block the simulator does not model.
    NotSimulated,
    /// It never comes to rest.
    StillChanging,
    /// Its lines don't fit its category's, in a way with no more to say
    /// than the sentence does.
    Lines,
    /// Anything else, as a file that can't be read.
    Other,
}

/// How to fix a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advice {
    /// What to change, in a sentence or two.
    pub fix: &'static str,
    /// The section of the Build Analysis page that pictures the rule, by
    /// its id, such as `data-inputs`.
    pub section: Option<&'static str>,
}

impl Rule {
    /// A short name for the rule, to count problems by.
    pub fn name(self) -> &'static str {
        match self {
            Rule::NoInputs => "no inputs",
            Rule::NoOutputs => "no outputs",
            Rule::InputAtBottom => "input at the bottom",
            Rule::OutputAtBottom => "output at the bottom",
            Rule::SwitchOnTopOrBottom => "switch on top or bottom",
            Rule::InputBlockNotSolid => "input block not solid",
            Rule::SwitchTouchesRedstone => "switch touches redstone",
            Rule::InputBlockWiring => "input block wiring",
            Rule::InputDrivesNothing => "input drives nothing",
            Rule::NoRoomForRepeater => "no room for a repeater",
            Rule::BlockBehindLever => "block behind a lever",
            Rule::NotBusDrivable => "lever not bus-drivable",
            Rule::HexLines => "hex lines",
            Rule::Buses => "buses",
            Rule::WithoutLevers => "inputs without levers",
            Rule::RegisterControls => "register controls",
            Rule::BufferControls => "buffer controls",
            Rule::WriteControls => "write controls",
            Rule::TooManyControlLines => "too many control lines",
            Rule::WrongCases => "wrong cases",
            Rule::NotSimulated => "not simulated",
            Rule::StillChanging => "still changing",
            Rule::Lines => "lines",
            Rule::Other => "other",
        }
    }

    /// Whether it is a fault only on a data input, which a bus may drive.
    pub fn data_only(self) -> bool {
        self == Rule::BlockBehindLever
    }

    /// How to fix a problem that breaks it; `None` for a rule with
    /// nothing more to say than its problem's sentence.
    pub fn advice(self) -> Option<Advice> {
        self.fix().map(|fix| Advice {
            fix,
            section: self.section(),
        })
    }

    /// The section of the Build Analysis page that pictures it, by its
    /// id, if one does.
    fn section(self) -> Option<&'static str> {
        match self {
            Rule::NoOutputs | Rule::InputAtBottom | Rule::OutputAtBottom => {
                Some("outputs")
            }
            Rule::NoInputs
            | Rule::SwitchOnTopOrBottom
            | Rule::InputBlockNotSolid
            | Rule::SwitchTouchesRedstone
            | Rule::InputBlockWiring
            | Rule::InputDrivesNothing
            | Rule::NoRoomForRepeater
            | Rule::BlockBehindLever
            | Rule::NotBusDrivable
            | Rule::WithoutLevers => Some("data-inputs"),
            Rule::Buses => Some("buses"),
            Rule::HexLines
            | Rule::RegisterControls
            | Rule::BufferControls
            | Rule::WriteControls
            | Rule::TooManyControlLines => Some("per-category"),
            Rule::WrongCases
            | Rule::NotSimulated
            | Rule::StillChanging
            | Rule::Lines
            | Rule::Other => None,
        }
    }

    /// What to change to keep it, in a sentence or two.
    fn fix(self) -> Option<&'static str> {
        Some(match self {
            Rule::NoInputs => {
                "Put a lever on the side of each input bit's block, one \
                 line of levers for each input, or for a hex digit a \
                 comparator with air behind it."
            }
            Rule::NoOutputs => {
                "Give each output bit a repeater, or a comparator, that \
                 powers nothing, the block in front of it air, on a block \
                 of any kind."
            }
            Rule::InputAtBottom | Rule::OutputAtBottom => {
                "Save the build with the block under it, so the file holds \
                 what it stands on."
            }
            Rule::SwitchOnTopOrBottom => "Put it on the side of its block.",
            Rule::InputBlockNotSolid => {
                "Put it on a full solid block, such as concrete, wool, or a \
                 lamp."
            }
            Rule::SwitchTouchesRedstone => {
                "Move it so it touches only its own block, and no other \
                 redstone."
            }
            Rule::InputBlockWiring => {
                "Wire each input's block into the circuit from the side, \
                 with no redstone above or below it."
            }
            Rule::InputDrivesNothing => {
                "Wire the input's block into the circuit, or take the input \
                 out."
            }
            Rule::NoRoomForRepeater | Rule::BlockBehindLever => {
                "Clear that block, so a repeater could drive the input in \
                 place of its lever, as a bus would."
            }
            Rule::NotBusDrivable => {
                "Leave room for a repeater where the lever is, so a bus \
                 could drive the input."
            }
            Rule::HexLines => {
                "Give a hex adder two buses of comparators with air behind \
                 them, A and B, one for each digit, and its sum a bus of \
                 repeaters or comparators facing air."
            }
            Rule::Buses => {
                "Put each input's levers in one evenly spaced line, one for \
                 each bit, every input as wide as A."
            }
            Rule::WithoutLevers => {
                "Put a lever on each input bit's block. If its inputs take \
                 a hex digit each, a comparator with air behind it, upload \
                 it as a Hex Adder."
            }
            Rule::RegisterControls => {
                "Give each cell a button that writes it and a lever that \
                 reads it onto the bus. One that shows what it holds all \
                 the time is a Buffer."
            }
            Rule::BufferControls => {
                "Take out its read levers, or upload it as a Register, \
                 which reads each cell with a lever."
            }
            Rule::WriteControls => "Give each cell a button that writes it.",
            Rule::TooManyControlLines => {
                "Take out every control line but the one its category \
                 takes, so a multiplier keeps only calculate."
            }
            Rule::WrongCases => {
                "See which cases fail under Simulation, fix the circuit, \
                 and upload it as a new version."
            }
            Rule::NotSimulated => {
                "The simulator doesn't model it yet, so the build can't be \
                 tested. Build it from parts the simulator models to verify \
                 it."
            }
            Rule::StillChanging => {
                "Make sure it comes to rest once its inputs stop: take out \
                 any clock or loop that keeps it changing."
            }
            Rule::Lines | Rule::Other => return None,
        })
    }
}

/// Something only a build's author can put right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub rule: Rule,
    /// The blocks at fault, an input's own block first.
    pub at: Vec<Pos>,
    /// What is wrong, in a sentence, naming `places`.
    pub message: String,
    /// The places its sentence names, in the order it names them.
    pub places: Vec<Pos>,
}

/// Where a block is, for a message: x, y, z from the file's corner.
pub fn place(pos: Pos) -> String {
    format!("({}, {}, {})", pos[0], pos[1], pos[2])
}

impl Problem {
    /// A problem that breaks `rule`, at blocks `at`. Each `{}` in
    /// `sentence` stands for one of `places`, in order.
    pub fn new(
        rule: Rule,
        at: Vec<Pos>,
        sentence: &str,
        places: &[Pos],
    ) -> Self {
        let mut message = sentence.to_owned();
        for &pos in places {
            message = message.replacen("{}", &place(pos), 1);
        }
        Problem {
            rule,
            at,
            message,
            places: places.to_vec(),
        }
    }

    /// A problem that breaks `rule`, as `message` says, at no place.
    pub fn said(rule: Rule, message: impl Into<String>) -> Self {
        Problem {
            rule,
            at: Vec::new(),
            message: message.into(),
            places: Vec::new(),
        }
    }

    /// What is wrong, with each place its sentence names left out, as `…`,
    /// so the same fault at five places reads as one kind of problem.
    pub fn kind(&self) -> String {
        let mut kind = self.message.clone();
        for &pos in &self.places {
            kind = kind.replacen(&place(pos), "…", 1);
        }
        kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A problem's sentence names its places, and its kind leaves them out.
    #[test]
    fn a_problem_names_its_places() {
        let problem = Problem::new(
            Rule::SwitchTouchesRedstone,
            vec![[5, 2, 3], [4, 2, 3]],
            "Lever at {}: touches repeater at {}",
            &[[5, 2, 3], [4, 2, 3]],
        );
        assert_eq!(
            problem.message,
            "Lever at (5, 2, 3): touches repeater at (4, 2, 3)"
        );
        assert_eq!(problem.kind(), "Lever at …: touches repeater at …");
        let plain = Problem::said(Rule::Lines, "Outputs: none");
        assert_eq!(plain.kind(), "Outputs: none");
    }

    /// Every rule a problem can break, but those with nothing more to say,
    /// has advice.
    #[test]
    fn each_rule_has_advice() {
        assert!(Rule::NotBusDrivable.advice().is_some());
        assert!(
            Rule::WrongCases
                .advice()
                .is_some_and(|a| a.section.is_none())
        );
        assert_eq!(Rule::Lines.advice(), None);
        assert_eq!(Rule::Other.advice(), None);
    }
}
