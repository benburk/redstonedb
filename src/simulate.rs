//! Runs a build in MCHPRS's redstone compiler, Redpiler, to check what it
//! computes.
//!
//! Redpiler models dust, torches, repeaters, comparators, levers, buttons,
//! and lamps, and the barrels, furnaces, and hoppers a comparator reads a
//! signal strength from, but not pistons, observers, or
//! the order in which the game updates blocks. A build using anything else
//! is refused rather than simulated wrongly.

mod stacking;
mod timing;

use std::{
    collections::HashSet,
    fmt,
    panic::AssertUnwindSafe,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, Result, bail};
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

pub use stacking::{Stacking, Step};
pub use timing::{BitTiming, Glitch, InputTiming, OutputTiming, Timing};

/// The most ticks a build may take to settle after its inputs change.
const MAX_TICKS: u32 = 1000;

/// A build set up for simulation. Each input is driven by its levers, and
/// each output bit is on while its block is powered; a signal-strength
/// input takes each digit's strength, and an output of a build with one
/// shows a digit per block, its strength.
///
/// A build driven by levers alone is compiled by Redpiler, whose
/// optimizations stay off: they keep only what drives a lamp or another
/// component it can read back, and an output block is neither. A lamp in
/// its place reads differently, lighting from a torch beside it or a
/// powered block beside it, which do not power the block. So every
/// component's state is written back to the world after each change, and
/// each output block's power is worked out from its neighbours.
///
/// Redpiler reads a container's strength once, when it compiles, so a
/// build with a signal-strength input runs in the world itself, block by
/// block, as the game would: a comparator beside each digit's dust, with a
/// barrel behind it holding the digit, drives it.
pub struct Circuit {
    world: TestWorld,
    /// Redpiler's circuit, for a build driven by levers alone.
    compiler: Option<Compiler>,
    /// How each input port is driven, in the order of `Interface::inputs`.
    inputs: Vec<Drive>,
    /// Each output port's blocks, least significant first, in the order of
    /// `Interface::outputs`, and whether it shows a digit per block.
    outputs: Vec<(Vec<BlockPos>, bool)>,
    /// The comparators beside an output block. Redpiler leaves their
    /// strength in the world as it was saved, so `settle` sets it.
    comparators: Vec<BlockPos>,
}

/// How an input port is driven: by a lever per bit, or by a barrel and
/// comparator per digit.
enum Drive {
    Levers(Vec<Lever>),
    Strengths(Vec<Driver>),
}

/// A lever, or a stone button, which `set` leaves alone and `press`
/// presses.
struct Lever {
    pos: BlockPos,
    /// Saved on, so that on means a 0.
    inverted: bool,
    on: bool,
    button: bool,
}

/// A comparator pointing into a digit's dust, reading the barrel behind it,
/// which holds the digit.
struct Driver {
    comparator: BlockPos,
    barrel: BlockPos,
    value: u8,
}

/// What a panic said, as `panic!` was given it.
pub(crate) fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "it panicked".to_owned())
}

/// How far the world reaches past the build on each side, west, east,
/// north, and south, for the drivers of signal-strength inputs.
const MARGIN: i32 = 2;

