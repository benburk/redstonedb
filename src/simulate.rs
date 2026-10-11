//! Runs a build in MCHPRS's redstone compiler, Redpiler, to check what it
//! computes.
//!
//! Redpiler models dust, torches, repeaters, comparators, levers, buttons,
//! and lamps, and the barrels, furnaces, and hoppers a comparator reads a
//! signal strength from, but not pistons, observers, or
//! the order in which the game updates blocks. A build using anything else
//! is refused rather than simulated wrongly.

mod timing;

use std::{
    collections::HashSet,
    fmt,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Result, bail};
use mchprs_blocks::{
    BlockFace, BlockPos,
    block_entities::{BlockEntity, ContainerType},
    blocks::Block,
};
use mchprs_redpiler::{Compiler, CompilerOptions};
use mchprs_world::{World, testing::TestWorld};
use serde::{Deserialize, Serialize};

use crate::{
    interface::{self, Interface, Port},
    lint,
    schem::{self, Blocks, Pos},
};

pub use timing::{BitTiming, Glitch, InputTiming, OutputTiming, Timing};

/// The most ticks a build may take to settle after its inputs change.
const MAX_TICKS: u32 = 1000;

/// A build compiled for simulation. Each input is driven by its levers, and
/// each output bit is on while its block is powered.
///
/// Redpiler's optimizations stay off: they keep only what drives a lamp or
/// another component it can read back, and an output block is neither. A
/// lamp in its place reads differently, lighting from a torch beside it or
/// a powered block beside it, which do not power the block. So every
/// component's state is written back to the world after each change, and
/// each output block's power is worked out from its neighbours.
pub struct Circuit {
    world: TestWorld,
    compiler: Compiler,
    /// Each input port's levers, least significant first, in the order of
    /// `Interface::inputs`.
    inputs: Vec<Vec<Lever>>,
    /// Each output port's blocks, least significant first, in the order of
    /// `Interface::outputs`.
    outputs: Vec<Vec<BlockPos>>,
    /// The comparators beside an output block. Redpiler leaves their
    /// strength in the world as it was saved, so `settle` sets it.
    comparators: Vec<BlockPos>,
}

struct Lever {
    pos: BlockPos,
    /// Saved on, so that on means a 0.
    inverted: bool,
    on: bool,
}

impl Circuit {
    /// Compiles `blocks`, whose interface is `interface`.
    pub fn new(blocks: &Blocks, interface: &Interface) -> Result<Self> {
        let world = world(blocks)?;
        let inputs = interface
            .inputs
            .iter()
            .map(|port| levers(blocks, port))
            .collect::<Result<_>>()?;
        let outputs: Vec<Vec<BlockPos>> = interface
            .outputs
            .iter()
            .map(|port| {
                port.bits
                    .iter()
                    .map(|bit| {
                        if schem::id(blocks.get(bit.pos)).ends_with("_glass") {
                            bail!(
                                "Output {} is stained glass, which the \
                                 simulator does not read yet.",
                                port.name
                            );
                        }
                        Ok(at(bit.pos))
                    })
                    .collect()
            })
            .collect::<Result<_>>()?;
        let comparators = outputs
            .iter()
            .flatten()
            .flat_map(|pos| BlockFace::values().map(|side| pos.offset(side)))
            .filter(|&pos| matches!(world.get_block(pos), Block::Comparator(_)))
            .collect();
        let mut compiler = Compiler::default();
        let options = CompilerOptions::default();
        let [x, y, z] = blocks.size;
        let bounds =
            (BlockPos::new(0, 0, 0), BlockPos::new(x - 1, y - 1, z - 1));
        let ticks = world.to_be_ticked.clone();
        let monitor = Arc::default();
        compiler.compile(&world, bounds, options, ticks, monitor);
        Ok(Circuit {
            world,
            compiler,
            inputs,
            outputs,
            comparators,
        })
    }

    /// Sets input `port` to `value`, flipping the levers that change.
    pub fn set(&mut self, port: usize, value: u64) {
        for (i, lever) in self.inputs[port].iter_mut().enumerate() {
            let on = (value >> i & 1 == 1) != lever.inverted;
            if lever.on != on {
                self.compiler.on_use_block(lever.pos);
                lever.on = on;
            }
        }
    }

    /// Runs until nothing is left to update, and returns the ticks taken.
    pub fn settle(&mut self) -> Result<u32> {
        let mut ticks = 0;
        while self.compiler.has_pending_ticks() {
            if ticks == MAX_TICKS {
                bail!(
                    "It was still changing {MAX_TICKS} ticks after its \
                     inputs changed."
                );
            }
            self.compiler.tick();
            ticks += 1;
        }
        self.show();
        Ok(ticks)
    }

    /// Runs one tick and shows its state to `get`; or, once nothing is
    /// left to update, returns false and does nothing.
    pub fn step(&mut self) -> bool {
        if !self.pending() {
            return false;
        }
        self.tick();
        true
    }

