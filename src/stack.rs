//! Copies of a stackable build joined into one schematic, as a player
//! stacks it in game:
//!
//! 1. Break the parts only the first copy keeps: gray blocks and what rests
//!    on them, such as logic the least significant bit alone needs or a
//!    barrel giving the strength it alone takes; the inputs the copy before
//!    feeds, such as a carry in; and the data inputs and outputs that would
//!    land in the copy before, whose buses run on through it; each with its
//!    levers, signs, and the dust on it. See `first_copy_only` and
//!    `shared_inputs`.
//! 2. `//rstack` what is left: copy 0 sits at the south-east-bottom corner
//!    and each later copy one step further along, pasted over the copies
//!    before it, so copies may overlap, and where they do, the later
//!    copy's block stands, though air never replaces anything and dust
//!    that meets joins. A build may rely on that, as a carry out capping
//!    one copy is covered by the next.
//! 3. Put the parts from step 1 back on the first copy.
//! 4. Turn every copy's labelled outputs and status lines but the last
//!    copy's white, signed or not, and take off their signs, since the
//!    next copy takes them over.
//!
//! A later copy's solid block on a different solid block of the copy
//! before, one of them an input or output, means the step is wrong, and is
//! refused. Several directions stack one after another.

use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

use anyhow::{Result, ensure};

use crate::{
    interface, lint,
    models::schematic::StackDirection,
    schem::{self, AIR, Blocks, DIRECTIONS, Pos},
};

/// The most copies along one direction.
const MAX_COPIES: usize = 64;
/// The most blocks in a stacked schematic.
const MAX_VOLUME: i64 = 16 * 1024 * 1024;

const WHITE: &str = "minecraft:white_concrete";

/// The categories whose copies, stacked across their bits, share their
/// data buses: more registers on one data bus, rather than more bits.
pub const SHARED_BUSES: [&str; 1] = ["register"];

/// What a build's copies do with its data buses, from its category and the
/// way its bits run. Copies along the bits are new bits, each with its own
/// data inputs and outputs; copies across a register's bits share its
/// buses, which only the first copy drives and reads.
#[derive(Clone, Copy)]
pub struct Buses<'a> {
    category: &'a str,
    orientation: &'a str,
}

impl<'a> Buses<'a> {
    pub fn of(category: &'a str, orientation: &'a str) -> Self {
        Buses {
            category,
            orientation,
        }
    }

    /// For a build whose every copy keeps its own buses, as an adder's.
    pub fn own() -> Buses<'static> {
        Buses {
            category: "",
            orientation: "",
        }
    }

    /// Whether copies stacked `direction` share the data buses.
    fn shared(self, direction: StackDirection) -> bool {
        SHARED_BUSES.contains(&self.category)
            && !along_bits(self.orientation, direction)
    }
}

/// Whether copies stacked `direction` of a build whose bits run
/// `orientation` add bits: west for a horizontal build, up for a vertical
/// one, west and up for a diagonal one.
fn along_bits(orientation: &str, direction: StackDirection) -> bool {
    matches!(
        (orientation, direction.name().as_str()),
        ("horz", "west") | ("vert", "up") | ("diag", "west-up")
    )
}

/// The stored file stacked with `copies` along each direction, with the
/// result's size, as [`interface::measured`] finds it.
pub fn stack(
    data: &[u8],
    copies: &[(StackDirection, usize)],
    buses: Buses,
) -> Result<(Vec<u8>, Pos)> {
    let mut data = data.to_vec();
    for &(direction, count) in copies {
        data = along(&data, direction, count, buses.shared(direction))?;
    }
    let (low, high) = interface::measured(&Blocks::read(&data)?);
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    Ok((data, size))
}

/// Stacks two copies each way, so a wrong step is refused when it is saved
/// rather than when someone downloads a stack. The `Err` is a sentence for
/// the person.
pub fn check(
    data: &[u8],
    directions: &[StackDirection],
    buses: Buses,
) -> Result<(), String> {
    for &direction in directions {
        along(data, direction, 2, buses.shared(direction)).map_err(|err| {
            format!("Two copies stacked {} don't fit: {err}.", direction.name())
        })?;
    }
    Ok(())
}

