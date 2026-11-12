//! A build's inputs and outputs, read from its blocks and signs. Which
//! blocks are inputs and outputs the rule [`crate::interface::inspect`]
//! checks says, colour playing no part: an input is a block with a lever on
//! it, or for a control line a button, or for a signal-strength input the
//! block under a comparator with air behind it; an output is the block
//! under a repeater or comparator facing air.
//!
//! What each line is for, its role, is no block's colour: it is the role
//! probing found for each block, which the site keeps with each version of
//! a build's file, see [`crate::schem::lines`], or for a file carrying
//! none, the role its shape gives it, as [`repeater_outputs::interface`]
//! reads it. Each role is one bus, named by a sign on any of its blocks,
//! an output's on the face under its front, see [`line_signs`], except
//! control lines and status lines, each one line named by its own sign, or
//! a row of them named by one sign on any of the row, as a memory's read
//! lines are. An output or signal-strength input with no role plays none,
//! as one a stacked copy hands on to the next; a lever or button with
//! none is a bus of its own. Each bus's least significant bit, and a
//! row's first line, is the one nearest the south-east-bottom corner of
//! the bounding box, unless an `lsb` or `msb` sign says otherwise; such a
//! sign on one bus orders every unmarked bus running alongside it too.

pub mod colour;
mod facts;
pub mod inspect;
pub mod repeater_outputs;

pub use facts::{Encoding, Orientation, orientation, suggested};

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::schem::{
    self, Blocks, Direction, Pos, Switch,
    lines::{Role, Roles},
};

#[derive(Default)]
pub struct Interface {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    /// Whether its outputs give a bit a block though it takes a
    /// signal-strength input, as a hex to binary converter's do; any other
    /// build that takes one shows a digit on each block of every output but
    /// a status line, as a hex adder's sum does.
    pub binary_outputs: bool,
}

/// A bus, or one named line such as `carry in`.
#[derive(Clone)]
pub struct Port {
    pub name: String,
    /// What it is for, such as A or a control line; `None` for a lever or
    /// button no role was found for.
    pub role: Option<Role>,
    /// Whether it is a line named on its own, a control line such as `carry
    /// in` or a status line such as `carry out`, or a row of them named by
    /// one sign, such as a memory's read lines, rather than a bus.
    pub labelled: bool,
    /// Least significant first.
    pub bits: Vec<Bit>,
    /// The weakest signal strength that drives an input correctly, from a
    /// sign token such as `4ss` on any of its bits.
    pub strength: u8,
    /// The words on its signs, lowercase, split at spaces, line breaks, and
    /// commas, such as an ALU control line's operations `and nand sub`;
    /// empty when it has none.
    pub words: Vec<String>,
    /// A signal-strength input: each block takes a value from 0 to 15 as
    /// its strength, through the comparator on top, rather than a bit from
    /// a lever. For an output, its test reads each block's strength as a
    /// value from 0 to 15, as a popcount's one output shows its count.
    pub signal_strength: bool,
}

impl Port {
    /// Whether it is a control line, an input named on its own.
    pub fn control(&self) -> bool {
        self.role == Some(Role::Control)
    }

    /// Whether it is a status line, an output named on its own.
    pub fn status(&self) -> bool {
        self.role == Some(Role::Status)
    }

    /// Whether it is operand A.
    pub fn a(&self) -> bool {
        self.role == Some(Role::Operand(0))
    }
}

#[derive(Clone)]
pub struct Bit {
    /// Position in the file.
    pub pos: Pos,
    /// For an input, the side power enters through, such as south: the
    /// side its lever is on, or the side its comparator reads from.
    pub entry: Option<Direction>,
    /// For an input, whether dust touches its block, so only strong power
    /// from a repeater, comparator, or torch drives it.
    pub hard: bool,
    /// For an input, whether it is saved with its lever on.
    pub inverted: bool,
}

impl Bit {
    /// A bit at `pos` with nothing more known of it, as an output's.
    pub fn at(pos: Pos) -> Self {
        Bit {
            pos,
            entry: None,
            hard: false,
            inverted: false,
        }
    }

    /// The input whose block is at `pos`, its switch among `switches`:
    /// entered from its switch's side, hard when dust touches its block,
    /// and inverted when its switch is saved on.
    pub fn input(
        blocks: &Blocks,
        switches: &HashMap<Pos, Switch>,
        pos: Pos,
    ) -> Self {
        let switch = &switches[&pos];
        Bit {
            pos,
            entry: Some(switch.side),
            hard: Direction::ALL.iter().any(|&side| {
                schem::id(blocks.get(schem::step(pos, side)))
                    == "minecraft:redstone_wire"
            }),
            inverted: switch.on,
        }
    }
}

