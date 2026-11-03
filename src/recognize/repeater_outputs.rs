//! The rule the site reads outputs and signal-strength inputs by, in place
//! of their colours: an output is a repeater or comparator powering
//! nothing, the cell in front of it, the side it gives out from, air. A
//! repeater gives a bit, on or off; whether a comparator's strength is read
//! as a bit or a value from 0 to 15 its category's test decides. Colour
//! plays no part, so any block may hold one up. Outputs side by side,
//! evenly spaced and facing the same way, are a bus; one alone is a line.
//! Inputs are levers and buttons on any block, and, for a hex digit, a
//! comparator with nothing behind it, the cell its `facing` points at air,
//! giving out into the build, which a tester drives by putting a barrel
//! there; these too are read by where they stand, not their colours.
//!
//! [`crate::recognize::inspect`] checks a build by this rule, and
//! [`interface`] reads its lines' roles by their shapes. `rsdb convert`
//! ports a build saved under the old rule, see
//! [`crate::recognize::legacy`], to this one by [`tap`], promoting each
//! output's driver, [`promotable`], or tapping it, and [`feed`], as the
//! owner ports one by hand.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;

use crate::{
    category::Kind,
    recognize::{
        inspect::{self, Run},
        interface::{self, Bit, Interface, Port},
        legacy,
        probe::{self, Fault, Found, Layout},
    },
    schem::{self, Blocks, Direction, Pos, lines::Role},
};

/// A repeater or comparator powering nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
    /// Where the repeater or comparator stands.
    pub at: Pos,
    /// A comparator, read as a strength, rather than a repeater, read as a
    /// bit.
    pub comparator: bool,
    /// Its block state's `facing`: the side it reads from, its input. It
    /// gives out the other way.
    pub facing: Direction,
}

impl Output {
    /// The way it gives out, opposite its `facing`.
    pub fn points(self) -> Direction {
        self.facing.opposite()
    }

    /// The cell it would power, which is air.
    pub fn front(self) -> Pos {
        schem::step(self.at, self.points())
    }

    /// The cell it reads from.
    pub fn behind(self) -> Pos {
        schem::step(self.at, self.facing)
    }

    /// The block holding it up, which the site's tests read an output by,
    /// as what stands on it.
    pub fn block(self) -> Pos {
        schem::step(self.at, Direction::Down)
    }

    /// Where a sign naming it goes: a wall sign on the outward face of the
    /// block holding it up, under its front, which stays air.
    pub fn sign_place(self) -> Pos {
        schem::step(self.block(), self.points())
    }

    /// `repeater` or `comparator`.
    pub fn kind(self) -> &'static str {
        if self.comparator {
            "comparator"
        } else {
            "repeater"
        }
    }
}

/// Whether `state` is air of any kind.
fn is_air(state: &str) -> bool {
    matches!(schem::short(state), "air" | "cave_air" | "void_air")
}

/// Every repeater and comparator in `blocks` whose front is air, beyond
/// the file's box too, in the file's order.
pub fn find(blocks: &Blocks) -> Vec<Output> {
    blocks
        .iter()
        .filter_map(|(at, state)| {
            let comparator = match schem::short(state) {
                "repeater" => false,
                "comparator" => true,
                _ => return None,
            };
            let facing = Direction::facing(state)?;
            let output = Output {
                at,
                comparator,
                facing,
            };
            is_air(blocks.get(output.front())).then_some(output)
        })
        .collect()
}

/// The sign naming each output of `blocks`, by the block holding the
/// output up: a wall sign on that block's outward face, under the
/// output's front, see [`Output::sign_place`]. A sign anywhere else on the
/// block names nothing.
pub fn signs(blocks: &Blocks) -> HashMap<Pos, &[String]> {
    find(blocks)
        .into_iter()
        .filter_map(|output| {
            let place = output.sign_place();
            let lines = blocks.signs.get(&place)?;
            (blocks.sign_block(place) == Some(output.block()))
                .then_some((output.block(), lines.as_slice()))
        })
        .collect()
}

/// `outputs` grouped into buses, each a line of outputs of one kind,
/// repeaters or comparators, evenly spaced and facing the same way, or two
/// such lines interleaved, facing the same way or opposite ways, the
/// longest first, then the lone lines; each run is of the outputs' own
/// places.
pub fn lines(outputs: &[Output]) -> Vec<Run> {
    let bits: Vec<(Pos, (Direction, bool))> = outputs
        .iter()
        .map(|o| (o.at, (o.facing, o.comparator)))
        .collect();
    // Two columns may give out on opposite faces, as a tall build's do.
    interleaved(inspect::runs(&bits), &bits, |a, b| {
        a.1 == b.1 && (a.0 == b.0 || a.0 == b.0.opposite())
    })
}

/// `runs` of `bits`, each with its kind, with each two that interleave
/// made one bus: as many bits each, two or more, the same step, kinds
/// that `alike` takes as one, the step even along one axis, and each bit
/// of one halfway between two of the other along it, as a bus whose bits
/// alternate between two columns, a layer apart where each column's are
/// two. Of the runs one could interleave with, the nearest; the longest
/// first again.
fn interleaved<K: Copy>(
    runs: Vec<Run>,
    bits: &[(Pos, K)],
    alike: impl Fn(K, K) -> bool,
) -> Vec<Run> {
    let kind: BTreeMap<Pos, K> = bits.iter().copied().collect();
    // The axis two runs interleave along and which comes first, if they do.
    let interleave = |a: &Run, b: &Run| -> Option<(usize, bool)> {
        if a.bits.len() != b.bits.len()
            || a.bits.len() < 2
            || a.step != b.step
            || !alike(kind[&a.bits[0]], kind[&b.bits[0]])
        {
            return None;
        }
        let axis = (0..3).find(|&axis| {
            a.step[axis] != 0
                && a.step[axis] % 2 == 0
                && (0..3).all(|o| o == axis || a.step[o] == 0)
        })?;
        let half = a.step[axis] / 2;
        let offset = |from: &Run, to: &Run, by: i32| {
            from.bits
                .iter()
                .zip(&to.bits)
                .all(|(f, t)| t[axis] == f[axis] + by)
        };
        if offset(a, b, half) {
            Some((axis, true))
        } else if offset(b, a, half) {
            Some((axis, false))
        } else {
            None
        }
    };
    let apart = |a: &Run, b: &Run| {
        (0..3)
            .map(|axis| (a.bits[0][axis] - b.bits[0][axis]).abs())
            .sum::<i32>()
    };
    let mut pairs: Vec<(i32, usize, usize)> = Vec::new();
    for i in 0..runs.len() {
        for j in i + 1..runs.len() {
            if interleave(&runs[i], &runs[j]).is_some() {
                pairs.push((apart(&runs[i], &runs[j]), i, j));
            }
        }
    }
    pairs.sort_unstable();
    let mut used = vec![false; runs.len()];
    let mut found = Vec::new();
    for (_, i, j) in pairs {
        if used[i] || used[j] {
            continue;
        }
        used[i] = true;
        used[j] = true;
        let Some((axis, a_first)) = interleave(&runs[i], &runs[j]) else {
            continue;
        };
        let (first, second) = if a_first {
            (&runs[i], &runs[j])
        } else {
            (&runs[j], &runs[i])
        };
        let mut step = [0; 3];
        step[axis] = first.step[axis] / 2;
        found.push(Run {
            bits: first
                .bits
                .iter()
                .zip(&second.bits)
                .flat_map(|(&f, &s)| [f, s])
                .collect(),
            step,
        });
    }
    found.extend(
        runs.into_iter()
            .zip(used)
            .filter(|(_, used)| !used)
            .map(|(run, _)| run),
    );
    found.sort_by_key(|run| std::cmp::Reverse(run.bits.len()));
    found
}

/// The output colour of the block under `output`, such as `yellow`, the
/// role the site coloured it in, or the old rule read it by; `None` for any
/// other block.
pub fn colour_under(blocks: &Blocks, output: Output) -> Option<&str> {
    legacy::output_colour(schem::short(blocks.get(output.block())))
}

/// A comparator with nothing behind it, the cell its `facing` points at
/// air, giving out into the build: a signal-strength input, a hex digit
/// from 0 to 15, as strong as what a tester puts behind it, a barrel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrengthInput {
    /// Where the comparator stands.
    pub at: Pos,
    /// Its block state's `facing`: the side it reads from, where the
    /// tester's barrel goes.
    pub facing: Direction,
}

impl StrengthInput {
    /// The cell it reads from, which is air.
    pub fn behind(self) -> Pos {
        schem::step(self.at, self.facing)
    }

    /// The cell it gives out into, the build's.
    pub fn front(self) -> Pos {
        schem::step(self.at, self.facing.opposite())
    }
}

/// The comparator at `at` as a signal-strength input, when it is one: air
/// behind it, beyond the file's box too, and something in front of it.
pub fn strength_input(blocks: &Blocks, at: Pos) -> Option<StrengthInput> {
    let state = blocks.get(at);
    if schem::short(state) != "comparator" {
        return None;
    }
    let input = StrengthInput {
        at,
        facing: Direction::facing(state)?,
    };
    (is_air(blocks.get(input.behind())) && !is_air(blocks.get(input.front())))
        .then_some(input)
}

/// Every signal-strength input in `blocks`, in the file's order.
pub fn strength_inputs(blocks: &Blocks) -> Vec<StrengthInput> {
    blocks
        .iter()
        .filter_map(|(at, _)| strength_input(blocks, at))
        .collect()
}

/// `inputs` grouped into buses as [`lines`] groups outputs, each run of
/// the comparators' own places.
pub fn strength_lines(inputs: &[StrengthInput]) -> Vec<Run> {
    let bits: Vec<(Pos, Direction)> =
        inputs.iter().map(|i| (i.at, i.facing)).collect();
    interleaved(inspect::runs(&bits), &bits, |a, b| a == b)
}

/// The signal-strength input driving the digit read at `bit`: the
/// comparator on `bit`, the block under it, as the site reads a digit, or
/// at `bit` itself; or one beside the dust on `bit` pointing into it, as a
/// port of a build whose digit inputs the old rule read by their colour
/// keeps that block.
pub fn driving(blocks: &Blocks, bit: Pos) -> Option<StrengthInput> {
    let on = schem::step(bit, Direction::Up);
    if let Some(input) =
        strength_input(blocks, on).or_else(|| strength_input(blocks, bit))
    {
        return Some(input);
    }
    let dust = on;
    if schem::short(blocks.get(dust)) != "redstone_wire" {
        return None;
    }
    Direction::SIDES.into_iter().find_map(|side| {
        strength_input(blocks, schem::step(dust, side))
            .filter(|input| input.front() == dust)
    })
}