    /// Runs one tick, whether or not anything is waiting for it, and shows
    /// its state to `get`.
    pub fn tick(&mut self) {
        self.compiler.tick();
        self.show();
    }

    /// Whether anything is left to update.
    pub fn pending(&mut self) -> bool {
        self.compiler.has_pending_ticks()
    }

    /// Writes Redpiler's state into the world, where `get` reads it.
    pub fn show(&mut self) {
        self.compiler.flush(&mut self.world);
        for &pos in &self.comparators {
            if let Block::Comparator(comparator) = self.world.get_block(pos) {
                let output_strength = if comparator.powered { 15 } else { 0 };
                self.world.set_block_entity(
                    pos,
                    BlockEntity::Comparator { output_strength },
                );
            }
        }
    }

    /// The value output `port` shows, as of the last `settle`. A bit is on
    /// while its block is powered, or while a repeater or comparator
    /// standing on it is on, as when the block marks the component that
    /// carries the output out of the build.
    pub fn get(&self, port: usize) -> u64 {
        self.outputs[port]
            .iter()
            .enumerate()
            .filter(|&(_, &pos)| {
                let block = self.world.get_block(pos);
                let face = BlockFace::Top;
                let powered = mchprs_redstone::get_redstone_power(
                    block,
                    &self.world,
                    pos,
                    face,
                ) > 0;
                let above = self.world.get_block(pos.offset(BlockFace::Top));
                powered
                    || matches!(above, Block::Repeater(repeater) if repeater.powered)
                    || matches!(above, Block::Comparator(comparator) if comparator.powered)
            })
            .fold(0, |value, (i, _)| value | 1 << i)
    }
}

/// An MCHPRS world holding `blocks` as they were saved.
fn world(blocks: &Blocks) -> Result<TestWorld> {
    let chunks = |n: i32| (n + 15) / 16;
    let [x, y, z] = blocks.size;
    let mut world = TestWorld::new(chunks(x), chunks(y), chunks(z));
    for (pos, state) in blocks.iter() {
        world.set_block(at(pos), block(state)?);
    }
    for (&pos, &output_strength) in &blocks.comparators {
        world.set_block_entity(
            at(pos),
            BlockEntity::Comparator { output_strength },
        );
    }
    for (&pos, items) in &blocks.containers {
        let ty = match world.get_block(at(pos)) {
            Block::Furnace { .. } => ContainerType::Furnace,
            Block::Barrel { .. } => ContainerType::Barrel,
            Block::Hopper { .. } => ContainerType::Hopper,
            _ => continue,
        };
        world.set_block_entity(
            at(pos),
            BlockEntity::Container {
                comparator_override: schem::container_strength(
                    items,
                    ty.num_slots().into(),
                ),
                inventory: Vec::new(),
                ty,
            },
        );
    }
    settle_world(&mut world, blocks.size)?;
    Ok(world)
}

/// Brings a world to rest as the game would: updates every block once,
/// then runs its scheduled ticks until none are left. A file may be saved
/// unsettled, as when a block was changed by hand and what it drives was
/// not, and Redpiler starts from the state it is given.
fn settle_world(world: &mut TestWorld, [x, y, z]: Pos) -> Result<()> {
    for y in 0..y {
        for z in 0..z {
            for x in 0..x {
                let pos = BlockPos::new(x, y, z);
                mchprs_redstone::update(world.get_block(pos), world, pos);
            }
        }
    }
    let mut ticks = 0;
    while !world.to_be_ticked.is_empty() {
        if ticks == MAX_TICKS {
            bail!(
                "It was still changing {MAX_TICKS} ticks after it was loaded."
            );
        }
        ticks += 1;
        world
            .to_be_ticked
            .sort_by_key(|entry| (entry.ticks_left, entry.tick_priority));
        for entry in &mut world.to_be_ticked {
            entry.ticks_left = entry.ticks_left.saturating_sub(1);
        }
        while world
            .to_be_ticked
            .first()
            .is_some_and(|e| e.ticks_left == 0)
        {
            let entry = world.to_be_ticked.remove(0);
            mchprs_redstone::tick(world.get_block(entry.pos), world, entry.pos);
        }
    }
    Ok(())
}

fn at([x, y, z]: Pos) -> BlockPos {
    BlockPos::new(x, y, z)
}

