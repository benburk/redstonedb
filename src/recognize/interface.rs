//! A build's inputs and outputs, read from its blocks and signs. Which
//! blocks are inputs and outputs the rule [`crate::recognize::inspect`]
//! checks says, colour playing no part: an input is a block with a lever on
//! it, or for a control line a button, or for a signal-strength input the
//! block under a comparator with air behind it; an output is the block
//! under a repeater or comparator facing air.
//!
//! What each line is for, its role, is read from the colours the site
//! gives a build once probing finds them, see [`crate::recognize::probe`]:
//! each colour one bus, named by a sign on any of its blocks, an output's
//! on the face under its front, see [`line_signs`], except magenta control
//! lines and pink status lines, each one line named by its own sign, or a
//! row of them named by one sign on any of the row, as a memory's read
//! lines are. In a file the site has coloured, an output or
//! signal-strength input on a block of no role colour plays no role, as
//! one a stacked copy hands on to the next. A file it hasn't coloured has
//! its outputs and signal-strength inputs read by their shapes, as
//! [`repeater_outputs::interface`] reads them, and its levers by the
//! blocks they are on. Each bus's least significant bit, and a row's first
//! line, is the one nearest the south-east-bottom corner of the bounding
//! box, unless an `lsb` or `msb` sign says otherwise; such a sign on one
//! bus orders every unmarked bus running alongside it too.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{
    category::{Behaviour, Kind, Lines},
    recognize::{colour, repeater_outputs},
    schem::{self, Blocks, Direction, Pos, Switch},
};

#[derive(Default)]
pub struct Interface {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}

/// A bus, or one named line such as `carry in`.
#[derive(Clone)]
pub struct Port {
    pub name: String,
    /// The block marking each bit, such as `lime concrete`.
    pub block: String,
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
}

/// Names a build's unsigned ports for what its `category` takes them to
/// be, as `category::Lines` gives them, so a page shows what the simulator
/// reads: an adder's lone magenta and pink lines are its carry in and
/// carry out; a register's data bus in and out are its data in and data
/// out, and each of its magenta lines a write, driven by a button, or a
/// read, by a lever, and a buffer's the same but every magenta line a
/// write; a comparison's one output is its result, such as `A > B`; and an
/// inverter's lone control line is its invert. A sign's name always
/// stands.
pub fn name_lines(
    category: impl Into<Option<Kind>>,
    blocks: &Blocks,
    interface: &mut Interface,
) {
    let lone = |ports: &mut Vec<Port>, colour: &str, name: &str| {
        let mut lines =
            ports.iter_mut().filter(|p| p.block.starts_with(colour));
        if let (Some(line), None) = (lines.next(), lines.next())
            && !line.labelled
            && line.bits.len() == 1
        {
            name.clone_into(&mut line.name);
        }
    };
    let (control, status, lone_output) = match Behaviour::of(category).lines {
        Lines::Lone {
            control,
            status,
            lone_output,
        } => (control, status, lone_output),
        Lines::Register => {
            return name_register(blocks, interface, false);
        }
        Lines::Buffer => return name_register(blocks, interface, true),
        Lines::Counter => return name_counter(blocks, interface),
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
            return lone(&mut interface.inputs, colour::CONTROL, name);
        }
        Lines::Signed => return,
    };
    lone(&mut interface.inputs, colour::CONTROL, control);
    lone(&mut interface.outputs, colour::STATUS, status);
    if lone_output
        && let [out] = interface.outputs.as_mut_slice()
        && !out.labelled
        && out.bits.len() == 1
    {
        status.clone_into(&mut out.name);
    }
}