/// Pairs each of `directions` with its count in `wanted`, keyed by
/// direction name such as `west`. A direction left out gets one copy.
pub fn plan<S: BuildHasher>(
    directions: &[StackDirection],
    wanted: &HashMap<String, usize, S>,
) -> Result<Vec<(StackDirection, usize)>> {
    for name in wanted.keys() {
        ensure!(
            directions.iter().any(|d| d.name() == *name),
            "this build does not stack {name}"
        );
    }
    Ok(directions
        .iter()
        .map(|&d| (d, wanted.get(&d.name()).copied().unwrap_or(1)))
        .collect())
}

/// `copies` copies of the file along one direction, sharing its data buses
/// if `shared`.
fn along(
    data: &[u8],
    direction: StackDirection,
    copies: usize,
    shared: bool,
) -> Result<Vec<u8>> {
    ensure!(copies >= 1, "a stack needs at least one copy");
    // Anyone can ask for a stack, so cap the work one request can make.
    ensure!(
        copies <= MAX_COPIES,
        "a stack has at most {MAX_COPIES} copies each way"
    );
    if let Some(max) = direction.max_copies {
        ensure!(
            i64::try_from(copies)? <= max,
            "this build stacks at most {max} times {}",
            direction.name()
        );
    }
    if copies == 1 {
        return Ok(data.to_vec());
    }
    let blocks = Blocks::read(data)?;
    let [west, up, north] =
        [direction.west, direction.up, direction.north].map(i32::try_from);
    let (west, up, north) = (west?, up?, north?);
    let last = i32::try_from(copies - 1)?;
    let step = [-west, up, -north];
    let [w, h, l] = blocks.size;
    let size = [w + last * west, h + last * up, l + last * north];
    let volume: i64 = size.iter().map(|&n| i64::from(n)).product();
    ensure!(volume <= MAX_VOLUME, "that stack would be too large");
    // Where copy `i`'s blocks move to in the stacked box.
    let shift = |i: i32| [(last - i) * west, i * up, (last - i) * north];

    let left_out = first_copy(&blocks, step, shared).all;
    let outputs = labelled_outputs(&blocks);

    // As in game: the parts only the first copy keeps are broken, the rest
    // stacked as `//rstack` stacks it, and the parts put back on the first
    // copy, over anything a later copy put there.
    let mut grid: HashMap<Pos, String> = HashMap::new();
    for i in 0..=last {
        let final_copy = i == last;
        for (pos, state) in blocks.iter() {
            if state == AIR
                || left_out.contains(&pos)
                || (!final_copy && outputs.signs.contains(&pos))
            {
                continue;
            }
            // A later copy's data blocks on a shared bus are plain blocks.
            let plain = (!final_copy && outputs.blocks.contains(&pos))
                || (shared && i > 0 && bus(short(state)));
            let state = if plain { WHITE } else { state };
            place(&mut grid, add(pos, shift(i)), state);
        }
    }
    for (pos, state) in blocks.iter() {
        if state != AIR
            && left_out.contains(&pos)
            && !(last > 0 && outputs.signs.contains(&pos))
        {
            let state = if last > 0 && outputs.blocks.contains(&pos) {
                WHITE
            } else {
                state
            };
            place(&mut grid, add(pos, shift(0)), state);
        }
    }

    let mut nbt = schem::decode(data)?;
    let entities = nbt.take_entities();
    // Where copies overlap, a later copy's entity stands, as its block
    // does, and one whose block a later copy covered goes with it.
    let mut placed: HashMap<Pos, (String, fastnbt::Value)> = HashMap::new();
    // Every copy without the first copy's own parts, then those put back.
    let passes = (0..=last).map(|i| (i, false)).chain([(0, true)]);
    for (i, restoring) in passes {
        for entity in &entities {
            let Some(pos) = schem::entity_pos(entity) else {
                continue;
            };
            let dropped = left_out.contains(&pos) != restoring
                || (i != last && outputs.signs.contains(&pos));
            if !dropped {
                let to = add(pos, shift(i));
                let block = schem::id(blocks.get(pos)).to_owned();
                placed.insert(to, (block, schem::with_pos(entity, to)));
            }
        }
    }
    let stacked_entities = placed
        .into_iter()
        .filter(|(pos, (block, _))| {
            grid.get(pos).is_some_and(|state| schem::id(state) == block)
        })
        .map(|(_, (_, entity))| entity)
        .collect();
    // The first copy stays at the south-east-bottom end and the rest go
    // west, north, and up, so the box's first block moved by this.
    let low = [-(last * west), 0, -(last * north)];
    nbt.write_blocks(low, size, &grid, stacked_entities)?;
    schem::encode(nbt)
}