/// A block state's id without its namespace, such as `lever`.
fn short(state: &str) -> &str {
    let id = schem::id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// The levers driving `port`'s bits.
fn levers(blocks: &Blocks, port: &Port) -> Result<Vec<Lever>> {
    if port.signal_strength {
        bail!(
            "Input {} takes a signal strength, which the simulator does not \
             drive yet.",
            port.name
        );
    }
    port.bits
        .iter()
        .map(|bit| {
            let pos =
                bit.entry.as_deref().map(|side| schem::step(bit.pos, side));
            let Some(pos) = pos
                .filter(|&pos| schem::id(blocks.get(pos)) == "minecraft:lever")
            else {
                bail!("A bit of input {} has no lever.", port.name);
            };
            Ok(Lever {
                pos: at(pos),
                inverted: bit.inverted,
                on: bit.inverted,
            })
        })
        .collect()
}

/// The MCHPRS block for a Sponge block state, such as
/// `minecraft:lever[face=wall,facing=south,powered=false]`.
fn block(state: &str) -> Result<Block> {
    let id = schem::id(state);
    let name = short(state);
    let known = name.ends_with("_concrete")
        || name.ends_with("_stained_glass")
        || name.ends_with("_sign")
        || matches!(
            name,
            "air"
                | "glass"
                | "redstone_wire"
                | "redstone_torch"
                | "redstone_wall_torch"
                | "repeater"
                | "comparator"
                | "lever"
                | "stone_button"
                | "redstone_block"
                | "redstone_lamp"
                | "target"
                | "furnace"
                | "barrel"
                | "hopper"
                // Shapes dust, which joins to it, and with nothing to trip
                // it gives no power.
                | "tripwire_hook"
        );
    let Some(mut block) = Block::from_name(id).filter(|_| known) else {
        bail!(
            "It has a {}, which the simulator does not model.",
            name.replace('_', " ")
        );
    };
    if let Some(properties) =
        state.split_once('[').and_then(|(_, p)| p.strip_suffix(']'))
    {
        block.set_properties(
            properties
                .split(',')
                .filter_map(|p| p.split_once('='))
                .collect(),
        );
    }
    Ok(block)
}

/// The test a build gets from its category and width: for now, adders and
/// combinational multipliers of up to 32 bits have one. Each tries every
/// case when they number few enough, else every case of each byte and the
/// test patterns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Test {
    /// The sums of A, B, and any carry in.
    Adder,
    /// The products of A and B, plus C when there is one.
    Multiplier,
}

impl Test {
    pub fn of(category: &str, width: Option<i64>) -> Option<Self> {
        if !width.is_some_and(|w| (1..=MAX_WIDTH).contains(&w)) {
            return None;
        }
        match category {
            "adder" => Some(Test::Adder),
            "multiplier" => Some(Test::Multiplier),
            _ => None,
        }
    }

    /// The name stored with its result.
    pub fn name(self) -> &'static str {
        match self {
            Test::Adder => "adder",
            Test::Multiplier => "multiplier",
        }
    }

    /// Runs the test's cases on `blocks` and checks each result.
    pub fn run(self, blocks: &Blocks) -> Report {
        let result = match self {
            Test::Adder => check_adder(blocks),
            Test::Multiplier => check_multiplier(blocks),
        };
        match result {
            Ok(run) => Report::Ran(Box::new(run)),
            Err(err) => match err.downcast_ref::<Broken>() {
                Some(Broken(reason)) => Report::Broken {
                    reason: reason.clone(),
                },
                None => Report::Skipped {
                    reason: err.to_string(),
                },
            },
        }
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        let mut interface = interface::read(blocks);
        let inputs = match self {
            Test::Adder => adder_bench(&mut interface)?.inputs,
            Test::Multiplier => multiplier_bench(&interface)?.inputs,
        };
        timing::measure(blocks, &interface, &inputs, cancel)
    }
}

/// Why a build cannot be what its category says, such as an adder whose
/// operands differ in width. Unlike a block the simulator does not model,
/// this is the build's fault, so it counts as failing.
#[derive(Debug)]
struct Broken(String);

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Broken {}

/// Fails with `Broken` and a message formatted as `format!` does.
macro_rules! broken {
    ($($arg:tt)*) => {
        return Err(Broken(format!($($arg)*)).into())
    };
}

/// What a test found.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Report {
    Ran(Box<Run>),
    /// The build could not be simulated, such as for a block the simulator
    /// does not model.
    Skipped {
        reason: String,
    },
    /// The build cannot be what its category says, such as an adder whose
    /// operands differ in width.
    Broken {
        reason: String,
    },
}

impl Report {
    pub fn passed(&self) -> bool {
        matches!(self, Report::Ran(run) if run.wrong == 0)
    }

    /// Whether the build got cases wrong or is broken; one that was skipped
    /// has not failed.
    pub fn failed(&self) -> bool {
        match self {
            Report::Ran(run) => run.wrong > 0,
            Report::Broken { .. } => true,
            Report::Skipped { .. } => false,
        }
    }

    /// When its outputs settle, once measured.
    pub fn timing(&self) -> Option<&Timing> {
        match self {
            Report::Ran(run) => run.timing.as_ref(),
            _ => None,
        }
    }
}