/// A build's lines as this rule reads them, named for the roles their
/// shapes give them, as a test reads the roles probing finds: its
/// signal-strength buses A, B, and on, the longest first, then its buses
/// of levers; a lone lever its control line, such as a carry in; each bus
/// of outputs an output, a comparator's read as a strength in a build
/// without signal-strength inputs, whose outputs each show a digit; and a
/// lone output beside a bus, or a lone repeater beside a comparator of a
/// build with signal-strength inputs, its status line, such as a carry
/// out. Each line is read by the blocks under its comparators and
/// repeaters, as [`inspect::inspect`] finds them.
pub fn interface(blocks: &Blocks) -> Interface {
    let switches: BTreeMap<Pos, schem::Switch> = blocks
        .switches()
        .into_iter()
        .map(|switch| (switch.block, switch))
        .collect();
    let report = inspect::inspect(blocks);
    let mut letters = (b'A'..=b'Z').map(|l| char::from(l).to_string());
    let mut operands = 0..=u8::MAX;
    let mut operand = |bits: Vec<Bit>, strength: bool| Port {
        name: letters.next().unwrap_or_default(),
        role: operands.next().map(Role::Operand),
        labelled: false,
        bits,
        strength: 1,
        words: Vec::new(),
        signal_strength: strength,
    };
    let mut inputs = Vec::new();
    for run in &report.digits {
        let mut bits = run.bits.clone();
        interface::lsb_first(blocks, &mut bits);
        let bits = bits.into_iter().map(Bit::at).collect();
        inputs.push(operand(bits, true));
    }
    let mut control = Vec::new();
    for run in &report.inputs {
        let mut members = run.bits.clone();
        interface::lsb_first(blocks, &mut members);
        let bits: Vec<Bit> = members
            .iter()
            .filter_map(|pos| {
                let switch = switches.get(pos)?;
                Some(Bit {
                    pos: *pos,
                    entry: Some(switch.side),
                    hard: Direction::ALL.iter().any(|&side| {
                        schem::short(blocks.get(schem::step(*pos, side)))
                            == "redstone_wire"
                    }),
                    inverted: switch.on,
                })
            })
            .collect();
        if bits.len() == 1 {
            control.push(Port {
                name: "control line".to_owned(),
                role: Some(Role::Control),
                labelled: false,
                bits,
                strength: 1,
                words: Vec::new(),
                signal_strength: false,
            });
        } else {
            inputs.push(operand(bits, false));
        }
    }
    inputs.extend(control);
    let digits = !report.digits.is_empty();
    let found = find(blocks);
    let by_block: BTreeMap<Pos, Output> =
        found.iter().map(|&o| (o.block(), o)).collect();
    let runs: Vec<Run> = report.outputs;
    let buses = runs.iter().filter(|run| run.bits.len() > 1).count();
    let comparators = found.iter().any(|o| o.comparator);
    let mut outputs = Vec::new();
    let mut places = 0..=u8::MAX;
    for (i, run) in runs.iter().enumerate() {
        let mut members = run.bits.clone();
        interface::lsb_first(blocks, &mut members);
        let comparator = members
            .iter()
            .all(|pos| by_block.get(pos).is_some_and(|o| o.comparator));
        let status = members.len() == 1
            && (buses > 0 || digits && comparators && !comparator);
        let (name, role) = if status {
            ("status line".to_owned(), Some(Role::Status))
        } else if i == 0 {
            ("output".to_owned(), places.next().map(Role::Output))
        } else {
            (format!("output {}", i + 1), places.next().map(Role::Output))
        };
        outputs.push(Port {
            name,
            role,
            labelled: false,
            bits: members.into_iter().map(Bit::at).collect(),
            strength: 1,
            words: Vec::new(),
            signal_strength: comparator && !digits && !status,
        });
    }
    Interface { inputs, outputs }
}

/// What probing makes of a build whose outputs this rule reads.
pub struct Probed {
    pub kind: Kind,
    /// Its data inputs, its file coloured by the roles found, and what it
    /// was found to be, in words.
    pub found: Found,
    /// For an adder, the roles found.
    pub layout: Option<Layout>,
}

/// The build in `data` probed with its outputs read by this rule, as
/// `meant` alone, or as each kind probing tries in turn, as
/// [`probe::recognize_as`] does: the first kind it holds as, or the fault
/// of each it was tried as. A build with faults in its inputs or outputs is
/// probed all the same, as a measure.
pub fn probe(
    data: &[u8],
    meant: Option<Kind>,
) -> Result<Result<Probed, Vec<(Kind, Fault)>>> {
    let simulated = probe::simulatable(data)?;
    let blocks = Blocks::read(&simulated)?;
    let report = inspect::inspect(&blocks);
    let mut faults = Vec::new();
    for (kind, ..) in Kind::probed() {
        if meant.is_some_and(|meant| probed_as(meant) != kind) {
            continue;
        }
        if kind == Kind::Adder {
            match probe::adder(&simulated, &blocks, &report) {
                Ok(probed) => {
                    let layout = probed.layout.clone();
                    let kind = probed.run.adder_category(false);
                    let found = format!(
                        "{}: all {} cases right, {}",
                        kind.a(),
                        probed.run.cases,
                        probed.run.pattern
                    );
                    let file = probe::with_layout(&simulated, &layout)?;
                    let data_inputs = [layout.a.clone(), layout.b.clone()];
                    return Ok(Ok(Probed {
                        kind,
                        found: (data_inputs.concat(), file, found),
                        layout: Some(layout),
                    }));
                }
                Err(fault) => faults.push((kind, fault)),
            }
            continue;
        }
        match probe::probe_as(kind, &simulated, &blocks, &report)? {
            Ok(found) => {
                return Ok(Ok(Probed {
                    kind,
                    found,
                    layout: None,
                }));
            }
            Err(fault) => faults.push((kind, fault)),
        }
    }
    Ok(Err(faults))
}

/// The kind `kind` is probed as, such as an adder for a subtractor.
fn probed_as(kind: Kind) -> Kind {
    match kind.facts().probe {
        crate::category::Probing::As(other) => other,
        _ => kind,
    }
}

/// The sides a tap is tried on, the build's front, north, first.
const OUTWARD: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

/// An output, as the old rule read it, tapped so this rule reads it: the
/// block it was read by whitened, what stands on it left, and beside it,
/// one block out of the build, a support with a repeater or comparator on
/// top reading what stands on the output block and giving out into air.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tapped {
    /// A repeater or comparator powering nothing stands on it already, and
    /// it is left as it is.
    Already { block: Pos },
    /// Tapped toward `side`: the support at `support`, white concrete, or
    /// white stained glass with `glass` as a torch is under it, and on it
    /// the repeater or comparator `now`. `clear` when every cell from the
    /// output block to the box's edge that way was air, the face the output
    /// sits on.
    Tapped {
        block: Pos,
        side: Direction,
        support: Pos,
        glass: bool,
        clear: bool,
        now: String,
    },
    /// Its driver promoted, as [`promotable`] finds it: the output block
    /// and the dust on it taken out, so the repeater or comparator
    /// `driver`, which pointed into one of them, gives out into air; or,
    /// where a torch drove it, the torch moved, `torch` saying from where
    /// to where, and `driver` put in front of it.
    Promoted {
        block: Pos,
        driver: Output,
        torch: Option<(Pos, Pos)>,
    },
    /// It can't be tapped, for `why`.
    Not { block: Pos, why: String },
}

impl Tapped {
    /// The output block it was made of.
    pub fn block(&self) -> Pos {
        match self {
            Tapped::Already { block }
            | Tapped::Tapped { block, .. }
            | Tapped::Promoted { block, .. }
            | Tapped::Not { block, .. } => *block,
        }
    }

    /// The output it was made into, where the rule for outputs reads it:
    /// a tap's repeater or comparator, `comparator` saying which, or a
    /// promoted driver; `None` for one already an output or not tapped.
    pub fn output(&self, comparator: bool) -> Option<Output> {
        match *self {
            Tapped::Tapped { side, support, .. } => Some(Output {
                at: schem::step(support, Direction::Up),
                comparator,
                facing: side.opposite(),
            }),
            Tapped::Promoted { driver, .. } => Some(driver),
            Tapped::Already { .. } | Tapped::Not { .. } => None,
        }
    }
}

/// The default of an output block's kind: white stained glass for stained
/// glass, else white concrete.
fn whitened(state: &str) -> &'static str {
    if schem::short(state).ends_with("_stained_glass") {
        "minecraft:white_stained_glass"
    } else {
        "minecraft:white_concrete"
    }
}

/// Whether a block in a cell would be inside the box of `blocks`.
fn inside(blocks: &Blocks, pos: Pos) -> bool {
    (0..3).all(|axis| (0..blocks.size[axis]).contains(&pos[axis]))
}

/// Whether every cell from `block` out to the box's edge toward `side`, at
/// its level and the one above, is air: the face of the build the output
/// sits on.
fn clear_to_edge(blocks: &Blocks, block: Pos, side: Direction) -> bool {
    let mut at = schem::step(block, side);
    while inside(blocks, at) {
        if !is_air(blocks.get(at))
            || !is_air(blocks.get(schem::step(at, Direction::Up)))
        {
            return false;
        }
        at = schem::step(at, side);
    }
    true
}

/// The signs on `block`, which name the output on it, in order.
fn own_signs(blocks: &Blocks, block: Pos) -> Vec<Pos> {
    let mut signs: Vec<Pos> = blocks
        .signs
        .keys()
        .copied()
        .filter(|&sign| blocks.sign_block(sign) == Some(block))
        .collect();
    signs.sort_unstable();
    signs
}

/// The cells a tap toward `side` from `block` takes: the support, the
/// repeater on it, and the air it gives out into, and where a sign on the
/// block goes, under that, see [`Output::sign_place`]. Each must be air,
/// or a sign on the block, which moves, and not a cell another tap took,
/// as `taken` holds them; why the tap can't be made when one isn't.
fn tap_cells(
    blocks: &Blocks,
    block: Pos,
    side: Direction,
    taken: &BTreeMap<Pos, String>,
) -> Result<(Pos, Pos), String> {
    let own = own_signs(blocks, block);
    let support = schem::step(block, side);
    let top = schem::step(support, Direction::Up);
    let front = schem::step(top, side);
    let mut cells =
        vec![(support, "support"), (top, "repeater"), (front, "front")];
    if !own.is_empty() {
        cells.push((schem::step(support, side), "sign"));
    }
    for (cell, what) in cells {
        let state = taken
            .get(&cell)
            .map_or_else(|| blocks.get(cell), String::as_str);
        // Air another tap's sign is to go in is taken.
        let free = (is_air(state)
            && taken.get(&cell).is_none_or(|t| t != schem::AIR))
            || (own.contains(&cell) && !taken.contains_key(&cell));
        if !free {
            let reason = match what {
                "front" => "it would give out into",
                "sign" => "its sign would go in",
                _ => "its place holds",
            };
            return Err(format!(
                "{side}: {} at {}, which the {what} {reason}",
                schem::short(state).replace('_', " "),
                inspect::place(cell)
            ));
        }
    }
    Ok((support, top))
}

