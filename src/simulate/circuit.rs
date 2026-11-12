//! A build set up in a world for the simulator: its inputs driven by
//! levers, buttons, or barrels, and its outputs read from their blocks.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    panic::AssertUnwindSafe,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use mchprs_blocks::{
    BlockFace, BlockPos,
    block_entities::{BlockEntity, ContainerType},
    blocks::Block,
};
use mchprs_redpiler::{
    Compiler, CompilerInput, CompilerOptions, TaskMonitor,
    compile_graph::{CompileGraph, CompileNode, LinkType, NodeType},
    passes::{PassRegistry, build_pass_pipeline},
};
use mchprs_world::{World, testing::TestWorld};

use super::{Broken, Value};
use crate::{
    interface::{Interface, Port, repeater_outputs},
    schem::{self, Blocks, Direction, Pos},
};

/// The most ticks a build may take to settle after its inputs change.
pub(super) const MAX_TICKS: u32 = 1000;

/// A build set up for simulation. Each input is driven by its levers, and
/// each output bit is on while the component standing on its block is; a
/// signal-strength input takes each digit's strength, and an output of a
/// build with one shows a digit per block, its strength.
///
/// A build is compiled by Redpiler, whose optimizations stay off: they keep only what drives a lamp or another
/// component it can read back, and an output block is neither. A lamp in
/// its place reads differently, lighting from a torch beside it or a
/// powered block beside it, which do not power the block. So every
/// component's state is written back to the world after each change, and
/// each output block's power is worked out from its neighbours.
///
/// A signal-strength input's digits are each driven by the comparator the
/// build has beside its dust, with an analog lever behind it set to the
/// digit, which the comparator reads as it would a barrel holding it:
/// Redpiler reads a container's strength only as it compiles, but sets a
/// lever's at any time.
pub struct Circuit {
    world: TestWorld,
    /// Redpiler's circuit.
    compiler: Option<Compiler>,
    /// How each input port is driven, in the order of `Interface::inputs`.
    inputs: Vec<Drive>,
    /// Each output port's blocks, least significant first, in the order of
    /// `Interface::outputs`, and whether it shows a digit per block.
    outputs: Vec<(Vec<BlockPos>, bool)>,
    /// Each wooden button held down, with the ticks left before it springs
    /// back; the world holds it as a lever, which `advance` turns off.
    held: Vec<(BlockPos, u32)>,
}

/// How an input port is driven: by a lever per bit, or by an analog lever
/// behind a comparator per digit.
enum Drive {
    Levers(Vec<Lever>),
    Strengths(Vec<Driver>),
}

/// A lever, or a button, which `set` leaves alone and `press` presses.
struct Lever {
    pos: BlockPos,
    on: bool,
    button: bool,
    /// A wooden button, held down longer than a stone one, which the
    /// simulator does not model: the world holds a lever in its place.
    wooden: bool,
    /// The strength it powers its block at when on: 15, as a lever does,
    /// unless `drive_at` sets it weaker.
    power: u8,
}

/// What drives a digit: an analog lever behind the comparator pointing into
/// its dust, at the digit's strength.
struct Driver {
    lever: BlockPos,
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

/// The ticks a press holds a wooden button down: 30 game ticks, where a
/// stone button's 20 are 10.
const WOODEN_PRESS: u32 = 15;

impl Circuit {
    /// Sets up `blocks`, whose interface is `interface`.
    pub fn new(blocks: &Blocks, interface: &Interface) -> Result<Self> {
        Self::resting(blocks, interface, &[])
    }

    /// Sets up `blocks` to run in the world itself, block by block, unless
    /// it takes digits, which only Redpiler drives: each block updating its
    /// neighbours in the order the game's do, and the ticks scheduled for
    /// one tick running in the order they were scheduled within each
    /// priority, as the game runs them, where Redpiler runs them in the
    /// order of its compiled links. Slower, so for tracing one change.
    pub fn in_world(blocks: &Blocks, interface: &Interface) -> Result<Self> {
        Self::set_up(blocks, interface, &[], false)
    }

    /// Sets up `blocks`, whose interface is `interface`, with each input
    /// port of `held` set to its value before the build first comes to
    /// rest, as a line read inverted rests on: a counter whose clock counts
    /// while its lever is off never rests with it off.
    pub fn resting(
        blocks: &Blocks,
        interface: &Interface,
        held: &[(usize, Value)],
    ) -> Result<Self> {
        Self::set_up(blocks, interface, held, true)
    }