/// Each switch of `blocks`, by its input block.
pub fn switches(blocks: &Blocks) -> HashMap<Pos, Switch> {
    blocks
        .switches()
        .into_iter()
        .map(|switch| (switch.block, switch))
        .collect()
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
    /// A register's: its unsigned control lines are its cells' writes, each
    /// driven by a button, and reads, by a lever, and its data bus in and
    /// out go by those roles.
    Register,
    /// A comparison's: its one output, an output bus or a status line, is
    /// its result, which needs no sign and goes by this name, such as
    /// `A > B`.
    Result(&'static str),
    /// An inverter's, multiplier's, or T flip-flop's: its lone control line
    /// needs no sign and goes by this name, such as `invert` or
    /// `calculate`; it has no status line.
    Control(&'static str),
    /// A buffer's: its data bus in and out as a register's, and its
    /// unsigned control lines its cells' writes, by a button, or by a lever
    /// when none has a button, as it has no reads; a lone one needs no
    /// sign.
    Buffer,
    /// A counter's: each control line a lever of its own, its clock or its
    /// write enable, which its test tells apart, a sign naming it.
    Counter,
}

/// Names a build's unsigned ports for what its category takes them to be,
/// as `lines` says, so a page shows what the simulator reads: an adder's
/// lone control and status lines are its carry in and
/// carry out; a register's data bus in and out are its data in and data
/// out, and each of its control lines a write, driven by a button, or a
/// read, by a lever, and a buffer's the same but every control line a
/// write; a comparison's one output is its result, such as `A > B`; and an
/// inverter's lone control line is its invert. A sign's name always
/// stands.
pub fn name_lines(lines: Lines, blocks: &Blocks, interface: &mut Interface) {
    let lone = |ports: &mut Vec<Port>, role: Role, name: &str| {
        let mut lines = ports.iter_mut().filter(|p| p.role == Some(role));
        if let (Some(line), None) = (lines.next(), lines.next())
            && !line.labelled
            && line.bits.len() == 1
        {
            name.clone_into(&mut line.name);
        }
    };
    let (control, status, lone_output) = match lines {
        Lines::Lone {
            control,
            status,
            lone_output,
        } => (control, status, lone_output),
        Lines::Register => {
            return name_register(blocks, interface, false);
        }
        Lines::Buffer => return name_register(blocks, interface, true),
        Lines::Counter => return name_counter(interface),
        Lines::Result(name) => {
            if let [out] = interface.outputs.as_mut_slice()
                && !out.labelled
                && out.bits.len() == 1
            {
                name.clone_into(&mut out.name);
            }
            return;
        }
        Lines::Control(name) => {
            return lone(&mut interface.inputs, Role::Control, name);
        }
        Lines::Signed => return,
    };
    lone(&mut interface.inputs, Role::Control, control);
    lone(&mut interface.outputs, Role::Status, status);
    if lone_output
        && let [out] = interface.outputs.as_mut_slice()
        && !out.labelled
        && out.bits.len() == 1
    {
        status.clone_into(&mut out.name);
    }
}

/// `name_lines` for a register: its data bus in and out by their roles,
/// and its unsigned control lines split into writes, each driven by a
/// button, and reads, by a lever, numbered in order when there are more.
/// A `buffer`'s are all writes when none has a button, as it has no
/// reads.
fn name_register(blocks: &Blocks, interface: &mut Interface, buffer: bool) {
    let control = Port::control;
    let data: Vec<&mut Port> = interface
        .inputs
        .iter_mut()
        .filter(|p| !control(p))
        .collect();
    if let [only] = data.as_slice()
        && only.name == "A"
    {
        let only = interface
            .inputs
            .iter_mut()
            .find(|p| !control(p))
            .expect("just found");
        "data in".clone_into(&mut only.name);
    }
    if let [out] = interface.outputs.as_mut_slice()
        && out.name == "output"
    {
        "data out".clone_into(&mut out.name);
    }
    let button = |bit: &Bit| button(blocks, bit);
    let levers_write = buffer
        && !interface
            .inputs
            .iter()
            .filter(|port| control(port) && !port.labelled)
            .flat_map(|port| &port.bits)
            .any(button);
    let mut split = Vec::new();
    for port in std::mem::take(&mut interface.inputs) {
        if !control(&port) || port.labelled {
            split.push(port);
            continue;
        }
        let (writes, reads): (Vec<_>, Vec<_>) = port
            .bits
            .iter()
            .cloned()
            .partition(|bit| levers_write || button(bit));
        for (bits, word) in [(writes, "write"), (reads, "read")] {
            let count = bits.len();
            for (i, bit) in bits.into_iter().enumerate() {
                let name = if count > 1 {
                    format!("{word} {}", i + 1)
                } else {
                    word.to_owned()
                };
                split.push(Port {
                    name,
                    bits: vec![bit],
                    ..port.clone()
                });
            }
        }
    }
    interface.inputs = split;
}

/// `name_lines` for a counter: each bit of its control lines a line of
/// its own, its clock and its write enable, two levers alike until its
/// test tells which pulses and which loads, so each unsigned one is named
/// for its place, numbered when there are more.
fn name_counter(interface: &mut Interface) {
    let (controls, mut split): (Vec<Port>, Vec<Port>) =
        std::mem::take(&mut interface.inputs)
            .into_iter()
            .partition(Port::control);
    let count: usize = controls.iter().map(|port| port.bits.len()).sum();
    let bits = controls
        .iter()
        .flat_map(|port| port.bits.iter().map(move |bit| (bit.clone(), port)));
    for (i, (bit, port)) in bits.enumerate() {
        let name = if port.labelled && port.bits.len() == 1 {
            port.name.clone()
        } else if count > 1 {
            format!("control {}", i + 1)
        } else {
            "control".to_owned()
        };
        split.push(Port {
            name,
            bits: vec![bit],
            ..port.clone()
        });
    }
    interface.inputs = split;
}

/// Whether a button, not a lever, drives input bit `bit`.
fn button(blocks: &Blocks, bit: &Bit) -> bool {
    bit.entry.is_some_and(|side| {
        schem::id(blocks.get(schem::step(bit.pos, side))).ends_with("_button")
    })
}

/// The bounds, inclusive, of the build itself: its box less any outer
/// layer that holds nothing but its interface, meaning its inputs, each
/// lever or button with the block it is on, or a signal-strength input's
/// comparator with the block under it, any torch on an input block, and
/// signs. Such a layer only carries the inputs, so it does not make the
/// build bigger, and a stack's copies share it.
pub fn core(blocks: &Blocks) -> (Pos, Pos) {
    trimmed(blocks, false)
}

/// The bounds, inclusive, a build is measured by: its core, less any outer
/// layer that holds nothing but its interface and its outputs, each
/// repeater or comparator facing air with the block holding it up, and
/// the sign under its front. An output that sticks out of the build is
/// read there, as an input sticking out is driven there, so neither makes
/// the build bigger; a layer counts once any other block of the build is
/// in it, as the dust a port's tap reads. Stacking goes by the core, whose
/// outputs a copy's step may need to reach past.
pub fn measured(blocks: &Blocks) -> (Pos, Pos) {
    trimmed(blocks, true)
}

/// The build's box less its outer layers of nothing but interface, and of
/// outputs too when `outputs`, taken off every face in turn until none
/// is left, as taking one face's may leave another's of nothing but them.
fn trimmed(blocks: &Blocks, outputs: bool) -> (Pos, Pos) {
    let interface = interface_cells(blocks);
    let read_out = output_cells(blocks);
    let only = |axis, at, low, high| {
        let (mut low, mut high): (Pos, Pos) = (low, high);
        low[axis] = at;
        high[axis] = at;
        (low[0]..=high[0]).all(|x| {
            (low[1]..=high[1]).all(|y| {
                (low[2]..=high[2]).all(|z| {
                    let pos = [x, y, z];
                    blocks.get(pos) == schem::AIR
                        || interface.contains(&pos)
                        || (outputs && read_out.contains(&pos))
                })
            })
        })
    };
    let mut low = [0; 3];
    let mut high = blocks.size.map(|n| n - 1);
    loop {
        let before = (low, high);
        for axis in 0..3 {
            while high[axis] > low[axis] && only(axis, low[axis], low, high) {
                low[axis] += 1;
            }
            while high[axis] > low[axis] && only(axis, high[axis], low, high) {
                high[axis] -= 1;
            }
        }
        if (low, high) == before {
            return (low, high);
        }
    }
}

/// The cells of a build's interface by the rule for inputs: each lever,
/// button, and sign, each input block, a lever's or the block under a
/// signal-strength input's comparator, the comparator itself, and any
/// torch on an input block.
fn interface_cells(blocks: &Blocks) -> HashSet<Pos> {
    let mut cells: HashSet<Pos> = HashSet::new();
    for switch in blocks.switches() {
        cells.extend([switch.pos, switch.block]);
    }
    for input in repeater_outputs::strength_inputs(blocks) {
        cells.extend([input.at, schem::step(input.at, Direction::Down)]);
    }
    let inputs = cells.clone();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        if name.ends_with("sign")
            || (name.contains("torch")
                && schem::torch_block(pos, state)
                    .is_some_and(|block| inputs.contains(&block)))
        {
            cells.insert(pos);
        }
    }
    cells
}

/// The cells of a build's outputs by the rule for them: each repeater or
/// comparator facing air and the block holding it up. The sign under its
/// front is the interface's, as every sign is.
fn output_cells(blocks: &Blocks) -> HashSet<Pos> {
    repeater_outputs::find(blocks)
        .into_iter()
        .flat_map(|output| [output.at, output.block()])
        .collect()
}

/// A line as the site keeps it with a version of a build's file, in the
/// database, as [`Kept`] writes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptLine {
    /// What it is for; `None` for a lever or button no role was found for.
    pub role: Option<Role>,
    pub name: String,
    /// Whether a sign names it, as a line named on its own, or a row of
    /// them named by one sign.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub signed: bool,
    /// The block of each bit, least significant first.
    pub bits: Vec<Pos>,
}