/// Marks the cells a tap's support and repeater take as `held`, and the
/// place its sign goes, if the output has one, as air another tap's cells
/// can't take.
fn take(
    taken: &mut BTreeMap<Pos, String>,
    blocks: &Blocks,
    (block, side): (Pos, Direction),
    (support, top): (Pos, Pos),
    held: &str,
) {
    taken.insert(support, held.to_owned());
    taken.insert(top, held.to_owned());
    if !own_signs(blocks, block).is_empty() {
        taken
            .entry(schem::step(support, side))
            .or_insert_with(|| schem::AIR.to_owned());
    }
}

/// What reads the output block, standing on it, that a tap can read from:
/// dust, or else why not.
fn tappable(blocks: &Blocks, top: Pos) -> Result<(), String> {
    match schem::short(blocks.get(top)) {
        "redstone_wire" => Ok(()),
        name @ ("repeater" | "comparator") => {
            let state = blocks.get(top);
            let front = Direction::facing(state)
                .map_or(top, |f| schem::step(top, f.opposite()));
            Err(format!(
                "its {name} gives out into {} at {}, and a tap reads only dust",
                schem::short(blocks.get(front)).replace('_', " "),
                inspect::place(front)
            ))
        }
        other => Err(format!("{} on it, not dust", other.replace('_', " "))),
    }
}

/// Whether `state`, beside a block, dust, or torch, takes no power from
/// it: air, a sign, or a block that is no redstone component, as a block
/// powers no block beside it.
fn unmoved(state: &str) -> bool {
    let name = schem::short(state);
    is_air(state) || name.ends_with("sign") || !schem::block::is_component(name)
}

/// A block in `blocks` at `at` in words, with `what` of it, such as
/// `stone at (1, 2, 3), beside its dust`.
fn held(blocks: &Blocks, at: Pos, what: &str) -> String {
    format!(
        "{} at {}, {what}",
        schem::short(blocks.get(at)).replace('_', " "),
        inspect::place(at)
    )
}

/// An output, as the old rule read it, with its driver promoted, as
/// [`promotable`] finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promotion {
    /// The repeater or comparator read as the output now.
    pub driver: Output,
    /// Where the torch that drove it stood and where it stands now, on
    /// another face of its block, when a torch drove it.
    pub torch: Option<(Pos, Pos)>,
    /// Every cell it changes: the output block and its dust taken out,
    /// and for a torch, the torch moved, the repeater or comparator put in
    /// front of it, and a support under that where there was none.
    pub changes: Vec<(Pos, String)>,
}

/// Why the output block at `block`, with its dust at `top`, can't be taken
/// out, if anything but its dust, its driver at `driver`, and the cells in
/// `going`, taken out with it, might take power from either: anything
/// beside the block that might, or beside the dust, anything the dust
/// joins, climbs, points into, or runs down into.
fn alone(
    blocks: &Blocks,
    (block, top): (Pos, Pos),
    driver: Pos,
    going: &[Pos],
) -> Result<(), String> {
    let beside_block = Direction::ALL
        .into_iter()
        .map(|side| schem::step(block, side))
        .filter(|&at| at != top && at != driver && !going.contains(&at));
    for at in beside_block {
        if !unmoved(blocks.get(at)) {
            return Err(held(
                blocks,
                at,
                "beside it, might take power from it",
            ));
        }
    }
    let dust = blocks.get(top);
    for side in OUTWARD {
        let at = schem::step(top, side);
        if at == driver || going.contains(&at) {
            continue;
        }
        let state = blocks.get(at);
        let joins = schem::property(dust, side.name());
        if joins == Some("up") {
            return Err(held(blocks, at, "which its dust climbs"));
        }
        if !unmoved(state) || (joins == Some("side") && !is_air(state)) {
            return Err(held(
                blocks,
                at,
                "beside its dust, might take power from it",
            ));
        }
        let below = schem::step(at, Direction::Down);
        if !schem::block::conductor(state)
            && schem::short(blocks.get(below)) == "redstone_wire"
            && !going.contains(&below)
        {
            return Err(held(
                blocks,
                below,
                "which its dust might run down into",
            ));
        }
    }
    Ok(())
}

/// Why a torch at `torch` might power anything beside it but the cells in
/// `except`, if it might: anything beside it that takes power, or a block
/// over it, which it powers, with anything beside that block that does.
fn powers_only(
    blocks: &Blocks,
    torch: Pos,
    except: &[Pos],
) -> Result<(), String> {
    for side in Direction::ALL {
        let at = schem::step(torch, side);
        if except.contains(&at) {
            continue;
        }
        if !unmoved(blocks.get(at)) {
            return Err(held(blocks, at, "beside its torch"));
        }
    }
    let over = schem::step(torch, Direction::Up);
    if schem::block::conductor(blocks.get(over)) {
        for side in Direction::ALL {
            let at = schem::step(over, side);
            if at != torch && !unmoved(blocks.get(at)) {
                return Err(held(
                    blocks,
                    at,
                    "beside the block its torch powers",
                ));
            }
        }
    }
    Ok(())
}

/// The output on `block`, as the old rule read it, with dust on top, as
/// its driver promoted reads it, the output block and its dust taken out,
/// the build no bigger, as the owner ports one by hand. Its driver, the
/// one thing that powers either, is a repeater or comparator pointing into
/// the block or into its dust, which then gives out into air, its side
/// inputs as they are; or a wall torch beside the dust, facing it, which
/// moves to another face of the block it is on, with a repeater in front
/// of it, or a comparator where `comparator` says the output is read as a
/// strength, giving out the way the torch faced into air, held up by the
/// block under it or white concrete put there, or white stained glass
/// over a torch or dust, as a tap's support is; the first face, north,
/// south, west, east, where the torch and repeater fit inside the build's
/// box, as outside it they would make the build bigger. Or why it
/// can't be: no such driver, or two; anything but the dust beside the
/// block that might take power from it, or beside the dust, but its
/// driver, that the dust joins, points into, or might power, see
/// [`alone`]; a torch, where it stood or where it would go, that powers
/// anything else, see [`powers_only`]; a repeater driving an output read
/// as a strength; a driver not held up by a solid block or glass, which
/// the old rule's test colours as the output block was; or a sign on the
/// block whose place, under the driver's front, holds something. `going`
/// holds the other cells taken out with it, as the other output blocks of
/// its run and their dust, which take no power from it then.
pub fn promotable(
    blocks: &Blocks,
    block: Pos,
    going: &[Pos],
    comparator: bool,
) -> Result<Promotion, String> {
    let top = schem::step(block, Direction::Up);
    if schem::short(blocks.get(top)) != "redstone_wire" {
        return Err("no dust on it".to_owned());
    }
    let own = own_signs(blocks, block);
    // A sign on the block goes under the driver's front, which must be
    // free for it.
    let sign_free = |driver: Output| {
        let place = driver.sign_place();
        if own.is_empty()
            || place == block
            || place == top
            || own.contains(&place)
            || is_air(blocks.get(place))
        {
            Ok(())
        } else {
            Err(held(blocks, place, "where its sign would go"))
        }
    };
    let cleared =
        vec![(block, schem::AIR.to_owned()), (top, schem::AIR.to_owned())];
    match drivers(blocks, block, top) {
        Driver::Component(driver) => {
            alone(blocks, (block, top), driver.at, going)?;
            if comparator && !driver.comparator {
                return Err(
                    "its repeater would read a strength as 15 for any on"
                        .to_owned(),
                );
            }
            if !holds_up(blocks.get(driver.block())) {
                return Err(held(
                    blocks,
                    driver.block(),
                    "holding its driver up, neither a solid block nor glass",
                ));
            }
            sign_free(driver)?;
            Ok(Promotion {
                driver,
                torch: None,
                changes: cleared,
            })
        }
        Driver::Torch(torch) => {
            alone(blocks, (block, top), torch, going)?;
            let state = blocks.get(torch);
            let out = Direction::facing(state).expect("a wall torch faces");
            let on = schem::step(torch, out.opposite());
            powers_only(blocks, torch, &[on, top])?;
            let mut blocked = Vec::new();
            for face in OUTWARD {
                if face.north_south() == out.north_south() {
                    continue;
                }
                let moved = torch_moved(blocks, torch, face, going, comparator)
                    .and_then(|mut promotion| {
                        sign_free(promotion.driver)?;
                        promotion.changes.splice(0..0, cleared.clone());
                        Ok(promotion)
                    });
                match moved {
                    Ok(promotion) => return Ok(promotion),
                    Err(why) => blocked.push(format!("{face}: {why}")),
                }
            }
            Err(format!(
                "its torch has no face to move to: {}",
                blocked.join("; ")
            ))
        }
        Driver::None => {
            Err("no repeater, comparator, or wall torch drives it alone"
                .to_owned())
        }
        Driver::Many => Err("more than one thing drives it".to_owned()),
    }
}

/// What drives an output block and its dust, as [`drivers`] finds it.
enum Driver {
    /// A repeater or comparator pointing into one of them.
    Component(Output),
    /// A wall torch beside the dust, facing it.
    Torch(Pos),
    None,
    /// More than one of these.
    Many,
}

/// What drives the output block at `block`, with its dust at `top`.
fn drivers(blocks: &Blocks, block: Pos, top: Pos) -> Driver {
    let mut drivers = Vec::new();
    for into in [block, top] {
        for side in OUTWARD {
            let at = schem::step(into, side);
            let state = blocks.get(at);
            let comparator = match schem::short(state) {
                "repeater" => false,
                "comparator" => true,
                _ => continue,
            };
            let Some(facing) = Direction::facing(state) else {
                continue;
            };
            let driver = Output {
                at,
                comparator,
                facing,
            };
            if driver.front() == into {
                drivers.push(driver);
            }
        }
    }
    let torches: Vec<Pos> = OUTWARD
        .into_iter()
        .map(|side| schem::step(top, side))
        .filter(|&at| {
            let state = blocks.get(at);
            schem::short(state) == "redstone_wall_torch"
                && Direction::facing(state)
                    .is_some_and(|f| schem::step(at, f) == top)
        })
        .collect();
    match (drivers.as_slice(), torches.as_slice()) {
        ([driver], []) => Driver::Component(*driver),
        ([], [torch]) => Driver::Torch(*torch),
        ([], []) => Driver::None,
        _ => Driver::Many,
    }
}

/// Whether the block in `state` can hold up an output the old rule's
/// test reads by colouring it: a solid block or glass, not a component.
fn holds_up(state: &str) -> bool {
    let name = schem::short(state);
    !schem::block::is_component(name)
        && (schem::block::solid(state) || name.ends_with("glass"))
}