/// The step a copy stacked `direction` sits from the one before, in the
/// file's coordinates: west and north count down, up counts up.
fn step_of(direction: StackDirection) -> Result<Pos> {
    let [west, up, north] =
        [direction.west, direction.up, direction.north].map(i32::try_from);
    Ok([-west?, up?, -north?])
}

/// What stacking a build one way looks like, for its builder to check:
/// the parts only its first copy keeps, the spots where two copies hold
/// different blocks, and whether two copies make a bigger build of the
/// same shape.
pub struct Review {
    pub direction: StackDirection,
    pub first_copy: FirstCopy,
    pub disagreements: Vec<Disagreement>,
    /// Why two copies do not make a build of the same shape, if not.
    pub shape: Option<String>,
}

/// A spot where two copies overlap with different blocks, in the copy
/// before's coordinates: its block, and the later copy's, which stands.
/// Each is named as a player would, such as `barrel of strength 1`.
pub struct Disagreement {
    pub pos: Pos,
    pub earlier: String,
    pub later: String,
}

/// Reviews the build in `data` stacked `direction`.
pub fn review(
    data: &[u8],
    direction: StackDirection,
    buses: Buses,
) -> Result<Review> {
    let blocks = Blocks::read(data)?;
    let step = step_of(direction)?;
    let shared = buses.shared(direction);
    let first_copy = first_copy(&blocks, step, shared);
    let disagreements = disagreements(&blocks, step, &first_copy);
    let along = along_bits(buses.orientation, direction);
    let shape = shape(data, &blocks, direction, along, shared)?;
    Ok(Review {
        direction,
        first_copy,
        disagreements,
        shape,
    })
}

/// The spots where a later copy, a `step` along, puts a block over a
/// different one of the copy before, each without the parts only the first
/// copy keeps and with labelled outputs white, as stacking leaves them:
/// another block, or a container giving another signal strength. Dust
/// that meets joins, so it is no disagreement.
fn disagreements(
    blocks: &Blocks,
    step: Pos,
    first_copy: &FirstCopy,
) -> Vec<Disagreement> {
    let outputs = labelled_outputs(blocks);
    let within = |pos: Pos| {
        (0..3).all(|axis| (0..blocks.size[axis]).contains(&pos[axis]))
    };
    let stacked = |pos: Pos| {
        if outputs.blocks.contains(&pos) {
            WHITE
        } else {
            blocks.get(pos)
        }
    };
    let strength = |pos: Pos, state: &str| {
        let items = blocks.containers.get(&pos).map_or(&[][..], Vec::as_slice);
        schem::slots(state).map(|slots| schem::container_strength(items, slots))
    };
    let named = |pos: Pos, state: &str| match strength(pos, state) {
        Some(strength) => {
            format!("{} of strength {strength}", short(state).replace('_', " "))
        }
        None => short(state).replace('_', " "),
    };
    blocks
        .iter()
        .filter_map(|(pos, _)| {
            let at = add(pos, step);
            if first_copy.all.contains(&pos)
                || first_copy.all.contains(&at)
                || !within(at)
            {
                return None;
            }
            let (later, earlier) = (stacked(pos), stacked(at));
            let dust = |state: &str| short(state) == "redstone_wire";
            // A container is told by its kind and the strength it gives.
            let other = match (strength(at, earlier), strength(pos, later)) {
                (Some(a), Some(b)) => {
                    schem::id(earlier) != schem::id(later) || a != b
                }
                _ => replaces(earlier, later),
            };
            let differ = later != AIR
                && earlier != AIR
                && !(dust(later) && dust(earlier))
                && other;
            differ.then(|| Disagreement {
                pos: at,
                earlier: named(at, earlier),
                later: named(pos, later),
            })
        })
        .collect()
}

