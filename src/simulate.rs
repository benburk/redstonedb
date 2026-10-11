//! Runs a build in MCHPRS's redstone compiler, Redpiler, to check what it
//! computes.
//!
//! Redpiler models dust, torches, repeaters, comparators, levers, buttons,
//! and lamps, but not pistons, observers, or the order in which the game
//! updates blocks. A build using anything else is refused rather than
//! simulated wrongly.

mod timing;

use std::{
    fmt,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Result, bail};
use mchprs_blocks::{
    BlockFace, BlockPos, block_entities::BlockEntity, blocks::Block,
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

    /// The value output `port` shows, as of the last `settle`.
    pub fn get(&self, port: usize) -> u64 {
        self.outputs[port]
            .iter()
            .enumerate()
            .filter(|&(_, &pos)| {
                let block = self.world.get_block(pos);
                let face = BlockFace::Top;
                mchprs_redstone::get_redstone_power(
                    block,
                    &self.world,
                    pos,
                    face,
                ) > 0
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

/// The test a build gets from its category and width: for now, only 8-bit
/// adders have one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Test {
    /// Every sum of A, B, and any carry in.
    Adder,
}

impl Test {
    pub fn of(category: &str, width: Option<i64>) -> Option<Self> {
        (category == "adder" && width == Some(8)).then_some(Test::Adder)
    }

    /// The name stored with its result.
    pub fn name(self) -> &'static str {
        match self {
            Test::Adder => "adder",
        }
    }

    /// Runs the test's cases on `blocks` and checks each result.
    pub fn run(self, blocks: &Blocks) -> Report {
        let result = match self {
            Test::Adder => check_adder(blocks),
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
            Test::Adder => {
                let ports = adder_ports(&mut interface)?;
                let mut inputs = vec![ports.a, ports.b];
                inputs.extend(ports.carry_in);
                inputs
            }
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
    /// Whether those were every combination of inputs.
    pub exhaustive: bool,
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

/// Up to this many input bits, every combination is tried; beyond it, a
/// fixed sample of `SAMPLES` is.
const EXHAUSTIVE_BITS: usize = 20;
const SAMPLES: u64 = 1 << 16;

/// The adder's ports, by their place in its interface.
struct AdderPorts {
    a: usize,
    b: usize,
    carry_in: Option<usize>,
    sum: usize,
    carry_out: Option<usize>,
}

/// Runs an adder through its inputs and checks every sum: two operand buses
/// and an optional `carry in`, into one output bus and an optional `carry
/// out` above it. When every sum is wrong, it tries again with carry in
/// flipped, since a lever saved the wrong way round does that.
pub fn check_adder(blocks: &Blocks) -> Result<Run> {
    let mut interface = interface::read(blocks);
    let ports = adder_ports(&mut interface)?;
    let mut run = sweep(blocks, &interface, &ports, false)?;
    if run.wrong == run.cases
        && let Some(port) = ports.carry_in
        && sweep(blocks, &interface, &ports, true)?.wrong == 0
    {
        let name = &interface.inputs[port].name;
        run.hint = Some(format!(
            "Every sum is right with {name} flipped, so its lever is saved \
             the wrong way round."
        ));
    }
    Ok(run)
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

/// Tries the adder's sums, with carry in the other way round when `flip`.
fn sweep(
    blocks: &Blocks,
    interface: &Interface,
    ports: &AdderPorts,
    flip: bool,
) -> Result<Run> {
    let mut circuit = Circuit::new(blocks, interface)?;
    let width = |port: usize| interface.inputs[port].bits.len();
    let (a_bits, b_bits) = (width(ports.a), width(ports.b));
    let sum_bits = interface.outputs[ports.sum].bits.len();
    let result_bits = sum_bits + usize::from(ports.carry_out.is_some());
    let input_bits = a_bits + b_bits + usize::from(ports.carry_in.is_some());
    let exhaustive = input_bits <= EXHAUSTIVE_BITS;
    let cases = if exhaustive { 1 << input_bits } else { SAMPLES };
    let mask = |bits: usize| (1u64 << bits) - 1;
    let mut random = Xorshift(0x9E37_79B9_7F4A_7C15);
    let mut wrong = 0;
    let mut wrong_by_bit = vec![0; result_bits];
    let mut examples = Vec::new();
    for case in 0..cases {
        let inputs = if exhaustive { case } else { random.next() };
        let a = inputs & mask(a_bits);
        let b = inputs >> a_bits & mask(b_bits);
        let carry =
            ports.carry_in.is_some() && inputs >> (a_bits + b_bits) & 1 == 1;
        circuit.set(ports.a, a);
        circuit.set(ports.b, b);
        if let Some(port) = ports.carry_in {
            circuit.set(port, u64::from(carry != flip));
        }
        circuit.settle()?;
        let expected = (a + b + u64::from(carry)) & mask(result_bits);
        let sum = circuit.get(ports.sum);
        let carry_out = ports.carry_out.map(|port| circuit.get(port));
        let got = sum | carry_out.unwrap_or(0) << sum_bits;
        if expected != got {
            wrong += 1;
            for (bit, count) in wrong_by_bit.iter_mut().enumerate() {
                *count += (expected ^ got) >> bit & 1;
            }
            if examples.len() < 5 {
                let split = |result: u64| {
                    let mut values = vec![result & mask(sum_bits)];
                    values.extend(carry_out.map(|_| result >> sum_bits));
                    values
                };
                let mut inputs = vec![a, b];
                inputs.extend(ports.carry_in.map(|_| u64::from(carry)));
                examples.push(Wrong {
                    inputs,
                    expected: split(expected),
                    got: split(got),
                });
            }
        }
    }
    let sum_name = &interface.outputs[ports.sum].name;
    let label = |bit: usize| match ports.carry_out {
        Some(port) if bit == sum_bits => interface.outputs[port].name.clone(),
        _ => format!("{sum_name} bit {bit}"),
    };
    let wrong_bits = wrong_by_bit
        .into_iter()
        .enumerate()
        .filter(|&(_, count)| count > 0)
        .map(|(bit, count)| (label(bit), count))
        .collect();
    let pattern = match ports.carry_in {
        Some(port) => format!("A + B + {}", interface.inputs[port].name),
        None => "A + B".to_owned(),
    };
    let signal = |port: &Port| Signal {
        name: port.name.clone(),
        bits: port.bits.len(),
    };
    let inputs = [Some(ports.a), Some(ports.b), ports.carry_in]
        .into_iter()
        .flatten()
        .map(|i| signal(&interface.inputs[i]))
        .collect();
    let outputs = [Some(ports.sum), ports.carry_out]
        .into_iter()
        .flatten()
        .map(|i| signal(&interface.outputs[i]))
        .collect();
    Ok(Run {
        pattern,
        cases,
        exhaustive,
        wrong,
        inputs,
        outputs,
        wrong_bits,
        examples,
        hint: None,
        timing: None,
    })
}

/// A small, fixed sequence of numbers, so a sampled check repeats exactly.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