/// The wall torch at `torch`, facing an output's dust, moved to the face
/// `face` of the block it is on, with a repeater, or a comparator with
/// `comparator`, in front of it giving out the way the torch faced into
/// air, on the block under it or a support put there; or why not: a cell
/// it takes outside the box or not air, or in `going`, the block under the
/// repeater one that can't hold it up, something beside the repeater that
/// would run into its side, or the torch powering anything else there.
fn torch_moved(
    blocks: &Blocks,
    torch: Pos,
    face: Direction,
    going: &[Pos],
    comparator: bool,
) -> Result<Promotion, String> {
    let state = blocks.get(torch);
    let out = Direction::facing(state).expect("a wall torch faces");
    let on = schem::step(torch, out.opposite());
    let moved = schem::step(on, face);
    let at = schem::step(torch, face);
    let under = schem::step(at, Direction::Down);
    // A torch moved out of the box would make the build bigger.
    if ![moved, at, under].iter().all(|&c| inside(blocks, c)) {
        return Err("out of the box".to_owned());
    }
    let driver = Output {
        at,
        comparator,
        facing: out.opposite(),
    };
    for (cell, what) in [
        (moved, "where its torch would go"),
        (at, "where its repeater would go"),
        (driver.front(), "where its repeater would give out"),
    ] {
        if !is_air(blocks.get(cell)) || going.contains(&cell) {
            return Err(held(blocks, cell, what));
        }
    }
    let below = blocks.get(under);
    let support = if is_air(below) {
        // As a tap's support is, glass over a torch or dust.
        let glass = matches!(
            schem::short(blocks.get(schem::step(under, Direction::Down))),
            "redstone_torch" | "redstone_wall_torch" | "redstone_wire"
        );
        Some(if glass {
            "minecraft:white_stained_glass"
        } else {
            "minecraft:white_concrete"
        })
    } else if holds_up(below) {
        None
    } else {
        return Err(held(blocks, under, "where its repeater would stand"));
    };
    let side = schem::step(at, face);
    if !unmoved(blocks.get(side)) {
        return Err(held(blocks, side, "beside its repeater"));
    }
    powers_only(blocks, moved, &[on, at])?;
    let now = if comparator {
        format!(
            "minecraft:comparator[facing={},mode=compare,powered=false]",
            driver.facing
        )
    } else {
        format!(
            "minecraft:repeater[delay=1,facing={},locked=false,powered=false]",
            driver.facing
        )
    };
    let mut changes = vec![
        (torch, schem::AIR.to_owned()),
        (moved, schem::with_property(state, "facing", face.name())),
        (at, now),
    ];
    changes.extend(support.map(|support| (under, support.to_owned())));
    Ok(Promotion {
        driver,
        torch: Some((torch, moved)),
        changes,
    })
}

/// The run of `bits` promoted, as [`promotable`] finds each, when every
/// bit's driver is, each the same kind, facing the same way, and standing
/// where it stands from its block, so a bus stays a bus, `comparator`
/// saying which bits are read as strengths: each with its bit; else
/// `None`.
fn promote_run(
    blocks: &Blocks,
    bits: &[Pos],
    comparator: impl Fn(Pos) -> bool,
) -> Option<Vec<(Pos, Promotion)>> {
    let going: Vec<Pos> = bits
        .iter()
        .flat_map(|&block| [block, schem::step(block, Direction::Up)])
        .collect();
    let promoted: Vec<(Pos, Promotion)> = bits
        .iter()
        .map(|&block| {
            promotable(blocks, block, &going, comparator(block))
                .ok()
                .map(|p| (block, p))
        })
        .collect::<Option<_>>()?;
    let (first, promotion) = promoted.first()?;
    let driver = promotion.driver;
    let offset = |block: Pos, at: Pos| [0, 1, 2].map(|i| at[i] - block[i]);
    let like = |(block, other): &(Pos, Promotion)| {
        other.driver.facing == driver.facing
            && other.driver.comparator == driver.comparator
            && other.torch.is_some() == promotion.torch.is_some()
            && offset(*block, other.driver.at) == offset(*first, driver.at)
    };
    promoted.iter().all(like).then_some(promoted)
}

/// The side a run's `bits` are all tapped on, so a bus stays a bus: of
/// the sides every bit can be tapped on, as `taken` leaves them, one not
/// `along` the way copies of the build stack, whose taps would reach into
/// the next copy, then the one whose cells out to the box's edge are air
/// for the most bits, then the first in `OUTWARD`. `None` when no side
/// serves every bit.
fn run_side(
    blocks: &Blocks,
    bits: &[Pos],
    taken: &BTreeMap<Pos, String>,
    along: &[Direction],
) -> Option<Direction> {
    OUTWARD
        .iter()
        .enumerate()
        .filter(|&(_, &side)| {
            let mut taken = taken.clone();
            bits.iter().all(|&block| {
                tap_cells(blocks, block, side, &taken).is_ok_and(|cells| {
                    let stone = "minecraft:stone";
                    take(&mut taken, blocks, (block, side), cells, stone);
                    true
                })
            })
        })
        .max_by_key(|&(first, &side)| {
            let clear = bits
                .iter()
                .filter(|&&block| clear_to_edge(blocks, block, side))
                .count();
            (!along.contains(&side), clear, std::cmp::Reverse(first))
        })
        .map(|(_, &side)| side)
}

/// The output on `block` tapped toward `shared`, its run's side, or with
/// none the first side it can be tapped on, those not `along` the way
/// copies stack first, then those clear to the box's edge, its changes
/// added to `changes`; or why it can't be.
fn tap_one(
    blocks: &Blocks,
    block: Pos,
    (shared, along): (Option<Direction>, &[Direction]),
    comparator: impl Fn(Pos) -> bool,
    changes: &mut BTreeMap<Pos, String>,
) -> Tapped {
    let sides = shared.map_or_else(
        || {
            let mut sides = OUTWARD.to_vec();
            sides.sort_by_key(|&side| {
                (along.contains(&side), !clear_to_edge(blocks, block, side))
            });
            sides
        },
        |side| vec![side],
    );
    let mut blocked = Vec::new();
    let mut tapped = None;
    for side in sides {
        match tap_cells(blocks, block, side, changes) {
            Ok(cells) => {
                tapped = Some((side, cells));
                break;
            }
            Err(why) => blocked.push(why),
        }
    }
    let Some((side, (support, top))) = tapped else {
        return Tapped::Not {
            block,
            why: format!("every side is blocked: {}", blocked.join("; ")),
        };
    };
    // A torch powers the block over it, and a solid block over dust cuts
    // the dust's climb to dust beside it, as glass does neither.
    let under = schem::short(blocks.get(schem::step(support, Direction::Down)));
    let glass = matches!(
        under,
        "redstone_torch" | "redstone_wall_torch" | "redstone_wire"
    );
    let reads = side.opposite();
    let now = if comparator(block) {
        format!(
            "minecraft:comparator[facing={reads},mode=compare,powered=false]"
        )
    } else {
        format!(
            "minecraft:repeater[delay=1,facing={reads},locked=false,\
             powered=false]"
        )
    };
    changes.insert(block, whitened(blocks.get(block)).to_owned());
    let held = if glass {
        "minecraft:white_stained_glass"
    } else {
        "minecraft:white_concrete"
    };
    take(changes, blocks, (block, side), (support, top), held);
    changes.insert(top, now.clone());
    Tapped::Tapped {
        block,
        side,
        support,
        glass,
        clear: clear_to_edge(blocks, block, side),
        now,
    }
}

/// Each run of `outputs`, the old rule's, ported as the owner ports them
/// by hand. With `promote`, a run whose every bit is driven by a
/// repeater or comparator that the block and its dust alone take power
/// from, as [`promotable`] finds them, has its drivers promoted: each
/// output block and its dust taken out, so its driver gives out into air,
/// the output in its place, and the build no bigger. Any other run is
/// tapped: each output block whitened, the dust on it left, and one block out
/// of the build beside it a support, white concrete, or white stained glass
/// over a torch, which powers the block above it, or over dust, whose climb
/// a solid block would cut, with a repeater on top, or a comparator where
/// `comparator` says the output is read as a strength, reading the dust
/// and giving out into air, and the place under that air kept for the
/// output's sign where it has one, which may stand where the support goes,
/// see [`sign`]. A run's bits are tapped the same way, so a bus stays a
/// bus: among the sides every bit can be tapped on, one not `along` the way
/// copies of the build stack, whose taps would reach into the next copy,
/// then the side whose cells out to the box's edge are air for most of its
/// bits, the face it sits on, then the front, north, before others alike;
/// failing one, each bit its own. A cell beyond the box is air, which the
/// box grows to hold. The changes, some maybe beyond the box, with what
/// came of each output.
pub fn tap(
    blocks: &Blocks,
    outputs: &[Run],
    comparator: impl Fn(Pos) -> bool,
    (along, promote): (&[Direction], bool),
) -> (BTreeMap<Pos, String>, Vec<Tapped>) {
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    // What promoting takes out, kept apart from the places kept air for
    // signs, which are dropped at the end.
    let mut cleared: BTreeMap<Pos, String> = BTreeMap::new();
    let mut done = Vec::new();
    let found: Vec<Pos> = find(blocks).iter().map(|o| o.block()).collect();
    for run in outputs {
        let mut bits = Vec::new();
        for &block in &run.bits {
            let top = schem::step(block, Direction::Up);
            if found.contains(&block) {
                done.push(Tapped::Already { block });
            } else if let Err(why) = tappable(blocks, top) {
                done.push(Tapped::Not { block, why });
            } else {
                bits.push(block);
            }
        }
        // A run promoted puts nothing where a tap already went.
        let promoted = promote
            .then(|| promote_run(blocks, &bits, &comparator))
            .flatten()
            .filter(|run| {
                run.iter().all(|(_, promotion)| {
                    promotion
                        .changes
                        .iter()
                        .all(|(at, _)| !changes.contains_key(at))
                })
            });
        if let Some(run) = promoted {
            for (block, promotion) in run {
                let driver = promotion.driver;
                for (at, state) in promotion.changes {
                    // Air is kept so, no tap taking it.
                    changes.insert(at, state.clone());
                    cleared.insert(at, state);
                }
                // Its front, and the place its sign goes, are kept air.
                changes
                    .entry(driver.front())
                    .or_insert_with(|| schem::AIR.to_owned());
                if !own_signs(blocks, block).is_empty() {
                    changes
                        .entry(driver.sign_place())
                        .or_insert_with(|| schem::AIR.to_owned());
                }
                done.push(Tapped::Promoted {
                    block,
                    driver,
                    torch: promotion.torch,
                });
            }
            continue;
        }
        let shared = run_side(blocks, &bits, &changes, along);
        for block in bits {
            done.push(tap_one(
                blocks,
                block,
                (shared, along),
                &comparator,
                &mut changes,
            ));
        }
    }
    // The places kept for signs, which [`sign`] puts them in.
    changes.retain(|_, state| state != schem::AIR);
    changes.extend(cleared);
    (changes, done)
}

/// A signal-strength input, as the old rule read it by its colour, an
/// input block with dust on top, fed so this rule reads it: a comparator
/// beside the dust, pointing into it, with air behind it, where a tester
/// puts a barrel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fed {
    /// A comparator with air behind it drives its dust already.
    Already { pad: Pos },
    /// Fed from `side`, the comparator at `comparator`, on the block under
    /// it, or on a support put there, white concrete, or white stained
    /// glass with `glass` as a torch or dust is under it.
    Fed {
        pad: Pos,
        side: Direction,
        comparator: Pos,
        support: Option<Pos>,
        glass: bool,
    },
    /// It can't be fed, for `why`.
    Not { pad: Pos, why: String },
}