/// Why two copies of `data`, `blocks`, stacked `direction` do not make a
/// bigger build of its shape, if not: copies the way its bits run should
/// double each data bus, and copies any other way keep each bus's width,
/// or each tap it, and, if it has control lines, add their own.
fn shape(
    data: &[u8],
    blocks: &Blocks,
    direction: StackDirection,
    along_bits: bool,
    shared: bool,
) -> Result<Option<String>> {
    let control = format!("{} concrete", lint::CONTROL);
    let buses = |interface: &interface::Interface| -> Vec<(String, usize)> {
        let inputs = interface
            .inputs
            .iter()
            .filter(|port| !port.labelled && port.block != control);
        let outputs = interface.outputs.iter().filter(|port| {
            !port.labelled && !port.block.starts_with(lint::STATUS)
        });
        inputs
            .chain(outputs)
            .map(|port| (port.name.clone(), port.bits.len()))
            .collect()
    };
    let lines = |interface: &interface::Interface| -> usize {
        interface
            .inputs
            .iter()
            .filter(|port| port.block == control)
            .map(|port| port.bits.len())
            .sum()
    };
    let single = interface::read(blocks);
    let two =
        interface::read(&Blocks::read(&along(data, direction, 2, shared)?)?);
    let way = direction.name().replace('-', " and ");
    for (name, bits) in buses(&single) {
        let stacked = buses(&two)
            .into_iter()
            .find(|(other, _)| *other == name)
            .map_or(0, |(_, bits)| bits);
        // Copies another way may share a bus or each tap it.
        let fits = if along_bits {
            stacked == 2 * bits
        } else {
            stacked == bits || stacked == 2 * bits
        };
        let wanted = if along_bits { 2 * bits } else { bits };
        if !fits {
            return Ok(Some(format!(
                "Two copies stacked {way} should give {name} {wanted} bits, \
                 but it has {stacked}."
            )));
        }
    }
    if !along_bits && lines(&single) > 0 && lines(&two) <= lines(&single) {
        return Ok(Some(format!(
            "Two copies stacked {way} share every control line, so the \
             second cannot be told apart from the first."
        )));
    }
    Ok(None)
}

/// The build in `data` without the parts only its first copy keeps when
/// stacked `direction`, as the stacking procedure's first step leaves it,
/// or with `parts` those parts alone, to put back after `//rstack`.
pub fn part(
    data: &[u8],
    direction: StackDirection,
    parts: bool,
    buses: Buses,
) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let first_copy =
        first_copy(&blocks, step_of(direction)?, buses.shared(direction));
    let kept = |pos: &Pos| first_copy.all.contains(pos) == parts;
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(pos, state)| *state != AIR && kept(pos))
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let mut nbt = schem::decode(data)?;
    let entities = nbt
        .take_entities()
        .into_iter()
        .filter(|entity| {
            schem::entity_pos(entity).is_some_and(|pos| kept(&pos))
        })
        .collect();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)?;
    schem::encode(nbt)
}

