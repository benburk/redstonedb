//! Every standard category, as a [`Kind`], and what the code does with
//! each one's builds: which simulation test they get, how probing finds
//! one, whether guessing does, what is read from their files, which of
//! their lines may go unsigned, and how their copies stack. Each kind's
//! facts are one row of one table, so adding a category is a row there,
//! with its test when it has one. A build may also name a category off
//! the list, in the same form, which the code knows nothing about: it gets
//! the default [`Behaviour`], nothing special.

mod table;

use crate::{recognize::probe::Line, simulate::Test};

/// A standard category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Adder,
    HexAdder,
    Subtractor,
    Alu,
    Multiplier,
    Incrementer,
    IncDec,
    Decrementer,
    Equality,
    GreaterThan,
    CarryOut,
    AbsoluteValue,
    Popcount,
    Threshold,
    Parity,
    BinaryToHex,
    XorGate,
    XnorGate,
    AndGate,
    NandGate,
    OrGate,
    NorGate,
    NandXnorGate,
    Inverter,
    Decoder,
    BarrelShifter,
    Monostable,
    DualEdgeMonostable,
    Register,
    Buffer,
    Counter,
    ShiftRegister,
    TFlipFlop,
    BcdToBinary,
    BinaryToBcd,
    Bussing,
    CarrySaveAdder,
    Dataloop,
    Display,
    Divider,
    HexToBinary,
    LineDrawer,
    NandNorGate,
    Ram,
    Rng,
    SerialAdder,
    SerialWriteMemory,
}

/// What the code knows of one standard category: its row in the table.
#[derive(Debug)]
pub struct Facts {
    /// The kind it is the row of, which is its place in the table.
    pub kind: Kind,
    /// As it is stored, such as `hex_adder`.
    pub name: &'static str,
    /// As people read it, such as `Hex Adder`.
    pub label: &'static str,
    /// A build of it in a sentence, without its article, such as
    /// `equality comparator`.
    pub noun: &'static str,
    /// For a blessed category, how the kinds of build that go on the list
    /// name it among them, such as `comparators`, which several share;
    /// `None` for any other. A blessed category's build is recognized by
    /// probing its levers and outputs, whatever colours they sit on, then
    /// coloured and standardized, and loses its unused blocks when
    /// uploaded, as a new version once the circuit is shown unchanged.
    /// Every other build keeps the blocks it came with.
    pub listed_as: Option<&'static str>,
    /// Whether and when probing tries it.
    pub probe: Probing,
    /// Whether and when guessing tries it.
    pub guess: Guess,
    /// The words a sign may name a role by that probing finds anyway, which
    /// it takes out of a probed build; an ALU's, a decoder's, and a shift
    /// register's signs are kept, as they tell what their lines do.
    pub role_words: &'static [&'static str],
    pub behaviour: Behaviour,
}

/// Whether and when probing tries a kind, a build being taken for the
/// first kind it holds as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probing {
    /// Never: nothing it is meant as is probed, as a hex adder's inputs,
    /// which take hex digits, are not, or it is no blessed category.
    Not,
    /// In `order`, the lowest first, by `how`. Gates go first, as a 1-bit
    /// XOR is a sum without a carry, an inverter's bit, and a 1-bit AND a
    /// product of one bit each, a 1-bit XNOR an equality, and a 1-bit
    /// NAND+XNOR an adder's inverted, which no wider build of those is; an
    /// adder before an incrementer, as a 1-bit adder is a 1-bit
    /// incrementer.
    Tried { order: u16, how: Probe },
    /// As this other kind, whose probe finds it, as an adder's finds a
    /// subtractor.
    As(Kind),
}