/// A test that ran, case by case.
#[derive(Serialize, Deserialize, Debug)]
pub struct Run {
    /// What each case is, such as `A + B + carry in`.
    pub pattern: String,
    /// The cases tried.
    pub cases: u64,
    /// Whether those were every combination of inputs. When not, they were
    /// every case of each byte and every pairing of the test patterns.
    pub exhaustive: bool,
    /// How many of the cases were pairings of the test patterns.
    #[serde(default)]
    pub patterns: u64,
    /// How many cases were wrong.
    pub wrong: u64,
    /// What each case sets, in order, such as A, B, and carry in. A
    /// passing result stored before these were kept has none.
    #[serde(default)]
    pub inputs: Vec<Signal>,
    /// What each case reads, in order, such as the output and carry out.
    #[serde(default)]
    pub outputs: Vec<Signal>,
    /// Each result bit that was ever wrong, such as `output bit 7` or
    /// `carry out`, with how many cases it was wrong in.
    pub wrong_bits: Vec<(String, u64)>,
    /// The first few wrong cases.
    pub examples: Vec<Wrong>,
    /// What would make every case right, when one change does, such as
    /// flipping carry in.
    pub hint: Option<String>,
    /// When its outputs settle; measured after the cases, and only for a
    /// build that gets them all right.
    #[serde(default)]
    pub timing: Option<Timing>,
}

/// A port a test sets or reads, by the build's name for it.
#[derive(Serialize, Deserialize, Debug)]
pub struct Signal {
    pub name: String,
    pub bits: usize,
}

/// One case a build got wrong: each input's value, and each output's
/// expected and actual value, in the order of `Run`'s.
#[derive(Serialize, Deserialize, Debug)]
pub struct Wrong {
    pub inputs: Vec<u64>,
    pub expected: Vec<u64>,
    pub got: Vec<u64>,
}

/// Up to this many input bits, every combination is tried; beyond it, every
/// case of each byte and every pairing of the test patterns.
const EXHAUSTIVE_BITS: usize = 20;

/// The widest adder tested, whose sum and carry fit one number.
const MAX_WIDTH: i64 = 32;

/// `bits` ones, the lowest bits.
fn mask(bits: usize) -> u64 {
    match bits {
        0 => 0,
        bits => u64::MAX >> 64usize.saturating_sub(bits),
    }
}