/// The inputs a later copy leaves out, a `step` along, since the copy
/// before feeds them: a control line that lands within it where it could
/// be fed, on a block or beside redstone, such as a carry in beside the
/// dust carrying the carry out it continues, and an input block of a layer outside the build's core, as
/// [`interface::core`] finds it, that lands in the core of the copy before,
/// as a memory's input layer does; with the levers, buttons, and signs on
/// them. A torch on one stays where the copy before has a solid block in
/// its place to hang on, as one inverting a carry in hangs on the carry out
/// it continues. A control line that lands beyond the copy before, or on
/// air with no redstone around in it, as a register stacked into more
/// registers keeps its own, stays, as does an
/// adder's next bit, which lands in its own input layer.
///
/// So does an input or output block of a bus outside the build as it is
/// measured, less its outer layers of interface and outputs, that lands
/// within the copy before as measured, or one that lands on any block of
/// the copy before, as on the block or dust carrying its data, with the
/// lever on it and the dust, repeater,
/// or comparator standing on it: the bus runs on through the copy before,
/// and is driven and read only at the first copy's end, as a register
/// file's data buses are.
fn shared_inputs(
    blocks: &Blocks,
    step: Pos,
    shared: bool,
) -> (Vec<Pos>, Vec<Pos>) {
    let (low, high) = interface::core(blocks);
    let in_core = |pos: Pos| {
        (0..3).all(|axis| (low[axis]..=high[axis]).contains(&pos[axis]))
    };
    let (low_m, high_m) = interface::measured(blocks);
    let in_measured = |pos: Pos| {
        (0..3).all(|axis| (low_m[axis]..=high_m[axis]).contains(&pos[axis]))
    };
    let within = |pos: Pos| {
        (0..3).all(|axis| (0..blocks.size[axis]).contains(&pos[axis]))
    };
    let (mut inputs, mut buses) = (Vec::new(), Vec::new());
    for (pos, state) in blocks.iter() {
        let name = short(state);
        let landing = add(pos, step);
        if shared
            && bus(name)
            && ((!in_measured(pos) && in_measured(landing))
                || (within(landing) && blocks.get(landing) != AIR))
        {
            buses.push(pos);
        } else if (name == format!("{}_concrete", lint::CONTROL)
            && within(landing)
            && fed(blocks, landing))
            || (lint::is_input(name) && !in_core(pos) && in_core(landing))
        {
            inputs.push(pos);
        }
    }
    (inputs, buses)
}

/// The blocks resting on `marks`, which go with them, a `step` along: the
/// dust, repeater, or comparator carrying an output out, the levers,
/// buttons, and signs on them, and a torch on one where the copy before
/// has no solid block in its place to hang on.
fn resting_on(
    blocks: &Blocks,
    inputs: &HashSet<Pos>,
    step: Pos,
) -> HashSet<Pos> {
    let mut shared = HashSet::new();
    // What carries an output out stands on it, and goes with it.
    shared.extend(blocks.iter().filter_map(|(pos, state)| {
        let below = schem::step(pos, "down");
        (matches!(short(state), "redstone_wire" | "repeater" | "comparator")
            && inputs.contains(&below)
            && lint::is_output(short(blocks.get(below))))
        .then_some(pos)
    }));
    shared.extend(
        blocks
            .switches()
            .into_iter()
            .filter(|switch| inputs.contains(&switch.block))
            .map(|switch| switch.pos),
    );
    shared.extend(blocks.signs.keys().copied().filter(|&sign| {
        blocks
            .sign_block(sign)
            .is_some_and(|on| inputs.contains(&on))
    }));
    shared.extend(blocks.iter().filter_map(|(pos, state)| {
        let on = schem::torch_block(pos, state)?;
        (inputs.contains(&on) && !solid(blocks.get(add(on, step))))
            .then_some(pos)
    }));
    shared
}

/// The parts only the first copy keeps when copies stack a `step` along,
/// which the stacking procedure breaks before `//rstack` and puts back on
/// the first copy after.
pub struct FirstCopy {
    /// How many gray blocks.
    pub gray: usize,
    /// The inputs the copy before feeds, such as a carry in.
    pub inputs: Vec<Pos>,
    /// The data input and output blocks whose buses run on through the
    /// copy before.
    pub buses: Vec<Pos>,
    /// The levers on data inputs, when copies share the buses, which only
    /// the first copy's drive.
    pub levers: usize,
    /// Every block broken: these, and what rests on them.
    pub all: HashSet<Pos>,
}