/// How probing finds a kind's roles; each runs the kind's test on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    /// An adder's operands, carry in, sum, and carry out.
    Adder,
    /// A register's or buffer's data buses and its cells' controls.
    Register,
    Shifter,
    Alu,
    Decoder,
    Multiplier,
    /// Two operands of one width, A and B, as a gate's or comparison's.
    Operands,
    /// One operand, A, with a control line as `Line` says.
    OneOperand(Line),
    /// A counter's, shift register's, or T flip-flop's held value and
    /// controls.
    Sequential,
}

/// Whether and when `simulate::guess` tries a kind's test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guess {
    /// Never: it has no test.
    Not,
    /// Its test is tried, a tie going to the lowest `order`. An adder goes
    /// first. An inverter comes before an incrementer, as a 1-bit
    /// incrementer without a carry out is A xor its line, as a 1-bit
    /// inverter is. An XNOR gate comes before an equality comparator, as a
    /// 1-bit one is A xnor B. A popcount and a threshold come before the
    /// BCD converters, whose telling values a popcount's strength output
    /// can pass. A parity comes after the popcount and the threshold, as a
    /// 1-bit odd one is a wire, as theirs are, so a wire is still taken for
    /// a popcount; a 1-bit even one is a NOT gate, and from 2 bits it gives
    /// A's bits xor or xnor, as neither does.
    Tried(u16),
    /// Found by this other kind's run, which tells which it is: a hex
    /// adder and a subtractor by the adder's, a decrementer and an inc/dec
    /// by the incrementer's, which tries every way of counting.
    ByRunOf(Kind),
}

impl Kind {
    /// Its row in the table.
    pub const fn facts(self) -> &'static Facts {
        &table::TABLE[self as usize]
    }

    /// Every kind, in the table's order: the blessed categories as the
    /// list names them, then the rest.
    pub fn all() -> impl Iterator<Item = Kind> {
        table::TABLE.iter().map(|facts| facts.kind)
    }

    /// The kind named `name`, such as `adder`; `None` for a category off
    /// the list.
    pub fn named(name: &str) -> Option<Kind> {
        table::TABLE
            .iter()
            .find(|facts| facts.name == name)
            .map(|facts| facts.kind)
    }

    pub fn name(self) -> &'static str {
        self.facts().name
    }

    pub fn label(self) -> &'static str {
        self.facts().label
    }

    /// The simulation test its builds get, when they are as wide as the
    /// simulator takes; see `Test::of`.
    pub fn test(self) -> Option<Test> {
        self.facts().behaviour.test
    }

    /// Whether it is a blessed category; see `Facts::listed_as`.
    pub fn blessed(self) -> bool {
        self.facts().listed_as.is_some()
    }

    /// A build of it in a sentence, such as `an equality comparator`.
    pub fn a(self) -> String {
        let noun = self.facts().noun;
        let article =
            if noun.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'O', 'X']) {
                "an"
            } else {
                "a"
            };
        format!("{article} {noun}")
    }

    /// The kinds probing tries, in order, each with how and its test.
    pub fn probed() -> Vec<(Kind, Probe, Test)> {
        let mut probed: Vec<(u16, Kind, Probe, Test)> = table::TABLE
            .iter()
            .filter_map(|facts| match (facts.probe, facts.behaviour.test) {
                (Probing::Tried { order, how }, Some(test)) => {
                    Some((order, facts.kind, how, test))
                }
                _ => None,
            })
            .collect();
        probed.sort_by_key(|&(order, ..)| order);
        probed
            .into_iter()
            .map(|(_, kind, how, test)| (kind, how, test))
            .collect()
    }

    /// The kinds whose tests guessing tries, in order, each with its test.
    pub fn guessed() -> Vec<(Kind, Test)> {
        let mut guessed: Vec<(u16, Kind, Test)> = table::TABLE
            .iter()
            .filter_map(|facts| match (facts.guess, facts.behaviour.test) {
                (Guess::Tried(order), Some(test)) => {
                    Some((order, facts.kind, test))
                }
                _ => None,
            })
            .collect();
        guessed.sort_by_key(|&(order, ..)| order);
        guessed
            .into_iter()
            .map(|(_, kind, test)| (kind, test))
            .collect()
    }

    /// The kinds of build that go on the list, which are the kinds the site
    /// recognizes and simulates, as a list in a sentence: `adders, hex
    /// adders, …, and T flip-flops`.
    pub fn listed() -> String {
        let mut kinds: Vec<&str> = Vec::new();
        for listed in table::TABLE.iter().filter_map(|f| f.listed_as) {
            if kinds.last() != Some(&listed) {
                kinds.push(listed);
            }
        }
        let (last, rest) = kinds.split_last().expect("some kinds are listed");
        format!("{}, and {last}", rest.join(", "))
    }
}

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

