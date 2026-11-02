//! What the code does with a build of each category: which simulation test
//! it gets, what is read from its file, which of its lines may go unsigned,
//! and how its copies stack. The categories themselves are rows in the
//! database, which people may add to; only those the code knows something
//! about have a row in `behaviour`, and every other gets the default,
//! nothing special.

use crate::simulate::Test;

/// What the code does with builds of one category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Behaviour {
    /// The simulation test its builds get, when they are as wide as the
    /// simulator takes; see `Test::of`.
    pub test: Option<Test>,
    /// What of a conventional build's file its width, orientation, and
    /// encoding are read from, rather than typed.
    pub reads: Reads,
    /// Whether a conventional build's flags are read from its file rather
    /// than ticked: an adder's carry in and carry out, or a subtractor's
    /// borrow in and borrow out, from its lone control and status lines,
    /// each flag named as its `Lines::Lone` line is.
    pub flags_from_file: bool,
    /// What its control and status lines mean without signs.
    pub lines: Lines,
    pub stacking: Stacking,
    /// What its copies stacked do with its buses and lines.
    pub copies: Copies,
    /// Whether a conventional build's every measured detail comes from
    /// `evaluate` rather than being typed: its stacking, latency,
    /// synchronized, throughput, and design. Such a build must pass its
    /// test to be one of the category; one that fails goes up as other.
    pub evaluated: bool,
}

/// What a category reads its builds' width, orientation, and encoding
/// from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reads {
    /// Nothing: they are typed.
    #[default]
    Nothing,
    /// Input A, its operand.
    InputA,
    /// Its widest output, the value it holds, as a counter's, which may
    /// have no A.
    Output,
}

/// The blessed categories: a build of one is recognized by probing its
/// levers and outputs, whatever colours they sit on, then coloured and
/// standardized, and loses its unused blocks when uploaded, as a new
/// version once the circuit is shown unchanged. Every other build keeps
/// the blocks it came with.
pub const BLESSED: [&str; 32] = [
    "adder",
    "hex_adder",
    "subtractor",
    "register",
    "alu",
    "decoder",
    "multiplier",
    "counter",
    "shift_register",
    "xor_gate",
    "xnor_gate",
    "and_gate",
    "nand_gate",
    "or_gate",
    "nor_gate",
    "nand_xnor_gate",
    "equality",
    "greater_than",
    "carry_out",
    "inverter",
    "absolute_value",
    "monostable",
    "dual_edge_monostable",
    "incrementer",
    "decrementer",
    "inc_dec",
    "buffer",
    "barrel_shifter",
    "popcount",
    "threshold",
    "binary_to_hex",
    "t_flip_flop",
];

/// What a category's unsigned control and status lines are taken to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lines {
    /// Every line needs a sign naming it.
    #[default]
    Signed,
    /// Its lone control line and lone status line need no sign, their roles
    /// being plain, and go by these names, such as an adder's carry in and
    /// carry out. With `lone_output`, its one output is the status line
    /// whatever its colour, as a carry out's is.
    Lone {
        control: &'static str,
        status: &'static str,
        lone_output: bool,
    },
    /// A register's: its unsigned magenta lines are its cells' writes, each
    /// driven by a button, and reads, by a lever, and its data bus in and
    /// out go by those roles.
    Register,
    /// A comparison's: its one output, yellow or pink, is its result, which
    /// needs no sign and goes by this name, such as `A > B`.
    Result(&'static str),
    /// An inverter's, multiplier's, or T flip-flop's: its lone control line
    /// needs no sign and goes by this name, such as `invert` or
    /// `calculate`; it has no status line.
    Control(&'static str),
    /// A buffer's: its data bus in and out as a register's, and its
    /// unsigned magenta lines its cells' writes, by a button, or by a lever
    /// when none has a button, as it has no reads; a lone one needs no
    /// sign.
    Buffer,
    /// A counter's: each magenta line its clock, driven by a button, or its
    /// write enable, by a lever, whatever its sign says.
    Counter,
}

/// What a category's copies stacked do with its data buses and its
/// labelled outputs and status lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Copies {
    /// Each keeps its own data buses, and the next takes over its labelled
    /// outputs and status lines, as an adder's carry out.
    #[default]
    Chain,
    /// Copies across its bits share its data buses, as more registers or
    /// words on one bus, rather than adding bits.
    SharedBuses,
    /// Each keeps its labelled outputs and status lines too, as a
    /// monostable's lone output is its own bit's.
    OwnOutputs,
}