/// A build's lines as the site keeps them with each version of its file:
/// each input and output line with its role, its name, and its bits in
/// order, as [`read`] reads them from the roles the file carries. Reading
/// the file with [`Kept::roles`] gives the same lines again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    pub inputs: Vec<KeptLine>,
    pub outputs: Vec<KeptLine>,
}

impl Kept {
    /// `interface`'s lines as they are kept.
    pub fn of(interface: &Interface) -> Kept {
        let lines = |ports: &[Port]| {
            ports
                .iter()
                .map(|port| KeptLine {
                    role: port.role,
                    name: port.name.clone(),
                    signed: port.labelled,
                    bits: port.bits.iter().map(|bit| bit.pos).collect(),
                })
                .collect()
        };
        Kept {
            inputs: lines(&interface.inputs),
            outputs: lines(&interface.outputs),
        }
    }

    /// The lines of the build in `blocks`, as its file carries their roles;
    /// `None` for a file carrying none.
    pub fn read(blocks: &Blocks) -> Option<Kept> {
        blocks.roles.as_ref()?;
        Some(Kept::of(&read(blocks)))
    }

    /// Whether its operands' bits, A's and B's, lie in one row running
    /// east to west, the way a horizontal build's bits run, alternating
    /// along it, as copies of a 1-bit build with A and B side by side do,
    /// rather than split, each operand's bits together, as one above the
    /// other. False for a build without both.
    pub fn interleaved(&self) -> bool {
        let operand = |n| {
            self.inputs
                .iter()
                .find(|line| line.role == Some(Role::Operand(n)))
        };
        let (Some(a), Some(b)) = (operand(0), operand(1)) else {
            return false;
        };
        let mut bits: Vec<(Pos, bool)> = a
            .bits
            .iter()
            .map(|&pos| (pos, true))
            .chain(b.bits.iter().map(|&pos| (pos, false)))
            .collect();
        let Some(&(first, _)) = bits.first() else {
            return false;
        };
        if bits
            .iter()
            .any(|(pos, _)| pos[1] != first[1] || pos[2] != first[2])
        {
            return false;
        }
        if a.bits.len() == 1 && b.bits.len() == 1 {
            return true;
        }
        bits.sort_by_key(|(pos, _)| pos[0]);
        let turns = bits.windows(2).filter(|two| two[0].1 != two[1].1).count();
        turns > 1
    }