impl Fed {
    /// The input block it was read by.
    pub fn pad(&self) -> Pos {
        match self {
            Fed::Already { pad }
            | Fed::Fed { pad, .. }
            | Fed::Not { pad, .. } => *pad,
        }
    }
}

/// Whether `state` would run into a comparator's side beside it, changing
/// what it gives out: dust, a repeater, a comparator, or a torch.
fn runs_into_side(state: &str) -> bool {
    matches!(
        schem::short(state),
        "redstone_wire"
            | "repeater"
            | "comparator"
            | "redstone_torch"
            | "redstone_wall_torch"
    )
}

/// The input on `pad` fed from `side`, its changes added to `changes`, or
/// why it can't be: the comparator's cell and the one behind it air,
/// nothing beside the comparator that would run into its side, and under
/// it a whole block, or air, where a support goes. `kept` holds the cells
/// behind inputs fed already, kept air for their barrels; this one's is
/// added.
fn feed_from(
    blocks: &Blocks,
    pad: Pos,
    side: Direction,
    (changes, kept): (&mut BTreeMap<Pos, String>, &mut Vec<Pos>),
) -> Result<Fed, String> {
    // What a cell holds, as changed; one kept air behind an input fed
    // already holds that input's barrel.
    let state = |cell: Pos| {
        if kept.contains(&cell) {
            return "minecraft:barrel".to_owned();
        }
        changes
            .get(&cell)
            .map_or_else(|| blocks.get(cell), String::as_str)
            .to_owned()
    };
    let dust = schem::step(pad, Direction::Up);
    let comparator = schem::step(dust, side);
    let behind = schem::step(comparator, side);
    let under = schem::step(comparator, Direction::Down);
    for (cell, what) in [(comparator, "comparator"), (behind, "air behind it")]
    {
        let held = state(cell);
        if !is_air(&held) {
            return Err(format!(
                "{side}: {} at {}, where the {what} goes",
                schem::short(&held).replace('_', " "),
                inspect::place(cell)
            ));
        }
    }
    let across = if side.north_south() {
        [Direction::East, Direction::West]
    } else {
        [Direction::North, Direction::South]
    };
    for way in across {
        let beside = schem::step(comparator, way);
        let held = state(beside);
        if runs_into_side(&held) {
            return Err(format!(
                "{side}: {} at {}, which runs into the comparator's side",
                schem::short(&held).replace('_', " "),
                inspect::place(beside)
            ));
        }
    }
    let below = state(under);
    let (support, glass) = if is_air(&below) {
        // A torch powers the block over it, and a solid block over dust
        // cuts the dust's climb, as glass does neither.
        let glass = matches!(
            schem::short(&state(schem::step(under, Direction::Down))),
            "redstone_torch" | "redstone_wall_torch" | "redstone_wire"
        );
        (Some(under), glass)
    } else if schem::block::whole(&below)
        && !schem::block::is_component(schem::short(&below))
    {
        (None, false)
    } else {
        return Err(format!(
            "{side}: {} at {}, which can't hold the comparator up",
            schem::short(&below).replace('_', " "),
            inspect::place(under)
        ));
    };
    if let Some(under) = support {
        let held = if glass {
            "minecraft:white_stained_glass"
        } else {
            "minecraft:white_concrete"
        };
        changes.insert(under, held.to_owned());
    }
    changes.insert(
        comparator,
        format!(
            "minecraft:comparator[facing={side},mode=compare,powered=false]"
        ),
    );
    kept.push(behind);
    Ok(Fed::Fed {
        pad,
        side,
        comparator,
        support,
        glass,
    })
}

/// Each signal-strength input of `pads`, the old rule's, input blocks with dust
/// on top, each with the side its dust is open to, fed as the owner feeds
/// one by hand: beside its dust a comparator pointing into it with air
/// behind, where a tester puts a barrel, on the block under it or on a
/// support put there, white concrete, or white stained glass over a torch
/// or dust, as a tap's support is; from the side the dust is open to, else
/// the first side, north, south, west, east, it can be. Everything else is
/// kept. Its changes added to `changes`, which hold the cells taken
/// already, with what came of each input.
pub fn feed(
    blocks: &Blocks,
    pads: &[(Pos, Option<Direction>)],
    changes: &mut BTreeMap<Pos, String>,
) -> Vec<Fed> {
    let mut kept = Vec::new();
    pads.iter()
        .map(|&(pad, entry)| {
            if driving(blocks, pad).is_some() {
                return Fed::Already { pad };
            }
            let mut sides: Vec<Direction> = entry.into_iter().collect();
            sides.extend(OUTWARD.into_iter().filter(|&s| Some(s) != entry));
            let mut blocked = Vec::new();
            for side in sides {
                match feed_from(blocks, pad, side, (changes, &mut kept)) {
                    Ok(fed) => return fed,
                    Err(why) => blocked.push(why),
                }
            }
            Fed::Not {
                pad,
                why: format!("every side is blocked: {}", blocked.join("; ")),
            }
        })
        .collect()
}

/// A sign naming an output, moved where the site reads it, under the
/// output's front, see [`Output::sign_place`], or left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signed {
    /// The sign at `sign`, on `block`, the block the output was read by,
    /// moved to `to`.
    Moved { block: Pos, sign: Pos, to: Pos },
    /// The sign at `sign`, on `block`, left where it is, for `why`.
    Not { block: Pos, sign: Pos, why: String },
}

/// The wall sign of the wood of sign `state` facing `facing`, such as
/// `minecraft:birch_wall_sign[facing=south,waterlogged=false]` for a
/// standing birch sign.
fn wall_sign(state: &str, facing: Direction) -> String {
    let id = schem::id(state);
    let wood = ["_wall_hanging_sign", "_hanging_sign", "_wall_sign", "_sign"]
        .into_iter()
        .find_map(|end| id.strip_suffix(end))
        .unwrap_or("minecraft:oak");
    format!("{wood}_wall_sign[facing={facing},waterlogged=false]")
}

/// Each sign naming an output of `named`, each the block an output was
/// read by with the output read there now, moved under the output's
/// front, on the face of the block holding it up, as the site reads it:
/// the sign on that block, unless one is there already. Its changes are
/// added to `changes`, which hold the cells taken already, and each sign's
/// move to `moves`. A sign stays where it is when its place holds
/// something, or another sign of the output's goes there, as one fits.
pub fn sign<S: std::hash::BuildHasher>(
    blocks: &Blocks,
    named: &[(Pos, Output)],
    changes: &mut BTreeMap<Pos, String>,
    moves: &mut HashMap<Pos, Pos, S>,
) -> Vec<Signed> {
    let mut signed = Vec::new();
    for &(block, output) in named {
        let place = output.sign_place();
        let mut signs: Vec<Pos> = blocks
            .signs
            .keys()
            .copied()
            .filter(|&sign| {
                sign != place && blocks.sign_block(sign) == Some(block)
            })
            .collect();
        signs.sort_unstable();
        let mut held = blocks.signs.contains_key(&place)
            && blocks.sign_block(place) == Some(output.block());
        for sign in signs {
            let state = changes
                .get(&place)
                .map_or_else(|| blocks.get(place), String::as_str);
            let why = if held {
                Some("another sign of it is under its front".to_owned())
            } else if !is_air(state) {
                Some(format!(
                    "{} at {}, under its front",
                    schem::short(state).replace('_', " "),
                    inspect::place(place)
                ))
            } else {
                None
            };
            if let Some(why) = why {
                signed.push(Signed::Not { block, sign, why });
                continue;
            }
            // A tap's support may stand where the sign was.
            changes.entry(sign).or_insert_with(|| schem::AIR.to_owned());
            changes.insert(place, wall_sign(blocks.get(sign), output.points()));
            moves.insert(sign, place);
            held = true;
            signed.push(Signed::Moved {
                block,
                sign,
                to: place,
            });
        }
    }
    signed
}

/// The file in `data` with the sign naming each output, by the rule for
/// outputs, moved under its front, as [`sign`] moves them, its box grown
/// to hold any beyond it; `None` when none moved. What came of each sign
/// that names one.
pub fn signed(data: &[u8]) -> Result<(Option<Vec<u8>>, Vec<Signed>)> {
    let blocks = Blocks::read(data)?;
    let named: Vec<(Pos, Output)> =
        find(&blocks).into_iter().map(|o| (o.block(), o)).collect();
    let (mut changes, mut moves) = (BTreeMap::new(), HashMap::new());
    let signed = sign(&blocks, &named, &mut changes, &mut moves);
    if moves.is_empty() {
        return Ok((None, signed));
    }
    let (file, _) =
        crate::schem::standardize::edit_grown_moving(data, &changes, &moves)?;
    Ok((Some(file), signed))
}

/// A port made by [`tapped`]: its file, unless an output or input couldn't
/// be ported; how far every old position moved as the box grew; and what
/// came of each output, input, and sign naming an output.
pub struct Ported {
    pub file: Option<Vec<u8>>,
    pub moved: Pos,
    pub tapped: Vec<Tapped>,
    pub fed: Vec<Fed>,
    pub signed: Vec<Signed>,
}

/// The file in `data` with each output of `outputs`, the old rule's,
/// tapped, or with `promote` its driver promoted, as [`tap`] ports them,
/// `along` the ways copies stack, each
/// signal-strength input of `pads`, the old rule's, fed as [`feed`] feeds
/// them, and each sign naming an output moved under its front, as [`sign`]
/// moves them, its box grown to hold the taps, then as `//update` leaves
/// it, [`crate::schem::rstack::settle`]: dust shaped as the game shapes it,
/// then every component at rest, each lever as it was; a file with a block
/// the simulator doesn't model, as an upload's slab, only its dust shaped,
/// as it can't be run to rest. No file unless every output was tapped or
/// already one and every input fed or already, and one of them, or a sign,
/// changed.
pub fn tapped(
    data: &[u8],
    (outputs, pads): (&[Run], &[(Pos, Option<Direction>)]),
    comparator: impl Fn(Pos) -> bool,
    (along, promote): (&[Direction], bool),
) -> Result<Ported> {
    let blocks = Blocks::read(data)?;
    let (mut changes, tapped) =
        tap(&blocks, outputs, &comparator, (along, promote));
    let fed = feed(&blocks, pads, &mut changes);
    let found = find(&blocks);
    let named: Vec<(Pos, Output)> = tapped
        .iter()
        .filter_map(|t| match *t {
            Tapped::Already { block } => found
                .iter()
                .find(|o| o.block() == block)
                .map(|&o| (block, o)),
            ref t => t.output(comparator(t.block())).map(|o| (t.block(), o)),
        })
        .collect();
    let mut signs_to = HashMap::new();
    let signed = sign(&blocks, &named, &mut changes, &mut signs_to);
    let ported = tapped
        .iter()
        .any(|t| matches!(t, Tapped::Tapped { .. } | Tapped::Promoted { .. }))
        || fed.iter().any(|f| matches!(f, Fed::Fed { .. }))
        || !signs_to.is_empty();
    if !ported
        || tapped.iter().any(|t| matches!(t, Tapped::Not { .. }))
        || fed.iter().any(|f| matches!(f, Fed::Not { .. }))
    {
        return Ok(Ported {
            file: None,
            moved: [0; 3],
            tapped,
            fed,
            signed,
        });
    }
    let (file, moved) = crate::schem::standardize::edit_grown_moving(
        data, &changes, &signs_to,
    )?;
    // A file with a block the simulator doesn't model, as an upload's slab,
    // can't come to rest, so only its dust is shaped.
    let file = if modelled(&file)? {
        crate::schem::rstack::settle(&file)?
    } else {
        crate::schem::dust::update(&file)?
    };
    Ok(Ported {
        file: Some(file),
        moved,
        tapped,
        fed,
        signed,
    })
}