impl Circuit {
    /// Sets up `blocks`, whose interface is `interface`.
    pub fn new(blocks: &Blocks, interface: &Interface) -> Result<Self> {
        let mut world = world(blocks)?;
        let digits = interface.inputs.iter().any(|port| port.signal_strength);
        let mut taken = HashSet::new();
        let inputs = interface
            .inputs
            .iter()
            .map(|port| {
                if port.signal_strength {
                    drivers(&mut world, port, &mut taken).map(Drive::Strengths)
                } else {
                    levers(blocks, port).map(Drive::Levers)
                }
            })
            .collect::<Result<_>>()?;
        settle_world(&mut world, world_size(blocks))?;
        let outputs: Vec<(Vec<BlockPos>, bool)> = interface
            .outputs
            .iter()
            .map(|port| {
                let blocks = port
                    .bits
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
                    .collect::<Result<_>>()?;
                Ok((blocks, digits && shows_digits(port)))
            })
            .collect::<Result<_>>()?;
        let comparators = outputs
            .iter()
            .flat_map(|(blocks, _)| blocks)
            .flat_map(|pos| BlockFace::values().map(|side| pos.offset(side)))
            .filter(|&pos| matches!(world.get_block(pos), Block::Comparator(_)))
            .collect();
        let compiler = if digits {
            None
        } else {
            // Redpiler panics on a circuit it cannot compile, such as one
            // whose line takes more than 255 inputs, as copies stacked too
            // close may make; that is a build it cannot run.
            let built = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let mut compiler = Compiler::default();
                let [x, y, z] = world_size(blocks);
                let bounds = (
                    BlockPos::new(0, 0, 0),
                    BlockPos::new(x - 1, y - 1, z - 1),
                );
                let ticks = world.to_be_ticked.clone();
                let options = CompilerOptions::default();
                compiler.compile(
                    &world,
                    bounds,
                    options,
                    ticks,
                    Arc::default(),
                );
                compiler
            }));
            match built {
                Ok(redpiler) => Some(redpiler),
                Err(panic) => bail!(
                    "The simulator could not compile it: {}.",
                    panic_message(&*panic)
                ),
            }
        };
        Ok(Circuit {
            world,
            compiler,
            inputs,
            outputs,
            comparators,
        })
    }

    /// Sets input `port` to `value`, flipping the levers that change, or
    /// filling each digit's barrel.
    pub fn set(&mut self, port: usize, value: u64) {
        match &mut self.inputs[port] {
            Drive::Levers(levers) => {
                for (i, lever) in levers.iter_mut().enumerate() {
                    let on = (value >> i & 1 == 1) != lever.inverted;
                    if lever.on != on && !lever.button {
                        if let Some(compiler) = &mut self.compiler {
                            compiler.on_use_block(lever.pos);
                        } else {
                            let block = self.world.get_block(lever.pos);
                            mchprs_redstone::on_use(
                                block,
                                &mut self.world,
                                lever.pos,
                            );
                        }
                        lever.on = on;
                    }
                }
            }
            Drive::Strengths(drivers) => {
                for (i, driver) in drivers.iter_mut().enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    let digit = (value >> (4 * i) & 15) as u8;
                    if driver.value != digit {
                        self.world
                            .set_block_entity(driver.barrel, container(digit));
                        let block = self.world.get_block(driver.comparator);
                        mchprs_redstone::update(
                            block,
                            &mut self.world,
                            driver.comparator,
                        );
                        driver.value = digit;
                    }
                }
            }
        }
    }

    /// Presses the button on bit `bit` of input `port`, as a player does: it
    /// powers its block until it springs back, ten ticks on.
    pub fn press(&mut self, port: usize, bit: usize) {
        let Drive::Levers(levers) = &self.inputs[port] else {
            return;
        };
        let Some(button) = levers.get(bit).filter(|lever| lever.button) else {
            return;
        };
        let pos = button.pos;
        if let Some(compiler) = &mut self.compiler {
            compiler.on_use_block(pos);
        } else {
            let block = self.world.get_block(pos);
            mchprs_redstone::on_use(block, &mut self.world, pos);
        }
    }

    /// Whether bit `bit` of input `port` is driven by a button.
    pub fn is_button(&self, port: usize, bit: usize) -> bool {
        matches!(&self.inputs[port], Drive::Levers(levers)
            if levers.get(bit).is_some_and(|lever| lever.button))
    }

    /// Runs until nothing is left to update, and returns the ticks taken.
    pub fn settle(&mut self) -> Result<u32> {
        let mut ticks = 0;
        while self.pending() {
            if ticks == MAX_TICKS {
                bail!(
                    "It was still changing {MAX_TICKS} ticks after its \
                     inputs changed."
                );
            }
            self.advance();
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
        self.advance();
        self.show();
    }

    /// One tick, without showing it.
    fn advance(&mut self) {
        match &mut self.compiler {
            Some(compiler) => compiler.tick(),
            None => tick_world(&mut self.world),
        }
    }

    /// Whether anything is left to update.
    pub fn pending(&mut self) -> bool {
        match &mut self.compiler {
            Some(compiler) => compiler.has_pending_ticks(),
            None => !self.world.to_be_ticked.is_empty(),
        }
    }

    /// Writes Redpiler's state into the world, where `get` reads it; the
    /// world holds it already when the build runs there.
    pub fn show(&mut self) {
        let Some(compiler) = &mut self.compiler else {
            return;
        };
        compiler.flush(&mut self.world);
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

    /// The value output `port` shows, as of the last `settle`. Where dust,
    /// a repeater, or a comparator stands on a bit's block, it is the
    /// output, as the component carrying it out of the build, and the bit
    /// is on while it is, however the block below is powered; a bare block
    /// is on while it is powered. A digit is the strength of the dust on
    /// its block, or of the block when it has none.
    pub fn get(&self, port: usize) -> u64 {
        let (blocks, digits) = &self.outputs[port];
        blocks.iter().enumerate().fold(0, |value, (i, &pos)| {
            let block = self.world.get_block(pos);
            let above = self.world.get_block(pos.offset(BlockFace::Top));
            let strength = || {
                mchprs_redstone::get_redstone_power(
                    block,
                    &self.world,
                    pos,
                    BlockFace::Top,
                )
            };
            if *digits {
                let strength = match above {
                    Block::RedstoneWire(wire) => wire.power,
                    _ => strength(),
                };
                return value | u64::from(strength) << (4 * i);
            }
            let on = match above {
                Block::RedstoneWire(wire) => wire.power > 0,
                Block::Repeater(repeater) => repeater.powered,
                Block::Comparator(comparator) => comparator.powered,
                _ => strength() > 0,
            };
            value | u64::from(on) << i
        })
    }

    /// For each output, the ticks the repeaters or comparators standing on
    /// its every block take to carry it out of the build, the least of
    /// them, which its value shows that much later than it arrives; 0 for
    /// an output with a block bare of one.
    pub fn carried(&self) -> Vec<i32> {
        let delay = |pos: BlockPos| match self
            .world
            .get_block(pos.offset(BlockFace::Top))
        {
            Block::Repeater(repeater) => i32::from(repeater.delay),
            Block::Comparator(_) => 1,
            _ => 0,
        };
        self.outputs
            .iter()
            .map(|(blocks, digits)| {
                if *digits {
                    0
                } else {
                    blocks.iter().map(|&pos| delay(pos)).min().unwrap_or(0)
                }
            })
            .collect()
    }
}

/// Whether an output of a build with a signal-strength input shows a digit
/// per block: any but a status line, such as a carry out.
fn shows_digits(port: &Port) -> bool {
    !port.block.starts_with(lint::STATUS)
}

/// How many bits input `port` carries: four for each block of a
/// signal-strength input, a digit per block, else one for each block.
fn input_bits(port: &Port) -> usize {
    port.bits.len() * if port.signal_strength { 4 } else { 1 }
}

/// How many bits output `port` of a build with `interface` carries: four
/// for each block when the build takes a signal strength and the port
/// shows digits, else one for each block.
fn output_bits(interface: &Interface, port: &Port) -> usize {
    let digits = interface.inputs.iter().any(|input| input.signal_strength)
        && shows_digits(port);
    port.bits.len() * if digits { 4 } else { 1 }
}

/// A barrel's block entity giving a comparator `strength`.
fn container(strength: u8) -> BlockEntity {
    BlockEntity::Container {
        comparator_override: strength,
        inventory: Vec::new(),
        ty: ContainerType::Barrel,
    }
}

/// A driver for each digit of signal-strength input `port`: a comparator
/// beside its dust, pointing into it, and a barrel behind, where both are
/// air and nothing runs into the comparator's sides, which would change
/// what it puts out. The side the dust is open to comes first. `taken`
/// holds the places drivers have already used.
fn drivers(
    world: &mut TestWorld,
    port: &Port,
    taken: &mut HashSet<Pos>,
) -> Result<Vec<Driver>> {
    const SIDES: [&str; 4] = ["north", "south", "east", "west"];
    let air = |world: &TestWorld, pos: Pos| {
        matches!(world.get_block(at(pos)), Block::Air)
    };
    let quiet = |world: &TestWorld, pos: Pos| {
        !matches!(
            world.get_block(at(pos)),
            Block::RedstoneWire { .. }
                | Block::Repeater { .. }
                | Block::Comparator { .. }
                | Block::RedstoneTorch { .. }
                | Block::RedstoneWallTorch { .. }
        )
    };
    port.bits
        .iter()
        .map(|bit| {
            let dust = schem::step(bit.pos, "up");
            let entry = bit.entry.as_deref();
            let mut sides: Vec<&str> = entry.into_iter().collect();
            sides.extend(SIDES.iter().filter(|side| Some(**side) != entry));
            let side = sides
                .into_iter()
                .find(|&side| {
                    let comparator = schem::step(dust, side);
                    let barrel = schem::step(comparator, side);
                    let across: [&str; 2] = if matches!(side, "north" | "south") {
                        ["east", "west"]
                    } else {
                        ["north", "south"]
                    };
                    [comparator, barrel].iter().all(|&pos| {
                        air(world, pos) && !taken.contains(&pos)
                    }) && across.iter().all(|&way| {
                        let beside = schem::step(comparator, way);
                        quiet(world, beside) && !taken.contains(&beside)
                    })
                })
                .with_context(|| {
                    format!(
                        "A digit of input {} has no room beside its dust for \
                         the simulator to drive it.",
                        port.name
                    )
                })?;
            let comparator = schem::step(dust, side);
            let barrel = schem::step(comparator, side);
            taken.extend([comparator, barrel]);
            world.set_block(
                at(comparator),
                block(&format!(
                    "minecraft:comparator[facing={side},mode=compare,powered=false]"
                ))?,
            );
            world.set_block_entity(
                at(comparator),
                BlockEntity::Comparator { output_strength: 0 },
            );
            world.set_block(
                at(barrel),
                block("minecraft:barrel[facing=up,open=false]")?,
            );
            world.set_block_entity(at(barrel), container(0));
            Ok(Driver {
                comparator: at(comparator),
                barrel: at(barrel),
                value: 0,
            })
        })
        .collect()
}

/// An MCHPRS world holding `blocks` as they were saved, `MARGIN` blocks in
/// from its west and north sides, not yet settled.
fn world(blocks: &Blocks) -> Result<TestWorld> {
    let chunks = |n: i32| (n + 15) / 16;
    let [x, y, z] = world_size(blocks);
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
    Ok(world)
}

/// The world's size: the build's, with `MARGIN` on each side across.
fn world_size(blocks: &Blocks) -> Pos {
    let [x, y, z] = blocks.size;
    [x + 2 * MARGIN, y, z + 2 * MARGIN]
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
        tick_world(world);
    }
    Ok(())
}