    /// Whether any bus of its, an operand or an output, isn't laid as the
    /// conventions lay one: its bits in one evenly spaced line, or in two
    /// as long running side by side, alternating, as `inspect::runs`
    /// groups them. Its top bits out of line, its bits unevenly spaced, as
    /// two runs end to end in one line with a gap between them are, or a
    /// staircase of short runs make it unaligned.
    pub fn unaligned(&self) -> bool {
        // Two runs side by side share no line; end to end, they do.
        let in_one_line =
            |one: &crate::interface::inspect::Run,
             other: &crate::interface::inspect::Run| {
                let Some(axis) = (0..3).find(|&i| one.step[i] != 0) else {
                    return false;
                };
                let across = |pos: &Pos| {
                    [0, 1, 2].map(|i| if i == axis { 0 } else { pos[i] })
                };
                let line = across(&one.bits[0]);
                one.bits
                    .iter()
                    .chain(&other.bits)
                    .all(|p| across(p) == line)
            };
        self.inputs
            .iter()
            .chain(&self.outputs)
            .filter(|line| {
                matches!(line.role, Some(Role::Operand(_) | Role::Output(_)))
            })
            .any(|line| {
                let bits: Vec<(Pos, ())> =
                    line.bits.iter().map(|&pos| (pos, ())).collect();
                match crate::interface::inspect::runs(&bits).as_slice() {
                    [] | [_] => false,
                    [one, other] => {
                        one.bits.len() != other.bits.len()
                            || one.step != other.step
                            || in_one_line(one, other)
                    }
                    _ => true,
                }
            })
    }

    /// The role of each bit's block, as a file carries them.
    pub fn roles(&self) -> Roles {
        let mut roles = Roles::default();
        for line in self.inputs.iter().chain(&self.outputs) {
            if let Some(role) = line.role {
                roles.mark(role, &line.bits);
            }
        }
        roles
    }
}

/// How an input block takes its signal.
pub enum Feed {
    /// A lever, or for a control line a button, on the block.
    Switch(Switch),
    /// A signal-strength input's comparator on the block, reading from the
    /// side `entry`, where a tester's barrel goes.
    Strength { entry: Option<Direction> },
}

/// What [`read`] takes from a file: every input block with what feeds it,
/// and the output blocks, as the rule for inputs and outputs finds them,
/// only the signal-strength inputs and outputs with a role.
struct Found {
    feeds: HashMap<Pos, Feed>,
    outputs: Vec<Pos>,
}

fn found(blocks: &Blocks, roles: &Roles) -> Found {
    let held = |at: Pos| at[1] > 0;
    let mut feeds: HashMap<Pos, Feed> =
        repeater_outputs::strength_inputs(blocks)
            .into_iter()
            .filter(|input| held(input.at))
            .map(|input| (schem::step(input.at, Direction::Down), input))
            .filter(|&(block, _)| roles.get(block).is_some_and(Role::input))
            .map(|(block, input)| {
                (
                    block,
                    Feed::Strength {
                        entry: Some(input.facing),
                    },
                )
            })
            .collect();
    feeds.extend(
        blocks
            .switches()
            .into_iter()
            .map(|switch| (switch.block, Feed::Switch(switch))),
    );
    let outputs = repeater_outputs::find(blocks)
        .into_iter()
        .filter(|output| held(output.at))
        .map(repeater_outputs::Output::block)
        .filter(|&block| {
            !feeds.contains_key(&block)
                && roles.get(block).is_some_and(|role| !role.input())
        })
        .collect();
    Found { feeds, outputs }
}

/// The roles `blocks` carries, or for a file carrying none, those its
/// shapes give it; see [`shaped_roles`].
pub fn roles(blocks: &Blocks) -> std::borrow::Cow<'_, Roles> {
    match &blocks.roles {
        Some(roles) => std::borrow::Cow::Borrowed(roles),
        None => std::borrow::Cow::Owned(shaped_roles(blocks)),
    }
}

