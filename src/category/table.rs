//! The table of every standard category, one row each, in the order of
//! [`Kind`]: the blessed categories as the list of what goes on it names
//! them, then the rest by name.

use super::{
    Behaviour, Copies, Facts, Guess, Kind, Lines, Probe, Probing, Reads,
    Stacking,
};
use crate::{recognize::probe::Line, simulate::Test};

/// What the code does with a category's builds when it does nothing
/// special: the default `Behaviour`, here where a constant needs it.
const NOTHING: Behaviour = Behaviour {
    test: None,
    reads: Reads::Nothing,
    flags_from_file: false,
    lines: Lines::Signed,
    stacking: Stacking::Given,
    copies: Copies::Chain,
    evaluated: false,
};

/// A category with test `test` and nothing else special.
const fn tested(test: Test) -> Behaviour {
    Behaviour {
        test: Some(test),
        ..NOTHING
    }
}

/// A category whose input A is its operand, which its width is read from.
const fn read(test: Test) -> Behaviour {
    Behaviour {
        reads: Reads::InputA,
        ..tested(test)
    }
}

/// A category whose every detail `evaluate` works out, stacked so.
const fn evaluated(test: Test, stacking: Stacking) -> Behaviour {
    Behaviour {
        stacking,
        evaluated: true,
        ..read(test)
    }
}

/// As `evaluated`, its lines meaning what `lines` says.
const fn lined(lines: Lines, test: Test, stacking: Stacking) -> Behaviour {
    Behaviour {
        lines,
        ..evaluated(test, stacking)
    }
}

/// A comparison's: its one output is its result, named `name`.
const fn compares(name: &'static str, test: Test) -> Behaviour {
    lined(Lines::Result(name), test, Stacking::Never)
}

/// A counter's or shift register's: its width is the value it holds.
const fn held(test: Test) -> Behaviour {
    Behaviour {
        reads: Reads::Output,
        ..evaluated(test, Stacking::Never)
    }
}

/// A monostable's: its lone output needs no sign, and its copies add bits,
/// each with its own.
const fn monostable(test: Test) -> Behaviour {
    Behaviour {
        copies: Copies::OwnOutputs,
        ..lined(Lines::Result("pulse"), test, Stacking::Inferred)
    }
}

/// Lone lines named `control` and `status`, needing no signs.
const fn lone(control: &'static str, status: &'static str) -> Lines {
    Lines::Lone {
        control,
        status,
        lone_output: false,
    }
}

/// A gate's: its copies add bits, each with its own A, B, and output.
const fn gate(test: Test) -> Behaviour {
    evaluated(test, Stacking::Inferred)
}

/// The name an unsigned carry in goes by; see `colour::CARRY_IN`.
const CARRY_IN: &str = crate::recognize::colour::CARRY_IN[0];

/// The name an unsigned carry out goes by; see `colour::CARRY_OUT`.
const CARRY_OUT: &str = crate::recognize::colour::CARRY_OUT[0];

/// An adder's, a hex adder's, and a subtractor's: a hex adder is an adder
/// whose inputs each take a hex digit per block, and a subtractor one
/// whose B is subtracted, each tested and read the same way.
const ADDER: Behaviour = Behaviour {
    flags_from_file: true,
    lines: lone(CARRY_IN, CARRY_OUT),
    stacking: Stacking::Inferred,
    evaluated: true,
    ..read(Test::Adder)
};

/// Probing in `order` by `how`.
const fn probed(order: u16, how: Probe) -> Probing {
    Probing::Tried { order, how }
}

/// Probing a gate's or a comparison's two operands, in `order`.
const fn operands(order: u16) -> Probing {
    probed(order, Probe::Operands)
}

/// Probing a build of one operand, in `order`, its control line as `line`
/// says.
const fn one_operand(order: u16, line: Line) -> Probing {
    probed(order, Probe::OneOperand(line))
}

/// A category off the list of blessed ones, and so never probed, named
/// `name`, read as `label`, with `behaviour`, its test tried in `guess`.
const fn unlisted(
    kind: Kind,
    name: &'static str,
    label: &'static str,
    noun: &'static str,
    guess: Guess,
    behaviour: Behaviour,
) -> Facts {
    Facts {
        kind,
        name,
        label,
        noun,
        listed_as: None,
        probe: Probing::Not,
        guess,
        role_words: &[],
        behaviour,
    }
}

