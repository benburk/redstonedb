//! A build set up in a world for the simulator: its inputs driven by
//! levers, buttons, or barrels, and its outputs read from their blocks.

use std::{collections::HashSet, panic::AssertUnwindSafe, sync::Arc};

use anyhow::{Context, Result, bail};
use mchprs_blocks::{
    BlockFace, BlockPos,
    block_entities::{BlockEntity, ContainerType},
    blocks::Block,
};
use mchprs_redpiler::{Compiler, CompilerOptions};
use mchprs_world::{World, testing::TestWorld};

use crate::{
    interface::{Interface, Port},
    lint,
    schem::{self, Blocks, Direction, Pos},
};

/// The most ticks a build may take to settle after its inputs change.
pub(super) const MAX_TICKS: u32 = 1000;

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
                        // Glass is never powered, so a glass bit is read
                        // only by what stands on it.
                        let glass =
                            schem::id(blocks.get(bit.pos)).ends_with("_glass");
                        let above =
                            world.get_block(at(bit.pos).offset(BlockFace::Top));
                        let read = matches!(
                            above,
                            Block::RedstoneWire(_)
                                | Block::Repeater(_)
                                | Block::Comparator(_)
                        );
                        if glass && !read {
                            bail!(
                                "Output {} has a glass block with no dust, \
                                 repeater, or comparator on it, which is how \
                                 a glass output is read.",
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
        // So `pressed` sees it down before the next tick.
        self.show();
    }

    /// Whether the button on bit `bit` of input `port` is still down, as of
    /// the last `step` or `settle`; false for a lever.
    pub fn pressed(&self, port: usize, bit: usize) -> bool {
        let Drive::Levers(levers) = &self.inputs[port] else {
            return false;
        };
        let Some(button) = levers.get(bit).filter(|lever| lever.button) else {
            return false;
        };
        matches!(
            self.world.get_block(button.pos),
            Block::StoneButton { powered: true, .. }
        )
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
pub(super) fn input_bits(port: &Port) -> usize {
    port.bits.len() * if port.signal_strength { 4 } else { 1 }
}

/// How many bits output `port` of a build with `interface` carries: four
/// for each block when the build takes a signal strength and the port
/// shows digits, else one for each block.
pub(super) fn output_bits(interface: &Interface, port: &Port) -> usize {
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
            let dust = schem::step(bit.pos, Direction::Up);
            let mut sides: Vec<Direction> = bit.entry.into_iter().collect();
            sides.extend(
                Direction::SIDES.into_iter().filter(|&s| Some(s) != bit.entry),
            );
            let side = sides
                .into_iter()
                .find(|&side| {
                    let comparator = schem::step(dust, side);
                    let barrel = schem::step(comparator, side);
                    let across = if side.north_south() {
                        [Direction::East, Direction::West]
                    } else {
                        [Direction::North, Direction::South]
                    };
                    [comparator, barrel].iter().all(|&pos| {
                        air(world, pos) && !taken.contains(&pos)
                    }) && across.into_iter().all(|way| {
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
            let pos = bit.entry.map(|side| schem::step(bit.pos, side));
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
    let name = schem::short(state);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