/// The roles a build's shapes give its lines, colour playing no part, as
/// [`repeater_outputs::interface`] reads them: its signal-strength buses
/// and buses of levers its operands, the longest first, each lone lever or
/// button a control line, each bus of outputs an output, and a lone output
/// beside a bus a status line; but a row of levers or buttons two or more
/// of which signs name is control lines, as a memory's reads and
/// writes are, its operands numbered on without it. A file probing hasn't
/// read, as an upload before it is, or a build kept as built that no probe
/// reads, is read so.
pub fn shaped_roles(blocks: &Blocks) -> Roles {
    let mut shaped = repeater_outputs::interface(blocks);
    let signs = line_signs(blocks);
    let mut operands = 0..=u8::MAX;
    for port in &mut shaped.inputs {
        if !matches!(port.role, Some(Role::Operand(_))) {
            continue;
        }
        let signed = port
            .bits
            .iter()
            .filter(|bit| {
                signs.get(&bit.pos).and_then(|l| sign_name(l)).is_some()
            })
            .count();
        port.role = if signed > 1 && !port.signal_strength {
            Some(Role::Control)
        } else {
            operands.next().map(Role::Operand)
        };
    }
    roles_of(&shaped)
}

/// Every input block, with what feeds it, as [`read`] takes them.
pub fn feeds(blocks: &Blocks) -> HashMap<Pos, Feed> {
    found(blocks, &roles(blocks)).feeds
}

/// Whether a build takes a signal strength, a hex digit per block, on an
/// input, as [`read`] reads its roles.
pub fn takes_digits(blocks: &Blocks) -> bool {
    read(blocks).inputs.iter().any(|port| port.signal_strength)
}

/// A build's lines: its inputs and outputs, by the rule for them, each
/// grouped into ports by its role, as the file carries them or, carrying
/// none, as its shapes give them; see [`roles`].
pub fn read(blocks: &Blocks) -> Interface {
    read_with(blocks, &roles(blocks))
}

/// A build's lines, grouped into ports by `roles`, whatever roles its file
/// carries.
pub fn read_with(blocks: &Blocks, roles: &Roles) -> Interface {
    let Found { feeds, outputs } = found(blocks, roles);
    read_from(blocks, &feeds, outputs, roles)
}

/// The lines of the input blocks `feeds` and output blocks `outputs`, each
/// grouped by its role in `roles` as [`ports`] groups them, the buses
/// running alongside a marked one ordered as it is.
pub(crate) fn read_from(
    blocks: &Blocks,
    feeds: &HashMap<Pos, Feed>,
    outputs: Vec<Pos>,
    roles: &Roles,
) -> Interface {
    let signs = line_signs(blocks);
    // In a fixed order, so bits as near the corner as each other, and
    // lines, always come out the same way.
    let mut inputs: Vec<Pos> = feeds.keys().copied().collect();
    inputs.sort_unstable();
    let mut interface = Interface {
        inputs: ports(blocks, roles, inputs, &signs, Some(feeds)),
        outputs: ports(blocks, roles, outputs, &signs, None),
        binary_outputs: false,
    };
    follow_marks(blocks, &mut interface);
    interface
}

/// The roles of `interface`'s lines, each bit's block by its port's role,
/// as a file carries them: those [`read`] groups into the same ports.
pub fn roles_of(interface: &Interface) -> Roles {
    let mut roles = Roles::default();
    for port in interface.inputs.iter().chain(&interface.outputs) {
        if let Some(role) = port.role {
            roles.mark(role, port.bits.iter().map(|bit| &bit.pos));
        }
    }
    roles
}

/// The signs naming a build's lines, each by the block it names: any sign
/// on an input block, and an output's sign under its front, see
/// [`repeater_outputs::signs`], as any other sign on an output's block
/// names nothing.
pub fn line_signs(blocks: &Blocks) -> HashMap<Pos, &[String]> {
    let mut signs = blocks.sign_texts();
    for output in repeater_outputs::find(blocks) {
        signs.remove(&output.block());
    }
    signs.extend(repeater_outputs::signs(blocks));
    signs
}

/// The way a bus's bits run, from its least significant: the axis it runs
/// along most, and whether its bits climb along it or fall.
fn running(port: &Port) -> Option<(usize, bool)> {
    let (first, last) = (port.bits.first()?.pos, port.bits.last()?.pos);
    let axis = (0..3).max_by_key(|&axis| (last[axis] - first[axis]).abs())?;
    (first[axis] != last[axis]).then(|| (axis, last[axis] > first[axis]))
}

/// Turns round each bus with no `lsb` or `msb` sign that runs along the
/// same axis as a bus with one, but the other way, so one sign orders every
/// bus running alongside: a build whose least significant bits sit at the
/// top need mark only one of its buses so. Buses marked different ways
/// along an axis leave the others as they are.
fn follow_marks(blocks: &Blocks, interface: &mut Interface) {
    let marked = |port: &Port| {
        port.bits.len() > 1
            && [port.bits.first(), port.bits.last()]
                .into_iter()
                .flatten()
                .any(|bit| order_mark(blocks, bit.pos).is_some())
    };
    let ports = || interface.inputs.iter().chain(&interface.outputs);
    let mut ways: HashMap<usize, HashSet<bool>> = HashMap::new();
    for port in ports().filter(|port| marked(port)) {
        if let Some((axis, climbs)) = running(port) {
            ways.entry(axis).or_default().insert(climbs);
        }
    }
    for port in interface.inputs.iter_mut().chain(&mut interface.outputs) {
        if port.labelled || marked(port) {
            continue;
        }
        let Some((axis, climbs)) = running(port) else {
            continue;
        };
        if ways
            .get(&axis)
            .is_some_and(|ways| ways.len() == 1 && !ways.contains(&climbs))
        {
            port.bits.reverse();
        }
    }
}