/// Who decides how a category's builds stack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stacking {
    /// Their builders, as they give it.
    #[default]
    Given,
    /// The simulation, for a conventional build: copies that work as their
    /// bits suggest make it stackable that way, and none that do make it
    /// not stack, whatever was given; see `Test::stacking`.
    Inferred,
    /// Nobody: its builds are never stacked, as a multiplier's rarely are
    /// to any use, so stacking is neither asked for, searched, nor kept.
    Never,
}

/// A category with test `test` and nothing else special.
fn tested(test: Test) -> Behaviour {
    Behaviour {
        test: Some(test),
        ..Behaviour::default()
    }
}

/// A category whose input A is its operand, which its width is read from.
fn read(test: Test) -> Behaviour {
    Behaviour {
        reads: Reads::InputA,
        ..tested(test)
    }
}

/// A category whose every detail `evaluate` works out, stacked so.
fn evaluated(test: Test, stacking: Stacking) -> Behaviour {
    Behaviour {
        stacking,
        evaluated: true,
        ..read(test)
    }
}

/// As `evaluated`, its lines meaning what `lines` says.
fn lined(lines: Lines, test: Test, stacking: Stacking) -> Behaviour {
    Behaviour {
        lines,
        ..evaluated(test, stacking)
    }
}

/// A comparison's: its one output is its result, named `name`.
fn compares(name: &'static str, test: Test) -> Behaviour {
    lined(Lines::Result(name), test, Stacking::Never)
}

/// A counter's or shift register's: its width is the value it holds.
fn held(test: Test) -> Behaviour {
    Behaviour {
        reads: Reads::Output,
        ..evaluated(test, Stacking::Never)
    }
}

/// A monostable's: its lone output needs no sign, and its copies add bits,
/// each with its own.
fn monostable(test: Test) -> Behaviour {
    Behaviour {
        copies: Copies::OwnOutputs,
        ..lined(Lines::Result("pulse"), test, Stacking::Inferred)
    }
}

/// Lone lines named `control` and `status`, needing no signs.
fn lone(control: &'static str, status: &'static str) -> Lines {
    Lines::Lone {
        control,
        status,
        lone_output: false,
    }
}