impl FirstCopy {
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}

fn first_copy(blocks: &Blocks, step: Pos, shared: bool) -> FirstCopy {
    let gray = first_copy_only(blocks);
    let (inputs, buses) = shared_inputs(blocks, step, shared);
    let marks: HashSet<Pos> = inputs.iter().chain(&buses).copied().collect();
    let mut all = resting_on(blocks, &marks, step);
    all.extend(&marks);
    all.extend(&gray);
    // On a shared bus, only the first copy's data levers drive it.
    let mut levers = 0;
    if shared {
        for switch in blocks.switches() {
            if bus(short(blocks.get(switch.block))) && all.insert(switch.pos) {
                levers += 1;
            }
        }
    }
    FirstCopy {
        gray: blocks
            .iter()
            .filter(|&(_, state)| lint::first_copy_only(short(state)))
            .count(),
        inputs,
        buses,
        levers,
        all,
    }
}

/// Whether a block id without its namespace marks a bit of a bus: an
/// operand's input colour, other than a control line, or an output.
fn bus(name: &str) -> bool {
    (lint::is_input(name) && name != format!("{}_concrete", lint::CONTROL))
        || lint::is_output(name)
}

/// Whether something at `pos` in the copy before could feed a later
/// copy's control line landing there: a block in its place, or redstone
/// beside it, as the dust carrying a carry out to the next copy's carry
/// in. A line landing on air with no redstone around is the copy's own.
fn fed(blocks: &Blocks, pos: Pos) -> bool {
    let redstone = |state: &str| {
        matches!(
            short(state),
            "redstone_wire"
                | "repeater"
                | "comparator"
                | "redstone_torch"
                | "redstone_wall_torch"
                | "observer"
        )
    };
    blocks.get(pos) != AIR
        || DIRECTIONS
            .iter()
            .any(|&side| redstone(blocks.get(schem::step(pos, side))))
}

/// The blocks only the first copy keeps: gray blocks, and the dust,
/// torches, repeaters, comparators, levers, buttons, signs, and containers
/// such as barrels resting on them.
fn first_copy_only(blocks: &Blocks) -> HashSet<Pos> {
    let mut only: HashSet<Pos> = blocks
        .iter()
        .filter(|&(_, state)| lint::first_copy_only(short(state)))
        .map(|(pos, _)| pos)
        .collect();
    let mut resting: Vec<(Pos, Pos)> = blocks
        .iter()
        .filter_map(|(pos, state)| {
            let on = match short(state) {
                "redstone_wire" | "repeater" | "comparator" | "barrel"
                | "hopper" | "furnace" => Some(schem::step(pos, "down")),
                _ => schem::torch_block(pos, state),
            };
            Some((pos, on?))
        })
        .collect();
    resting.extend(blocks.switches().into_iter().map(|s| (s.pos, s.block)));
    resting.extend(
        blocks
            .signs
            .keys()
            .filter_map(|&sign| Some((sign, blocks.sign_block(sign)?))),
    );
    let on_them: Vec<Pos> = resting
        .into_iter()
        .filter(|(_, on)| only.contains(on))
        .map(|(pos, _)| pos)
        .collect();
    only.extend(on_them);
    only
}

struct Outputs {
    blocks: HashSet<Pos>,
    signs: HashSet<Pos>,
}

/// The blocks of labelled outputs and status lines, such as a carry out,
/// signed or not, and their signs.
fn labelled_outputs(blocks: &Blocks) -> Outputs {
    let interface = interface::read(blocks);
    let outputs: HashSet<Pos> = interface
        .outputs
        .iter()
        .filter(|port| port.labelled || port.block.starts_with(lint::STATUS))
        .flat_map(|port| port.bits.iter().map(|bit| bit.pos))
        .collect();
    let signs = blocks
        .signs
        .keys()
        .copied()
        .filter(|&sign| {
            blocks
                .sign_block(sign)
                .is_some_and(|b| outputs.contains(&b))
        })
        .collect();
    Outputs {
        blocks: outputs,
        signs,
    }
}