/// Groups `members` into ports by their `roles`, `feeds` being given for
/// inputs. Each control or status line with a name sign is a line of its
/// own. Every other role is one bus, named by a sign on any of its blocks,
/// or else by the next letter for an input, A for the first operand, or by
/// its place among the outputs; levers and buttons with no role are one
/// bus too, lettered after the operands.
fn ports(
    blocks: &Blocks,
    roles: &Roles,
    members: impl IntoIterator<Item = Pos>,
    signs: &HashMap<Pos, &[String]>,
    feeds: Option<&HashMap<Pos, Feed>>,
) -> Vec<Port> {
    let line = if feeds.is_some() {
        Role::Control
    } else {
        Role::Status
    };
    let members: Vec<Pos> = members.into_iter().collect();
    let rows = line_rows(roles, &members, signs, line);
    let in_row: HashSet<Pos> = rows.values().flatten().copied().collect();
    let mut named = Vec::new();
    // Operands and outputs in their order, then the lines, then what has
    // no role.
    let mut buses: BTreeMap<(bool, Option<Role>), Vec<Pos>> = BTreeMap::new();
    for pos in members.into_iter().filter(|pos| !in_row.contains(pos)) {
        let lines = signs.get(&pos).copied().unwrap_or_default();
        let role = roles.get(pos);
        match sign_name(lines).filter(|_| role == Some(line)) {
            Some(name) => {
                // The line, and any row it names.
                let mut row = vec![pos];
                row.extend(rows.get(&pos).into_iter().flatten());
                let mut port = port(blocks, name, role, row, lines, feeds);
                port.labelled = true;
                named.push(port);
            }
            None => buses.entry((role.is_none(), role)).or_default().push(pos),
        }
    }
    let mut letters = 'A'..='Z';
    let output_count = buses.keys().filter(|(_, r)| *r != Some(line)).count();
    let mut result: Vec<Port> = std::mem::take(&mut buses)
        .into_iter()
        .enumerate()
        .map(|(i, ((_, role), members))| {
            // A sign on any bit can carry the bus's strength.
            let lines: Vec<String> = members
                .iter()
                .filter_map(|pos| signs.get(pos))
                .flat_map(|lines| lines.iter().cloned())
                .collect();
            let given = members
                .iter()
                .find_map(|pos| signs.get(pos).and_then(|l| sign_name(l)));
            let name = if role == Some(line) {
                let what = if feeds.is_some() { "control" } else { "status" };
                format!("unnamed {what}")
            } else if let Some(given) = given {
                given
            } else if feeds.is_some() {
                letters.next().map(String::from).unwrap_or_default()
            } else if output_count == 1 {
                "output".to_owned()
            } else {
                format!("output {}", i + 1)
            };
            port(blocks, name, role, members, &lines, feeds)
        })
        .collect();
    result.extend(named);
    result
}

/// The rows that signs name among the `members` of role `line`: for each
/// signed block, the unsigned blocks of that role that continue it.
fn line_rows(
    roles: &Roles,
    members: &[Pos],
    signs: &HashMap<Pos, &[String]>,
    line: Role,
) -> HashMap<Pos, Vec<Pos>> {
    let named = |pos: &Pos| signs.get(pos).and_then(|l| sign_name(l)).is_some();
    let of_line: Vec<Pos> = members
        .iter()
        .copied()
        .filter(|&pos| roles.get(pos) == Some(line))
        .collect();
    let signed: Vec<Pos> = of_line.iter().copied().filter(named).collect();
    let unsigned: HashSet<Pos> =
        of_line.into_iter().filter(|pos| !named(pos)).collect();
    rows(&signed, &unsigned)
}

/// The farthest apart two lines of a row may be.
const ROW_GAP: i32 = 16;

/// Rows of lines named by one sign. From each block in `signed`, a row runs
/// both ways through blocks of `unsigned`, in a straight line and evenly
/// spaced, the spacing being the nearest such block's; of the three axes,
/// the one with the longest row wins. Each signed block maps to the
/// unsigned blocks of its row; one with none is left out.
fn rows(signed: &[Pos], unsigned: &HashSet<Pos>) -> HashMap<Pos, Vec<Pos>> {
    let mut sorted = signed.to_vec();
    sorted.sort_unstable();
    let mut claimed: HashSet<Pos> = HashSet::new();
    let mut rows = HashMap::new();
    for head in sorted {
        let free = |pos: &Pos| unsigned.contains(pos) && !claimed.contains(pos);
        let along = |axis: usize, k: i32| {
            let mut pos = head;
            pos[axis] += k;
            pos
        };
        let best = (0..3)
            .filter_map(|axis| {
                let gap = (1..=ROW_GAP).find(|&d| {
                    free(&along(axis, d)) || free(&along(axis, -d))
                })?;
                let run = |sign: i32| {
                    (1..)
                        .map(move |k| along(axis, sign * k * gap))
                        .take_while(|pos| free(pos))
                };
                let row: Vec<Pos> = run(1).chain(run(-1)).collect();
                Some(row)
            })
            .max_by_key(Vec::len);
        if let Some(row) = best {
            claimed.extend(&row);
            rows.insert(head, row);
        }
    }
    rows
}