/// `name_lines` for a register: its data bus in and out by their roles,
/// and its unsigned magenta lines split into writes, each driven by a
/// button, and reads, by a lever, numbered in order when there are more.
/// A `buffer`'s are all writes when none has a button, as it has no
/// reads.
fn name_register(blocks: &Blocks, interface: &mut Interface, buffer: bool) {
    let control = |port: &Port| port.block.starts_with(colour::CONTROL);
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

/// `name_lines` for a counter: each bit of its magenta lines, signed or
/// not, a line of its own, its clock when a button drives it and its write
/// enable when a lever does, numbered in order when there are more.
fn name_counter(blocks: &Blocks, interface: &mut Interface) {
    let mut split = Vec::new();
    let mut controls = Vec::new();
    for port in std::mem::take(&mut interface.inputs) {
        if port.block.starts_with(colour::CONTROL) {
            controls.push(port);
        } else {
            split.push(port);
        }
    }
    let bits: Vec<(Bit, &Port)> = controls
        .iter()
        .flat_map(|port| port.bits.iter().map(move |bit| (bit.clone(), port)))
        .collect();
    let (clocks, enables): (Vec<_>, Vec<_>) =
        bits.into_iter().partition(|(bit, _)| button(blocks, bit));
    for (bits, word) in [(clocks, "clock"), (enables, "write enable")] {
        let count = bits.len();
        for (i, (bit, port)) in bits.into_iter().enumerate() {
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
    interface.inputs = split;
}

/// Whether a button, not a lever, drives input bit `bit`.
fn button(blocks: &Blocks, bit: &Bit) -> bool {
    bit.entry.is_some_and(|side| {
        schem::id(blocks.get(schem::step(bit.pos, side))).ends_with("_button")
    })
}

/// The bounds, inclusive, of the build itself: its box less any outer
/// layer that holds nothing but its interface, meaning input blocks with
/// their levers, dust, signs, and torches. Such a layer only carries the
/// inputs, so it does not make the build bigger, and a stack's copies share
/// it.
pub fn core(blocks: &Blocks) -> (Pos, Pos) {
    trimmed(blocks, false)
}

/// The bounds, inclusive, a build is measured by: its core, less any outer
/// layer that holds nothing but its interface and its outputs, each output
/// block with the dust, repeater, or comparator standing on it to carry it
/// out. An output that sticks out of the build is read there, as an input
/// sticking out is driven there, so neither makes the build bigger. Stacking
/// goes by the core, whose outputs a copy's step may need to reach past.
pub fn measured(blocks: &Blocks) -> (Pos, Pos) {
    trimmed(blocks, true)
}

/// The build's box less its outer layers of nothing but interface, and of
/// outputs too when `outputs`.
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
    for axis in 0..3 {
        while high[axis] > low[axis] && only(axis, low[axis], low, high) {
            low[axis] += 1;
        }
        while high[axis] > low[axis] && only(axis, high[axis], low, high) {
            high[axis] -= 1;
        }
    }
    (low, high)
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
/// comparator facing air and the block under it.
fn output_cells(blocks: &Blocks) -> HashSet<Pos> {
    repeater_outputs::find(blocks)
        .into_iter()
        .flat_map(|output| [output.at, output.block()])
        .collect()
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
/// and the output blocks, as the rule for inputs and outputs finds them;
/// in a file the site has coloured, see [`coloured`], only the
/// signal-strength inputs and outputs on blocks of a role colour.
struct Found {
    feeds: HashMap<Pos, Feed>,
    outputs: Vec<Pos>,
    coloured: bool,
}

/// Whether the site has coloured in the roles of a build with
/// signal-strength inputs `digits` and outputs `outputs`, each by the
/// block under it: any of those blocks is of a role colour, as probing
/// colours them.
fn coloured(
    blocks: &Blocks,
    digits: &[repeater_outputs::StrengthInput],
    outputs: &[repeater_outputs::Output],
) -> bool {
    let name = |pos: Pos| schem::short(blocks.get(pos));
    digits.iter().any(|input| {
        colour::input_mark(name(schem::step(input.at, Direction::Down)))
            .is_some()
    }) || outputs
        .iter()
        .any(|output| colour::is_output(name(output.block())))
}

fn found(blocks: &Blocks) -> Found {
    let held = |at: Pos| at[1] > 0;
    let digits: Vec<repeater_outputs::StrengthInput> =
        repeater_outputs::strength_inputs(blocks)
            .into_iter()
            .filter(|input| held(input.at))
            .collect();
    let found: Vec<repeater_outputs::Output> = repeater_outputs::find(blocks)
        .into_iter()
        .filter(|output| held(output.at))
        .collect();
    let coloured = coloured(blocks, &digits, &found);
    let name = |pos: Pos| schem::short(blocks.get(pos));
    let mut feeds: HashMap<Pos, Feed> = digits
        .iter()
        .map(|input| (schem::step(input.at, Direction::Down), input))
        .filter(|&(block, _)| {
            !coloured || colour::input_mark(name(block)).is_some()
        })
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
    let outputs = found
        .iter()
        .map(|output| output.block())
        .filter(|&block| {
            !feeds.contains_key(&block)
                && (!coloured || colour::is_output(name(block)))
        })
        .collect();
    Found {
        feeds,
        outputs,
        coloured,
    }
}

/// Every input block, with what feeds it, as [`read`] takes them.
pub fn feeds(blocks: &Blocks) -> HashMap<Pos, Feed> {
    if crate::recognize::legacy::old() {
        return crate::recognize::legacy::feeds(blocks);
    }
    found(blocks).feeds
}

/// Whether a build takes a signal strength, a hex digit per block, on an
/// input, as [`read`] reads its roles.
pub fn takes_digits(blocks: &Blocks) -> bool {
    read(blocks).inputs.iter().any(|port| port.signal_strength)
}

/// A build's lines: its inputs and outputs, by the rule for them, each
/// grouped into ports by the role the site coloured it in, or in a file
/// it hasn't coloured, its signal-strength inputs and outputs by their
/// shapes. Inside [`crate::recognize::legacy::reading`], by the old rule,
/// as only `rsdb convert` reads a build saved under it.
pub fn read(blocks: &Blocks) -> Interface {
    if crate::recognize::legacy::old() {
        return crate::recognize::legacy::read(blocks);
    }
    let Found {
        feeds,
        outputs,
        coloured,
    } = found(blocks);
    if coloured {
        return read_from(blocks, &feeds, outputs);
    }
    let levers: HashMap<Pos, Feed> = feeds
        .into_iter()
        .filter(|(_, feed)| matches!(feed, Feed::Switch(_)))
        .collect();
    let mut interface = read_from(blocks, &levers, Vec::new());
    let shaped = repeater_outputs::interface(blocks);
    let digits: Vec<Port> = shaped
        .inputs
        .into_iter()
        .filter(|port| port.signal_strength)
        .collect();
    if !digits.is_empty() {
        // The digits' buses are A, B, and on, so a lever bus named by
        // the next letter follows them.
        let mut letters = ('A'..='Z').skip(digits.len());
        for port in &mut interface.inputs {
            let lettered = !port.labelled
                && port.name.len() == 1
                && port.name.chars().all(|c| c.is_ascii_uppercase());
            if lettered {
                port.name =
                    letters.next().map(String::from).unwrap_or_default();
            }
        }
        interface.inputs.splice(0..0, digits);
    }
    interface.outputs = shaped.outputs;
    interface
}

/// The lines of the input blocks `feeds` and output blocks `outputs`, each
/// grouped by its colour as [`ports`] groups them, the buses running
/// alongside a marked one ordered as it is.
pub(crate) fn read_from(
    blocks: &Blocks,
    feeds: &HashMap<Pos, Feed>,
    outputs: Vec<Pos>,
) -> Interface {
    let signs = line_signs(blocks);
    // In a fixed order, so bits as near the corner as each other, and
    // lines, always come out the same way.
    let mut inputs: Vec<Pos> = feeds.keys().copied().collect();
    inputs.sort_unstable();
    let mut interface = Interface {
        inputs: ports(blocks, inputs, &signs, Some(feeds)),
        outputs: ports(blocks, outputs, &signs, None),
    };
    follow_marks(blocks, &mut interface);
    interface
}

/// The signs naming a build's lines, each by the block it names: any sign
/// on an input block, and an output's sign under its front, see
/// [`repeater_outputs::signs`], as any other sign on an output's block
/// names nothing. Inside [`crate::recognize::legacy::reading`], every sign
/// by the block it is on, as the old rule read them.
pub fn line_signs(blocks: &Blocks) -> HashMap<Pos, &[String]> {
    let mut signs = blocks.sign_texts();
    if crate::recognize::legacy::old() {
        return signs;
    }
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

/// Groups `members` into ports, `feeds` being given for inputs. Each
/// control or status line with a name sign is a line of its own. Every
/// other colour is one bus, named by a sign on any of its blocks, or else
/// by the next letter for an input, A for lime, or by its place among the
/// output colours.
fn ports(
    blocks: &Blocks,
    members: impl IntoIterator<Item = Pos>,
    signs: &HashMap<Pos, &[String]>,
    feeds: Option<&HashMap<Pos, Feed>>,
) -> Vec<Port> {
    let (order, line): (&[&str], _) = if feeds.is_some() {
        (&colour::INPUT_COLOURS, colour::CONTROL)
    } else {
        (&colour::OUTPUT_COLOURS, colour::STATUS)
    };
    let members: Vec<Pos> = members.into_iter().collect();
    let rows = line_rows(blocks, &members, signs, line);
    let in_row: HashSet<Pos> = rows.values().flatten().copied().collect();
    let mut named = Vec::new();
    let mut buses: BTreeMap<&str, Vec<Pos>> = BTreeMap::new();
    for pos in members.into_iter().filter(|pos| !in_row.contains(pos)) {
        let lines = signs.get(&pos).copied().unwrap_or_default();
        let colour = colour(blocks.get(pos));
        match sign_name(lines).filter(|_| colour == line) {
            Some(name) => {
                // The line, and any row it names.
                let mut row = vec![pos];
                row.extend(rows.get(&pos).into_iter().flatten());
                let mut port = port(blocks, name, row, lines, feeds);
                port.labelled = true;
                named.push(port);
            }
            None => buses.entry(colour).or_default().push(pos),
        }
    }
    let rank = |colour: &str| {
        order
            .iter()
            .position(|c| *c == colour)
            .unwrap_or(usize::MAX)
    };
    let mut colours: Vec<&str> = buses.keys().copied().collect();
    colours.sort_by_key(|colour| (rank(colour), *colour));
    let mut letters = 'A'..='Z';
    let output_count = colours.iter().filter(|c| **c != line).count();
    let mut result: Vec<Port> = colours
        .into_iter()
        .enumerate()
        .map(|(i, colour)| {
            let members = buses.remove(colour).unwrap_or_default();
            // A sign on any bit can carry the bus's strength.
            let lines: Vec<String> = members
                .iter()
                .filter_map(|pos| signs.get(pos))
                .flat_map(|lines| lines.iter().cloned())
                .collect();
            let given = members
                .iter()
                .find_map(|pos| signs.get(pos).and_then(|l| sign_name(l)));
            let name = if colour == line {
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
            port(blocks, name, members, &lines, feeds)
        })
        .collect();
    result.extend(named);
    result
}

/// The rows that signs name among the `line` colour's `members`: for each
/// signed block, the unsigned blocks of that colour that continue it.
fn line_rows(
    blocks: &Blocks,
    members: &[Pos],
    signs: &HashMap<Pos, &[String]>,
    line: &str,
) -> HashMap<Pos, Vec<Pos>> {
    let named = |pos: &Pos| signs.get(pos).and_then(|l| sign_name(l)).is_some();
    let of_line: Vec<Pos> = members
        .iter()
        .copied()
        .filter(|&pos| colour(blocks.get(pos)) == line)
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
    if let Some(&lsb) = members.iter().min_by_key(|&&pos| distance(pos, corner))
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
        block: members
            .first()
            .map(|&pos| block_label(blocks.get(pos)))
            .unwrap_or_default(),
        labelled: false,
        bits,
        strength: lines.iter().find_map(|l| strength(l)).unwrap_or(1),
        words,
        signal_strength,
    }
}

/// The colour an input or output block marks, such as `yellow` for yellow
/// stained glass or `lime` for lime stained glass under a comparator. Any
/// other block is its own kind.
fn colour(state: &str) -> &str {
    let block = schem::short(state);
    colour::output_colour(block)
        .or_else(|| colour::input_mark(block))
        .or_else(|| block.strip_suffix("_concrete"))
        .unwrap_or(block)
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

fn distance(a: Pos, b: Pos) -> i32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).sum()
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
    ((pos[axis] - from[axis]).abs(), distance(pos, from))
}

/// A block state as words, such as `lime concrete`.
fn block_label(state: &str) -> String {
    let id = schem::id(state);
    id.strip_prefix("minecraft:")
        .unwrap_or(id)
        .replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: [&str; 4]) -> Vec<String> {
        text.map(str::to_owned).to_vec()
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