/// Values of `width` bits that make an adder carry far and across its
/// groups: none and all set; ones and zeros alternating in runs of one,
/// two, and four, at every shift; and each bit alone set or clear.
pub(crate) fn patterns(width: usize) -> Vec<u64> {
    let all = mask(width);
    let mut found = vec![0, all];
    for period in [2, 4, 8] {
        for shift in 0..period {
            let runs = (0..width)
                .filter(|bit| (bit + shift) % period < period / 2)
                .fold(0, |value, bit| value | 1 << bit);
            found.push(runs);
        }
    }
    for bit in 0..width {
        found.extend([1 << bit, all & !(1 << bit)]);
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// The sums an adder too wide to try every combination of is checked on,
/// as A, B, and carry in, with how many pair the test patterns. Each byte
/// takes every value of A and B, with a carry into it and without: the
/// bits below are all set in A, and carry in, or B's lowest bit when there
/// is no carry in line, carries through them. Then every pairing of
/// `patterns`, with carry in each way.
fn adder_cases(width: usize, carry_in: bool) -> (Vec<Case>, u64) {
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |cases: &mut Vec<_>, case: (u64, u64, bool)| {
        let case = [case.0, case.1, u64::from(case.2)];
        if seen.insert(case) {
            cases.push(case);
        }
    };
    for low in (0..width).step_by(8) {
        let bits = (width - low).min(8);
        for carry in [false, true] {
            let below = match (low, carry, carry_in) {
                (0, true, false) => continue,
                (0, _, _) => (0, 0, carry),
                (_, false, _) => (mask(low), 0, false),
                (_, true, true) => (mask(low), 0, true),
                (_, true, false) => (mask(low), 1, false),
            };
            for a in 0..1 << bits {
                for b in 0..1 << bits {
                    let case =
                        (below.0 | a << low, below.1 | b << low, below.2);
                    add(&mut cases, case);
                }
            }
        }
    }
    let bytes = cases.len();
    let values = patterns(width);
    for &a in &values {
        for &b in &values {
            add(&mut cases, (a, b, false));
            if carry_in {
                add(&mut cases, (a, b, true));
            }
        }
    }
    let paired = (cases.len() - bytes) as u64;
    (cases, paired)
}

/// One case's input values, in the order of its bench's inputs; those it
/// has no input for are 0.
type Case = [u64; 3];

/// What a test drives and reads.
struct Bench {
    /// Input ports, by their place in the interface, in the order of each
    /// case's values.
    inputs: Vec<usize>,
    /// Output ports, least significant first, read together as one number.
    outputs: Vec<usize>,
    /// What each case is, such as `A + B + carry in`.
    pattern: String,
}

/// Runs an adder through its inputs and checks every sum: two operand buses
/// and an optional `carry in`, into one output bus and an optional `carry
/// out` above it. When every sum is wrong, it tries again with carry in
/// flipped, since a lever saved the wrong way round does that.
pub fn check_adder(blocks: &Blocks) -> Result<Run> {
    let mut interface = interface::read(blocks);
    let bench = adder_bench(&mut interface)?;
    let widths = bench.widths(&interface);
    let carry_in = bench.inputs.get(2).copied();
    let (cases, exhaustive, patterns) =
        cases(&widths, || adder_cases(widths[0], carry_in.is_some()));
    let sum = |case: &Case| case[0] + case[1] + case[2];
    let sweep = |flip| {
        sweep(blocks, &interface, &bench, &cases, flip, sum)
            .map(|run| run.covering(exhaustive, patterns))
    };
    let mut run = sweep(None)?;
    if run.wrong == run.cases
        && let Some(port) = carry_in
        && sweep(Some(2))?.wrong == 0
    {
        let name = &interface.inputs[port].name;
        run.hint = Some(format!(
            "Every sum is right with {name} flipped, so its lever is saved \
             the wrong way round."
        ));
    }
    Ok(run)
}

/// What an adder's test drives and reads: A, B, and its carry in, if any;
/// its output, then its carry out, if any.
fn adder_bench(interface: &mut Interface) -> Result<Bench> {
    let ports = adder_ports(interface)?;
    let mut inputs = vec![ports.a, ports.b];
    inputs.extend(ports.carry_in);
    let mut outputs = vec![ports.sum];
    outputs.extend(ports.carry_out);
    let pattern = match ports.carry_in {
        Some(port) => format!("A + B + {}", interface.inputs[port].name),
        None => "A + B".to_owned(),
    };
    Ok(Bench {
        inputs,
        outputs,
        pattern,
    })
}

/// The adder's ports, by their place in its interface.
struct AdderPorts {
    a: usize,
    b: usize,
    carry_in: Option<usize>,
    sum: usize,
    carry_out: Option<usize>,
}

/// The adder's ports. Its one magenta control line, if any, is its carry
/// in and its one pink status line its carry out, signed or not; a sign on
/// either must say so. An unsigned one is named for its role here.
fn adder_ports(interface: &mut Interface) -> Result<AdderPorts> {
    let carry_in = adder_line(
        &mut interface.inputs,
        lint::CONTROL,
        &lint::CARRY_IN,
        "control line is its carry in",
    )?;
    let carry_out = adder_line(
        &mut interface.outputs,
        lint::STATUS,
        &lint::CARRY_OUT,
        "status line is its carry out",
    )?;
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| Some(i) != carry_in)
        .collect();
    let &[a, b] = operands.as_slice() else {
        broken!(
            "An adder takes two operands, but this has {}.",
            operands.len()
        );
    };
    let buses: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| Some(i) != carry_out)
        .collect();
    let &[sum] = buses.as_slice() else {
        broken!(
            "An adder has one output bus and a carry out, but this has {} \
             output buses.",
            buses.len()
        );
    };
    // The output may hold the carry as its top bit when there is no carry
    // out line.
    let bits = |port: &Port| port.bits.len();
    let (a_bits, b_bits) =
        (bits(&interface.inputs[a]), bits(&interface.inputs[b]));
    let sum_bits = bits(&interface.outputs[sum]);
    let carry_bit = usize::from(carry_out.is_none());
    if a_bits != b_bits || !(a_bits..=a_bits + carry_bit).contains(&sum_bits) {
        broken!(
            "A has {a_bits} bits, B {b_bits}, and the output {sum_bits}, but \
             an adder's operands and output have the same number."
        );
    }
    Ok(AdderPorts {
        a,
        b,
        carry_in,
        sum,
        carry_out,
    })
}

/// The one line of `colour` among `ports`, if there is one: unsigned, when
/// it takes its role's first name, or signed with one of `names`. More
/// than one, or a sign naming something else, is broken; `role` says what
/// the line is, as in `control line is its carry in`.
fn adder_line(
    ports: &mut [Port],
    colour: &str,
    names: &[&str],
    role: &str,
) -> Result<Option<usize>> {
    let lines: Vec<usize> = (0..ports.len())
        .filter(|&i| ports[i].block.starts_with(colour))
        .collect();
    let count: usize = lines.iter().map(|&i| ports[i].bits.len()).sum();
    if count > 1 {
        broken!("An adder's {role}, but this has {count} such lines.");
    }
    let Some(&line) = lines.first() else {
        return Ok(None);
    };
    let port = &mut ports[line];
    if !port.labelled {
        names[0].clone_into(&mut port.name);
    } else if !names.contains(&port.name.as_str()) {
        broken!("An adder's {role}, but this one is signed {}.", port.name);
    }
    Ok(Some(line))
}