/// One game tick of a world: what was scheduled for now runs, in order of
/// priority.
fn tick_world(world: &mut TestWorld) {
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

/// Where the build's block at `pos` sits in the world.
fn at([x, y, z]: Pos) -> BlockPos {
    BlockPos::new(x + MARGIN, y, z + MARGIN)
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
            let switch = pos.map(|pos| (pos, schem::id(blocks.get(pos))));
            let (pos, button) = match switch {
                Some((pos, "minecraft:lever")) => (pos, false),
                Some((pos, "minecraft:stone_button")) => (pos, true),
                _ => bail!("A bit of input {} has no lever.", port.name),
            };
            Ok(Lever {
                pos: at(pos),
                inverted: bit.inverted,
                on: bit.inverted,
                button,
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

/// The test a build gets from its category and width: for now, adders,
/// incrementers, decrementers, inc/decs, stateless multipliers, and
/// stateless converters between binary and BCD of up to 32 bits have
/// one.
/// Each tries every case when they number few enough, else every case of
/// each byte and the test patterns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Test {
    /// The sums of A, B, and any carry in.
    Adder,
    /// A plus 1, or plus its control line.
    Incrementer,
    /// A minus 1, or minus its control line.
    Decrementer,
    /// A plus 1 one way of its control line and minus 1 the other.
    IncDec,
    /// The products of A and B, plus C when there is one.
    Multiplier,
    /// A in binary-coded decimal: each decimal digit in four bits, the
    /// ones lowest.
    BinaryToBcd,
    /// A's number from binary-coded decimal, for each A whose digits are
    /// all decimal and whose number the outputs can hold.
    BcdToBinary,
    /// Each operation an ALU's control lines name, and any other they give.
    Alu,
    /// A's magnitude, A taken as a signed number.
    AbsoluteValue,
    /// Each value written into every cell, then read.
    Register,
}

/// The categories whose stacking the simulation decides rather than their
/// builders: copies that add right as their bits suggest make the build
/// stackable that way; see `Test::stacking`.
pub const INFERS_STACKING: [&str; 1] = ["adder"];

/// The categories whose stacking the simulation sets when it finds copies
/// that work, leaving what their builders gave otherwise: a register,
/// whose copies may stack in ways the search does not try, as a 1-bit
/// cell's do into wider registers.
pub const FINDS_STACKING: [&str; 1] = ["register"];

/// Every test, in the order a guess tries them.
const TESTS: [Test; 10] = [
    Test::Adder,
    Test::Incrementer,
    Test::Decrementer,
    Test::IncDec,
    Test::Multiplier,
    Test::BinaryToBcd,
    Test::BcdToBinary,
    Test::Alu,
    Test::Register,
    Test::AbsoluteValue,
];

impl Test {
    pub fn of(category: &str, width: Option<i64>) -> Option<Self> {
        if !width.is_some_and(|w| (1..=MAX_WIDTH).contains(&w)) {
            return None;
        }
        TESTS.into_iter().find(|test| test.name() == category)
    }

    /// The name stored with its result, which is its category's.
    pub fn name(self) -> &'static str {
        match self {
            Test::Adder => "adder",
            Test::Incrementer => "incrementer",
            Test::Decrementer => "decrementer",
            Test::IncDec => "inc_dec",
            Test::Multiplier => "multiplier",
            Test::BinaryToBcd => "binary_to_bcd",
            Test::BcdToBinary => "bcd_to_binary",
            Test::Alu => "alu",
            Test::Register => "register",
            Test::AbsoluteValue => "absolute_value",
        }
    }

    /// Runs the test's cases on `blocks` and checks each result.
    pub fn run(self, blocks: &Blocks) -> Report {
        let result = match self {
            Test::Adder => check_adder(blocks),
            Test::Incrementer | Test::Decrementer | Test::IncDec => {
                check_incrementer(blocks, false, Some(self.name()))
            }
            Test::Multiplier => check_multiplier(blocks),
            Test::BinaryToBcd => check_binary_to_bcd(blocks, false),
            Test::BcdToBinary => check_bcd_to_binary(blocks, false),
            Test::Alu => alu::check(blocks),
            Test::Register => register::check(blocks),
            Test::AbsoluteValue => check_absolute_value(blocks, false),
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

    /// Whether copies of the build in `data` stack into a wider one of its
    /// kind: for an adder, which this is only tried on, see `stacking`.
    pub fn stacking(self, data: &[u8]) -> Option<Stacking> {
        match self {
            Test::Adder => stacking::check(data),
            Test::Register => register::stacking(data),
            Test::Incrementer
            | Test::Decrementer
            | Test::IncDec
            | Test::Multiplier
            | Test::BinaryToBcd
            | Test::BcdToBinary
            | Test::Alu
            | Test::AbsoluteValue => None,
        }
    }

    /// Measures when `blocks`' outputs settle, on every core, stopping
    /// early once `cancel` is set. Meant for a build whose cases all came
    /// out right.
    pub fn time(self, blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
        match self {
            Test::Alu => return alu::time(blocks, cancel),
            Test::Register => return register::time(blocks, cancel),
            _ => {}
        }
        let mut interface = interface::read(blocks);
        // A BCD input takes only its valid values, as its cases do.
        let mut values = None;
        let inputs = match self {
            Test::Adder => adder_bench(&mut interface)?.inputs,
            Test::Incrementer | Test::Decrementer | Test::IncDec => {
                incrementer_bench(&interface)?.inputs
            }
            Test::Multiplier => multiplier_bench(&interface)?.inputs,
            Test::BinaryToBcd => converter_bench(&interface, "")?.inputs,
            Test::AbsoluteValue => {
                one_input_bench(&interface, "", "absolute value")?.inputs
            }
            Test::Alu | Test::Register => unreachable!("timed above"),
            Test::BcdToBinary => {
                let bench = converter_bench(&interface, "")?;
                let width = bench.widths(&interface)[0];
                let result = bench.output_widths(&interface).iter().sum();
                let (valid, every) = bcd_values(width, result, false);
                values = Some((vec![valid], !every));
                bench.inputs
            }
        };
        timing::measure(blocks, &interface, &inputs, &[], values, cancel)
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

// After `broken!`, which it uses.
mod alu;
pub use alu::op as alu_op;
mod register;

/// What a test found.
#[derive(Serialize, Deserialize, Debug, Clone)]
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
#[derive(Serialize, Deserialize, Debug, Clone)]
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
    /// Whether, not trying every combination, each hex digit took its
    /// every case rather than each byte, as for a hex adder.
    #[serde(default)]
    pub by_digit: bool,
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
    /// Whether copies of an adder that gets them all right stack into a
    /// wider one; `None` for any other build, or one too wide to try.
    #[serde(default)]
    pub stacking: Option<Stacking>,
}

/// A port a test sets or reads, by the build's name for it.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Signal {
    pub name: String,
    pub bits: usize,
}

/// One case a build got wrong: each input's value, and each output's
/// expected and actual value, in the order of `Run`'s.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Wrong {
    /// The operation it was on, for a build of several, such as an ALU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
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
/// as A, B, and carry in, with how many pair the test patterns. Each piece
/// of `piece` bits, a byte or a hex digit, takes every value of A and B,
/// with a carry into it and without: the bits below are all set in A, and
/// carry in, or B's lowest bit when there is no carry in line, carries
/// through them. Then every pairing of `patterns`, with carry in each way.
fn adder_cases(width: usize, carry_in: bool, piece: usize) -> (Vec<Case>, u64) {
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |cases: &mut Vec<_>, case: (u64, u64, bool)| {
        let case = [case.0, case.1, u64::from(case.2)];
        if seen.insert(case) {
            cases.push(case);
        }
    };
    for low in (0..width).step_by(piece) {
        let bits = (width - low).min(piece);
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
    /// Inputs set once and held through every case, by their place in the
    /// interface, such as the control lines choosing an ALU's operation.
    held: Vec<(usize, u64)>,
    /// The bits of the outputs read together that the cases decide; the
    /// rest, such as a status line an operation leaves meaningless, are
    /// not checked.
    care: u64,
}

/// Runs an adder through its inputs and checks every sum: two operand buses
/// and an optional `carry in`, into one output bus and an optional `carry
/// out` above it. When every sum is wrong, it tries again with carry in
/// flipped, since a lever saved the wrong way round does that.
pub fn check_adder(blocks: &Blocks) -> Result<Run> {
    check_adder_on(blocks, false)
}

/// `check_adder`, or with `quick` only the pairings of the test patterns.
fn check_adder_on(blocks: &Blocks, quick: bool) -> Result<Run> {
    let mut interface = interface::read(blocks);
    let bench = adder_bench(&mut interface)?;
    let widths = bench.widths(&interface);
    let carry_in = bench.inputs.get(2).copied();
    // A hex adder, slower to run, takes each digit's every case, and a
    // binary one each byte's.
    let hex = interface.inputs.iter().any(|port| port.signal_strength);
    let (cases, exhaustive, patterns) = if quick {
        let (cases, _) = paired(&widths);
        let count = cases.len() as u64;
        (cases, false, count)
    } else {
        let piece = if hex { 4 } else { 8 };
        cases(&widths, || {
            adder_cases(widths[0], carry_in.is_some(), piece)
        })
    };
    let sum = |case: &Case| case[0] + case[1] + case[2];
    let sweep = |flip| {
        sweep(blocks, &interface, &bench, &cases, flip, sum)
            .map(|run| run.covering(exhaustive, patterns))
    };
    let mut run = sweep(None)?;
    if run.wrong == run.cases
        && let Some(port) = carry_in
        && sweep(Some((2, 1)))?.wrong == 0
    {
        let name = &interface.inputs[port].name;
        run.hint = Some(format!(
            "Every sum is right with {name} flipped, so its lever is saved \
             the wrong way round."
        ));
    }
    run.by_digit = hex && !run.exhaustive;
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
        held: Vec::new(),
        care: u64::MAX,
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

/// Runs a stateless multiplier through its inputs and checks every
/// product: A times B, plus C for a fused multiply-add, against as many low
/// bits as its outputs hold together, the first holding the lowest.
pub fn check_multiplier(blocks: &Blocks) -> Result<Run> {
    check_multiplier_on(blocks, false)
}

/// `check_multiplier`, or with `quick` only the pairings of the test
/// patterns.
fn check_multiplier_on(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = interface::read(blocks);
    let bench = multiplier_bench(&interface)?;
    let widths = bench.widths(&interface);
    let (cases, exhaustive, patterns) = if quick {
        let (cases, _) = paired(&widths);
        let count = cases.len() as u64;
        (cases, false, count)
    } else {
        cases(&widths, || multiplier_cases(&widths))
    };
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
            "It has a control line, {}, so it is stateful; only a stateless \
             multiplier is tested.",
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
        held: Vec::new(),
        care: u64::MAX,
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

/// `value` in binary-coded decimal: each decimal digit in four bits, the
/// ones lowest.
fn to_bcd(mut value: u64) -> u64 {
    let mut bcd = 0;
    let mut shift = 0;
    while value > 0 && shift < 64 {
        bcd |= (value % 10) << shift;
        value /= 10;
        shift += 4;
    }
    bcd
}

/// The number binary-coded decimal `bcd` holds, its digits being decimal.
fn from_bcd(bcd: u64) -> u64 {
    (0..16)
        .rev()
        .fold(0, |value, digit| value * 10 + (bcd >> (4 * digit) & 0xf))
}

/// Whether `value` fits in `bits` bits.
fn fits(value: u64, bits: usize) -> bool {
    bits >= 64 || value >> bits == 0
}

/// The binary-coded decimal values of `width` bits whose numbers fit in
/// `result` bits, in order, with whether they are every one: every one
/// when there are few enough and not `quick`, else every number below
/// 1,000, each digit's every value alone, each run of nines, and the
/// largest.
fn bcd_values(width: usize, result: usize, quick: bool) -> (Vec<u64>, bool) {
    let valid = |n: u64| fits(to_bcd(n), width) && fits(n, result);
    if !quick {
        // Both tests rise with the number, so the valid ones run from 0.
        let every: Vec<u64> = (0..=1u64 << EXHAUSTIVE_BITS)
            .take_while(|&n| valid(n))
            .map(to_bcd)
            .collect();
        if every.len() <= 1 << EXHAUSTIVE_BITS {
            return (every, true);
        }
    }
    let mut numbers: Vec<u64> = (0..1000).collect();
    let mut power = 1u64;
    while let Some(next) = power.checked_mul(10) {
        numbers.extend((1..10).map(|d| d * power));
        numbers.push(next - 1);
        power = next;
    }
    // The largest valid number, found digit by digit from the top.
    let mut largest = 0u64;
    for digit in (0..16).rev() {
        for d in (0..10).rev() {
            let n = largest + d * 10u64.pow(digit);
            if valid(n) {
                largest = n;
                break;
            }
        }
    }
    numbers.push(largest);
    numbers.retain(|&n| valid(n));
    numbers.sort_unstable();
    numbers.dedup();
    (numbers.into_iter().map(to_bcd).collect(), false)
}

/// What a converter's test drives and reads: its one input, A, and its
/// output buses, status lines left out, the first holding the lowest
/// digits or bits. One with a control line, such as a clock, is
/// sequential, which this test does not cover.
fn converter_bench(interface: &Interface, pattern: &str) -> Result<Bench> {
    one_input_bench(interface, pattern, "converter")
}

/// What a stateless build of one input, A, is tested on, such as a
/// converter, named `what` in what is wrong with it: A, and its outputs
/// read together, the lowest first.
fn one_input_bench(
    interface: &Interface,
    pattern: &str,
    what: &str,
) -> Result<Bench> {
    let control = |port: &&Port| port.block.starts_with(lint::CONTROL);
    if let Some(line) = interface.inputs.iter().find(control) {
        bail!(
            "It has a control line, {}, so it is stateful; only a stateless \
             {what} is tested.",
            line.name
        );
    }
    if interface.inputs.len() != 1 {
        broken!(
            "A {what} takes one input, A, but this has {}.",
            interface.inputs.len()
        );
    }
    let outputs: Vec<usize> = (0..interface.outputs.len())
        .filter(|&i| !interface.outputs[i].block.starts_with(lint::STATUS))
        .collect();
    if outputs.is_empty() {
        broken!("A {what} has an output, but this has none.");
    }
    Ok(Bench {
        inputs: vec![0],
        outputs,
        pattern: pattern.to_owned(),
        held: Vec::new(),
        care: u64::MAX,
    })
}

/// Runs an absolute-value build through its inputs and checks each result:
/// A taken as a signed number, two's complement, and its magnitude,
/// unsigned in as many bits as A, so the most negative A, which has no
/// positive, gives itself, as negating it does; checked on as many low bits
/// as the outputs hold. Every value of A when there are few enough, else
/// every value of each byte and the test patterns; with `quick`, only the
/// test patterns.
fn check_absolute_value(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = interface::read(blocks);
    let bench = one_input_bench(&interface, "|A|", "absolute value")?;
    let width = bench.widths(&interface)[0];
    let (cases, exhaustive, patterns) = if quick {
        let cases: Vec<Case> =
            patterns(width).into_iter().map(|a| [a, 0, 0]).collect();
        let count = cases.len() as u64;
        (cases, false, count)
    } else {
        cases(&[width], || byte_cases(width))
    };
    let magnitude = |case: &Case| absolute(case[0], width);
    let run = sweep(blocks, &interface, &bench, &cases, None, magnitude)?;
    let run = run.covering(exhaustive, patterns);
    with_saved_on_hint(run, blocks, &interface, &bench, &cases, magnitude)
}

/// The magnitude of `a`, a signed number of `width` bits, unsigned in as
/// many bits: `a` when its top bit is clear, else its negative, which for
/// the most negative is itself.
fn absolute(a: u64, width: usize) -> u64 {
    if width > 0 && a >> (width - 1) & 1 == 1 {
        a.wrapping_neg() & mask(width)
    } else {
        a
    }
}

/// The values of A a one-input build too wide to try every one of is
/// checked on: every value of each byte, the other bits clear, and the test
/// patterns, with how many are patterns.
fn byte_cases(width: usize) -> (Vec<Case>, u64) {
    let mut values: Vec<u64> = (0..width)
        .step_by(8)
        .flat_map(|low| {
            (0..1u64 << (width - low).min(8)).map(move |v| v << low)
        })
        .collect();
    let bytes = values.len();
    values.extend(patterns(width));
    values.sort_unstable();
    values.dedup();
    let paired = values.len().saturating_sub(bytes) as u64;
    (values.into_iter().map(|a| [a, 0, 0]).collect(), paired)
}

/// Runs a binary-to-BCD converter through its inputs and checks each
/// result, A in BCD, the ones digit lowest: every value of A when there
/// are few enough, else every value of each byte, the test patterns, and
/// each power of ten and the number before it. With `quick`, only the
/// test patterns are tried. Outputs too few for A's largest value in BCD
/// make it broken.
fn check_binary_to_bcd(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = interface::read(blocks);
    let bench = converter_bench(&interface, "A in BCD")?;
    let width = bench.widths(&interface)[0];
    let result: usize = bench.output_widths(&interface).iter().sum();
    let largest = mask(width);
    let needed = 64 - to_bcd(largest).leading_zeros() as usize;
    if result < needed {
        broken!(
            "A has {width} bits, so up to {largest}, which takes {needed} \
             bits in BCD, but the outputs have {result}."
        );
    }
    let (cases, exhaustive, patterns) = if quick {
        let cases: Vec<Case> =
            patterns(width).into_iter().map(|a| [a, 0, 0]).collect();
        let count = cases.len() as u64;
        (cases, false, count)
    } else {
        cases(&[width], || binary_cases(width))
    };
    let to = |case: &Case| to_bcd(case[0]);
    let run = sweep(blocks, &interface, &bench, &cases, None, to)?;
    let run = run.covering(exhaustive, patterns);
    with_saved_on_hint(run, blocks, &interface, &bench, &cases, to)
}

/// The values a binary-to-BCD converter too wide to try every one of is
/// checked on: every value of each byte, the other bits clear; the test
/// patterns; and each power of ten and the number before it, as far as A
/// holds them.
fn binary_cases(width: usize) -> (Vec<Case>, u64) {
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |cases: &mut Vec<Case>, a: u64| {
        if seen.insert(a) {
            cases.push([a, 0, 0]);
        }
    };
    for low in (0..width).step_by(8) {
        for v in 0..1u64 << (width - low).min(8) {
            add(&mut cases, v << low);
        }
    }
    let bytes = cases.len();
    for a in patterns(width) {
        add(&mut cases, a);
    }
    let mut power = 1u64;
    while fits(power, width) {
        add(&mut cases, power);
        add(&mut cases, power - 1);
        match power.checked_mul(10) {
            Some(next) => power = next,
            None => break,
        }
    }
    let rest = (cases.len() - bytes) as u64;
    (cases, rest)
}

/// Runs a BCD-to-binary converter through every value of A whose digits
/// are all decimal and whose number the outputs can hold, or with too many
/// of them, or `quick`, a selection, and checks each result, A's number
/// in binary.
fn check_bcd_to_binary(blocks: &Blocks, quick: bool) -> Result<Run> {
    let interface = interface::read(blocks);
    let bench = converter_bench(&interface, "A from BCD")?;
    let width = bench.widths(&interface)[0];
    let result: usize = bench.output_widths(&interface).iter().sum();
    let (values, every) = bcd_values(width, result, quick);
    let cases: Vec<Case> = values.into_iter().map(|a| [a, 0, 0]).collect();
    let selected = if every { 0 } else { cases.len() as u64 };
    let from = |case: &Case| from_bcd(case[0]);
    let run = sweep(blocks, &interface, &bench, &cases, None, from)?;
    let run = run.covering(every, selected);
    with_saved_on_hint(run, blocks, &interface, &bench, &cases, from)
}

/// `run`, saying which lever is saved the wrong way round when some cases
/// are wrong but every one is right with one of A's bits whose lever is
/// saved on driven as a plain input: a lever saved on reads as an inverted
/// input, so one left on by mistake gets most cases wrong.
fn with_saved_on_hint(
    mut run: Run,
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    expected: impl Fn(&Case) -> u64,
) -> Result<Run> {
    if run.wrong == 0 {
        return Ok(run);
    }
    let port = &interface.inputs[bench.inputs[0]];
    let saved_on = port.bits.iter().enumerate().filter(|(_, b)| b.inverted);
    for (bit, _) in saved_on {
        let flip = Some((0, 1 << bit));
        if sweep(blocks, interface, bench, cases, flip, &expected)?.wrong == 0 {
            run.hint = Some(format!(
                "Every case is right with bit {bit} of {} as a plain input, \
                 so its lever is saved on by mistake: a lever saved on marks \
                 an inverted input.",
                port.name
            ));
            break;
        }
    }
    Ok(run)
}

/// Runs an incrementer, a decrementer, or an inc/dec through its inputs,
/// every value of A with its control line each way, and checks the result
/// against what each kind of `category` gives: an incrementer's A plus 1
/// or plus its control line, a decrementer's A minus either, and an
/// inc/dec's plus 1 one way and minus 1 the other. With no category, every
/// kind is tried, as for a guess. The kind it gets most right names its
/// cases. With `quick`, only the test patterns are tried.
fn check_incrementer(
    blocks: &Blocks,
    quick: bool,
    category: Option<&str>,
) -> Result<Run> {
    let interface = interface::read(blocks);
    let bench = incrementer_bench(&interface)?;
    let widths = bench.widths(&interface);
    let (cases, exhaustive, patterns) = if quick {
        let (cases, _) = paired(&widths);
        let count = cases.len() as u64;
        (cases, false, count)
    } else {
        cases(&widths, || incrementer_cases(&widths))
    };
    let got = observe(blocks, &interface, &bench, &cases, None)?;
    let line = bench
        .inputs
        .get(1)
        .map(|&i| interface.inputs[i].name.clone());
    let kinds: Vec<Change> = changes(line.as_deref())
        .into_iter()
        .filter(|change| category.is_none_or(|c| change.category == c))
        .collect();
    if kinds.is_empty() {
        broken!(
            "An inc/dec has a control line to choose between adding and \
             subtracting 1, but this has none."
        );
    }
    let best = kinds
        .into_iter()
        .map(|change| {
            let mut run = judge(&interface, &bench, &cases, &got, |case| {
                (change.apply)(case[0], case[1])
            });
            run.pattern = change.name;
            run
        })
        .min_by_key(|run| run.wrong)
        .context("an incrementer has a kind")?;
    Ok(best.covering(exhaustive, patterns))
}

/// One kind of incrementer: its cases' name, such as `A + increment`, the
/// category it is, and what it gives for A and its control line.
struct Change {
    name: String,
    category: &'static str,
    apply: fn(u64, u64) -> u64,
}

/// The kinds of incrementer with control line `line`, if any.
fn changes(line: Option<&str>) -> Vec<Change> {
    let change = |name: String, category, apply| Change {
        name,
        category,
        apply,
    };
    match line {
        None => vec![
            change("A + 1".to_owned(), "incrementer", |a, _| a.wrapping_add(1)),
            change("A − 1".to_owned(), "decrementer", |a, _| {
                a.wrapping_sub(1)
            }),
        ],
        Some(line) => vec![
            change(format!("A + {line}"), "incrementer", u64::wrapping_add),
            change(format!("A − {line}"), "decrementer", u64::wrapping_sub),
            change(
                format!("A + 1, or − 1 with {line} on"),
                "inc_dec",
                |a, c| {
                    if c == 1 {
                        a.wrapping_sub(1)
                    } else {
                        a.wrapping_add(1)
                    }
                },
            ),
            change(
                format!("A − 1, or + 1 with {line} on"),
                "inc_dec",
                |a, c| {
                    if c == 1 {
                        a.wrapping_add(1)
                    } else {
                        a.wrapping_sub(1)
                    }
                },
            ),
        ],
    }
}

/// What an incrementer's test drives and reads: A and its control line, if
/// any; its output buses, then any status line, such as a carry out.
fn incrementer_bench(interface: &Interface) -> Result<Bench> {
    let control =
        |i: &usize| interface.inputs[*i].block.starts_with(lint::CONTROL);
    let all: Vec<usize> = (0..interface.inputs.len()).collect();
    let (lines, operands): (Vec<usize>, Vec<usize>) =
        all.into_iter().partition(control);
    let &[a] = operands.as_slice() else {
        broken!(
            "An incrementer takes one operand, but this has {}.",
            operands.len()
        );
    };
    if lines.len() > 1 {
        broken!(
            "An incrementer has at most one control line, such as \
             increment, but this has {}.",
            lines.len()
        );
    }
    let status =
        |i: &usize| interface.outputs[*i].block.starts_with(lint::STATUS);
    let (mut status_lines, mut outputs): (Vec<usize>, Vec<usize>) =
        (0..interface.outputs.len()).partition(status);
    outputs.append(&mut status_lines);
    let bits = interface.inputs[a].bits.len();
    let result: usize = outputs
        .iter()
        .map(|&i| interface.outputs[i].bits.len())
        .sum();
    if !(bits..=bits + 1).contains(&result) {
        broken!(
            "A has {bits} bits, so the result has {bits}, or one more for a \
             carry, but the outputs have {result}."
        );
    }
    let mut inputs = vec![a];
    inputs.extend(lines);
    Ok(Bench {
        inputs,
        outputs,
        pattern: "A".to_owned(),
        held: Vec::new(),
        care: u64::MAX,
    })
}

/// The values an incrementer too wide to try every one of is checked on:
/// every value of each byte of A, the bits below it all clear and all set,
/// so a carry or borrow runs into it, and the test patterns; each with its
/// control line, if any, each way.
fn incrementer_cases(widths: &[usize]) -> (Vec<Case>, u64) {
    let lines: &[u64] = if widths.len() > 1 { &[0, 1] } else { &[0] };
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |cases: &mut Vec<Case>, case: Case| {
        if seen.insert(case) {
            cases.push(case);
        }
    };
    for low in (0..widths[0]).step_by(8) {
        let bits = (widths[0] - low).min(8);
        for below in [0, mask(low)] {
            for a in 0..1u64 << bits {
                for &line in lines {
                    add(&mut cases, [below | a << low, line, 0]);
                }
            }
        }
    }
    let before = cases.len();
    for a in patterns(widths[0]) {
        for &line in lines {
            add(&mut cases, [a, line, 0]);
        }
    }
    let paired = (cases.len() - before) as u64;
    (cases, paired)
}

/// Every pairing of a few telling values of each input's width, a 1-bit one
/// taking 0 and 1, with how many there are: a quick look, enough to tell
/// an adder from a multiplier, even for a build slow to run.
fn paired(widths: &[usize]) -> (Vec<Case>, u64) {
    let choices: Vec<Vec<u64>> = widths
        .iter()
        .map(|&width| {
            if width == 1 {
                vec![0, 1]
            } else {
                telling(width)
            }
        })
        .collect();
    let mut cases = vec![[0; 3]];
    for (i, values) in choices.iter().enumerate() {
        cases = cases
            .iter()
            .flat_map(|case| {
                values.iter().map(move |&value| {
                    let mut case = *case;
                    case[i] = value;
                    case
                })
            })
            .collect();
    }
    let count = cases.len() as u64;
    (cases, count)
}

/// About a dozen values of `width` bits that tell functions apart: none,
/// one, and all set; alternate bits and pairs of bits, each way; the top
/// bit alone; the low half set; and two with no pattern.
fn telling(width: usize) -> Vec<u64> {
    let all = mask(width);
    let mut values: Vec<u64> = [
        0,
        1,
        u64::MAX,
        0x5555_5555_5555_5555,
        0xAAAA_AAAA_AAAA_AAAA,
        0x3333_3333_3333_3333,
        0xCCCC_CCCC_CCCC_CCCC,
        1 << (width - 1),
        mask(width / 2),
        0x9E37_79B9_7F4A_7C15,
        0x6A09_E667_F3BC_C908,
    ]
    .iter()
    .map(|value| value & all)
    .collect();
    values.sort_unstable();
    values.dedup();
    values
}

/// What a build seems to be, from a few telling values run through each
/// test its inputs and outputs fit.
#[derive(Debug, Clone)]
pub struct Guess {
    /// Its category, such as `adder`.
    pub category: &'static str,
    /// What its cases are, such as `A + B + carry in`.
    pub pattern: String,
    pub cases: u64,
    pub wrong: u64,
}

/// What the build in `blocks` seems to be: of the tests its inputs and
/// outputs fit, the one whose telling values come out right most often, if
/// three in four do. Quick enough to run before anyone says what it is,
/// even for a hex build, which runs slower.
pub fn guess(blocks: &Blocks) -> Option<Guess> {
    // One run tries every kind of incrementer, and the kind it gets most
    // right names its category.
    let tried = [
        Test::Adder,
        Test::Incrementer,
        Test::Multiplier,
        Test::BinaryToBcd,
        Test::BcdToBinary,
        Test::Register,
        Test::AbsoluteValue,
    ];
    tried
        .into_iter()
        .filter_map(|test| {
            let run = match test {
                Test::Adder => check_adder_on(blocks, true),
                Test::Multiplier => check_multiplier_on(blocks, true),
                Test::BinaryToBcd => check_binary_to_bcd(blocks, true),
                Test::BcdToBinary => check_bcd_to_binary(blocks, true),
                Test::Register => register::check_quick(blocks),
                Test::AbsoluteValue => check_absolute_value(blocks, true),
                _ => check_incrementer(blocks, true, None),
            }
            .ok()?;
            let category = if test == Test::Incrementer {
                let line = run.inputs.get(1).map(|signal| signal.name.as_str());
                changes(line)
                    .into_iter()
                    .find(|change| change.name == run.pattern)
                    .map_or("incrementer", |change| change.category)
            } else {
                test.name()
            };
            Some(Guess {
                category,
                pattern: run.pattern,
                cases: run.cases,
                wrong: run.wrong,
            })
        })
        // Three in four right, so a build with a few wrong bits is still
        // known for what it is.
        .filter(|guess| (guess.cases - guess.wrong) * 4 >= guess.cases * 3)
        .min_by_key(|guess| guess.wrong * 1000 / guess.cases.max(1))
}

impl Bench {
    /// Each input's width in bits, in the order of `inputs`.
    fn widths(&self, interface: &Interface) -> Vec<usize> {
        self.inputs
            .iter()
            .map(|&i| input_bits(&interface.inputs[i]))
            .collect()
    }

    /// Each output's width in bits, in the order of `outputs`.
    fn output_widths(&self, interface: &Interface) -> Vec<usize> {
        self.outputs
            .iter()
            .map(|&i| output_bits(interface, &interface.outputs[i]))
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
    /// Takes in `other`'s cases, as another operation of the same build,
    /// keeping the first five wrong cases of all.
    fn absorb(&mut self, other: Run) {
        self.cases += other.cases;
        self.wrong += other.wrong;
        self.patterns += other.patterns;
        self.exhaustive &= other.exhaustive;
        for (label, count) in other.wrong_bits {
            match self.wrong_bits.iter_mut().find(|(l, _)| *l == label) {
                Some(bit) => bit.1 += count,
                None => self.wrong_bits.push((label, count)),
            }
        }
        let room = 5usize.saturating_sub(self.examples.len());
        self.examples.extend(other.examples.into_iter().take(room));
    }

    fn covering(mut self, exhaustive: bool, patterns: u64) -> Self {
        self.exhaustive = exhaustive;
        self.patterns = patterns;
        self
    }
}

/// The bits of one of a bench's inputs, by its place in each case, to
/// drive the other way round, as a lever saved the wrong way does.
type Flip = Option<(usize, u64)>;

/// Runs `cases` through the bench's inputs, driving the bits `flip` gives,
/// if any, the other way round, and checks each result against `expected`,
/// cut to as many bits as the outputs hold.
fn sweep(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    flip: Flip,
    expected: impl Fn(&Case) -> u64,
) -> Result<Run> {
    let got = observe(blocks, interface, bench, cases, flip)?;
    Ok(judge(interface, bench, cases, &got, expected))
}

/// What the bench's outputs show, read together as one number, for each
/// of `cases` run through its inputs, the bits `flip` gives, if any, driven
/// the other way round.
fn observe(
    blocks: &Blocks,
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    flip: Flip,
) -> Result<Vec<u64>> {
    let mut circuit = Circuit::new(blocks, interface)?;
    for &(port, value) in &bench.held {
        circuit.set(port, value);
    }
    let widths = bench.output_widths(interface);
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        for (i, &port) in bench.inputs.iter().enumerate() {
            let flipped = flip.filter(|&(input, _)| input == i);
            circuit.set(port, case[i] ^ flipped.map_or(0, |(_, bits)| bits));
        }
        circuit.settle()?;
        let mut got = 0;
        let mut shift = 0;
        for (&port, &width) in bench.outputs.iter().zip(&widths) {
            got |= circuit.get(port) << shift;
            shift += width;
        }
        results.push(got);
    }
    Ok(results)
}

/// How `got`, what the outputs showed for each of `cases`, compares with
/// `expected`, cut to as many bits as the outputs hold.
fn judge(
    interface: &Interface,
    bench: &Bench,
    cases: &[Case],
    got: &[u64],
    expected: impl Fn(&Case) -> u64,
) -> Run {
    let widths = bench.output_widths(interface);
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
    for (case, &got) in cases.iter().zip(got) {
        // The bits the cases leave open are taken as they came out.
        let want = (expected(case) & bench.care | got & !bench.care)
            & mask(result_bits);
        if want != got {
            wrong += 1;
            for (bit, count) in wrong_by_bit.iter_mut().enumerate() {
                *count += (want ^ got) >> bit & 1;
            }
            if examples.len() < 5 {
                examples.push(Wrong {
                    op: None,
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
            let bits = output_bits(interface, port);
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
    Run {
        pattern: bench.pattern.clone(),
        cases: cases.len() as u64,
        exhaustive: true,
        patterns: 0,
        wrong,
        inputs: bench
            .inputs
            .iter()
            .map(|&i| Signal {
                name: interface.inputs[i].name.clone(),
                bits: input_bits(&interface.inputs[i]),
            })
            .collect(),
        outputs: bench
            .outputs
            .iter()
            .map(|&i| Signal {
                name: interface.outputs[i].name.clone(),
                bits: output_bits(interface, &interface.outputs[i]),
            })
            .collect(),
        wrong_bits,
        examples,
        hint: None,
        timing: None,
        stacking: None,
        by_digit: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_negative_input_is_its_own_absolute_value() {
        assert_eq!(absolute(5, 8), 5);
        assert_eq!(absolute(0xff, 8), 1);
        assert_eq!(absolute(0x81, 8), 0x7f);
        assert_eq!(absolute(0x80, 8), 0x80);
    }

    #[test]
    fn adders_up_to_32_bits_are_tested() {
        assert_eq!(Test::of("adder", Some(16)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(32)), Some(Test::Adder));
        assert_eq!(Test::of("adder", Some(33)), None);
        assert_eq!(Test::of("adder", None), None);
        assert_eq!(Test::of("register", Some(8)), Some(Test::Register));
        assert_eq!(Test::of("ram", Some(8)), None);
    }

    #[test]
    fn an_incrementer_names_what_it_does_by_its_control_line() {
        let kinds: Vec<(String, &str)> = changes(Some("increment"))
            .into_iter()
            .map(|change| (change.name, change.category))
            .collect();
        assert_eq!(kinds[0], ("A + increment".to_owned(), "incrementer"));
        assert_eq!(kinds[1].1, "decrementer");
        // Plus 1 with the line off, minus 1 with it on.
        let inc_dec = &changes(Some("decrement"))[2];
        assert_eq!(((inc_dec.apply)(5, 0), (inc_dec.apply)(5, 1)), (6, 4));
        assert_eq!(Test::of("incrementer", Some(8)), Some(Test::Incrementer));
        assert_eq!(Test::of("inc_dec", Some(8)), Some(Test::IncDec));
    }

    #[test]
    fn bcd_holds_each_decimal_digit_in_four_bits() {
        assert_eq!(to_bcd(0), 0);
        assert_eq!(to_bcd(255), 0x255);
        assert_eq!(from_bcd(0x255), 255);
        assert_eq!(from_bcd(to_bcd(4_294_967_295)), 4_294_967_295);
    }

    #[test]
    fn a_bcd_input_takes_only_decimal_digits_that_its_outputs_hold() {
        // Ten bits of BCD hold up to 399, but eight output bits only 255.
        let (values, every) = bcd_values(10, 8, false);
        assert!(every);
        assert_eq!(values.len(), 256);
        assert_eq!((values[0], values[255]), (0, 0x255));
        assert!(values.contains(&0x199) && !values.contains(&0x1a0));
        // Too many to try every one: a selection, the largest of the eight
        // digits among them.
        let (values, every) = bcd_values(32, 27, false);
        assert!(!every);
        assert_eq!(values.last(), Some(&to_bcd(99_999_999)));
        assert!(values.iter().all(|&v| from_bcd(v) < 1 << 27));
    }

    #[test]
    fn converters_between_binary_and_bcd_are_tested() {
        assert_eq!(Test::of("binary_to_bcd", Some(8)), Some(Test::BinaryToBcd));
        assert_eq!(
            Test::of("bcd_to_binary", Some(10)),
            Some(Test::BcdToBinary)
        );
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
        let (cases, paired) = adder_cases(16, true, 8);
        // Each byte with and without a carry, less two the high byte
        // repeats: A's low byte all set, with or without carry in.
        assert_eq!(cases.len() as u64 - paired, 4 * (1 << 16) - 2);
        // A carry from carry in through the low byte into the high one.
        assert!(cases.contains(&[0x00FF, 0x0F00, 1]));
        // The longest carry: all ones plus one.
        assert!(cases.contains(&[0xFFFF, 0x0001, 0]));
        // Without a carry in line, B's lowest bit makes the carry.
        let (cases, _) = adder_cases(16, false, 8);
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
            words: Vec::new(),
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