/// Whether the simulator models every block of the file in `data`, which
/// [`probe::simulatable`] then leaves as it is.
fn modelled(data: &[u8]) -> Result<bool> {
    let states = |data: &[u8]| -> Result<Vec<(Pos, String)>> {
        Ok(Blocks::read(data)?
            .iter()
            .map(|(pos, state)| (pos, state.to_owned()))
            .collect())
    };
    Ok(states(data)? == states(&probe::simulatable(data)?)?)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::recognize::legacy;

    const LEVER: &str = "lever[face=wall,facing=west,powered=false]";

    /// A lever on a block driving a line of dust east into a repeater on
    /// stone, facing `facing`, with `front` east of it.
    fn line(facing: &str, front: &str) -> Blocks {
        let repeater = format!(
            "repeater[delay=1,facing={facing},locked=false,powered=false]"
        );
        let dust =
            "redstone_wire[east=side,north=none,power=0,south=none,west=side]";
        let states: HashMap<Pos, String> = [
            ([0, 1, 0], LEVER),
            ([1, 1, 0], "white_concrete"),
            ([2, 0, 0], "stone"),
            ([2, 1, 0], dust),
            ([3, 0, 0], "stone"),
            ([3, 1, 0], repeater.as_str()),
            ([4, 1, 0], front),
        ]
        .into_iter()
        .filter(|(_, state)| *state != "air")
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        Blocks::from_grid([6, 2, 1], &states)
    }

    /// A repeater's `facing` is the side it reads from, so one facing west
    /// gives out east; with air there it powers nothing and is an output.
    #[test]
    fn a_repeater_giving_out_into_air_is_an_output() {
        let blocks = line("west", "air");
        let found = find(&blocks);
        assert_eq!(
            found,
            [Output {
                at: [3, 1, 0],
                comparator: false,
                facing: Direction::West,
            }]
        );
        assert_eq!(found[0].points(), Direction::East);
        assert_eq!(found[0].front(), [4, 1, 0]);
        let report = inspect::inspect(&blocks);
        assert_eq!(report.outputs.len(), 1);
        assert_eq!(report.outputs[0].bits, [[3, 0, 0]]);
        assert_eq!(report.inputs.len(), 1);
    }

    /// A repeater powering a block, or dust, carries the circuit on, and
    /// one facing the other way gives out into the dust that feeds it.
    #[test]
    fn a_repeater_powering_anything_is_no_output() {
        for front in [
            "stone",
            "redstone_wire[east=none,north=none,power=0,south=none,west=side]",
        ] {
            assert_eq!(find(&line("west", front)), [], "{front}");
        }
        assert_eq!(find(&line("east", "air")), []);
    }

    /// A repeater on stone giving out east with a wall sign at `sign`
    /// facing `facing`, saying `carry out`.
    fn signed_output(sign: Pos, facing: &str) -> Blocks {
        let states: HashMap<Pos, String> = [
            ([1, 0, 1], "stone".to_owned()),
            (
                [1, 1, 1],
                "repeater[delay=1,facing=west,locked=false,powered=false]"
                    .to_owned(),
            ),
            (
                sign,
                format!("oak_wall_sign[facing={facing},waterlogged=false]"),
            ),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        let mut blocks = Blocks::from_grid([3, 2, 3], &states);
        let text = ["carry out", "", "", ""].map(str::to_owned).to_vec();
        blocks.signs.insert(sign, text);
        blocks
    }

    /// An output's sign is the one on the face of the block holding it up
    /// under its front, where `sign` moves one from another face, which
    /// names nothing.
    #[test]
    fn an_outputs_sign_goes_under_its_front() {
        let under = signed_output([2, 0, 1], "east");
        let named = signs(&under);
        assert_eq!(
            named.get(&[1, 0, 1]).and_then(|lines| lines.first()),
            Some(&"carry out".to_owned())
        );
        let beside = signed_output([1, 0, 2], "south");
        assert!(signs(&beside).is_empty());
        let output = find(&beside)[0];
        assert_eq!(output.sign_place(), [2, 0, 1]);
        let (mut changes, mut moves) = (BTreeMap::new(), HashMap::new());
        let signed = sign(
            &beside,
            &[(output.block(), output)],
            &mut changes,
            &mut moves,
        );
        assert_eq!(
            signed,
            [Signed::Moved {
                block: [1, 0, 1],
                sign: [1, 0, 2],
                to: [2, 0, 1],
            }]
        );
        assert_eq!(
            changes[&[2, 0, 1]],
            "minecraft:oak_wall_sign[facing=east,waterlogged=false]"
        );
        assert_eq!(moves[&[1, 0, 2]], [2, 0, 1]);
        // Something under the front keeps the sign where it is.
        changes.clear();
        changes.insert([2, 0, 1], "minecraft:stone".to_owned());
        let signed = sign(
            &beside,
            &[(output.block(), output)],
            &mut changes,
            &mut HashMap::new(),
        );
        assert!(matches!(signed[..], [Signed::Not { .. }]));
    }

    /// A comparator facing air is an output too, whatever block holds it
    /// up, and a repeater at the edge of the file gives out beyond it.
    #[test]
    fn a_comparator_on_any_block_or_one_at_the_edge_is_an_output() {
        let states: HashMap<Pos, String> = [
            ([0, 0, 0], "minecraft:lime_wool".to_owned()),
            (
                [0, 1, 0],
                "minecraft:comparator[facing=south,mode=compare,\
                 powered=false]"
                    .to_owned(),
            ),
        ]
        .into_iter()
        .collect();
        let found = find(&Blocks::from_grid([1, 2, 1], &states));
        assert_eq!(found.len(), 1);
        assert!(found[0].comparator);
        assert_eq!(found[0].points(), Direction::North);
    }

    /// Evenly spaced outputs facing one way are a bus; one facing another
    /// way beside them is a line of its own.
    #[test]
    fn outputs_facing_one_way_evenly_spaced_are_a_bus() {
        let at = |x, facing| Output {
            at: [x, 1, 0],
            comparator: false,
            facing,
        };
        let outputs = [
            at(0, Direction::North),
            at(2, Direction::North),
            at(4, Direction::North),
            at(6, Direction::South),
        ];
        let runs = lines(&outputs);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].bits, [[0, 1, 0], [2, 1, 0], [4, 1, 0]]);
        assert_eq!(runs[1].bits, [[6, 1, 0]]);
    }

    /// The owner's hand-made one-bit adders: the first two read their sum
    /// through dust on yellow concrete, so only the carry out's repeater is
    /// an output by this rule; the third gives both out through repeaters.
    #[test]
    fn the_owners_one_bit_adders_have_repeater_outputs() {
        let count = |name: &str| {
            let data =
                std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
            find(&Blocks::read(&data).unwrap()).len()
        };
        assert_eq!(count("old/rca-1-repeater-a"), 1);
        assert_eq!(count("old/rca-1-repeater-b"), 1);
        assert_eq!(count("rca-1-repeater-c"), 2);
    }

    /// The owner's 8-bit carry-cancel adder reworked to give every output
    /// out through a repeater probes as the adder it is with those outputs
    /// alone, its sum one bus and its carry out a line.
    #[test]
    #[ignore = "probes a whole build"]
    fn a_carry_cancel_adder_with_repeater_outputs_probes_as_an_adder() {
        let data =
            std::fs::read("tests/fixtures/cca-8-repeater-out.schem").unwrap();
        let probed = probe(&data, Some(Kind::Adder))
            .unwrap()
            .unwrap_or_else(|faults| panic!("{}", faults[0].1));
        let layout = probed.layout.unwrap();
        assert_eq!(layout.sum.len(), 8);
        assert!(layout.carry_out.is_some());
    }

    /// An output block, yellow concrete at (2, 1, 2), with dust on it fed
    /// from the south by dust on stone, in a box 5 by 4 by 5, with `extra`
    /// blocks put in or, as air, taken out.
    fn tap_fixture(extra: &[(Pos, &str)]) -> Blocks {
        let wire = "redstone_wire[east=none,north=side,power=0,south=side,\
                    west=none]";
        let mut states: HashMap<Pos, String> = [
            ([2, 1, 2], "yellow_concrete"),
            ([2, 2, 2], wire),
            ([2, 1, 3], "stone"),
            ([2, 2, 3], wire),
            ([2, 1, 4], "stone"),
            ([2, 2, 4], wire),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        for &(pos, state) in extra {
            if state == "air" {
                states.remove(&pos);
            } else {
                states.insert(pos, format!("minecraft:{state}"));
            }
        }
        Blocks::from_grid([5, 4, 5], &states)
    }

    /// The one output of `tap_fixture` tapped, with `along` the sides
    /// copies stack toward.
    fn tap_one_output(
        blocks: &Blocks,
        along: &[Direction],
    ) -> (BTreeMap<Pos, String>, Tapped) {
        let run = Run {
            bits: vec![[2, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, mut done) =
            tap(blocks, &[run], |_| false, (along, false));
        assert_eq!(done.len(), 1);
        (changes, done.remove(0))
    }

    /// With every side open, the tap goes north, the front: the output
    /// block whitened, a support of white concrete north of it, and on
    /// that a repeater facing south, reading the dust on the output block
    /// and giving out north into air.
    #[test]
    fn a_tap_goes_north_by_default() {
        let (changes, tapped) = tap_one_output(&tap_fixture(&[]), &[]);
        assert_eq!(
            tapped,
            Tapped::Tapped {
                block: [2, 1, 2],
                side: Direction::North,
                support: [2, 1, 1],
                glass: false,
                clear: true,
                now: "minecraft:repeater[delay=1,facing=south,locked=false,\
                      powered=false]"
                    .to_owned(),
            }
        );
        assert_eq!(changes[&[2, 1, 2]], "minecraft:white_concrete");
        assert_eq!(changes[&[2, 1, 1]], "minecraft:white_concrete");
        assert_eq!(changes.len(), 3);
        // Nothing else changes: the dust on the output block stays.
        assert!(!changes.contains_key(&[2, 2, 2]));
    }

    /// A block further north, out to the box's edge, makes north no face
    /// the output sits on though a tap fits there, so the tap goes to a
    /// side clear to the edge, west before east.
    #[test]
    fn a_tap_goes_toward_the_face_the_output_sits_on() {
        let blocks = tap_fixture(&[([2, 1, 0], "stone")]);
        let (_, tapped) = tap_one_output(&blocks, &[]);
        assert!(matches!(
            tapped,
            Tapped::Tapped {
                side: Direction::West,
                clear: true,
                ..
            }
        ));
    }

    /// A side copies stack toward is tried last, as a tap there would
    /// reach into the next copy.
    #[test]
    fn a_tap_avoids_the_way_copies_stack() {
        let along = [Direction::North, Direction::South];
        let (_, tapped) = tap_one_output(&tap_fixture(&[]), &along);
        assert!(matches!(
            tapped,
            Tapped::Tapped {
                side: Direction::West,
                ..
            }
        ));
    }

    /// A torch under the support would power it, and dust under it would
    /// have its climb cut, so the support is white stained glass; a
    /// stained glass output block is whitened as stained glass.
    #[test]
    fn a_support_over_a_torch_or_dust_is_glass() {
        for under in [
            "redstone_torch[lit=true]",
            "redstone_wall_torch[facing=north,lit=true]",
            "redstone_wire[east=none,north=none,power=0,south=none,\
             west=none]",
        ] {
            let blocks = tap_fixture(&[([2, 0, 1], under)]);
            let (changes, tapped) = tap_one_output(&blocks, &[]);
            assert!(
                matches!(tapped, Tapped::Tapped { glass: true, .. }),
                "{under}"
            );
            assert_eq!(changes[&[2, 1, 1]], "minecraft:white_stained_glass");
        }
        let blocks = tap_fixture(&[([2, 1, 2], "yellow_stained_glass")]);
        let (changes, tapped) = tap_one_output(&blocks, &[]);
        assert!(matches!(tapped, Tapped::Tapped { glass: false, .. }));
        assert_eq!(changes[&[2, 1, 2]], "minecraft:white_stained_glass");
        assert_eq!(changes[&[2, 1, 1]], "minecraft:white_concrete");
    }

    /// Where the support, the repeater, or the air it gives out into is
    /// taken on every side, the output isn't tapped, and why names each
    /// side's block; a cell beyond the box is air.
    #[test]
    fn an_output_blocked_on_every_side_is_not_tapped() {
        let blocks = tap_fixture(&[
            ([2, 2, 1], "stone"),
            ([1, 1, 2], "stone"),
            ([4, 2, 2], "stone"),
        ]);
        let (changes, tapped) = tap_one_output(&blocks, &[]);
        assert!(changes.is_empty());
        let Tapped::Not { why, .. } = tapped else {
            panic!("tapped: {tapped:?}");
        };
        assert!(why.starts_with("every side is blocked"), "{why}");
        for at in ["(2, 2, 1)", "(1, 1, 2)", "(4, 2, 2)", "(2, 1, 3)"] {
            assert!(why.contains(at), "{at} in {why}");
        }
        // At the west edge, closed north, south, and east, where its
        // repeater would give out into the dust of the other output, the
        // tap goes beyond the box.
        let blocks = tap_fixture(&[
            ([0, 2, 1], "stone"),
            ([0, 1, 3], "stone"),
            ([0, 1, 2], "yellow_concrete"),
            (
                [0, 2, 2],
                "redstone_wire[east=side,north=none,power=0,\
                          south=none,west=none]",
            ),
        ]);
        let run = Run {
            bits: vec![[0, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, done) = tap(&blocks, &[run], |_| false, (&[], false));
        assert!(matches!(
            done[0],
            Tapped::Tapped {
                side: Direction::West,
                support: [-1, 1, 2],
                ..
            }
        ));
        assert!(changes.contains_key(&[-1, 2, 2]));
    }

    /// An output block, yellow concrete with dust on it, whose dust a
    /// comparator south of it, on stone, points into, reading dust behind
    /// it, with `extra` added, in a box 5 by 4 by 5.
    fn driven(extra: &[(Pos, &str)]) -> Blocks {
        let line = "redstone_wire[east=none,north=side,power=0,south=side,\
                    west=none]";
        let mut states: HashMap<Pos, String> = [
            ([2, 1, 2], "yellow_concrete"),
            ([2, 2, 2], line),
            ([2, 1, 3], "stone"),
            (
                [2, 2, 3],
                "comparator[facing=south,mode=compare,powered=false]",
            ),
            ([2, 1, 4], "stone"),
            ([2, 2, 4], line),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        for &(pos, state) in extra {
            if state == "air" {
                states.remove(&pos);
            } else {
                states.insert(pos, format!("minecraft:{state}"));
            }
        }
        Blocks::from_grid([5, 4, 5], &states)
    }

    /// The output of `blocks` at (2, 1, 2) ported with promoting.
    fn promote_one(blocks: &Blocks) -> (BTreeMap<Pos, String>, Tapped) {
        let run = Run {
            bits: vec![[2, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, mut done) = tap(blocks, &[run], |_| false, (&[], true));
        assert_eq!(done.len(), 1);
        (changes, done.remove(0))
    }

    /// A comparator pointing into an output's dust, the one thing either
    /// powers, is promoted: the block and its dust taken out, so it gives
    /// out into air, an output where the output was, and the build no
    /// bigger; nothing else changes. A repeater pointing into the block
    /// itself is promoted so too.
    #[test]
    fn a_driver_pointing_into_an_output_gives_out_into_air() {
        let (changes, done) = promote_one(&driven(&[]));
        let driver = Output {
            at: [2, 2, 3],
            comparator: true,
            facing: Direction::South,
        };
        assert_eq!(
            done,
            Tapped::Promoted {
                block: [2, 1, 2],
                driver,
                torch: None,
            }
        );
        assert_eq!(
            changes,
            BTreeMap::from([
                ([2, 1, 2], schem::AIR.to_owned()),
                ([2, 2, 2], schem::AIR.to_owned()),
            ])
        );
        let grid: HashMap<Pos, String> = driven(&[])
            .iter()
            .filter(|&(pos, _)| !changes.contains_key(&pos))
            .map(|(pos, state)| (pos, state.to_owned()))
            .collect();
        let after = Blocks::from_grid([5, 4, 5], &grid);
        assert_eq!(find(&after), [driver]);

        // A repeater at the block's level, on stone, pointing into it.
        let blocks = driven(&[
            ([2, 2, 3], "air"),
            (
                [2, 1, 3],
                "repeater[delay=1,facing=south,locked=false,powered=false]",
            ),
            ([2, 0, 3], "stone"),
        ]);
        let (_, done) = promote_one(&blocks);
        assert!(
            matches!(done, Tapped::Promoted { driver, .. } if driver.at == [2, 1, 3] && !driver.comparator),
            "{done:?}"
        );
    }

    /// A driver isn't promoted where taking out the block or its dust
    /// would take power from anything else: a repeater reading the block,
    /// dust the output's dust runs into, or a torch on the block; nor
    /// where two things drive it, or none does. The output is tapped
    /// instead.
    #[test]
    fn a_driver_is_not_promoted_where_its_output_powers_more() {
        let reader = "repeater[delay=1,facing=east,locked=false,powered=false]";
        let joined = "redstone_wire[east=side,north=none,power=0,south=none,\
                      west=side]";
        for (extra, why) in [
            (vec![([1, 1, 2], reader)], "repeater at (1, 1, 2)"),
            (vec![([3, 2, 2], joined)], "redstone wire at (3, 2, 2)"),
            (
                vec![([2, 1, 1], "redstone_wall_torch[facing=north,lit=true]")],
                "redstone wall torch at (2, 1, 1)",
            ),
            (
                vec![
                    (
                        [1, 2, 2],
                        "repeater[delay=1,facing=west,locked=false,\
                         powered=false]",
                    ),
                    ([1, 1, 2], "stone"),
                ],
                "more than one",
            ),
            (vec![([2, 2, 3], "stone")], "drives it alone"),
        ] {
            let blocks = driven(&extra);
            let Err(err) = promotable(&blocks, [2, 1, 2], &[], false) else {
                panic!("promoted with {extra:?}");
            };
            assert!(err.contains(why), "{err}");
            let (_, done) = promote_one(&blocks);
            assert!(matches!(done, Tapped::Tapped { .. }), "{done:?}");
        }
        // An output read as a strength isn't read off a repeater.
        let blocks = driven(&[(
            [2, 2, 3],
            "repeater[delay=1,facing=south,locked=false,powered=false]",
        )]);
        assert!(promotable(&blocks, [2, 1, 2], &[], true).is_err());
        assert!(promotable(&blocks, [2, 1, 2], &[], false).is_ok());
    }

    /// A sign on a promoted output's block goes under its driver's front,
    /// on the face of the block holding the driver up, as every output's
    /// does; where that place holds something, the driver isn't promoted.
    #[test]
    fn a_promoted_outputs_sign_goes_under_its_drivers_front() {
        let on = [2, 1, 1];
        let mut blocks =
            driven(&[(on, "oak_wall_sign[facing=north,waterlogged=false]")]);
        blocks
            .signs
            .insert(on, ["sum", "", "", ""].map(str::to_owned).to_vec());
        let (mut changes, done) = promote_one(&blocks);
        let driver = done.output(true).expect("promoted");
        // Under the comparator's front is where the block was.
        assert_eq!(driver.sign_place(), [2, 1, 2]);
        let mut moves = HashMap::new();
        let signed =
            sign(&blocks, &[(done.block(), driver)], &mut changes, &mut moves);
        assert_eq!(
            signed,
            [Signed::Moved {
                block: [2, 1, 2],
                sign: on,
                to: [2, 1, 2],
            }]
        );
        assert_eq!(
            changes[&[2, 1, 2]],
            "minecraft:oak_wall_sign[facing=north,waterlogged=false]"
        );
        assert_eq!(changes[&on], schem::AIR);
        assert_eq!(changes[&[2, 2, 2]], schem::AIR);

        // A comparator at the block's level, its sign's place under its
        // front taken by stone.
        let mut blocks = driven(&[
            ([2, 2, 3], "air"),
            (
                [2, 1, 3],
                "comparator[facing=south,mode=compare,powered=false]",
            ),
            ([2, 0, 3], "stone"),
            ([2, 0, 2], "stone"),
            (on, "oak_wall_sign[facing=north,waterlogged=false]"),
        ]);
        blocks
            .signs
            .insert(on, ["sum", "", "", ""].map(str::to_owned).to_vec());
        let err = promotable(&blocks, [2, 1, 2], &[], false).unwrap_err();
        assert!(err.contains("where its sign would go"), "{err}");
    }

    /// A wall torch beside an output's dust, facing it, moves to another
    /// face of the block it is on, inside the box, with a repeater in
    /// front of it giving out the way the torch faced into air, on white
    /// concrete put under it, as the owner ported an adder's carry out.
    #[test]
    fn a_torch_driving_an_output_moves_with_a_repeater_before_it() {
        let wire = "redstone_wire[east=side,north=none,power=0,south=none,\
                    west=side]";
        let states: HashMap<Pos, String> = [
            ([3, 1, 2], "yellow_concrete"),
            ([3, 2, 2], wire),
            ([1, 2, 2], "stone"),
            ([2, 2, 2], "redstone_wall_torch[facing=east,lit=true]"),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        let blocks = Blocks::from_grid([5, 4, 5], &states);
        let promotion = promotable(&blocks, [3, 1, 2], &[], false).unwrap();
        assert_eq!(
            promotion.driver,
            Output {
                at: [2, 2, 1],
                comparator: false,
                facing: Direction::West,
            }
        );
        assert_eq!(promotion.torch, Some(([2, 2, 2], [1, 2, 1])));
        let changes: BTreeMap<Pos, String> =
            promotion.changes.into_iter().collect();
        assert_eq!(
            changes[&[1, 2, 1]],
            "minecraft:redstone_wall_torch[facing=north,lit=true]"
        );
        assert_eq!(changes[&[2, 1, 1]], "minecraft:white_concrete");
        for gone in [[3, 1, 2], [3, 2, 2], [2, 2, 2]] {
            assert_eq!(changes[&gone], schem::AIR);
        }
        // In a box one block deep, it has nowhere to go but out of it.
        let narrow: HashMap<Pos, String> = states
            .iter()
            .map(|(&[x, y, _], state)| ([x, y, 0], state.clone()))
            .collect();
        let narrow = Blocks::from_grid([5, 4, 1], &narrow);
        let err = promotable(&narrow, [3, 1, 0], &[], false).unwrap_err();
        assert!(err.contains("out of the box"), "{err}");
    }

    /// An output read as a strength takes a comparator, and one read by a
    /// repeater that powers something can't be tapped, a tap reading dust
    /// only.
    #[test]
    fn a_strength_takes_a_comparator_and_a_repeater_reader_is_left() {
        let run = Run {
            bits: vec![[2, 1, 2]],
            step: [0, 0, 0],
        };
        let (changes, _) = tap(
            &tap_fixture(&[]),
            std::slice::from_ref(&run),
            |_| true,
            (&[], false),
        );
        assert!(
            changes[&[2, 2, 1]]
                .starts_with("minecraft:comparator[facing=south")
        );
        let blocks = tap_fixture(&[
            (
                [2, 2, 2],
                "repeater[delay=1,facing=south,locked=false,powered=false]",
            ),
            ([2, 2, 1], "stone"),
        ]);
        let (changes, done) = tap(&blocks, &[run], |_| false, (&[], false));
        assert!(changes.is_empty());
        assert!(
            matches!(&done[0], Tapped::Not { why, .. } if why.contains("reads only dust"))
        );
    }

    /// A tapped file is settled as `//update` leaves it: the box grown to
    /// hold the taps, the dust on each output block shaped to join its
    /// repeater, so settling it again changes nothing, and the site's
    /// rule finds every tap an output.
    #[test]
    fn a_tapped_file_is_settled() {
        let data = std::fs::read("tests/fixtures/old/inc-8.schem").unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let today = legacy::inspect(&blocks, legacy::SITE);
        let Ported {
            file,
            moved,
            tapped: done,
            ..
        } = tapped(&data, (&today.outputs, &[]), |_| false, (&[], false))
            .unwrap();
        let file = file.expect("every output tapped");
        assert_eq!(
            crate::schem::rstack::settle(&file).unwrap(),
            file,
            "settled already"
        );
        let after = Blocks::read(&file).unwrap();
        let found: Vec<Pos> = find(&after).iter().map(|o| o.block()).collect();
        assert!(!done.is_empty());
        for tapped in &done {
            let Tapped::Tapped {
                block,
                side,
                support,
                ..
            } = tapped
            else {
                panic!("not tapped: {tapped:?}");
            };
            let top = schem::step(schem::add(*block, moved), Direction::Up);
            let dust = after.get(top);
            assert_eq!(schem::short(dust), "redstone_wire");
            assert_eq!(schem::property(dust, side.name()), Some("side"));
            assert_eq!(
                schem::short(after.get(schem::add(*block, moved))),
                "white_concrete"
            );
            assert!(found.contains(&schem::add(*support, moved)));
        }
    }

    /// Lime concrete with dust on it, open to the south, a digit input as
    /// the old rule read it by its colour, in a box 3 by 3 by 3.
    fn pad(extra: &[(Pos, &str)]) -> Blocks {
        let wire = "redstone_wire[east=none,north=side,power=0,south=side,\
                    west=none]";
        let mut states: HashMap<Pos, String> = [
            ([1, 0, 1], "lime_concrete"),
            ([1, 1, 1], wire),
            ([1, 0, 0], "stone"),
            ([1, 1, 0], wire),
        ]
        .into_iter()
        .map(|(pos, state)| (pos, format!("minecraft:{state}")))
        .collect();
        for &(pos, state) in extra {
            states.insert(pos, format!("minecraft:{state}"));
        }
        Blocks::from_grid([3, 3, 3], &states)
    }

    const COMPARATOR: &str = "comparator[facing=south,mode=compare,\
                              powered=false]";

    /// A comparator with air behind it giving out into dust is a
    /// signal-strength input, and drives the digit on the block under the
    /// dust; with a block behind it, it reads that, and is none, and one
    /// giving out into air is an output.
    #[test]
    fn a_comparator_with_air_behind_is_a_strength_input() {
        let blocks = pad(&[([1, 1, 2], COMPARATOR)]);
        let found = strength_inputs(&blocks);
        assert_eq!(
            found,
            [StrengthInput {
                at: [1, 1, 2],
                facing: Direction::South,
            }]
        );
        assert_eq!(found[0].front(), [1, 1, 1]);
        assert_eq!(driving(&blocks, [1, 0, 1]), Some(found[0]));
        assert!(find(&blocks).is_empty());
        let behind = Blocks::from_grid(
            [3, 3, 4],
            &[
                ([1, 1, 1], "minecraft:redstone_wire".to_owned()),
                ([1, 1, 2], format!("minecraft:{COMPARATOR}")),
                ([1, 1, 3], "minecraft:barrel".to_owned()),
            ]
            .into_iter()
            .collect(),
        );
        assert!(strength_inputs(&behind).is_empty());
        let alone = Blocks::from_grid(
            [1, 1, 1],
            &[([0, 0, 0], format!("minecraft:{COMPARATOR}"))]
                .into_iter()
                .collect(),
        );
        assert!(strength_inputs(&alone).is_empty());
        assert_eq!(find(&alone).len(), 1);
    }

    /// Two lines evenly spaced two apart, each a layer above the other's,
    /// are one bus whose bits alternate between them, as a tall hex
    /// adder's digits do.
    #[test]
    fn two_interleaved_lines_are_one_bus() {
        let at = |x, y| StrengthInput {
            at: [x, y, 0],
            facing: Direction::North,
        };
        let inputs = [at(5, 3), at(5, 5), at(5, 7), at(6, 4), at(6, 6)];
        let runs = strength_lines(&inputs);
        assert_eq!(runs.len(), 2);
        let inputs =
            [at(5, 3), at(5, 5), at(5, 7), at(6, 4), at(6, 6), at(6, 8)];
        let runs = strength_lines(&inputs);
        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].bits,
            [
                [5, 3, 0],
                [6, 4, 0],
                [5, 5, 0],
                [6, 6, 0],
                [5, 7, 0],
                [6, 8, 0]
            ]
        );
    }

    /// A digit input is fed from the side its dust is open to: a
    /// comparator pointing into the dust, on a support, with air behind it,
    /// which this rule reads as the input driving it.
    #[test]
    fn a_digit_input_is_fed_from_its_open_side() {
        let blocks = pad(&[]);
        let mut changes = BTreeMap::new();
        let fed = feed(
            &blocks,
            &[([1, 0, 1], Some(Direction::South))],
            &mut changes,
        );
        assert_eq!(
            fed,
            [Fed::Fed {
                pad: [1, 0, 1],
                side: Direction::South,
                comparator: [1, 1, 2],
                support: Some([1, 0, 2]),
                glass: false,
            }]
        );
        let states: HashMap<Pos, String> = blocks
            .iter()
            .filter(|(_, state)| !is_air(state))
            .map(|(pos, state)| (pos, state.to_owned()))
            .chain(changes)
            .collect();
        let after = Blocks::from_grid([3, 3, 4], &states);
        assert_eq!(
            driving(&after, [1, 0, 1]).map(|input| input.at),
            Some([1, 1, 2])
        );
        // Fed already, it is left as it is.
        let mut again = BTreeMap::new();
        let fed =
            feed(&after, &[([1, 0, 1], Some(Direction::South))], &mut again);
        assert_eq!(fed, [Fed::Already { pad: [1, 0, 1] }]);
        assert!(again.is_empty());
    }

    /// A digit whose every side is taken can't be fed: dust running into
    /// the comparator's side would change what it gives out.
    #[test]
    fn a_digit_input_with_dust_beside_its_comparator_is_not_fed_so() {
        let wire = "redstone_wire";
        let blocks = pad(&[
            ([0, 1, 2], wire),
            ([2, 1, 0], wire),
            ([0, 1, 1], "stone"),
            ([2, 1, 1], "stone"),
        ]);
        let mut changes = BTreeMap::new();
        let fed = feed(
            &blocks,
            &[([1, 0, 1], Some(Direction::South))],
            &mut changes,
        );
        let [Fed::Not { why, .. }] = fed.as_slice() else {
            panic!("fed: {fed:?}");
        };
        assert!(why.contains("runs into the comparator's side"), "{why}");
    }

    /// The owner's 8-digit carry-cancel hex adder, its digit inputs read
    /// by their colour, fed and its outputs tapped: the site's rule reads
    /// the port with no colour, its comparators A and B, and it adds as the
    /// original does.
    #[test]
    #[ignore = "simulates a whole build"]
    fn a_hex_adder_fed_and_tapped_adds_by_the_proposed_rule() {
        let data = std::fs::read("tests/fixtures/old/hex-cca-8.schem").unwrap();
        let data = probe::simulatable(&data).unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let today = legacy::inspect(&blocks, legacy::SITE);
        let pads: Vec<(Pos, Option<Direction>)> =
            legacy::dust_inputs(&blocks).into_iter().collect();
        assert_eq!(pads.len(), 16);
        let sum: Vec<Pos> = legacy::read(&blocks)
            .outputs
            .iter()
            .filter(|port| port.bits.len() > 1)
            .flat_map(|port| port.bits.iter().map(|bit| bit.pos))
            .collect();
        let ported = tapped(
            &data,
            (&today.outputs, &pads),
            |pos| sum.contains(&pos),
            (&[], false),
        )
        .unwrap();
        assert!(ported.fed.iter().all(|f| matches!(f, Fed::Fed { .. })));
        let after = Blocks::read(&ported.file.expect("ported")).unwrap();
        assert_eq!(strength_inputs(&after).len(), 16);
        let lines = interface(&after);
        let report = Kind::HexAdder.test().unwrap().run_with(&after, lines);
        let crate::simulate::Report::Ran(run) = report else {
            panic!("not run: {report:?}");
        };
        assert_eq!(run.wrong, 0);
        assert_eq!(run.inputs[0].bits, 32);
    }
}