/// A bus's `members` ordered from the one nearest the south-east-bottom
/// corner of the bounding box along the way the bus runs, unless an `lsb`
/// sign on its far end or an `msb` sign on its near one turns it round.
pub fn lsb_first(blocks: &Blocks, members: &mut [Pos]) {
    let [width, _, length] = blocks.size;
    let corner = [width - 1, 0, length - 1];
    let axis = longest_axis(members);
    if let Some(&lsb) = members
        .iter()
        .min_by_key(|&&pos| schem::distance(pos, corner))
    {
        members.sort_by_key(|&pos| along(pos, lsb, axis));
    }
    if let (Some(&near), Some(&far)) = (members.first(), members.last())
        && near != far
        && (order_mark(blocks, far) == Some("lsb")
            || order_mark(blocks, near) == Some("msb"))
    {
        members.sort_by_key(|&pos| along(pos, far, axis));
    }
}

/// A port with its bits ordered as [`lsb_first`] orders them.
fn port(
    blocks: &Blocks,
    name: String,
    role: Option<Role>,
    mut members: Vec<Pos>,
    lines: &[String],
    feeds: Option<&HashMap<Pos, Feed>>,
) -> Port {
    lsb_first(blocks, &mut members);
    let inverted_line = lines.iter().any(|l| l.trim() == "inverted");
    let words = sign_words(lines);
    let feed = |pos: Pos| feeds.and_then(|feeds| feeds.get(&pos));
    let signal_strength = !members.is_empty()
        && members
            .iter()
            .all(|&pos| matches!(feed(pos), Some(Feed::Strength { .. })));
    let bits = members
        .iter()
        .map(|&pos| {
            let lever = match feed(pos) {
                Some(Feed::Switch(lever)) => Some(lever),
                _ => None,
            };
            Bit {
                pos,
                entry: match feed(pos) {
                    Some(Feed::Switch(lever)) => Some(lever.side),
                    Some(Feed::Strength { entry }) => *entry,
                    None => None,
                },
                hard: lever.is_some()
                    && Direction::ALL.into_iter().any(|d| {
                        schem::short(blocks.get(schem::step(pos, d)))
                            == "redstone_wire"
                    }),
                inverted: lever.is_some_and(|lever| lever.on) || inverted_line,
            }
        })
        .collect();
    Port {
        name,
        role,
        labelled: false,
        bits,
        strength: lines.iter().find_map(|l| strength(l)).unwrap_or(1),
        words,
        signal_strength,
    }
}

/// The port a sign names: its first line with a name on it, so a name
/// written on any line counts, as people often centre one word on the
/// second. A token is no name: `lsb` and `msb` mark an end of a bus, and
/// `4ss` gives a strength.
pub fn sign_name(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .map(|line| line.trim().to_lowercase())
        .find(|line| {
            !line.is_empty()
                && !matches!(line.as_str(), "lsb" | "msb")
                && strength(line).is_none()
        })
}

/// The words on a sign's `lines`, lowercase, split at spaces, line breaks,
/// and commas, leaving out tokens such as `lsb` and `4ss`.
fn sign_words(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .flat_map(|line| line.split(|c: char| c.is_whitespace() || c == ','))
        .map(str::to_lowercase)
        .filter(|word| {
            !word.is_empty()
                && !matches!(word.as_str(), "lsb" | "msb")
                && strength(word).is_none()
        })
        .collect()
}

/// `lsb` or `msb` when a sign on the block at `pos` marks it as its bus's
/// least or most significant bit.
pub fn order_mark(blocks: &Blocks, pos: Pos) -> Option<&'static str> {
    let lines = line_signs(blocks).get(&pos).copied()?;
    lines
        .iter()
        .find_map(|line| match line.trim().to_lowercase().as_str() {
            "lsb" => Some("lsb"),
            "msb" => Some("msb"),
            _ => None,
        })
}

/// A strength token such as `4ss`.
fn strength(line: &str) -> Option<u8> {
    line.trim().strip_suffix("ss")?.parse().ok()
}

/// The axis `members` spread furthest along, which a bus runs along even
/// when its bits zig-zag across it, as a double density adder's alternate
/// between two columns.
fn longest_axis(members: &[Pos]) -> usize {
    (0..3)
        .max_by_key(|&axis| {
            let coords = members.iter().map(|pos| pos[axis]);
            coords.clone().max().unwrap_or(0) - coords.min().unwrap_or(0)
        })
        .unwrap_or(0)
}