/// What the code does with builds of the category named `category`, such
/// as `adder`; the default for one it knows nothing about.
pub fn behaviour(category: &str) -> Behaviour {
    match category {
        // A hex adder is an adder whose inputs each take a hex digit per
        // block, and a subtractor one whose B is subtracted, each tested
        // and read the same way.
        "adder" | "hex_adder" | "subtractor" => Behaviour {
            flags_from_file: true,
            lines: lone(CARRY_IN, CARRY_OUT),
            stacking: Stacking::Inferred,
            evaluated: true,
            ..read(Test::Adder)
        },
        // Its copies chain as an adder's, its carry out the next's carry in.
        "carry_out" => Behaviour {
            lines: Lines::Lone {
                control: CARRY_IN,
                status: CARRY_OUT,
                lone_output: true,
            },
            ..evaluated(Test::CarryOut, Stacking::Inferred)
        },
        // Its copies chain as an adder's, its carry out the next's
        // increment.
        "incrementer" => lined(
            lone("increment", CARRY_OUT),
            Test::Incrementer,
            Stacking::Inferred,
        ),
        "decrementer" => lined(
            lone("decrement", "borrow out"),
            Test::Decrementer,
            Stacking::Inferred,
        ),
        // Its line may mean either way, so it keeps its own name.
        "inc_dec" => read(Test::IncDec),
        // Its flags are the operations its test finds; it never stacks.
        "alu" => Behaviour {
            flags_from_file: true,
            ..evaluated(Test::Alu, Stacking::Never)
        },
        // Copies of these make no wider one, so they never stack.
        "absolute_value" => evaluated(Test::AbsoluteValue, Stacking::Never),
        "popcount" => evaluated(Test::Popcount, Stacking::Never),
        "binary_to_hex" => evaluated(Test::BinaryToHex, Stacking::Never),
        "threshold" => evaluated(Test::Threshold, Stacking::Never),
        "greater_than" => compares("A > B", Test::GreaterThan),
        "equality" => compares("A = B", Test::Equality),
        // Its copies add bits, each with its own A, B, and output.
        "xor_gate" => evaluated(Test::Xor, Stacking::Inferred),
        "xnor_gate" => evaluated(Test::Xnor, Stacking::Inferred),
        "and_gate" => evaluated(Test::And, Stacking::Inferred),
        "nand_gate" => evaluated(Test::Nand, Stacking::Inferred),
        "or_gate" => evaluated(Test::Or, Stacking::Inferred),
        "nor_gate" => evaluated(Test::Nor, Stacking::Inferred),
        "nand_xnor_gate" => evaluated(Test::NandXnor, Stacking::Inferred),
        // Its copies are more decoders, each with its own input and outputs.
        "decoder" => Behaviour {
            copies: Copies::OwnOutputs,
            ..evaluated(Test::Decoder, Stacking::Inferred)
        },
        "barrel_shifter" => evaluated(Test::BarrelShifter, Stacking::Never),
        "register" => Behaviour {
            lines: Lines::Register,
            copies: Copies::SharedBuses,
            ..evaluated(Test::Register, Stacking::Inferred)
        },
        // Its copies add bits, as its cells cannot share a bus.
        "buffer" => lined(Lines::Buffer, Test::Buffer, Stacking::Inferred),
        "monostable" => monostable(Test::Monostable),
        "dual_edge_monostable" => monostable(Test::DualEdgeMonostable),
        // Its copies add bits, sharing its control line.
        "inverter" => {
            lined(Lines::Control("invert"), Test::Inverter, Stacking::Inferred)
        }
        "counter" => Behaviour {
            lines: Lines::Counter,
            ..held(Test::Counter)
        },
        "shift_register" => held(Test::ShiftRegister),
        // Its lone control line needs no sign: it toggles the bit held.
        "t_flip_flop" => Behaviour {
            lines: Lines::Control("toggle"),
            ..held(Test::TFlipFlop)
        },
        "multiplier" => Behaviour {
            flags_from_file: true,
            stacking: Stacking::Never,
            evaluated: true,
            lines: Lines::Control("calculate"),
            ..read(Test::Multiplier)
        },
        "binary_to_bcd" => tested(Test::BinaryToBcd),
        "bcd_to_binary" => tested(Test::BcdToBinary),
        "ram" => Behaviour {
            copies: Copies::SharedBuses,
            ..Behaviour::default()
        },
        _ => Behaviour::default(),
    }
}

/// The name an unsigned carry in goes by; see `colour::CARRY_IN`.
const CARRY_IN: &str = crate::recognize::colour::CARRY_IN[0];

/// The name an unsigned carry out goes by; see `colour::CARRY_OUT`.
const CARRY_OUT: &str = crate::recognize::colour::CARRY_OUT[0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_test_is_its_categorys() {
        for test in Test::ALL {
            assert_eq!(behaviour(test.name()).test, Some(test));
        }
        assert_eq!(behaviour("ram").test, None);
        assert_eq!(behaviour("piston_door"), Behaviour::default());
    }
}