/// Puts `state` at `pos`, over what an earlier copy put there, as
/// `//rstack` does, or a player putting back a part only the first copy
/// keeps: dust that meets joins every way either joined. Where the two
/// differ otherwise, `review` says so.
fn place(grid: &mut HashMap<Pos, String>, pos: Pos, state: &str) {
    let placed = match grid.get(&pos) {
        Some(existing)
            if short(existing) == "redstone_wire"
                && short(state) == "redstone_wire" =>
        {
            joined(existing, state)
        }
        _ => state.to_owned(),
    };
    grid.insert(pos, placed);
}

/// Whether `state` is a solid block rather than a component standing on
/// or beside one, such as dust, a torch, or a lever.
fn solid(state: &str) -> bool {
    let name = short(state);
    !(name == "air"
        || name == "redstone_wire"
        || name == "repeater"
        || name == "comparator"
        || name.ends_with("torch")
        || name == "lever"
        || name.ends_with("_button")
        || name.ends_with("sign")
        || name == "tripwire_hook")
}

/// Whether `state` would replace a different block at `existing`: anything
/// but air or the same block, though in another state of power, which the
/// game works out again.
fn replaces(existing: &str, state: &str) -> bool {
    existing != AIR && unpowered(existing) != unpowered(state)
}

/// A block state without what depends on its power or neighbours: dust's
/// strength and joins, and whether a component is powered or lit.
fn unpowered(state: &str) -> String {
    if short(state) == "redstone_wire" {
        return schem::id(state).to_owned();
    }
    let Some((id, properties)) = state.split_once('[') else {
        return state.to_owned();
    };
    let kept: Vec<&str> = properties
        .trim_end_matches(']')
        .split(',')
        .filter(|p| !p.starts_with("powered=") && !p.starts_with("lit="))
        .collect();
    format!("{id}[{}]", kept.join(","))
}

/// Dust joined every way either `a` or `b` joins, `up` over `side` over
/// `none`, at the greater strength.
fn joined(a: &str, b: &str) -> String {
    let rank = |join: &str| match join {
        "up" => 2,
        "side" => 1,
        _ => 0,
    };
    let joins = ["north", "south", "east", "west"].map(|side| {
        let (x, y) = (
            schem::property(a, side).unwrap_or("none"),
            schem::property(b, side).unwrap_or("none"),
        );
        format!("{side}={}", if rank(y) > rank(x) { y } else { x })
    });
    let power = |s: &str| {
        schem::property(s, "power").and_then(|p| p.parse::<u8>().ok())
    };
    let power = power(a).max(power(b)).unwrap_or(0);
    let [north, south, east, west] = joins;
    format!(
        "minecraft:redstone_wire[{east},{north},power={power},{south},{west}]"
    )
}

/// A block's id without its namespace, such as `redstone_wire`.
fn short(state: &str) -> &str {
    let id = schem::id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dust_where_copies_meet_joins_both_ways() {
        let below = "minecraft:redstone_wire[east=up,north=none,power=4,\
                     south=side,west=side]";
        let above = "minecraft:redstone_wire[east=up,north=none,power=0,\
                     south=none,west=up]";
        assert!(!replaces(below, above));
        assert_eq!(
            joined(below, above),
            "minecraft:redstone_wire[east=up,north=none,power=4,south=side,\
             west=up]"
        );
    }

    #[test]
    fn only_another_block_collides() {
        let lit = "minecraft:redstone_wall_torch[facing=north,lit=true]";
        let unlit = "minecraft:redstone_wall_torch[facing=north,lit=false]";
        assert!(!replaces(lit, unlit));
        assert!(!replaces(AIR, WHITE));
        assert!(replaces(WHITE, unlit));
        assert!(replaces(
            "minecraft:repeater[delay=1,facing=west]",
            "minecraft:repeater[delay=1,facing=east]"
        ));
    }
}