    /// As [`Circuit::resting`], compiled by Redpiler when `redpiler` says or
    /// it takes digits, else run block by block in the world.
    fn set_up(
        blocks: &Blocks,
        interface: &Interface,
        held: &[(usize, Value)],
        redpiler: bool,
    ) -> Result<Self> {
        let mut world = world(blocks)?;
        let digits = interface.inputs.iter().any(|port| port.signal_strength);
        let mut taken = HashSet::new();
        let mut inputs = interface
            .inputs
            .iter()
            .map(|port| {
                if port.signal_strength {
                    drivers(blocks, &mut world, port, &mut taken)
                        .map(Drive::Strengths)
                } else {
                    levers(blocks, port).map(Drive::Levers)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        hold(&mut world, &mut inputs, held);
        settle_world(&mut world, world_size(blocks))?;
        let outputs: Vec<(Vec<BlockPos>, bool)> = interface
            .outputs
            .iter()
            .map(|port| {
                let blocks = port
                    .bits
                    .iter()
                    .map(|bit| {
                        // A bit is read only by what stands on its block,
                        // the component carrying it out of the build, never
                        // by the block's own power.
                        let above =
                            world.get_block(at(bit.pos).offset(BlockFace::Top));
                        let read = matches!(
                            above,
                            Block::Repeater(_) | Block::Comparator(_)
                        );
                        // A build so wired breaks the conventions, so it
                        // fails rather than going untested.
                        if !read {
                            broken!(
                                "Output {} has a block with no repeater or \
                                 comparator on it, which is how an output is \
                                 read.",
                                port.name
                            );
                        }
                        Ok(at(bit.pos))
                    })
                    .collect::<Result<_>>()?;
                Ok((
                    blocks,
                    digits && shows_digits(interface, port)
                        || port.signal_strength,
                ))
            })
            .collect::<Result<_>>()?;
        // A digit is driven only through Redpiler, by an analog lever, so a
        // build taking digits compiles even to run block by block.
        let compiler = if !redpiler && !digits {
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
                // Its levers may be driven at any strength.
                let options = CompilerOptions {
                    analog_inputs: true,
                    ..CompilerOptions::default()
                };
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
            held: Vec::new(),
        })
    }

    /// Sets input `port` to `value`, flipping the levers that change, or
    /// filling each digit's barrel.
    pub fn set(&mut self, port: usize, value: Value) {
        match &mut self.inputs[port] {
            Drive::Levers(levers) => {
                for (i, lever) in levers.iter_mut().enumerate() {
                    let on = value >> i & 1 == 1;
                    if lever.on != on && !lever.button {
                        if let Some(compiler) = &mut self.compiler {
                            let power = if on { lever.power } else { 0 };
                            compiler.set_power(lever.pos, power);
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
                    if driver.value != digit
                        && let Some(compiler) = &mut self.compiler
                    {
                        compiler.set_power(driver.lever, digit);
                        driver.value = digit;
                    }
                }
            }
        }
    }

    /// Drives input `port`'s levers at `power` from now on, as a comparator
    /// pointing into each block would, each on now set so at once. Only
    /// Redpiler sets a strength, so a build it couldn't compile can't be
    /// driven so.
    pub fn drive_at(&mut self, port: usize, power: u8) -> Result<()> {
        let Some(compiler) = &mut self.compiler else {
            bail!("A build running block by block can't be driven weakly.");
        };
        let Drive::Levers(levers) = &mut self.inputs[port] else {
            bail!("A signal-strength input takes its value as its strength.");
        };
        for lever in levers.iter_mut().filter(|lever| !lever.button) {
            lever.power = power;
            if lever.on {
                compiler.set_power(lever.pos, power);
            }
        }
        Ok(())
    }

    /// Presses the button on bit `bit` of input `port`, as a player does: it
    /// powers its block until it springs back, ten ticks on, or fifteen for
    /// a wooden one. Pressing it while it is down does nothing.
    pub fn press(&mut self, port: usize, bit: usize) {
        let Drive::Levers(levers) = &self.inputs[port] else {
            return;
        };
        let Some(button) = levers.get(bit).filter(|lever| lever.button) else {
            return;
        };
        let pos = button.pos;
        if button.wooden {
            if !self.held.iter().any(|&(held, _)| held == pos) {
                self.held.push((pos, WOODEN_PRESS));
                self.flip(pos, true);
            }
        } else if let Some(compiler) = &mut self.compiler {
            compiler.on_use_block(pos);
        } else {
            let block = self.world.get_block(pos);
            mchprs_redstone::on_use(block, &mut self.world, pos);
        }
        // So `pressed` sees it down before the next tick.
        self.show();
    }

    /// Turns the lever at `pos` on or off, from off or on.
    fn flip(&mut self, pos: BlockPos, on: bool) {
        if let Some(compiler) = &mut self.compiler {
            compiler.set_power(pos, if on { 15 } else { 0 });
        } else {
            let block = self.world.get_block(pos);
            mchprs_redstone::on_use(block, &mut self.world, pos);
        }
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
                | Block::Lever { powered: true, .. }
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

    /// One tick, without showing it, after which each wooden button held
    /// down its time springs back, as a stone one does within the tick.
    fn advance(&mut self) {
        match &mut self.compiler {
            Some(compiler) => compiler.tick(),
            None => tick_world(&mut self.world),
        }
        for (_, left) in &mut self.held {
            *left -= 1;
        }
        let (up, held) = self.held.iter().partition(|&&(_, left)| left == 0);
        self.held = held;
        for (pos, _) in up {
            self.flip(pos, false);
        }
    }

    /// Whether anything is left to update, a wooden button still down
    /// among it.
    pub fn pending(&mut self) -> bool {
        !self.held.is_empty()
            || match &mut self.compiler {
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
    }

    /// Whether the component at `pos`, a torch, repeater, comparator, or
    /// lever, is on, as of the last `settle`.
    pub fn on(&self, pos: Pos) -> bool {
        match self.world.get_block(at(pos)) {
            Block::RedstoneTorch { lit }
            | Block::RedstoneWallTorch { lit, .. } => lit,
            Block::Repeater(repeater) => repeater.powered,
            Block::Comparator(comparator) => comparator.powered,
            Block::Lever { powered, .. }
            | Block::StoneButton { powered, .. } => powered,
            Block::RedstoneWire(wire) => wire.power > 0,
            _ => false,
        }
    }

    /// The strength the component at `pos` gives out, as of the last
    /// `settle`: a comparator's own, and 15 for any other on.
    pub fn strength(&self, pos: Pos) -> u8 {
        match self.world.get_block_entity(at(pos)) {
            Some(BlockEntity::Comparator { output_strength }) => {
                *output_strength
            }
            _ => 15 * u8::from(self.on(pos)),
        }
    }

    /// The value output `port` shows, as of the last `settle`, read from
    /// the dust, repeater, or comparator standing on each bit's block, the
    /// component carrying it out of the build, however the block below is
    /// powered. A bit is on while its component is; a digit is the strength
    /// its component gives out.
    pub fn get(&self, port: usize) -> Value {
        let (blocks, digits) = &self.outputs[port];
        blocks.iter().enumerate().fold(0, |value, (i, &pos)| {
            let top = pos.offset(BlockFace::Top);
            let (on, strength) = match self.world.get_block(top) {
                Block::RedstoneWire(wire) => (wire.power > 0, wire.power),
                Block::Repeater(repeater) => {
                    (repeater.powered, if repeater.powered { 15 } else { 0 })
                }
                Block::Comparator(comparator) => {
                    let strength = match self.world.get_block_entity(top) {
                        Some(BlockEntity::Comparator { output_strength }) => {
                            *output_strength
                        }
                        _ => 0,
                    };
                    (comparator.powered, strength)
                }
                // `new` takes no output with a bit bare of one.
                _ => (false, 0),
            };
            if *digits {
                value | Value::from(strength) << (4 * i)
            } else {
                value | Value::from(on) << i
            }
        })
    }

    /// The strength each bit of output `port` carries out of the build,
    /// as of the last `settle`, least significant first: for a repeater
    /// or comparator standing on its block, on, the strength arriving at
    /// its input, as [`Circuit::arriving_at`] reads it, whatever it gives
    /// out itself; for dust there, its power; 0 for a bit off.
    pub fn arriving_strengths(&self, port: usize) -> Vec<u8> {
        let (blocks, _) = &self.outputs[port];
        blocks
            .iter()
            .map(|&pos| {
                let top = pos.offset(BlockFace::Top);
                match self.world.get_block(top) {
                    Block::RedstoneWire(wire) => wire.power,
                    Block::Repeater(repeater) if repeater.powered => {
                        self.arriving_at(top)
                    }
                    Block::Comparator(comparator) if comparator.powered => {
                        self.arriving_at(top)
                    }
                    _ => 0,
                }
            })
            .collect()
    }

    /// The strength arriving at the input of the repeater or comparator at
    /// the world's `pos`, as of the last `settle`, as the game reads it:
    /// the power of what is behind it, the side its `facing` names, dust at
    /// its level, and for a comparator a container's fullness; 0 at any
    /// other block.
    fn arriving_at(&self, pos: BlockPos) -> u8 {
        let facing = match self.world.get_block(pos) {
            Block::Repeater(repeater) => repeater.facing,
            Block::Comparator(comparator) => comparator.facing,
            _ => return 0,
        };
        let face = facing.block_face();
        let behind = pos.offset(face);
        let block = self.world.get_block(behind);
        if let Block::Comparator(_) = self.world.get_block(pos)
            && mchprs_redstone::comparator::has_override(block)
        {
            return mchprs_redstone::comparator::get_override(
                block,
                &self.world,
                behind,
            );
        }
        let power = mchprs_redstone::get_redstone_power(
            block,
            &self.world,
            behind,
            face,
        );
        match block {
            Block::RedstoneWire(wire) if power == 0 => wire.power,
            _ => power,
        }
    }

    /// The power at `pos`, as of the last `settle`: a solid block's, from
    /// what powers it, dust's level, and 15 for any other component on,
    /// as [`Circuit::on`] says.
    pub fn power(&self, pos: Pos) -> u8 {
        let block = self.world.get_block(at(pos));
        match block {
            Block::RedstoneWire(wire) => wire.power,
            _ if block.is_solid() => mchprs_redstone::get_redstone_power(
                block,
                &self.world,
                at(pos),
                BlockFace::Top,
            ),
            _ => 15 * u8::from(self.on(pos)),
        }
    }

    /// For each input, the ticks its drivers take to change it once it is
    /// set: a signal-strength input's comparators, one, so its dust takes
    /// its value a tick on; a lever's, none.
    pub fn lead(&self) -> Vec<i32> {
        self.inputs
            .iter()
            .map(|drive| match drive {
                Drive::Levers(_) => 0,
                Drive::Strengths(_) => 1,
            })
            .collect()
    }

    /// For each output, the ticks the repeater or comparator standing on
    /// each bit's block takes to carry it out of the build, which the bit
    /// shows that much later than it arrives: a repeater its delay, from 1
    /// to 4, and a comparator 1, whether its bit is read as on or off or as
    /// a strength. One fed from a side, a repeater a side may lock or a
    /// comparator a side may subtract from or compare against, does the
    /// build's own work, as a latch holding its bit does, so carries it out
    /// in no ticks: its own count in the build's.
    pub fn carried(&self) -> Carried {
        let delay = |pos: BlockPos| {
            let top = pos.offset(BlockFace::Top);
            match self.world.get_block(top) {
                _ if self.fed_from_a_side(top) => 0,
                Block::Repeater(repeater) => i32::from(repeater.delay),
                Block::Comparator(_) => 1,
                _ => 0,
            }
        };
        Carried(
            self.outputs
                .iter()
                .map(|(blocks, digits)| {
                    let each = if *digits { 4 } else { 1 };
                    blocks
                        .iter()
                        .flat_map(|&pos| std::iter::repeat_n(delay(pos), each))
                        .collect()
                })
                .collect(),
        )
    }

    /// Whether the repeater or comparator at `pos` has a side something may
    /// power, as the simulator reads its sides: a repeater or comparator
    /// giving out into it, which locks a repeater, and for a comparator
    /// also dust or a block of redstone beside it. Whether that side is on
    /// is no matter: a latch's lock is off as often as on.
    fn fed_from_a_side(&self, pos: BlockPos) -> bool {
        let (facing, comparator) = match self.world.get_block(pos) {
            Block::Repeater(repeater) => (repeater.facing, false),
            Block::Comparator(c) => (c.facing, true),
            _ => return false,
        };
        [facing.rotate(), facing.rotate_ccw()]
            .into_iter()
            .any(|side| {
                match self.world.get_block(pos.offset(side.block_face())) {
                    // A diode gives out away from the way it faces.
                    Block::Repeater(r) => r.facing == side,
                    Block::Comparator(c) => c.facing == side,
                    Block::RedstoneWire(_) | Block::RedstoneBlock => comparator,
                    _ => false,
                }
            })
    }
}

/// The ticks each output bit's repeater or comparator takes to carry it out
/// of the build, per output, per bit of its value, as [`Circuit::carried`]
/// finds them. Every timing leaves them out: an output is timed as its
/// signal arrives at its repeater or comparator, which is the build's own
/// work, and not as that gives it out, so a tick a tester's repeater would
/// take counts for nothing. Moving each change back by its own bit's delay
/// is exact: each changes what it gives out only its delay after what
/// arrives at its back does. One fed from a side is the build's work, a
/// latch's or a subtraction's, so is moved back by none.
#[derive(Debug, Clone, Default)]
pub struct Carried(Vec<Vec<i32>>);

impl Carried {
    /// The ticks bit `bit` of output `port`'s value is carried; none for a
    /// bit past its width.
    pub fn bit(&self, port: usize, bit: usize) -> i32 {
        self.0
            .get(port)
            .and_then(|bits| bits.get(bit))
            .copied()
            .unwrap_or(0)
    }

    /// The most ticks any bit of output `port` is carried, as its timing
    /// reports it.
    pub fn port(&self, port: usize) -> i32 {
        self.0
            .get(port)
            .and_then(|bits| bits.iter().copied().max())
            .unwrap_or(0)
    }

    /// The ticks of `changes`, each bit of output `port`'s, the ticks it
    /// changed on, each moved back by what carries its own bit out.
    pub fn arrived(&self, port: usize, changes: &[Vec<i32>]) -> Vec<Vec<i32>> {
        changes
            .iter()
            .enumerate()
            .map(|(bit, ticks)| {
                let carried = self.bit(port, bit);
                ticks.iter().map(|tick| tick - carried).collect()
            })
            .collect()
    }

    /// Output `port`'s value `before` and each tick it then changed on with
    /// its value from then, `steps`, as they arrived: each bit's change
    /// moved back by its own bit's carry, the value at each tick made of
    /// every bit as it had arrived by then.
    pub fn arrived_steps(
        &self,
        port: usize,
        before: Value,
        steps: &[(i32, Value)],
    ) -> Vec<(i32, Value)> {
        let mut flips: BTreeMap<i32, Value> = BTreeMap::new();
        let mut last = before;
        for &(tick, value) in steps {
            let flipped = value ^ last;
            for bit in
                (0..Value::BITS as usize).filter(|b| flipped >> b & 1 == 1)
            {
                *flips.entry(tick - self.bit(port, bit)).or_default() ^=
                    1 << bit;
            }
            last = value;
        }
        let mut value = before;
        flips
            .into_iter()
            .filter(|&(_, flipped)| flipped != 0)
            .map(|(tick, flipped)| {
                value ^= flipped;
                (tick, value)
            })
            .collect()
    }

    /// Output `port`'s bits grouped by what carries them out, each group's
    /// ticks and a mask of its bits, of the `bits` its value has: where to
    /// read each bit to see it as it arrived some tick before.
    pub fn groups(&self, port: usize, bits: usize) -> Vec<(i32, Value)> {
        let mut groups: BTreeMap<i32, Value> = BTreeMap::new();
        for bit in 0..bits {
            *groups.entry(self.bit(port, bit)).or_default() |= 1 << bit;
        }
        groups.into_iter().collect()
    }
}

/// Whether an output of a build with a signal-strength input shows a digit
/// per block: any but a status line, such as a carry out, unless its
/// outputs give bits, as a hex to binary converter's do.
fn shows_digits(interface: &Interface, port: &Port) -> bool {
    !port.status() && !interface.binary_outputs
}

/// How many bits input `port` carries: four for each block of a
/// signal-strength input, a digit per block, else one for each block.
pub(super) fn input_bits(port: &Port) -> usize {
    port.bits.len() * if port.signal_strength { 4 } else { 1 }
}

/// How many bits output `port` of a build with `interface` carries: four
/// for each block when the build takes a signal strength and the port
/// shows digits, or when its test reads it as a strength, else one for
/// each block.
pub(super) fn output_bits(interface: &Interface, port: &Port) -> usize {
    let digits = interface.inputs.iter().any(|input| input.signal_strength)
        && shows_digits(interface, port)
        || port.signal_strength;
    port.bits.len() * if digits { 4 } else { 1 }
}

/// A driver for each digit of signal-strength input `port`: an analog lever
/// behind the build's own comparator for it, the one with nothing behind
/// it that the rule for inputs reads, see [`repeater_outputs::driving`].
/// `taken` holds the places drivers have already used.
fn drivers(
    blocks: &Blocks,
    world: &mut TestWorld,
    port: &Port,
    taken: &mut HashSet<Pos>,
) -> Result<Vec<Driver>> {
    port.bits
        .iter()
        .map(|bit| {
            let input = repeater_outputs::driving(blocks, bit.pos)
                .filter(|input| !taken.contains(&input.behind()))
                .with_context(|| {
                    format!(
                        "A digit of input {} has no comparator of its own, \
                         with nothing behind it, for the simulator to drive.",
                        port.name
                    )
                })?;
            let behind = input.behind();
            taken.insert(behind);
            // An analog lever, hung on the comparator so it powers no block
            // of the build, gives the comparator the digit as a barrel of
            // that strength would, and Redpiler can set it.
            let away = match [behind[0] - input.at[0], behind[2] - input.at[2]]
            {
                [1, _] => "east",
                [-1, _] => "west",
                [_, 1] => "south",
                _ => "north",
            };
            world.set_block(
                at(behind),
                block(&format!(
                    "minecraft:lever[face=wall,facing={away},powered=false]"
                ))?,
            );
            Ok(Driver {
                lever: at(behind),
                value: 0,
            })
        })
        .collect()
}

/// A link of a compiled circuit: the part it runs from and the part it
/// runs to, by position, and whether it goes into that part's side.
pub type Link = (Pos, Pos, bool);

/// The circuit Redpiler compiles from a build, as its graph stands before
/// any optimization: each component's kind, such as a repeater with its
/// delay, by position, and each link into a component or into the dust on
/// an output block, which is how the site reads it, with the signal
/// strength it loses on the way. Two builds with the same circuit compute
/// the same in the same ticks.
#[derive(Debug, PartialEq, Eq)]
pub struct Compiled {
    pub kinds: BTreeMap<Pos, String>,
    pub links: BTreeMap<Link, u8>,
}

impl Compiled {
    /// A hash of the whole circuit.
    pub fn key(&self) -> String {
        crate::import::sha256(format!("{self:?}").as_bytes())
    }
}

/// The graph Redpiler compiles from `blocks`, settled with every lever off,
/// before any optimization: each component a node, and each way one
/// powers another, through dust or a block, a link. Only how its
/// components link matters, so one that never rests with every lever off,
/// as a counter whose clock reads inverted, is compiled as it stands once
/// the world has tried.
pub(super) fn compile_graph(blocks: &Blocks) -> Result<CompileGraph> {
    let mut world = world(blocks)?;
    let size = world_size(blocks);
    let _restless = settle_world(&mut world, size);
    let [x, y, z] = size;
    let input = CompilerInput {
        world: &world,
        bounds: (BlockPos::new(0, 0, 0), BlockPos::new(x - 1, y - 1, z - 1)),
        ticks: &[],
    };
    let options = CompilerOptions::default();
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        build_pass_pipeline(&PassRegistry::default(), &options).run_passes(
            &options,
            &input,
            CompileGraph::default(),
            Arc::new(TaskMonitor::default()),
        )
    }))
    .map_err(|panic| {
        anyhow::anyhow!(
            "The simulator could not compile it: {}.",
            panic_message(&*panic)
        )
    })
}

/// Where a node of a compiled graph sits in the build: its first block.
pub(super) fn node_place(node: &CompileNode) -> Option<Pos> {
    let origin = at([0, 0, 0]);
    node.block
        .first()
        .map(|&(p, _)| [p.x - origin.x, p.y - origin.y, p.z - origin.z])
}

/// The circuit of `blocks`; see [`Compiled`].
pub fn compiled(blocks: &Blocks) -> Result<Compiled> {
    let graph = compile_graph(blocks)?;
    let pos = node_place;
    let read: HashSet<Pos> = crate::interface::read(blocks)
        .outputs
        .iter()
        .flat_map(|p| p.bits.iter().map(|b| schem::step(b.pos, Direction::Up)))
        .collect();
    let kinds = graph
        .node_indices()
        .filter(|&i| graph[i].ty != NodeType::Wire)
        .filter_map(|i| {
            let node = &graph[i];
            // A constant, as a barrel read by a comparator, is its strength.
            let kind = match &node.ty {
                NodeType::Constant => {
                    format!("Constant {{ ss: {} }}", node.state.output_strength)
                }
                ty => format!("{ty:?}"),
            };
            Some((pos(node)?, kind))
        })
        .collect();
    let mut links = BTreeMap::new();
    for edge in graph.all_edges() {
        let ends = (pos(&graph[edge.source()]), pos(&graph[edge.target()]));
        let (Some(from), Some(to)) = ends else {
            continue;
        };
        if graph[edge.target()].ty == NodeType::Wire && !read.contains(&to) {
            continue;
        }
        let link = edge.weight();
        let ss = links.entry((from, to, link.ty == LinkType::Side));
        let ss = ss.or_insert(link.ss);
        *ss = (*ss).min(link.ss);
    }
    Ok(Compiled { kinds, links })
}

/// An MCHPRS world holding `blocks` as they were saved, `MARGIN` blocks in
/// from its west and north sides, not yet settled, every lever off: how a
/// lever was saved says nothing, and each line's test finds whether it
/// reads inverted.
fn world(blocks: &Blocks) -> Result<TestWorld> {
    let chunks = |n: i32| (n + 15) / 16;
    let [x, y, z] = world_size(blocks);
    let mut world = TestWorld::new(chunks(x), chunks(y), chunks(z));
    for (pos, state) in blocks.iter() {
        let mut block = block(state)?;
        if let Block::Lever { powered, .. } = &mut block {
            *powered = false;
        }
        world.set_block(at(pos), block);
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

/// The components whose power a file saves in their state.
const POWERED: [&str; 8] = [
    "redstone_wire",
    "repeater",
    "comparator",
    "redstone_torch",
    "redstone_wall_torch",
    "redstone_lamp",
    "target",
    "observer",
];

/// The properties of a component's state that say how it is powered.
const POWER: [&str; 4] = ["power", "powered", "lit", "locked"];

/// What `blocks` comes to rest as with every lever off but those at `on`,
/// as the game settles it: each block whose state changes, as its state
/// with its power alone changed, and each comparator whose strength does.
pub fn settled(
    blocks: &Blocks,
    on: &BTreeSet<Pos>,
) -> Result<(BTreeMap<Pos, String>, HashMap<Pos, u8>)> {
    let mut world = world(blocks)?;
    for &pos in on {
        let mut block = world.get_block(at(pos));
        if let Block::Lever { powered, .. } = &mut block {
            *powered = true;
            world.set_block(at(pos), block);
        }
    }
    settle_world(&mut world, world_size(blocks))?;
    let mut changes = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let short = schem::short(state);
        let block = world.get_block(at(pos));
        let now = if short == "lever" {
            set_properties(state, &[("powered", on.contains(&pos).to_string())])
        } else if POWERED.contains(&short) {
            let properties = block.properties();
            let power: Vec<(&str, String)> = POWER
                .into_iter()
                .filter_map(|key| Some((key, properties.get(key)?.clone())))
                .collect();
            set_properties(state, &power)
        } else {
            continue;
        };
        if now != state {
            changes.insert(pos, now);
        }
    }
    let strengths = blocks
        .comparators
        .iter()
        .filter_map(|(&pos, &was)| {
            let BlockEntity::Comparator { output_strength } =
                world.get_block_entity(at(pos))?
            else {
                return None;
            };
            (*output_strength != was).then_some((pos, *output_strength))
        })
        .collect();
    Ok((changes, strengths))
}

/// `state` with each of `properties` it has set to its value.
fn set_properties(state: &str, properties: &[(&str, String)]) -> String {
    let Some((id, rest)) = state.split_once('[') else {
        return state.to_owned();
    };
    let fields: Vec<String> = rest
        .trim_end_matches(']')
        .split(',')
        .map(|field| match field.split_once('=') {
            Some((key, _)) => {
                match properties.iter().find(|(k, _)| *k == key) {
                    Some((_, value)) => format!("{key}={value}"),
                    None => field.to_owned(),
                }
            }
            None => field.to_owned(),
        })
        .collect();
    format!("{id}[{}]", fields.join(","))
}

/// Turns on, in `world` as it was saved, the levers of each input port of
/// `held` its value sets, as `inputs` drive them.
fn hold(world: &mut TestWorld, inputs: &mut [Drive], held: &[(usize, Value)]) {
    for &(port, value) in held {
        let Some(Drive::Levers(levers)) = inputs.get_mut(port) else {
            continue;
        };
        for (i, lever) in levers.iter_mut().enumerate() {
            if value >> i & 1 == 0 || lever.button {
                continue;
            }
            let mut block = world.get_block(lever.pos);
            if let Block::Lever { powered, .. } = &mut block {
                *powered = true;
            }
            world.set_block(lever.pos, block);
            lever.on = true;
        }
    }
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
        bail!("Couldn't recognize input {} without levers.", port.name);
    }
    port.bits
        .iter()
        .map(|bit| {
            let pos = bit.entry.map(|side| schem::step(bit.pos, side));
            let switch = pos.map(|pos| (pos, schem::id(blocks.get(pos))));
            let Some((pos, id)) = switch else {
                bail!("A bit of input {} has no lever.", port.name);
            };
            let name = schem::short(id);
            let (button, wooden) = match crate::schem::block::like(name) {
                "lever" => (false, false),
                "stone_button" => (true, false),
                "oak_button" => (true, true),
                _ => bail!("A bit of input {} has no lever.", port.name),
            };
            Ok(Lever {
                pos: at(pos),
                on: false,
                button,
                wooden,
                power: 15,
            })
        })
        .collect()
}

/// The MCHPRS block for a Sponge block state, such as
/// `minecraft:lever[face=wall,facing=south,powered=false]`.
fn block(state: &str) -> Result<Block> {
    let name = schem::short(state);
    // A button is held down as long as any of its kind is: a wooden one as
    // a lever `Circuit` lets up, the simulator having none.
    let id = match crate::schem::block::like(name) {
        "stone_button" => "minecraft:stone_button",
        "oak_button" => "minecraft:lever",
        _ => schem::id(state),
    };
    let name = schem::short(id);
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
    // Any other full block that carries power, such as wool, terracotta,
    // or stone, does as concrete does.
    // And a slab, stairs, or glowstone, which redstone sees through, as
    // glass does.
    // A trapdoor, hung on an output to show it flipping, plays no part in
    // the circuit, so is air.
    let id = if name.ends_with("_trapdoor") {
        "minecraft:air"
    } else if !known
        && !crate::schem::block::is_component(name)
        && crate::schem::block::solid(state)
    {
        "minecraft:white_concrete"
    } else if !known && crate::schem::block::like_glass(name) {
        "minecraft:white_stained_glass"
    } else {
        id
    };
    let known = known
        || matches!(
            id,
            "minecraft:white_concrete"
                | "minecraft:white_stained_glass"
                | "minecraft:air"
        );
    let Some(mut block) = Block::from_name(id).filter(|_| known) else {
        bail!("Couldn't simulate its {}.", name.replace('_', " "));
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
    fn each_bit_arrives_its_own_delay_before_it_shows() {
        // Bit 0 given out by a 1-tick repeater, bit 1 by a 3-tick one.
        let carried = Carried(vec![vec![1, 3]]);
        assert_eq!(
            carried.arrived(0, &[vec![4], vec![6, 9]]),
            [vec![3], vec![3, 6]]
        );
        // Both bits turn on as shown at 4 and 6, so both arrived at 3.
        let steps = carried.arrived_steps(0, 0, &[(4, 0b01), (6, 0b11)]);
        assert_eq!(steps, [(3, 0b11)]);
        assert_eq!(carried.groups(0, 2), [(1, 0b01), (3, 0b10)]);
        assert_eq!(carried.port(0), 3);
    }

    /// A trapdoor, hung on an output to show it, is air to the circuit,
    /// open or not.
    #[test]
    fn a_trapdoor_is_air() {
        for state in [
            "minecraft:iron_trapdoor[facing=east,half=top,open=false,powered=false,waterlogged=false]",
            "minecraft:oak_trapdoor[facing=north,half=bottom,open=true,powered=true,waterlogged=false]",
        ] {
            assert!(matches!(block(state).unwrap(), Block::Air), "{state}");
        }
    }

    #[test]
    fn a_block_it_does_not_model_is_refused_by_name() {
        let err =
            block("minecraft:piston[extended=false,facing=up]").unwrap_err();
        assert_eq!(err.to_string(), "Couldn't simulate its piston.");
    }

    #[test]
    fn parses_a_block_state_with_its_properties() {
        let lever =
            block("minecraft:lever[face=wall,facing=south,powered=true]")
                .unwrap();
        assert!(matches!(lever, Block::Lever { powered: true, .. }));
    }
}
