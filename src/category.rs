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
    /// Whether a conventional build's width, orientation, and encoding are
    /// read from input A in its file rather than typed.
    pub reads_file: bool,
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
    /// An inverter's: its lone control line needs no sign and goes by this
    /// name, `invert`; it has no status line.
    Control(&'static str),
    /// A buffer's: its data bus in and out as a register's, and its
    /// unsigned magenta lines its cells' writes, by a button, or by a lever
    /// when none has a button, as it has no reads; a lone one needs no
    /// sign.
    Buffer,
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
    /// The simulation when it finds copies that work, its builders
    /// otherwise: a register's copies may stack in ways the search does not
    /// try, as a 1-bit cell's do into wider registers.
    Found,
    /// Nobody: its builds are never stacked, as a multiplier's rarely are
    /// to any use, so stacking is neither asked for, searched, nor kept.
    Never,
}

/// What the code does with builds of the category named `category`, such
/// as `adder`; the default for one it knows nothing about.
pub fn behaviour(category: &str) -> Behaviour {
    let tested = |test| Behaviour {
        test: Some(test),
        ..Behaviour::default()
    };
    // A category whose input A is its operand reads its width and the way
    // its bits run from it.
    let read = |test| Behaviour {
        reads_file: true,
        ..tested(test)
    };
    // A category whose every detail `evaluate` works out, its stacking as
    // `stacking` says.
    let evaluated = |test, stacking| Behaviour {
        stacking,
        evaluated: true,
        ..read(test)
    };
    // A monostable's lone output needs no sign; its copies add bits, each
    // with its own.
    let monostable = |test| Behaviour {
        lines: Lines::Result("pulse"),
        copies: Copies::OwnOutputs,
        ..evaluated(test, Stacking::Inferred)
    };
    let lone = |control, status| Lines::Lone {
        control,
        status,
        lone_output: false,
    };
    match category {
        "adder" => Behaviour {
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
        "incrementer" => Behaviour {
            lines: lone("increment", CARRY_OUT),
            ..evaluated(Test::Incrementer, Stacking::Inferred)
        },
        "decrementer" => Behaviour {
            lines: lone("decrement", "borrow out"),
            ..evaluated(Test::Decrementer, Stacking::Inferred)
        },
        // Its line may mean either way, so it keeps its own name.
        "inc_dec" => read(Test::IncDec),
        // Its flags are the operations its test finds; copies of it make
        // no wider one, so it never stacks.
        "alu" => Behaviour {
            flags_from_file: true,
            ..evaluated(Test::Alu, Stacking::Never)
        },
        // Copies of these make no wider one, so they never stack.
        "absolute_value" => evaluated(Test::AbsoluteValue, Stacking::Never),
        "greater_than" => Behaviour {
            lines: Lines::Result("A > B"),
            ..evaluated(Test::GreaterThan, Stacking::Never)
        },
        "equality" => Behaviour {
            lines: Lines::Result("A = B"),
            ..evaluated(Test::Equality, Stacking::Never)
        },
        // Its copies add bits, each with its own A, B, and output.
        "xor_xnor_gate" => evaluated(Test::XorXnor, Stacking::Inferred),
        "nand_xnor_gate" => evaluated(Test::NandXnor, Stacking::Inferred),
        // Copies of these make no wider one, so they never stack.
        "decoder" => evaluated(Test::Decoder, Stacking::Never),
        "barrel_shifter" => evaluated(Test::BarrelShifter, Stacking::Never),
        "register" => Behaviour {
            lines: Lines::Register,
            copies: Copies::SharedBuses,
            ..evaluated(Test::Register, Stacking::Inferred)
        },
        // Its copies add bits, as its cells cannot share a bus.
        "buffer" => Behaviour {
            lines: Lines::Buffer,
            ..evaluated(Test::Buffer, Stacking::Inferred)
        },
        "monostable" => monostable(Test::Monostable),
        "dual_edge_monostable" => monostable(Test::DualEdgeMonostable),
        // Its copies add bits, sharing its control line.
        "inverter" => Behaviour {
            lines: Lines::Control("invert"),
            ..evaluated(Test::Inverter, Stacking::Inferred)
        },
        "multiplier" => Behaviour {
            flags_from_file: true,
            stacking: Stacking::Never,
            evaluated: true,
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

/// The name an unsigned carry in goes by; see `lint::CARRY_IN`.
const CARRY_IN: &str = crate::lint::CARRY_IN[0];

/// The name an unsigned carry out goes by; see `lint::CARRY_OUT`.
const CARRY_OUT: &str = crate::lint::CARRY_OUT[0];

impl Lines {
    /// Whether a lone control or status line may go without a sign.
    pub fn lone_unsigned(self) -> bool {
        matches!(
            self,
            Lines::Lone { .. }
                | Lines::Buffer
                | Lines::Result(_)
                | Lines::Control(_)
        )
    }
}

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
