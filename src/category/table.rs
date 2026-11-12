//! The table of every standard category, one row each, in the order of
//! [`Kind`]: the standard categories as the list of what goes on it names
//! them, then the rest by name. It is the list of standard categories:
//! people add to it here.

use super::Logic;
use super::{
    Behaviour, Copies, Design, Facts, Flag, Group, Guess, Kind, Line, Lines,
    Measure, Probe, Probing, Reads, Section, Stacking, Test, adder, alu,
    converters, decoder, gate, incrementer, inverter, monostable, multiplier,
    negator, register, sequential, shifter,
};

/// What the code does with a category's builds when it does nothing
/// special: the default `Behaviour`, here where a constant needs it.
const NOTHING: Behaviour = Behaviour {
    test: None,
    reads: Reads::Nothing,
    flags_from_file: false,
    lines: Lines::Signed,
    stacking: Stacking::Given,
    copies: Copies::Chain,
    group: Group::CrossesBits,
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
/// each a cell with its own.
const fn monostable(test: Test) -> Behaviour {
    Behaviour {
        copies: Copies::OwnOutputs,
        group: Group::Cells,
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

/// A gate's: its copies add bits, each a cell with its own A, B, and
/// output.
const fn gate(test: Test) -> Behaviour {
    Behaviour {
        group: Group::Cells,
        ..evaluated(test, Stacking::Inferred)
    }
}

/// The name an unsigned carry in goes by; see `colour::CARRY_IN`.
const CARRY_IN: &str = crate::interface::colour::CARRY_IN[0];

/// The name an unsigned carry out goes by; see `colour::CARRY_OUT`.
const CARRY_OUT: &str = crate::interface::colour::CARRY_OUT[0];

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

/// Probing in `order` a build whose lines matching reads.
const fn matched(order: u16) -> Probing {
    probed(order, Probe::Matched)
}

/// Probing a build of one operand, in `order`, its control line as `line`
/// says.
const fn one_operand(order: u16, line: Line) -> Probing {
    probed(order, Probe::OneOperand(line))
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

/// What a row holds where it says nothing: a category measured by its
/// width alone, with no designs, flags, or aliases, which goes on no list,
/// is never probed or guessed, and gets nothing special. Its kind, names,
/// section, and logic every row gives.
const ROW: Facts = Facts {
    kind: Kind::Adder,
    name: "",
    label: "",
    noun: "",
    plural: None,
    section: Section::Arithmetic,
    logic: None,
    measures: &[Measure::Width],
    designs: &[],
    flags: &[],
    aliases: &[],
    listed_as: "",
    probe: Probing::As(Kind::Adder),
    guess: Guess::ByRunOf(Kind::Adder),
    role_words: &[],
    digit_inputs: false,
    runs: None,
    behaviour: NOTHING,
};

/// A design named `name`, its usual short name `abbreviation`.
const fn design(
    name: &'static str,
    abbreviation: Option<&'static str>,
) -> Design {
    Design { name, abbreviation }
}

/// An adder's, a hex adder's, and a subtractor's designs, as a subtractor's
/// borrows run through the same chains an adder's carries do.
const ADDER_DESIGNS: &[Design] = &[
    design("carry_cancel", Some("CCA")),
    design("carry_look_everywhere", Some("CLE")),
    design("carry_lookahead", Some("CLA")),
    design("hybrid", None),
    design("ripple_carry", Some("RCA")),
];

/// A flag named `name`, read as `label`.
const fn flag(name: &'static str, label: &'static str) -> Flag {
    Flag { name, label }
}

const CARRY_IN_FLAG: Flag = flag(Flag::CARRY_IN, "Carry in");
const CARRY_OUT_FLAG: Flag = flag(Flag::CARRY_OUT, "Carry out");
const INVERTED_CARRY_IN_FLAG: Flag =
    flag(Flag::INVERTED_CARRY_IN, "Inverted carry in");
const INVERTED_CARRY_OUT_FLAG: Flag =
    flag(Flag::INVERTED_CARRY_OUT, "Inverted carry out");

/// An adder's and a hex adder's flags.
const ADDER_FLAGS: &[Flag] = &[
    CARRY_IN_FLAG,
    CARRY_OUT_FLAG,
    flag("subtract", "Subtract"),
    INVERTED_CARRY_IN_FLAG,
    INVERTED_CARRY_OUT_FLAG,
];

/// An ALU's flags: the operations it offers beyond adding and subtracting,
/// its lines, and its status lines.
const ALU_FLAGS: &[Flag] = &[
    flag("and", "AND"),
    flag("or", "OR"),
    flag("xor", "XOR"),
    flag("shl", "SHL"),
    flag("shr", "SHR"),
    CARRY_IN_FLAG,
    CARRY_OUT_FLAG,
    flag("overflow", "Overflow"),
    flag("zero", "Zero"),
    flag("nand", "NAND"),
    flag("nor", "NOR"),
    flag("xnor", "XNOR"),
    flag("asr", "ASR"),
    flag("nadd", "NADD"),
    flag("nsub", "NSUB"),
];

pub(super) const TABLE: [Facts; 36] = [
    Facts {
        kind: Kind::Adder,
        runs: Some(&adder::ADDER),
        name: "adder",
        label: "Adder",
        noun: "adder",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        designs: ADDER_DESIGNS,
        flags: ADDER_FLAGS,
        listed_as: "adders",
        probe: matched(150),
        guess: Guess::Tried(10),
        role_words: ADDER_WORDS,
        behaviour: ADDER,
        ..ROW
    },
    Facts {
        kind: Kind::HexAdder,
        name: "hex_adder",
        label: "Hex Adder",
        noun: "hex adder",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        designs: ADDER_DESIGNS,
        flags: ADDER_FLAGS,
        listed_as: "hex adders",
        // First, as only a build with signal-strength inputs can be one,
        // and one without is told so at once.
        probe: probed(5, Probe::HexAdder),
        guess: Guess::ByRunOf(Kind::Adder),
        role_words: ADDER_WORDS,
        digit_inputs: true,
        behaviour: ADDER,
        ..ROW
    },
    Facts {
        kind: Kind::Subtractor,
        name: "subtractor",
        label: "Subtractor",
        noun: "subtractor",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        designs: ADDER_DESIGNS,
        flags: &[
            CARRY_IN_FLAG,
            CARRY_OUT_FLAG,
            INVERTED_CARRY_IN_FLAG,
            INVERTED_CARRY_OUT_FLAG,
        ],
        listed_as: "subtractors",
        probe: Probing::As(Kind::Adder),
        guess: Guess::ByRunOf(Kind::Adder),
        behaviour: ADDER,
        ..ROW
    },
    // An adder whose sum reads inverted, as fits a data loop that wants it
    // so, its own category, as it is used apart from an adder.
    Facts {
        kind: Kind::InvertedAdder,
        name: "inverted_adder",
        label: "Inverted Adder",
        noun: "inverted adder",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        designs: ADDER_DESIGNS,
        flags: &[
            CARRY_IN_FLAG,
            CARRY_OUT_FLAG,
            INVERTED_CARRY_IN_FLAG,
            INVERTED_CARRY_OUT_FLAG,
        ],
        listed_as: "inverted adders",
        probe: Probing::As(Kind::Adder),
        guess: Guess::ByRunOf(Kind::Adder),
        role_words: ADDER_WORDS,
        behaviour: ADDER,
        ..ROW
    },
    Facts {
        kind: Kind::Alu,
        runs: Some(&alu::Operations),
        name: "alu",
        label: "ALU",
        noun: "ALU",
        plural: Some("ALUs"),
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        flags: ALU_FLAGS,
        listed_as: "ALUs",
        probe: probed(240, Probe::Alu),
        guess: Guess::Tried(270),
        // Its flags are the operations its test finds; it never stacks.
        behaviour: Behaviour {
            flags_from_file: true,
            ..evaluated(Test::Alu, Stacking::Never)
        },
        ..ROW
    },
    Facts {
        kind: Kind::Multiplier,
        runs: Some(&multiplier::MULTIPLIER),
        name: "multiplier",
        label: "Multiplier",
        noun: "multiplier",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        flags: &[
            flag("upper_product", "Upper product"),
            flag("fma", "FMA (A×B+C)"),
        ],
        listed_as: "multipliers",
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
        ..ROW
    },
    Facts {
        kind: Kind::Incrementer,
        runs: Some(&incrementer::INCREMENTER),
        name: "incrementer",
        label: "Incrementer",
        noun: "incrementer",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        aliases: &["inc"],
        listed_as: "incrementers",
        probe: matched(160),
        guess: Guess::Tried(150),
        role_words: &["increment", "inc", "carry out", "cout"],
        // Its copies chain as an adder's, its carry out the next's
        // increment.
        behaviour: lined(
            lone("increment", CARRY_OUT),
            Test::Incrementer,
            Stacking::Inferred,
        ),
        ..ROW
    },
    Facts {
        kind: Kind::IncDec,
        runs: Some(&incrementer::INC_DEC),
        name: "inc_dec",
        label: "Inc/Dec",
        noun: "inc/dec",
        plural: Some("Inc/Dec"),
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        listed_as: "incrementers",
        probe: matched(180),
        guess: Guess::ByRunOf(Kind::Incrementer),
        // Its line may mean either way, so it keeps its own name.
        behaviour: read(Test::IncDec),
        ..ROW
    },
    Facts {
        kind: Kind::Decrementer,
        runs: Some(&incrementer::DECREMENTER),
        name: "decrementer",
        label: "Decrementer",
        noun: "decrementer",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        aliases: &["dec"],
        listed_as: "decrementers",
        probe: matched(170),
        guess: Guess::ByRunOf(Kind::Incrementer),
        role_words: &["decrement", "dec", "borrow out", "bout"],
        behaviour: lined(
            lone("decrement", "borrow out"),
            Test::Decrementer,
            Stacking::Inferred,
        ),
        ..ROW
    },
    Facts {
        kind: Kind::Equality,
        runs: Some(&adder::EQUALITY),
        name: "equality",
        label: "Equality",
        noun: "equality comparator",
        plural: Some("Equality"),
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        aliases: &["comparator", "eq"],
        listed_as: "comparators",
        probe: matched(80),
        guess: Guess::Tried(240),
        behaviour: compares("A = B", Test::Equality),
        ..ROW
    },
    Facts {
        kind: Kind::GreaterThan,
        runs: Some(&adder::GREATER_THAN),
        name: "greater_than",
        label: "Greater Than",
        noun: "greater-than comparator",
        plural: Some("Greater Than"),
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        flags: &[flag("signed", "Signed")],
        aliases: &["comparator", "gt"],
        listed_as: "comparators",
        probe: matched(90),
        guess: Guess::Tried(230),
        behaviour: compares("A > B", Test::GreaterThan),
        ..ROW
    },
    Facts {
        kind: Kind::CarryOut,
        runs: Some(&adder::CARRY_OUT),
        name: "carry_out",
        label: "Carry Out",
        noun: "carry out",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        aliases: &["cout"],
        listed_as: "carry outs",
        probe: matched(100),
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
        ..ROW
    },
    // Copies of these make no wider one, so they never stack.
    Facts {
        kind: Kind::AbsoluteValue,
        runs: Some(&converters::ABSOLUTE_VALUE),
        name: "absolute_value",
        label: "Absolute Value",
        noun: "absolute value",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        aliases: &["abs"],
        listed_as: "absolute values",
        probe: matched(120),
        guess: Guess::Tried(210),
        behaviour: evaluated(Test::AbsoluteValue, Stacking::Never),
        ..ROW
    },
    // After an absolute value, which a 1-bit one, a wire, also is.
    Facts {
        kind: Kind::Negator,
        runs: Some(&negator::NEGATOR),
        name: "negator",
        label: "Negator",
        noun: "negator",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        aliases: &["neg", "negate", "twos complement"],
        listed_as: "negators",
        probe: matched(125),
        guess: Guess::Tried(215),
        role_words: &["negate", "neg"],
        behaviour: lined(
            Lines::Control("negate"),
            Test::Negator,
            Stacking::Never,
        ),
        ..ROW
    },
    Facts {
        kind: Kind::Popcount,
        runs: Some(&converters::POPCOUNT),
        name: "popcount",
        label: "Popcount",
        noun: "popcount",
        section: Section::Arithmetic,
        logic: Some(Logic::Stateless),
        listed_as: "popcounts",
        probe: one_operand(190, Line::Never),
        guess: Guess::Tried(120),
        behaviour: evaluated(Test::Popcount, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::Threshold,
        runs: Some(&converters::THRESHOLD),
        name: "threshold",
        label: "Threshold",
        noun: "threshold",
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        measures: &[Measure::Width, Measure::AtLeast],
        aliases: &["majority"],
        listed_as: "thresholds",
        probe: matched(210),
        guess: Guess::Tried(140),
        behaviour: evaluated(Test::Threshold, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::Parity,
        runs: Some(&converters::PARITY),
        name: "parity",
        label: "Parity",
        noun: "parity circuit",
        plural: Some("Parity Circuits"),
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        aliases: &["parity checker", "parity generator", "xor reduce"],
        listed_as: "parity circuits",
        probe: matched(215),
        guess: Guess::Tried(145),
        behaviour: evaluated(Test::Parity, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::BinaryToHex,
        runs: Some(&converters::BINARY_TO_HEX),
        name: "binary_to_hex",
        label: "Binary to Hex",
        noun: "binary to hex",
        plural: Some("Binary to Hex"),
        section: Section::Conversion,
        logic: Some(Logic::Stateless),
        listed_as: "binary-to-hex converters",
        probe: one_operand(200, Line::Never),
        guess: Guess::Tried(130),
        behaviour: evaluated(Test::BinaryToHex, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::HexToBinary,
        runs: Some(&converters::HEX_TO_BINARY),
        name: "hex_to_binary",
        label: "Hex to Binary",
        noun: "hex to binary",
        plural: Some("Hex to Binary"),
        section: Section::Conversion,
        logic: Some(Logic::Stateless),
        listed_as: "hex-to-binary converters",
        // Next to the hex adder, as only a build with a signal-strength
        // input can be one.
        probe: probed(6, Probe::HexToBinary),
        guess: Guess::Tried(135),
        digit_inputs: true,
        behaviour: evaluated(Test::HexToBinary, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::XorGate,
        runs: Some(&gate::XOR),
        name: "xor_gate",
        label: "XOR Gate",
        noun: "XOR gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["gate", "xor", "xor xnor gate"],
        listed_as: "gates",
        probe: matched(10),
        guess: Guess::Tried(60),
        behaviour: gate(Test::Xor),
        ..ROW
    },
    Facts {
        kind: Kind::XnorGate,
        runs: Some(&gate::XNOR),
        name: "xnor_gate",
        label: "XNOR Gate",
        noun: "XNOR gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["gate", "xnor", "xor xnor gate"],
        listed_as: "gates",
        probe: matched(20),
        guess: Guess::Tried(70),
        behaviour: gate(Test::Xnor),
        ..ROW
    },
    Facts {
        kind: Kind::AndGate,
        runs: Some(&gate::AND),
        name: "and_gate",
        label: "AND Gate",
        noun: "AND gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["and", "and nand gate", "gate"],
        listed_as: "gates",
        probe: matched(30),
        guess: Guess::Tried(80),
        behaviour: gate(Test::And),
        ..ROW
    },
    Facts {
        kind: Kind::NandGate,
        runs: Some(&gate::NAND),
        name: "nand_gate",
        label: "NAND Gate",
        noun: "NAND gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["and nand gate", "gate", "nand"],
        listed_as: "gates",
        probe: matched(40),
        guess: Guess::Tried(90),
        behaviour: gate(Test::Nand),
        ..ROW
    },
    Facts {
        kind: Kind::OrGate,
        runs: Some(&gate::OR),
        name: "or_gate",
        label: "OR Gate",
        noun: "OR gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["gate", "or", "or nor gate"],
        listed_as: "gates",
        probe: matched(50),
        guess: Guess::Tried(100),
        behaviour: gate(Test::Or),
        ..ROW
    },
    Facts {
        kind: Kind::NorGate,
        runs: Some(&gate::NOR),
        name: "nor_gate",
        label: "NOR Gate",
        noun: "NOR gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["gate", "nor", "or nor gate"],
        listed_as: "gates",
        probe: matched(60),
        guess: Guess::Tried(110),
        behaviour: gate(Test::Nor),
        ..ROW
    },
    Facts {
        kind: Kind::NandXnorGate,
        runs: Some(&gate::NAND_XNOR_GATE),
        name: "nand_xnor_gate",
        label: "NAND+XNOR Gate",
        noun: "NAND+XNOR gate",
        section: Section::Gates,
        logic: Some(Logic::Stateless),
        aliases: &["gate"],
        listed_as: "gates",
        probe: matched(70),
        guess: Guess::Tried(30),
        behaviour: gate(Test::NandXnor),
        ..ROW
    },
    Facts {
        kind: Kind::Inverter,
        runs: Some(&inverter::INVERTER),
        name: "inverter",
        label: "Inverter",
        noun: "inverter",
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        aliases: &["conditional inverter", "controlled inverter", "xor bank"],
        listed_as: "inverters",
        probe: matched(110),
        guess: Guess::Tried(50),
        role_words: &["invert", "inv"],
        // Its copies add bits, sharing its control line.
        behaviour: Behaviour {
            group: Group::SharedLines,
            ..lined(
                Lines::Control("invert"),
                Test::Inverter,
                Stacking::Inferred,
            )
        },
        ..ROW
    },
    Facts {
        kind: Kind::Decoder,
        runs: Some(&decoder::DECODER),
        name: "decoder",
        label: "Decoder",
        noun: "decoder",
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        listed_as: "decoders",
        probe: matched(250),
        guess: Guess::Tried(20),
        // Its copies side by side are separate decoders, each with its own
        // input and outputs, not a wider one, so it never stacks.
        behaviour: evaluated(Test::Decoder, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::BarrelShifter,
        runs: Some(&shifter::Shifts),
        name: "barrel_shifter",
        label: "Barrel Shifter",
        noun: "barrel shifter",
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        flags: &[
            flag("bidirectional", "Bidirectional"),
            flag("rotate", "Rotate"),
            flag("arithmetic", "Arithmetic"),
        ],
        listed_as: "barrel shifters",
        probe: probed(260, Probe::Shifter),
        guess: Guess::Tried(40),
        behaviour: evaluated(Test::BarrelShifter, Stacking::Never),
        ..ROW
    },
    Facts {
        kind: Kind::Monostable,
        runs: Some(&monostable::Monostable { dual: false }),
        name: "monostable",
        label: "Monostable",
        noun: "monostable",
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        measures: &[Measure::Pulse],
        flags: &[
            flag("inverted", "Inverted"),
            flag("falling_edge", "Falling edge"),
        ],
        aliases: &["mono", "pulse generator"],
        listed_as: "monostables",
        probe: one_operand(130, Line::Never),
        guess: Guess::Tried(250),
        behaviour: monostable(Test::Monostable),
        ..ROW
    },
    Facts {
        kind: Kind::DualEdgeMonostable,
        runs: Some(&monostable::Monostable { dual: true }),
        name: "dual_edge_monostable",
        label: "Dual-Edge Monostable",
        noun: "dual-edge monostable",
        section: Section::Logic,
        logic: Some(Logic::Stateless),
        measures: &[Measure::Pulse],
        flags: &[flag("inverted", "Inverted")],
        aliases: &["mono", "monostable", "pulse generator"],
        listed_as: "monostables",
        probe: one_operand(140, Line::Never),
        guess: Guess::Tried(260),
        behaviour: monostable(Test::DualEdgeMonostable),
        ..ROW
    },
    Facts {
        kind: Kind::Register,
        runs: Some(&register::Kind::Register),
        name: "register",
        label: "Register",
        noun: "register",
        section: Section::Memory,
        logic: Some(Logic::Stateful),
        flags: &[flag("inverted", "Inverted reads")],
        listed_as: "registers",
        probe: probed(220, Probe::Register),
        guess: Guess::Tried(190),
        role_words: &["write", "read", "write enable", "data in", "data out"],
        behaviour: Behaviour {
            lines: Lines::Register,
            copies: Copies::SharedBuses,
            group: Group::SharedLines,
            ..evaluated(Test::Register, Stacking::Inferred)
        },
        ..ROW
    },
    Facts {
        kind: Kind::Buffer,
        runs: Some(&register::Kind::Buffer),
        name: "buffer",
        label: "Buffer",
        noun: "buffer",
        section: Section::Memory,
        logic: Some(Logic::Stateful),
        listed_as: "buffers",
        probe: probed(230, Probe::Register),
        guess: Guess::Tried(200),
        role_words: &["write", "write enable", "data in", "data out"],
        // Its copies add bits, sharing its control lines, as its cells
        // cannot share a bus.
        behaviour: Behaviour {
            group: Group::SharedLines,
            ..lined(Lines::Buffer, Test::Buffer, Stacking::Inferred)
        },
        ..ROW
    },
    Facts {
        kind: Kind::Counter,
        runs: Some(&sequential::Kind::Counter),
        name: "counter",
        label: "Counter",
        noun: "counter",
        section: Section::Memory,
        logic: Some(Logic::Stateful),
        listed_as: "counters",
        probe: probed(280, Probe::Sequential),
        guess: Guess::Tried(280),
        role_words: &["clock", "clk", "write", "write enable", "load"],
        behaviour: Behaviour {
            lines: Lines::Counter,
            ..held(Test::Counter)
        },
        ..ROW
    },
    Facts {
        kind: Kind::ShiftRegister,
        runs: Some(&sequential::Kind::ShiftRegister),
        name: "shift_register",
        label: "Shift Register",
        noun: "shift register",
        section: Section::Memory,
        logic: Some(Logic::Stateful),
        flags: &[
            flag("bidirectional", "Bidirectional"),
            flag("writeable", "Writeable"),
        ],
        listed_as: "shift registers",
        probe: probed(290, Probe::Sequential),
        guess: Guess::Tried(290),
        behaviour: held(Test::ShiftRegister),
        ..ROW
    },
    Facts {
        kind: Kind::TFlipFlop,
        runs: Some(&sequential::Kind::TFlipFlop),
        name: "t_flip_flop",
        label: "T Flip-Flop",
        noun: "T flip-flop",
        section: Section::Memory,
        logic: Some(Logic::Stateful),
        // Its one bit names nothing, so it is ranked by nothing.
        measures: &[],
        aliases: &["flip flop", "tff", "toggle"],
        listed_as: "T flip-flops",
        probe: probed(300, Probe::Sequential),
        guess: Guess::Tried(300),
        role_words: &["toggle", "t", "clock", "clk", "q"],
        // Its lone control line needs no sign: it toggles the bit held.
        // Its copies add bits, each a cell with its own toggle and bit.
        behaviour: Behaviour {
            lines: Lines::Control("toggle"),
            copies: Copies::OwnOutputs,
            group: Group::Cells,
            stacking: Stacking::Inferred,
            ..held(Test::TFlipFlop)
        },
        ..ROW
    },
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
        assert!(
            !row.name.is_empty() && !row.label.is_empty(),
            "a row unnamed"
        );
        let tested = row.behaviour.test.is_some();
        // The category a test is made for says how it runs, and no other.
        let own = match row.behaviour.test {
            Some(test) => test.kind() as usize == i,
            None => false,
        };
        assert!(row.runs.is_some() == own, "a row's test runs elsewhere");
        if let Probing::Tried { .. } = row.probe {
            assert!(tested, "a kind probed with no test");
        }
        match row.guess {
            Guess::Tried(_) => assert!(tested, "a kind guessed with no test"),
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