/// Runs a combinational multiplier through its inputs and checks every
/// product: A times B, plus C for a fused multiply-add, against as many low
/// bits as its outputs hold together, the first holding the lowest.
pub fn check_multiplier(blocks: &Blocks) -> Result<Run> {
    let interface = interface::read(blocks);
    let bench = multiplier_bench(&interface)?;
    let widths = bench.widths(&interface);
    let (cases, exhaustive, patterns) =
        cases(&widths, || multiplier_cases(&widths));
    let product = |case: &Case| {
        let exact =
            u128::from(case[0]) * u128::from(case[1]) + u128::from(case[2]);
        // The low bits are all that is compared.
        #[allow(clippy::cast_possible_truncation)]
        let low = exact as u64;
        low
    };
    Ok(sweep(blocks, &interface, &bench, &cases, None, product)?
        .covering(exhaustive, patterns))
}

/// What a multiplier's test drives and reads: A, B, and C, if any; its
/// output buses, status lines left out. A build with control lines, such
/// as a clock, is sequential, which this test does not cover.
fn multiplier_bench(interface: &Interface) -> Result<Bench> {
    let control = |port: &&Port| port.block.starts_with(lint::CONTROL);
    if let Some(line) = interface.inputs.iter().find(control) {
        bail!(
            "It has a control line, {}, so it is not combinational; only a \
             combinational multiplier is tested.",
            line.name
        );
    }
    let inputs: Vec<usize> = (0..interface.inputs.len()).collect();
    let outputs: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| !interface.outputs[i].block.starts_with(lint::STATUS))
        .collect();
    let widths: Vec<usize> = inputs
        .iter()
        .map(|&i| interface.inputs[i].bits.len())
        .collect();
    let pattern = match widths.len() {
        2 => "A × B",
        3 => "A × B + C",
        n => broken!(
            "A multiplier takes two operands, or three for A × B + C, but \
             this has {n}."
        ),
    };
    let result: usize = outputs
        .iter()
        .map(|&i| interface.outputs[i].bits.len())
        .sum();
    let (a, b) = (widths[0], widths[1]);
    if result == 0 || result < a.max(b) || result > a + b {
        broken!(
            "A and B have {a} and {b} bits, so the product has from {} to {} \
             bits, but the outputs have {result}.",
            a.max(b),
            a + b
        );
    }
    Ok(Bench {
        inputs,
        outputs,
        pattern: pattern.to_owned(),
    })
}

/// The products a multiplier too wide to try every one of is checked on:
/// every value of each byte of A with every value of each byte of B, the
/// other bits clear; every value of each byte of C, if any, with A and B
/// clear or all set; and every pairing of the test patterns.
fn multiplier_cases(widths: &[usize]) -> (Vec<Case>, u64) {
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |cases: &mut Vec<Case>, case: Case| {
        if seen.insert(case) {
            cases.push(case);
        }
    };
    let bytes = |width: usize| {
        (0..width)
            .step_by(8)
            .map(move |low| (low, (width - low).min(8)))
    };
    for (low_a, bits_a) in bytes(widths[0]) {
        for (low_b, bits_b) in bytes(widths[1]) {
            for a in 0..1u64 << bits_a {
                for b in 0..1u64 << bits_b {
                    add(&mut cases, [a << low_a, b << low_b, 0]);
                }
            }
        }
    }
    let addend = widths.get(2).copied();
    if let Some(width) = addend {
        let full = [mask(widths[0]), mask(widths[1])];
        for (low, bits) in bytes(width) {
            for c in 0..1u64 << bits {
                add(&mut cases, [0, 0, c << low]);
                add(&mut cases, [full[0], full[1], c << low]);
            }
        }
    }
    let before = cases.len();
    let addends = addend.map_or_else(|| vec![0], patterns);
    for &a in &patterns(widths[0]) {
        for &b in &patterns(widths[1]) {
            for &c in &addends {
                add(&mut cases, [a, b, c]);
            }
        }
    }
    let paired = (cases.len() - before) as u64;
    (cases, paired)
}

impl Bench {
    /// Each input's width, in the order of `inputs`.
    fn widths(&self, interface: &Interface) -> Vec<usize> {
        self.inputs
            .iter()
            .map(|&i| interface.inputs[i].bits.len())
            .collect()
    }
}

/// Every combination of inputs of `widths` bits when they come to at most
/// `EXHAUSTIVE_BITS`, else what `structured` gives, with whether they were
/// every one and how many pair the test patterns.
fn cases(
    widths: &[usize],
    structured: impl FnOnce() -> (Vec<Case>, u64),
) -> (Vec<Case>, bool, u64) {
    let bits: usize = widths.iter().sum();
    if bits > EXHAUSTIVE_BITS {
        let (cases, patterns) = structured();
        return (cases, false, patterns);
    }
    let every = (0..1u64 << bits)
        .map(|packed| {
            let mut case = [0; 3];
            let mut shift = 0;
            for (value, &width) in case.iter_mut().zip(widths) {
                *value = packed >> shift & mask(width);
                shift += width;
            }
            case
        })
        .collect();
    (every, true, 0)
}