/// How far `pos` is from `from` along `axis`, then in all, so bits level
/// along the axis count up from the nearer.
fn along(pos: Pos, from: Pos, axis: usize) -> (i32, i32) {
    ((pos[axis] - from[axis]).abs(), schem::distance(pos, from))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines `a` and `b`, A's and B's bits, as kept.
    fn operands(a: &[Pos], b: &[Pos]) -> Kept {
        let line = |n, bits: &[Pos]| KeptLine {
            role: Some(Role::Operand(n)),
            name: String::new(),
            signed: false,
            bits: bits.to_vec(),
        };
        Kept {
            inputs: vec![line(0, a), line(1, b)],
            outputs: Vec::new(),
        }
    }

    /// A 1-bit build's A and B side by side in a row east to west, as a
    /// 3-wide XOR gate's, alternate once stacked, and so do 2-bit buses
    /// whose bits take turns; buses one above the other, or side by side
    /// in a row each together, or side by side north to south, are split.
    #[test]
    fn operands_alternating_along_a_row_are_interleaved() {
        assert!(operands(&[[0, 0, 4]], &[[2, 0, 4]]).interleaved());
        let turns = operands(&[[0, 0, 0], [4, 0, 0]], &[[2, 0, 0], [6, 0, 0]]);
        assert!(turns.interleaved());
        let above = operands(&[[0, 0, 0], [2, 0, 0]], &[[0, 2, 0], [2, 2, 0]]);
        assert!(!above.interleaved());
        let halves = operands(&[[0, 0, 0], [2, 0, 0]], &[[4, 0, 0], [6, 0, 0]]);
        assert!(!halves.interleaved());
        assert!(!operands(&[[0, 0, 0]], &[[0, 0, 2]]).interleaved());
        assert!(!Kept::default().interleaved());
    }

    fn lines(text: [&str; 4]) -> Vec<String> {
        text.map(str::to_owned).to_vec()
    }

    /// A lever on a block driving dust east into a repeater giving out
    /// east into air, its sign under its front, with `more` besides.
    fn measured_line(more: &[(Pos, &str)]) -> Blocks {
        let states: HashMap<Pos, String> = [
            ([0, 1, 1], "lever[face=wall,facing=west,powered=false]"),
            ([1, 1, 1], "white_concrete"),
            ([2, 0, 1], "stone"),
            (
                [2, 1, 1],
                "redstone_wire[east=side,north=none,power=0,south=none,\
                 west=side]",
            ),
            ([3, 0, 1], "stone"),
            (
                [3, 1, 1],
                "repeater[delay=1,facing=west,locked=false,powered=false]",
            ),
            ([4, 0, 1], "oak_wall_sign[facing=east,waterlogged=false]"),
        ]
        .iter()
        .chain(more)
        .map(|&(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        let mut blocks = Blocks::from_grid([5, 2, 3], &states);
        let text = ["carry out", "", "", ""].map(str::to_owned).to_vec();
        blocks.signs.insert([4, 0, 1], text);
        blocks
    }

    /// The layers of nothing but its input, the lever and its block, and
    /// of nothing but its output, the repeater, the block holding it up,
    /// and the sign under its front, don't count toward a build's size; the
    /// output's layer does once anything else of the build is in it. The
    /// core keeps the output.
    #[test]
    fn layers_of_only_inputs_or_outputs_are_not_measured() {
        let blocks = measured_line(&[]);
        assert_eq!(measured(&blocks), ([2, 0, 1], [2, 1, 1]));
        assert_eq!(core(&blocks), ([2, 0, 1], [3, 1, 1]));
        let wired = measured_line(&[([3, 0, 0], "stone")]);
        assert_eq!(measured(&wired), ([2, 0, 0], [3, 1, 1]));
    }

    #[test]
    fn a_sign_names_on_whichever_line_has_a_name() {
        assert_eq!(
            sign_name(&lines(["Next", "", "", ""])).as_deref(),
            Some("next")
        );
        assert_eq!(
            sign_name(&lines(["", "next", "", ""])).as_deref(),
            Some("next")
        );
        assert_eq!(
            sign_name(&lines(["lsb", "addend", "", ""])).as_deref(),
            Some("addend")
        );
        assert_eq!(sign_name(&lines(["", "4ss", "", ""])), None);
        assert_eq!(sign_name(&lines(["", "", "", ""])), None);
    }

    #[test]
    fn one_sign_names_an_evenly_spaced_row() {
        // Read lines every other block along x, signed at x = 0.
        let unsigned: HashSet<Pos> = (1..8).map(|i| [i * 2, 1, 4]).collect();
        let rows = rows(&[[0, 1, 4]], &unsigned);
        let mut row = rows[&[0, 1, 4]].clone();
        row.sort_unstable();
        assert_eq!(row, (1..8).map(|i| [i * 2, 1, 4]).collect::<Vec<_>>());
    }

    #[test]
    fn a_row_stops_at_a_break_in_its_spacing_and_two_signs_split_it() {
        // A gap of 3 after x = 2 ends the row; a second sign starts another.
        let unsigned: HashSet<Pos> =
            [[1, 0, 0], [2, 0, 0], [5, 0, 0], [7, 0, 0]].into();
        let rows = rows(&[[0, 0, 0], [6, 0, 0]], &unsigned);
        let mut first = rows[&[0, 0, 0]].clone();
        first.sort_unstable();
        assert_eq!(first, [[1, 0, 0], [2, 0, 0]]);
        let mut second = rows[&[6, 0, 0]].clone();
        second.sort_unstable();
        assert_eq!(second, [[5, 0, 0], [7, 0, 0]]);
    }

    #[test]
    fn a_signed_line_alone_has_no_row() {
        let rows = rows(&[[0, 0, 0]], &HashSet::new());
        assert!(rows.is_empty());
    }
}
