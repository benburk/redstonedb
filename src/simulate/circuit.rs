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
    compile_graph::{CompileGraph, LinkType, NodeType},
    passes::{PassRegistry, build_pass_pipeline},
};
use mchprs_world::{World, testing::TestWorld};

use super::Broken;
use crate::{
    colour,
    interface::{Interface, Port},
    schem::{self, Blocks, Direction, Pos},
};

/// The most ticks a build may take to settle after its inputs change.
pub(super) const MAX_TICKS: u32 = 1000;

/// A build set up for simulation. Each input is driven by its levers, and
/// each output bit is on while the component standing on its block is; a
/// signal-strength input takes each digit's strength, and an output of a
/// build with one shows a digit per block, its strength.
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
    on: bool,
    button: bool,
    /// The strength it powers its block at when on: 15, as a lever does,
    /// unless `drive_at` sets it weaker.
    power: u8,
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
                        // A bit is read only by what stands on its block,
                        // the component carrying it out of the build, never
                        // by the block's own power.
                        let above =
                            world.get_block(at(bit.pos).offset(BlockFace::Top));
                        let read = matches!(
                            above,
                            Block::RedstoneWire(_)
                                | Block::Repeater(_)
                                | Block::Comparator(_)
                        );
                        // A build so wired breaks the conventions, so it
                        // fails rather than going untested.
                        if !read {
                            broken!(
                                "Output {} has a block with no dust, \
                                 repeater, or comparator on it, which is how \
                                 an output is read.",
                                port.name
                            );
                        }
                        Ok(at(bit.pos))
                    })
                    .collect::<Result<_>>()?;
                Ok((
                    blocks,
                    digits && shows_digits(port) || port.signal_strength,
                ))
            })
            .collect::<Result<_>>()?;
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
        })
    }

    /// Sets input `port` to `value`, flipping the levers that change, or
    /// filling each digit's barrel.
    pub fn set(&mut self, port: usize, value: u64) {
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

    /// Drives input `port`'s levers at `power` from now on, as a comparator
    /// pointing into each block would, each on now set so at once. Only
    /// Redpiler sets a strength, so a build running block by block, as one
    /// with a signal-strength input does, can't be driven so.
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

    /// The value output `port` shows, as of the last `settle`, read from
    /// the dust, repeater, or comparator standing on each bit's block, the
    /// component carrying it out of the build, however the block below is
    /// powered. A bit is on while its component is; a digit is the strength
    /// its component gives out.
    pub fn get(&self, port: usize) -> u64 {
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
                value | u64::from(strength) << (4 * i)
            } else {
                value | u64::from(on) << i
            }
        })
    }

    /// The strength each bit of output `port` shows, as of the last
    /// `settle`, least significant first: the power of the dust standing
    /// on its block, 15 for a repeater on, or a comparator's strength.
    pub fn strengths(&self, port: usize) -> Vec<u8> {
        let (blocks, _) = &self.outputs[port];
        blocks
            .iter()
            .map(|&pos| {
                let top = pos.offset(BlockFace::Top);
                match self.world.get_block(top) {
                    Block::RedstoneWire(wire) => wire.power,
                    Block::Repeater(repeater) => {
                        15 * u8::from(repeater.powered)
                    }
                    Block::Comparator(_) => {
                        match self.world.get_block_entity(top) {
                            Some(BlockEntity::Comparator {
                                output_strength,
                            }) => *output_strength,
                            _ => 0,
                        }
                    }
                    _ => 0,
                }
            })
            .collect()
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

    /// For each output, the ticks the repeaters or comparators standing on
    /// its every block take to carry it out of the build, the least of
    /// them, which its value shows that much later than it arrives.
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
    !port.block.starts_with(colour::STATUS)
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
        && shows_digits(port)
        || port.signal_strength;
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

    /// The components that could hold state: those on a feedback loop, and
    /// repeaters locked from their side. A comparator in compare mode that
    /// reads, from its side, one in compare mode whose back is fed just as
    /// its own is, itself included, is no loop: what that one puts out is
    /// their shared back's strength or nothing, and reaches the side no
    /// stronger, so it can never decide the comparison the back already
    /// wins. Nor is a link that loses all a component could ever put out,
    /// as from a comparator reading a barrel of strength 1 to its own side.
    pub fn memory(&self) -> BTreeSet<Pos> {
        let compares = |p: &Pos| {
            self.kinds.get(p).is_some_and(|k| {
                k.contains("mode: Compare") && k.contains("far_input: None")
            })
        };
        let backs: BTreeMap<Pos, Vec<(Pos, u8)>> = self
            .links
            .iter()
            .filter(|((_, _, side), _)| !side)
            .fold(BTreeMap::new(), |mut backs, (&(from, to, _), &loss)| {
                backs.entry(to).or_default().push((from, loss));
                backs
            });
        let strongest = self.strongest(&backs);
        let inert = |&(from, to, side): &Link| {
            side && compares(&from)
                && compares(&to)
                && backs.get(&from) == backs.get(&to)
        };
        let carries = |link: &Link| {
            strongest.get(&link.0).copied().unwrap_or(15) > self.links[link]
        };
        let mut next: BTreeMap<Pos, Vec<Pos>> = BTreeMap::new();
        let mut found = BTreeSet::new();
        for link in self.links.keys().filter(|l| carries(l) && !inert(l)) {
            let &(from, to, side) = link;
            if from == to {
                found.insert(from);
            } else {
                next.entry(from).or_default().push(to);
            }
            if side
                && self
                    .kinds
                    .get(&to)
                    .is_some_and(|k| k.starts_with("Repeater"))
            {
                found.insert(to);
            }
        }
        // A component is on a loop when it reaches itself: every one a path
        // from it comes back to.
        for &start in self.kinds.keys() {
            let mut seen = BTreeSet::new();
            let mut stack: Vec<Pos> =
                next.get(&start).cloned().unwrap_or_default();
            while let Some(p) = stack.pop() {
                if p == start {
                    found.insert(start);
                    break;
                }
                if seen.insert(p) {
                    stack.extend(next.get(&p).into_iter().flatten());
                }
            }
        }
        found
    }

    /// The strongest signal each component could ever put out: a
    /// constant's own, and a comparator's no more than what reaches its
    /// back, `backs`; any other may reach 15.
    fn strongest(
        &self,
        backs: &BTreeMap<Pos, Vec<(Pos, u8)>>,
    ) -> BTreeMap<Pos, u8> {
        let mut strongest: BTreeMap<Pos, u8> = self
            .kinds
            .iter()
            .map(|(&p, k)| {
                let constant = k
                    .strip_prefix("Constant { ss: ")
                    .and_then(|ss| ss.trim_end_matches(" }").parse().ok());
                (p, constant.unwrap_or(15))
            })
            .collect();
        // Lowered from 15 until no comparator's back can reach more.
        loop {
            let mut lowered = false;
            for (p, k) in &self.kinds {
                if !k.starts_with("Comparator")
                    || !k.contains("far_input: None")
                {
                    continue;
                }
                let back =
                    backs.get(p).into_iter().flatten().map(|(from, loss)| {
                        strongest
                            .get(from)
                            .copied()
                            .unwrap_or(15)
                            .saturating_sub(*loss)
                    });
                let most = back.max().unwrap_or(0);
                if most < strongest[p] {
                    strongest.insert(*p, most);
                    lowered = true;
                }
            }
            if !lowered {
                return strongest;
            }
        }
    }
}