impl Run {
    fn covering(mut self, exhaustive: bool, patterns: u64) -> Self {
        self.exhaustive = exhaustive;
        self.patterns = patterns;
        self
    }
}

/// Runs `cases` through the bench's inputs, driving the input at `flip`,
/// if given, the other way round, and checks each result against
/// `expected`, cut to as many bits as the outputs hold.
fn sweep(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    flip: Option<usize>,
    expected: impl Fn(&Case) -> u64,
) -> Result<Run> {
    let mut circuit = Circuit::new(blocks, interface)?;
    let widths: Vec<usize> = bench
        .outputs
        .iter()
        .map(|&i| interface.outputs[i].bits.len())
        .collect();
    let result_bits: usize = widths.iter().sum();
    // Each output's value from a result, the first holding the lowest bits.
    let split = |result: u64| {
        let mut shift = 0;
        widths
            .iter()
            .map(|&width| {
                let value = result >> shift & mask(width);
                shift += width;
                value
            })
            .collect::<Vec<u64>>()
    };
    let mut wrong = 0;
    let mut wrong_by_bit = vec![0; result_bits];
    let mut examples = Vec::new();
    for case in cases {
        for (i, &port) in bench.inputs.iter().enumerate() {
            let value = case[i];
            circuit.set(port, if flip == Some(i) { value ^ 1 } else { value });
        }
        circuit.settle()?;
        let want = expected(case) & mask(result_bits);
        let mut got = 0;
        let mut shift = 0;
        for (&port, &width) in bench.outputs.iter().zip(&widths) {
            got |= circuit.get(port) << shift;
            shift += width;
        }
        if want != got {
            wrong += 1;
            for (bit, count) in wrong_by_bit.iter_mut().enumerate() {
                *count += (want ^ got) >> bit & 1;
            }
            if examples.len() < 5 {
                examples.push(Wrong {
                    inputs: case[..bench.inputs.len()].to_vec(),
                    expected: split(want),
                    got: split(got),
                });
            }
        }
    }
    // Each result bit as its output's name, and its place in it when it has
    // more than one.
    let labels: Vec<String> = bench
        .outputs
        .iter()
        .flat_map(|&i| {
            let port = &interface.outputs[i];
            let bits = port.bits.len();
            (0..bits).map(move |bit| match bits {
                1 => port.name.clone(),
                _ => format!("{} bit {bit}", port.name),
            })
        })
        .collect();
    let wrong_bits = wrong_by_bit
        .into_iter()
        .zip(labels)
        .filter(|&(count, _)| count > 0)
        .map(|(count, label)| (label, count))
        .collect();
    let signal = |port: &Port| Signal {
        name: port.name.clone(),
        bits: port.bits.len(),
    };
    Ok(Run {
        pattern: bench.pattern.clone(),
        cases: cases.len() as u64,
        exhaustive: true,
        patterns: 0,
        wrong,
        inputs: bench
            .inputs
            .iter()
            .map(|&i| signal(&interface.inputs[i]))
            .collect(),
        outputs: bench
            .outputs
            .iter()
            .map(|&i| signal(&interface.outputs[i]))
            .collect(),
        wrong_bits,
        examples,
        hint: None,
        timing: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adders_up_to_32_bits_are_tested() {
        assert_eq!(Test::of("adder", Some(16)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(32)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(33)), None);
        assert_eq!(Test::of("adder", None), None);
        assert_eq!(Test::of("register", Some(8)), None);
    }

    #[test]
    fn combinational_multipliers_are_tested() {
        assert_eq!(Test::of("multiplier", Some(8)), Some(Test::Multiplier));
        assert_eq!(Test::of("multiplier", Some(33)), None);
    }

    #[test]
    fn a_wide_multiplier_pairs_every_byte_of_a_with_every_byte_of_b() {
        let (cases, paired) = multiplier_cases(&[16, 16]);
        let bytes = cases.len() as u64 - paired;
        // Four pairs of bytes, less the cases they share, such as 0 × 0.
        assert!(bytes > 4 * (1 << 16) - 1024 && bytes <= 4 * (1 << 16));
        assert!(cases.contains(&[0xFF00, 0x00FF, 0]));
        assert!(cases.contains(&[0xFFFF, 0xFFFF, 0]));
        // A × B + C: each byte of C, with A and B clear and all set.
        let (cases, _) = multiplier_cases(&[8, 8, 16]);
        assert!(cases.contains(&[0, 0, 0xAB00]));
        assert!(cases.contains(&[0xFF, 0xFF, 0x00CD]));
    }

    #[test]
    fn the_patterns_take_every_shift_and_each_bit_alone() {
        let found = patterns(16);
        assert_eq!(found.len(), 48);
        for value in [
            0, 0xFFFF, 0x5555, 0xAAAA, 0x3333, 0x6666, 0x0F0F, 0xF0F0, 0x8000,
            0xFFFE,
        ] {
            assert!(found.contains(&value), "{value:#x}");
        }
    }

    #[test]
    fn a_wide_adder_takes_every_case_of_each_byte_and_the_patterns() {
        let (cases, paired) = adder_cases(16, true);
        // Each byte with and without a carry, less two the high byte
        // repeats: A's low byte all set, with or without carry in.
        assert_eq!(cases.len() as u64 - paired, 4 * (1 << 16) - 2);
        // A carry from carry in through the low byte into the high one.
        assert!(cases.contains(&[0x00FF, 0x0F00, 1]));
        // The longest carry: all ones plus one.
        assert!(cases.contains(&[0xFFFF, 0x0001, 0]));
        // Without a carry in line, B's lowest bit makes the carry.
        let (cases, _) = adder_cases(16, false);
        assert!(cases.contains(&[0x00FF, 0x0101, 0]));
        assert!(cases.iter().all(|case| case[2] == 0));
    }

    #[test]
    fn a_block_it_does_not_model_is_refused_by_name() {
        let err =
            block("minecraft:piston[extended=false,facing=up]").unwrap_err();
        assert_eq!(
            err.to_string(),
            "It has a piston, which the simulator does not model."
        );
    }

    #[test]
    fn parses_a_block_state_with_its_properties() {
        let lever =
            block("minecraft:lever[face=wall,facing=south,powered=true]")
                .unwrap();
        assert!(matches!(lever, Block::Lever { powered: true, .. }));
    }

    /// A port of `bits` bits named `name`, of `block`.
    fn port(name: &str, block: &str, labelled: bool, bits: usize) -> Port {
        Port {
            name: name.to_owned(),
            block: block.to_owned(),
            labelled,
            bits: (0..bits)
                .map(|_| interface::Bit {
                    pos: [0, 0, 0],
                    entry: Some("south".to_owned()),
                    hard: false,
                    inverted: false,
                })
                .collect(),
            strength: 1,
            signal_strength: false,
        }
    }

    fn adder(a: usize, b: usize, output: usize, carry_out: bool) -> Interface {
        let mut outputs =
            vec![port("output", "yellow concrete", false, output)];
        if carry_out {
            outputs.push(port("carry out", "pink concrete", true, 1));
        }
        Interface {
            inputs: vec![
                port("A", "lime concrete", false, a),
                port("B", "light blue concrete", false, b),
            ],
            outputs,
        }
    }

    #[test]
    fn an_adder_whose_operands_and_output_differ_in_width_is_broken() {
        let broken = |mut interface: Interface| {
            adder_ports(&mut interface)
                .err()
                .is_some_and(|err| err.downcast_ref::<Broken>().is_some())
        };
        assert!(!broken(adder(8, 8, 8, true)));
        assert!(broken(adder(8, 7, 8, true)));
        assert!(broken(adder(8, 8, 9, true)));
        // Without a carry out line, the output holds the carry on top.
        assert!(!broken(adder(8, 8, 9, false)));
    }

    #[test]
    fn an_adders_lone_lines_are_its_carries_signed_or_not() {
        let ports = |mut interface: Interface| {
            let ports = adder_ports(&mut interface)?;
            let names = (
                ports.carry_in.map(|i| interface.inputs[i].name.clone()),
                ports.carry_out.map(|o| interface.outputs[o].name.clone()),
            );
            anyhow::Ok(names)
        };
        let carries = |carry_in: Port, carry_out: Port| {
            let mut interface = adder(8, 8, 8, false);
            interface.inputs.push(carry_in);
            interface.outputs.push(carry_out);
            interface
        };
        let unsigned = carries(
            port("unnamed control", "magenta concrete", false, 1),
            port("unnamed status", "pink concrete", false, 1),
        );
        assert_eq!(
            ports(unsigned).unwrap(),
            (Some("carry in".to_owned()), Some("carry out".to_owned()))
        );
        let signed = carries(
            port("cin", "magenta concrete", true, 1),
            port("cout", "pink concrete", true, 1),
        );
        assert_eq!(
            ports(signed).unwrap(),
            (Some("cin".to_owned()), Some("cout".to_owned()))
        );
        let subtract = carries(
            port("subtract", "magenta concrete", true, 1),
            port("cout", "pink concrete", true, 1),
        );
        assert!(ports(subtract).is_err());
        let two = carries(
            port("unnamed control", "magenta concrete", false, 2),
            port("cout", "pink concrete", true, 1),
        );
        assert!(ports(two).is_err());
    }

    #[test]
    fn refuses_a_piston() {
        assert!(block("minecraft:piston[extended=false,facing=up]").is_err());
    }
}