/// A category the code does nothing special with.
const fn plain(
    kind: Kind,
    name: &'static str,
    label: &'static str,
    noun: &'static str,
) -> Facts {
    unlisted(kind, name, label, noun, Guess::Not, NOTHING)
}

/// The words a sign may name an adder's lines by.
const ADDER_WORDS: &[&str] = &[
    "carry in",
    "carry-in",
    "cin",
    "c in",
    "carry out",
    "carry-out",
    "cout",
    "c out",
    "borrow in",
    "borrow out",
    "bin",
    "bout",
    "subtract",
];

pub(super) const TABLE: [Facts; 47] = [
    Facts {
        kind: Kind::Adder,
        name: "adder",
        label: "Adder",
        noun: "adder",
        listed_as: Some("adders"),
        probe: probed(150, Probe::Adder),
        guess: Guess::Tried(10),
        role_words: ADDER_WORDS,
        behaviour: ADDER,
    },
    Facts {
        kind: Kind::HexAdder,
        name: "hex_adder",
        label: "Hex Adder",
        noun: "hex adder",
        listed_as: Some("hex adders"),
        probe: Probing::Not,
        guess: Guess::ByRunOf(Kind::Adder),
        role_words: &[],
        behaviour: ADDER,
    },
    Facts {
        kind: Kind::Subtractor,
        name: "subtractor",
        label: "Subtractor",
        noun: "subtractor",
        listed_as: Some("subtractors"),
        probe: Probing::As(Kind::Adder),
        guess: Guess::ByRunOf(Kind::Adder),
        role_words: &[],
        behaviour: ADDER,
    },
    Facts {
        kind: Kind::Alu,
        name: "alu",
        label: "ALU",
        noun: "ALU",
        listed_as: Some("ALUs"),
        probe: probed(240, Probe::Alu),
        guess: Guess::Tried(270),
        role_words: &[],
        // Its flags are the operations its test finds; it never stacks.
        behaviour: Behaviour {
            flags_from_file: true,
            ..evaluated(Test::Alu, Stacking::Never)
        },
    },
    Facts {
        kind: Kind::Multiplier,
        name: "multiplier",
        label: "Multiplier",
        noun: "multiplier",
        listed_as: Some("multipliers"),
        probe: probed(270, Probe::Multiplier),
        guess: Guess::Tried(160),
        role_words: &["calculate", "calc", "reset"],
        behaviour: Behaviour {
            flags_from_file: true,
            stacking: Stacking::Never,
            evaluated: true,
            lines: Lines::Control("calculate"),
            ..read(Test::Multiplier)
        },
    },
    Facts {
        kind: Kind::Incrementer,
        name: "incrementer",
        label: "Incrementer",
        noun: "incrementer",
        listed_as: Some("incrementers"),
        probe: one_operand(160, Line::Maybe),
        guess: Guess::Tried(150),
        role_words: &["increment", "inc", "carry out", "cout"],
        // Its copies chain as an adder's, its carry out the next's
        // increment.
        behaviour: lined(
            lone("increment", CARRY_OUT),
            Test::Incrementer,
            Stacking::Inferred,
        ),
    },
    Facts {
        kind: Kind::IncDec,
        name: "inc_dec",
        label: "Inc/Dec",
        noun: "inc/dec",
        listed_as: Some("incrementers"),
        probe: one_operand(180, Line::Maybe),
        guess: Guess::ByRunOf(Kind::Incrementer),
        role_words: &[],
        // Its line may mean either way, so it keeps its own name.
        behaviour: read(Test::IncDec),
    },
    Facts {
        kind: Kind::Decrementer,
        name: "decrementer",
        label: "Decrementer",
        noun: "decrementer",
        listed_as: Some("decrementers"),
        probe: one_operand(170, Line::Maybe),
        guess: Guess::ByRunOf(Kind::Incrementer),
        role_words: &["decrement", "dec", "borrow out", "bout"],
        behaviour: lined(
            lone("decrement", "borrow out"),
            Test::Decrementer,
            Stacking::Inferred,
        ),
    },
    Facts {
        kind: Kind::Equality,
        name: "equality",
        label: "Equality",
        noun: "equality comparator",
        listed_as: Some("comparators"),
        probe: operands(80),
        guess: Guess::Tried(240),
        role_words: &[],
        behaviour: compares("A = B", Test::Equality),
    },
    Facts {
        kind: Kind::GreaterThan,
        name: "greater_than",
        label: "Greater Than",
        noun: "greater-than comparator",
        listed_as: Some("comparators"),
        probe: operands(90),
        guess: Guess::Tried(230),
        role_words: &[],
        behaviour: compares("A > B", Test::GreaterThan),
    },
    Facts {
        kind: Kind::CarryOut,
        name: "carry_out",
        label: "Carry Out",
        noun: "carry out",
        listed_as: Some("carry outs"),
        probe: operands(100),
        guess: Guess::Tried(220),
        role_words: &["carry in", "cin", "carry out", "cout"],
        // Its copies chain as an adder's, its carry out the next's carry in.
        behaviour: Behaviour {
            lines: Lines::Lone {
                control: CARRY_IN,
                status: CARRY_OUT,
                lone_output: true,
            },
            ..evaluated(Test::CarryOut, Stacking::Inferred)
        },
    },
    // Copies of these make no wider one, so they never stack.
    Facts {
        kind: Kind::AbsoluteValue,
        name: "absolute_value",
        label: "Absolute Value",
        noun: "absolute value",
        listed_as: Some("absolute values"),
        probe: one_operand(120, Line::Never),
        guess: Guess::Tried(210),
        role_words: &[],
        behaviour: evaluated(Test::AbsoluteValue, Stacking::Never),
    },
    Facts {
        kind: Kind::Popcount,
        name: "popcount",
        label: "Popcount",
        noun: "popcount",
        listed_as: Some("popcounts"),
        probe: one_operand(190, Line::Never),
        guess: Guess::Tried(120),
        role_words: &[],
        behaviour: evaluated(Test::Popcount, Stacking::Never),
    },
    Facts {
        kind: Kind::Threshold,
        name: "threshold",
        label: "Threshold",
        noun: "threshold",
        listed_as: Some("thresholds"),
        probe: one_operand(210, Line::Never),
        guess: Guess::Tried(140),
        role_words: &[],
        behaviour: evaluated(Test::Threshold, Stacking::Never),
    },
    Facts {
        kind: Kind::Parity,
        name: "parity",
        label: "Parity",
        noun: "parity circuit",
        listed_as: Some("parity circuits"),
        probe: one_operand(215, Line::Never),
        guess: Guess::Tried(145),
        role_words: &[],
        behaviour: evaluated(Test::Parity, Stacking::Never),
    },
    Facts {
        kind: Kind::BinaryToHex,
        name: "binary_to_hex",
        label: "Binary to Hex",
        noun: "binary to hex",
        listed_as: Some("binary-to-hex converters"),
        probe: one_operand(200, Line::Never),
        guess: Guess::Tried(130),
        role_words: &[],
        behaviour: evaluated(Test::BinaryToHex, Stacking::Never),
    },
    Facts {
        kind: Kind::XorGate,
        name: "xor_gate",
        label: "XOR Gate",
        noun: "XOR gate",
        listed_as: Some("gates"),
        probe: operands(10),
        guess: Guess::Tried(60),
        role_words: &[],
        behaviour: gate(Test::Xor),
    },
    Facts {
        kind: Kind::XnorGate,
        name: "xnor_gate",
        label: "XNOR Gate",
        noun: "XNOR gate",
        listed_as: Some("gates"),
        probe: operands(20),
        guess: Guess::Tried(70),
        role_words: &[],
        behaviour: gate(Test::Xnor),
    },
    Facts {
        kind: Kind::AndGate,
        name: "and_gate",
        label: "AND Gate",
        noun: "AND gate",
        listed_as: Some("gates"),
        probe: operands(30),
        guess: Guess::Tried(80),
        role_words: &[],
        behaviour: gate(Test::And),
    },
    Facts {
        kind: Kind::NandGate,
        name: "nand_gate",
        label: "NAND Gate",
        noun: "NAND gate",
        listed_as: Some("gates"),
        probe: operands(40),
        guess: Guess::Tried(90),
        role_words: &[],
        behaviour: gate(Test::Nand),
    },
    Facts {
        kind: Kind::OrGate,
        name: "or_gate",
        label: "OR Gate",
        noun: "OR gate",
        listed_as: Some("gates"),
        probe: operands(50),
        guess: Guess::Tried(100),
        role_words: &[],
        behaviour: gate(Test::Or),
    },
    Facts {
        kind: Kind::NorGate,
        name: "nor_gate",
        label: "NOR Gate",
        noun: "NOR gate",
        listed_as: Some("gates"),
        probe: operands(60),
        guess: Guess::Tried(110),
        role_words: &[],
        behaviour: gate(Test::Nor),
    },
    Facts {
        kind: Kind::NandXnorGate,
        name: "nand_xnor_gate",
        label: "NAND+XNOR Gate",
        noun: "NAND+XNOR gate",
        listed_as: Some("gates"),
        probe: operands(70),
        guess: Guess::Tried(30),
        role_words: &[],
        behaviour: gate(Test::NandXnor),
    },
    Facts {
        kind: Kind::Inverter,
        name: "inverter",
        label: "Inverter",
        noun: "inverter",
        listed_as: Some("inverters"),
        probe: one_operand(110, Line::Always),
        guess: Guess::Tried(50),
        role_words: &["invert", "inv"],
        // Its copies add bits, sharing its control line.
        behaviour: lined(
            Lines::Control("invert"),
            Test::Inverter,
            Stacking::Inferred,
        ),
    },
    Facts {
        kind: Kind::Decoder,
        name: "decoder",
        label: "Decoder",
        noun: "decoder",
        listed_as: Some("decoders"),
        probe: probed(250, Probe::Decoder),
        guess: Guess::Tried(20),
        role_words: &[],
        // Its copies are more decoders, each with its own input and outputs.
        behaviour: Behaviour {
            copies: Copies::OwnOutputs,
            ..evaluated(Test::Decoder, Stacking::Inferred)
        },
    },
    Facts {
        kind: Kind::BarrelShifter,
        name: "barrel_shifter",
        label: "Barrel Shifter",
        noun: "barrel shifter",
        listed_as: Some("barrel shifters"),
        probe: probed(260, Probe::Shifter),
        guess: Guess::Tried(40),
        role_words: &[],
        behaviour: evaluated(Test::BarrelShifter, Stacking::Never),
    },
    Facts {
        kind: Kind::Monostable,
        name: "monostable",
        label: "Monostable",
        noun: "monostable",
        listed_as: Some("monostables"),
        probe: one_operand(130, Line::Never),
        guess: Guess::Tried(250),
        role_words: &[],
        behaviour: monostable(Test::Monostable),
    },
    Facts {
        kind: Kind::DualEdgeMonostable,
        name: "dual_edge_monostable",
        label: "Dual-Edge Monostable",
        noun: "dual-edge monostable",
        listed_as: Some("monostables"),
        probe: one_operand(140, Line::Never),
        guess: Guess::Tried(260),
        role_words: &[],
        behaviour: monostable(Test::DualEdgeMonostable),
    },
    Facts {
        kind: Kind::Register,
        name: "register",
        label: "Register",
        noun: "register",
        listed_as: Some("registers"),
        probe: probed(220, Probe::Register),
        guess: Guess::Tried(190),
        role_words: &["write", "read", "write enable", "data in", "data out"],
        behaviour: Behaviour {
            lines: Lines::Register,
            copies: Copies::SharedBuses,
            ..evaluated(Test::Register, Stacking::Inferred)
        },
    },
    Facts {
        kind: Kind::Buffer,
        name: "buffer",
        label: "Buffer",
        noun: "buffer",
        listed_as: Some("buffers"),
        probe: probed(230, Probe::Register),
        guess: Guess::Tried(200),
        role_words: &["write", "write enable", "data in", "data out"],
        // Its copies add bits, as its cells cannot share a bus.
        behaviour: lined(Lines::Buffer, Test::Buffer, Stacking::Inferred),
    },
    Facts {
        kind: Kind::Counter,
        name: "counter",
        label: "Counter",
        noun: "counter",
        listed_as: Some("counters"),
        probe: probed(280, Probe::Sequential),
        guess: Guess::Tried(280),
        role_words: &["clock", "clk", "write", "write enable", "load"],
        behaviour: Behaviour {
            lines: Lines::Counter,
            ..held(Test::Counter)
        },
    },
    Facts {
        kind: Kind::ShiftRegister,
        name: "shift_register",
        label: "Shift Register",
        noun: "shift register",
        listed_as: Some("shift registers"),
        probe: probed(290, Probe::Sequential),
        guess: Guess::Tried(290),
        role_words: &[],
        behaviour: held(Test::ShiftRegister),
    },
    Facts {
        kind: Kind::TFlipFlop,
        name: "t_flip_flop",
        label: "T Flip-Flop",
        noun: "T flip-flop",
        listed_as: Some("T flip-flops"),
        probe: probed(300, Probe::Sequential),
        guess: Guess::Tried(300),
        role_words: &["toggle", "t", "clock", "clk", "q"],
        // Its lone control line needs no sign: it toggles the bit held.
        behaviour: Behaviour {
            lines: Lines::Control("toggle"),
            ..held(Test::TFlipFlop)
        },
    },
    unlisted(
        Kind::BcdToBinary,
        "bcd_to_binary",
        "BCD to Binary",
        "bcd to binary",
        Guess::Tried(180),
        tested(Test::BcdToBinary),
    ),
    unlisted(
        Kind::BinaryToBcd,
        "binary_to_bcd",
        "Binary to BCD",
        "binary to bcd",
        Guess::Tried(170),
        tested(Test::BinaryToBcd),
    ),
    plain(Kind::Bussing, "bussing", "Bussing", "bussing"),
    plain(
        Kind::CarrySaveAdder,
        "carry_save_adder",
        "Carry Save Adder",
        "carry save adder",
    ),
    plain(Kind::Dataloop, "dataloop", "Dataloop", "dataloop"),
    plain(Kind::Display, "display", "Display", "display"),
    plain(Kind::Divider, "divider", "Divider", "divider"),
    plain(
        Kind::HexToBinary,
        "hex_to_binary",
        "Hex to Binary",
        "hex to binary",
    ),
    plain(
        Kind::LineDrawer,
        "line_drawer",
        "Line Drawer",
        "line drawer",
    ),
    plain(
        Kind::NandNorGate,
        "nand_nor_gate",
        "NAND+NOR Gate",
        "NAND+NOR gate",
    ),
    // Its copies across its bits are more words on one bus.
    unlisted(
        Kind::Ram,
        "ram",
        "RAM",
        "RAM",
        Guess::Not,
        Behaviour {
            copies: Copies::SharedBuses,
            ..NOTHING
        },
    ),
    plain(Kind::Rng, "rng", "RNG", "RNG"),
    plain(
        Kind::SerialAdder,
        "serial_adder",
        "Serial Adder",
        "serial adder",
    ),
    plain(
        Kind::SerialWriteMemory,
        "serial_write_memory",
        "Serial Write Memory",
        "serial write memory",
    ),
];