/// The circuit of `blocks`; see [`Compiled`].
pub fn compiled(blocks: &Blocks) -> Result<Compiled> {
    let mut world = world(blocks)?;
    let size = world_size(blocks);
    settle_world(&mut world, size)?;
    let [x, y, z] = size;
    let input = CompilerInput {
        world: &world,
        bounds: (BlockPos::new(0, 0, 0), BlockPos::new(x - 1, y - 1, z - 1)),
        ticks: &[],
    };
    let options = CompilerOptions::default();
    let graph = std::panic::catch_unwind(AssertUnwindSafe(|| {
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
    })?;
    let origin = at([0, 0, 0]);
    let pos = |node: &mchprs_redpiler::compile_graph::CompileNode| {
        node.block
            .first()
            .map(|&(p, _)| [p.x - origin.x, p.y - origin.y, p.z - origin.z])
    };
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
            let (pos, button) = match switch {
                Some((pos, "minecraft:lever")) => (pos, false),
                Some((pos, "minecraft:stone_button")) => (pos, true),
                _ => bail!("A bit of input {} has no lever.", port.name),
            };
            Ok(Lever {
                pos: at(pos),
                on: false,
                button,
                power: 15,
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
    // Any other full block that carries power, such as wool, terracotta,
    // or stone, does as concrete does.
    let id = if !known
        && !crate::colour::is_component(name)
        && crate::dust::solid(state)
    {
        "minecraft:white_concrete"
    } else {
        id
    };
    let known = known || id == "minecraft:white_concrete";
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