impl Behaviour {
    /// What the code does with builds of `kind`; the default for a category
    /// off the list, `None`.
    pub fn of(kind: impl Into<Option<Kind>>) -> Behaviour {
        kind.into()
            .map_or_else(Behaviour::default, |kind| kind.facts().behaviour)
    }
}

/// What the code does with builds of the category named `category`, such
/// as `adder`; the default for one off the list.
pub fn behaviour(category: &str) -> Behaviour {
    Behaviour::of(Kind::named(category))
}

/// Whether the category named `category` is a blessed one; see
/// `Facts::listed_as`.
pub fn blessed(category: &str) -> bool {
    Kind::named(category).is_some_and(Kind::blessed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_test_is_its_categorys() {
        for test in Test::all() {
            assert_eq!(test.kind().test(), Some(test));
            assert_eq!(behaviour(test.name()).test, Some(test));
        }
        assert_eq!(behaviour("ram").test, None);
        assert_eq!(behaviour("piston_door"), Behaviour::default());
    }

    /// Every kind is named back by its name, and stands in the table at
    /// its own place, which `facts` reads by.
    #[test]
    fn each_kind_is_named_by_its_name() {
        for kind in Kind::all() {
            assert_eq!(kind.facts().kind, kind);
            assert_eq!(Kind::named(kind.name()), Some(kind));
        }
    }

    /// Kinds that share a name on the list stand together, so the list
    /// names each once.
    #[test]
    fn the_list_names_each_kind_once() {
        let listed: Vec<&str> =
            Kind::all().filter_map(|k| k.facts().listed_as).collect();
        let mut seen: Vec<&str> = Vec::new();
        for name in listed {
            if seen.last() != Some(&name) {
                assert!(!seen.contains(&name), "{name} is split");
                seen.push(name);
            }
        }
    }

    /// Probing and guessing try each kind with a test once, and each other
    /// kind by the run of one tried.
    #[test]
    fn every_test_is_tried() {
        for kind in Kind::all() {
            let facts = kind.facts();
            if let Probing::As(other) = facts.probe {
                assert!(matches!(other.facts().probe, Probing::Tried { .. }));
            }
            match facts.guess {
                Guess::Not => assert_eq!(kind.test(), None, "{kind:?}"),
                Guess::Tried(_) => assert!(kind.test().is_some()),
                Guess::ByRunOf(other) => {
                    assert!(matches!(other.facts().guess, Guess::Tried(_)));
                }
            }
        }
        // None is left out for want of a test.
        let probes = Kind::all()
            .filter(|k| matches!(k.facts().probe, Probing::Tried { .. }));
        assert_eq!(Kind::probed().len(), probes.count());
        let guesses =
            Kind::all().filter(|k| matches!(k.facts().guess, Guess::Tried(_)));
        assert_eq!(Kind::guessed().len(), guesses.count());
    }

    /// Only a blessed category is probed, and every one is found by
    /// probing or meant as one, but for a hex adder.
    #[test]
    fn probing_finds_the_blessed_categories() {
        for kind in Kind::all() {
            let probed = kind.facts().probe != Probing::Not;
            assert_eq!(probed, kind.blessed() && kind != Kind::HexAdder);
        }
    }
}