// The table holds together by construction: each row stands at its kind's
// place, which `Kind::facts` reads by; no two kinds are probed or guessed
// at the same place in order; and a kind probed or guessed has a test,
// which a kind with none is not.
const _: () = {
    let mut i = 0;
    while i < TABLE.len() {
        let row = &TABLE[i];
        assert!(row.kind as usize == i, "a row out of its kind's place");
        let tested = row.behaviour.test.is_some();
        if let Probing::Tried { .. } = row.probe {
            assert!(tested, "a kind probed with no test");
        }
        match row.guess {
            Guess::Tried(_) => assert!(tested, "a kind guessed with no test"),
            Guess::Not => assert!(!tested, "a kind with a test not guessed"),
            Guess::ByRunOf(_) => assert!(tested, "a kind found with no test"),
        }
        let mut j = 0;
        while j < i {
            let other = &TABLE[j];
            if let (
                Probing::Tried { order: x, .. },
                Probing::Tried { order: y, .. },
            ) = (row.probe, other.probe)
            {
                assert!(x != y, "two kinds probed at one place");
            }
            if let (Guess::Tried(x), Guess::Tried(y)) = (row.guess, other.guess)
            {
                assert!(x != y, "two kinds guessed at one place");
            }
            j += 1;
        }
        i += 1;
    }
};
