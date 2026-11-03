//! Copies of a stackable build joined into one schematic, as a player
//! stacks it in game. A build stacks by its box: the build less the layers
//! on the face its stack starts from, the east for a stack west and the
//! bottom for one up, that only the first copy needs, each block of which is interface, an input or output and what is
//! on it, or is what the copy before already has where it lands. See
//! `first_copy`. Where its copies don't work, a box leaving out more of
//! the build may, as only testing them tells; see `find_box`. Such a box
//! is stored with the direction, as its `faces`.
//!
//! 1. Paste the first copy whole.
//! 2. `//rstack` the box: each later copy one step further along, pasted
//!    over the copies before it, so copies may overlap, and where they do,
//!    the later copy's block stands, though air never replaces anything
//!    and dust that meets joins. A build may rely on that, as a carry out
//!    capping one copy is covered by the next.
//! 3. Only roles change, every block staying as placed: every copy's
//!    labelled outputs and status lines but the last copy's lose theirs,
//!    signed or not, their signs off, since the next copy takes them over,
//!    unless each copy keeps its own, as a monostable's lone output is its
//!    own bit's; a later copy's input block whose lever stayed with the
//!    first copy loses its role; and so do a later copy's data blocks on a
//!    shared bus, read only at the first copy's end.
//!
//! Where two copies hold different blocks, `review` says so, and finds the
//! shortest step at which they do not. Several directions stack one after
//! another. Copies stack west or up, never north, east, or down, and the
//! site's tests count copies as stacking only where they touch or
//! overlap, each box reaching the next; see [`touches`].

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::BuildHasher,
};

use anyhow::{Result, ensure};

use crate::{
    category::{self, Behaviour, Kind},
    models::schematic::{Faces, Orientation, StackDirection},
    recognize::{interface, repeater_outputs},
    schem::{
        self, AIR, Blocks, Direction, Pos,
        lines::{Role, Roles},
    },
};

/// The most copies along one direction.
const MAX_COPIES: usize = 64;
/// The most blocks in a stacked schematic.
const MAX_VOLUME: i64 = 16 * 1024 * 1024;

const GLASS: &str = "minecraft:glass";

/// What a build's copies do with its data buses, from its category and the
/// way its bits run. Copies along the bits are new bits, each with its own
/// data inputs and outputs; copies across a register's bits share its
/// buses, which only the first copy drives and reads.
#[derive(Clone, Copy)]
pub struct Buses {
    /// Whether the category's copies across its bits share its buses, as
    /// `category::Copies::SharedBuses` says.
    shared: bool,
    /// The way the build's bits run; `None` when it does not matter.
    orientation: Option<Orientation>,
    /// Whether each copy keeps its labelled outputs and status lines, as
    /// `category::Copies::OwnOutputs` says.
    own_outputs: bool,
}

impl Buses {
    /// For a build of `kind`, or `None` a category off the list, its bits
    /// running `orientation`.
    pub fn of(kind: impl Into<Option<Kind>>, orientation: Orientation) -> Self {
        let behaviour = Behaviour::of(kind);
        Buses {
            shared: behaviour.copies == category::Copies::SharedBuses,
            orientation: Some(orientation),
            own_outputs: behaviour.copies == category::Copies::OwnOutputs,
        }
    }

    /// For a build whose every copy keeps its own buses, as an adder's.
    pub fn own() -> Buses {
        Buses {
            shared: false,
            orientation: None,
            own_outputs: false,
        }
    }

    /// Whether copies stacked `direction` add bits, running the way the
    /// build's bits do.
    fn along_bits(self, direction: StackDirection) -> bool {
        self.orientation
            .is_some_and(|orientation| direction.adds_bits(orientation))
    }

    /// Whether copies stacked `direction` share the data buses.
    fn shared(self, direction: StackDirection) -> bool {
        self.shared && !self.along_bits(direction)
    }
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
        data = along(&data, direction, count, buses)?;
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
        along(data, direction, 2, buses).map_err(|err| {
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
/// and outputs as `buses` says.
fn along(
    data: &[u8],
    direction: StackDirection,
    copies: usize,
    buses: Buses,
) -> Result<Vec<u8>> {
    let shared = buses.shared(direction);
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
    let [west, up] = [direction.west, direction.up].map(i32::try_from);
    let (west, up) = (west?, up?);
    let last = i32::try_from(copies - 1)?;
    let step = [-west, up, 0];
    let [w, h, l] = blocks.size;
    let size = [w + last * west, h + last * up, l];
    let volume: i64 = size.iter().map(|&n| i64::from(n)).product();
    ensure!(volume <= MAX_VOLUME, "that stack would be too large");
    // Where copy `i`'s blocks move to in the stacked box.
    let shift = |i: i32| [(last - i) * west, i * up, 0];

    let left_out = first_copy(&blocks, step, direction.faces).all;
    let orphaned = orphaned(&blocks, &left_out);
    let outputs = if buses.own_outputs {
        Outputs {
            blocks: HashSet::new(),
            signs: HashSet::new(),
        }
    } else {
        labelled_outputs(&blocks)
    };

    // As in game: the first copy whole, then each later copy's box over
    // the copies before, where its block stands, with its role.
    let mut grid: HashMap<Pos, String> = HashMap::new();
    let mut roles = BTreeMap::new();
    for i in 0..=last {
        let final_copy = i == last;
        for (pos, state) in blocks.iter() {
            if state == AIR
                || (i > 0 && left_out.contains(&pos))
                || (!final_copy && outputs.signs.contains(&pos))
            {
                continue;
            }
            let role = blocks.roles.as_ref().and_then(|roles| roles.get(pos));
            let plain = (!final_copy && outputs.blocks.contains(&pos))
                || (i > 0 && orphaned.contains(&pos))
                || (shared && i > 0 && role.is_some_and(bus));
            let to = schem::add(pos, shift(i));
            place(&mut grid, to, state);
            match role.filter(|_| !plain) {
                Some(role) => roles.insert(to, role),
                None => roles.remove(&to),
            };
        }
    }

    let mut nbt = schem::decode(data)?;
    if blocks.roles.is_some() {
        nbt.set_roles(Some(&Roles(roles)));
    }
    let entities = nbt.take_entities();
    // Where copies overlap, a later copy's entity stands, as its block
    // does, and one whose block a later copy covered goes with it.
    let mut placed: HashMap<Pos, (String, fastnbt::Value)> = HashMap::new();
    for i in 0..=last {
        for entity in &entities {
            let Some(pos) = schem::entity_pos(entity) else {
                continue;
            };
            let dropped = (i > 0 && left_out.contains(&pos))
                || (i != last && outputs.signs.contains(&pos));
            if !dropped {
                let to = schem::add(pos, shift(i));
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
    // The first copy stays at the east-bottom end and the rest go west and
    // up, so the box's first block moved by this.
    let low = [-(last * west), 0, 0];
    nbt.write_blocks(low, size, &grid, stacked_entities)?;
    schem::encode(nbt)
}

/// The input blocks whose switch or comparator only the first copy keeps,
/// one of `left_out`, which in a later copy are plain blocks, with no
/// role.
fn orphaned(blocks: &Blocks, left_out: &HashSet<Pos>) -> HashSet<Pos> {
    blocks
        .switches()
        .into_iter()
        .filter(|switch| left_out.contains(&switch.pos))
        .map(|switch| switch.block)
        .chain(
            repeater_outputs::strength_inputs(blocks)
                .into_iter()
                .filter(|input| left_out.contains(&input.at))
                .map(|input| schem::step(input.at, Direction::Down)),
        )
        .collect()
}

/// The step a copy stacked `direction` sits from the one before, in the
/// file's coordinates: west counts down, up counts up.
fn step_of(direction: StackDirection) -> Result<Pos> {
    let [west, up] = [direction.west, direction.up].map(i32::try_from);
    Ok([-west?, up?, 0])
}

/// Whether copies of the build in `data` stacked `direction` touch or
/// overlap: the box later copies repeat reaches at least a step along
/// each way it steps, so no layer of air lies between one copy and the
/// next. Copies apart, however near, are separate builds side by side,
/// not a stack, but for one exception: a build 1 block wide along the
/// way it steps counts as 2 wide, so its copies 2 apart, a block of air
/// between them, stack. No wider build's copies may leave air between.
pub fn touches(data: &[u8], direction: StackDirection) -> Result<bool> {
    let blocks = Blocks::read(data)?;
    let step = step_of(direction)?;
    let [low, high] = first_copy(&blocks, step, direction.faces).sized;
    Ok((0..3).all(|axis| {
        let reach = if blocks.size[axis] == 1 {
            2
        } else {
            high[axis] - low[axis] + 1
        };
        reach >= step[axis].abs()
    }))
}

/// Whether copies of the build in `data` stacked `direction` are 2 apart
/// along a way the build is 1 block wide, so a block of air lies between
/// each and the next, and nothing between them to join them: as `touches`
/// has it, such a build counts as 2 wide.
pub fn one_wide_apart(data: &[u8], direction: StackDirection) -> Result<bool> {
    let blocks = Blocks::read(data)?;
    let step = step_of(direction)?;
    let stepped: Vec<usize> = (0..3).filter(|&axis| step[axis] != 0).collect();
    Ok(!stepped.is_empty()
        && stepped
            .iter()
            .all(|&axis| step[axis].abs() == 2 && blocks.size[axis] == 1))
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
    /// When copies this far apart disagree, the shortest step the same way
    /// at which they do not, if any, as trying `//rstack` at each would
    /// find.
    pub better: Option<i64>,
}

/// A spot where two copies overlap with different blocks, in the copy
/// before's coordinates: its block, and the later copy's, which stands.
/// Each is named as a player would, such as `barrel of strength 1`.
pub struct Disagreement {
    pub pos: Pos,
    pub earlier: String,
    pub later: String,
    /// Where the later copy's block is in its own copy, as in the file.
    pub from: Pos,
}

/// Reviews the build in `data` stacked `direction`.
pub fn review(
    data: &[u8],
    direction: StackDirection,
    buses: Buses,
) -> Result<Review> {
    let blocks = Blocks::read(data)?;
    let step = step_of(direction)?;
    let first_copy = first_copy(&blocks, step, direction.faces);
    let disagreements = disagreements(&blocks, step, &first_copy);
    let along = buses.along_bits(direction);
    let shape = shape(data, &blocks, direction, along, buses)?;
    let better = if disagreements.is_empty() {
        None
    } else {
        agreeing(&blocks, direction)
    };
    Ok(Review {
        direction,
        first_copy,
        disagreements,
        shape,
        better,
    })
}

/// The shortest step `direction`'s way, short of the build's length that
/// way, at which two copies overlap and hold no different blocks anywhere
/// they do.
fn agreeing(blocks: &Blocks, direction: StackDirection) -> Option<i64> {
    let (low, high) = interface::core(blocks);
    let [width, height] =
        [0, 1].map(|axis| i64::from(high[axis] - low[axis] + 1));
    let reach = [(direction.west > 0, width), (direction.up > 0, height)]
        .into_iter()
        .filter(|&(along, _)| along)
        .map(|(_, extent)| extent)
        .min()?;
    // Copies a whole length apart do not overlap, so agree trivially.
    (1..reach).find(|&n| {
        let at = |part: i64| if part > 0 { n } else { 0 };
        let candidate = StackDirection {
            west: at(direction.west),
            up: at(direction.up),
            max_copies: None,
            faces: None,
            pitch: None,
        };
        let Ok(step) = step_of(candidate) else {
            return false;
        };
        disagreements(blocks, step, &first_copy(blocks, step, None)).is_empty()
    })
}

/// The spots where a later copy, a `step` along, puts a block over a
/// different one of the copy before, each without the parts only the first
/// copy keeps, as stacking leaves them: another block, or a container
/// giving another signal strength. Dust that meets joins, so it is no
/// disagreement.
fn disagreements(
    blocks: &Blocks,
    step: Pos,
    first_copy: &FirstCopy,
) -> Vec<Disagreement> {
    let within = |pos: Pos| {
        (0..3).all(|axis| (0..blocks.size[axis]).contains(&pos[axis]))
    };
    let stacked = |pos: Pos| blocks.get(pos);
    let strength = |pos: Pos, state: &str| {
        let items = blocks.containers.get(&pos).map_or(&[][..], Vec::as_slice);
        schem::slots(state).map(|slots| schem::container_strength(items, slots))
    };
    let named = |pos: Pos, state: &str| match strength(pos, state) {
        Some(strength) => {
            format!(
                "{} of strength {strength}",
                schem::short(state).replace('_', " ")
            )
        }
        None => schem::short(state).replace('_', " "),
    };
    blocks
        .iter()
        .filter_map(|(pos, _)| {
            let at = schem::add(pos, step);
            if first_copy.all.contains(&pos)
                || first_copy.all.contains(&at)
                || !within(at)
            {
                return None;
            }
            let (later, earlier) = (stacked(pos), stacked(at));
            let dust = |state: &str| schem::short(state) == "redstone_wire";
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
                from: pos,
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
    stacking: Buses,
) -> Result<Option<String>> {
    let buses = |interface: &interface::Interface| -> Vec<(String, usize)> {
        let inputs = interface
            .inputs
            .iter()
            .filter(|port| !port.labelled && !port.control());
        let outputs = interface
            .outputs
            .iter()
            .filter(|port| !port.labelled && !port.status());
        inputs
            .chain(outputs)
            .map(|port| (port.name.clone(), port.bits.len()))
            .collect()
    };
    let lines = |interface: &interface::Interface| -> usize {
        interface
            .inputs
            .iter()
            .filter(|port| port.control())
            .map(|port| port.bits.len())
            .sum()
    };
    let single = interface::read(blocks);
    let two =
        interface::read(&Blocks::read(&along(data, direction, 2, stacking)?)?);
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
) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let first_copy = first_copy(&blocks, step_of(direction)?, direction.faces);
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

/// The build in `data` with a plain glass block at two opposite corners
/// of the box it `//rstack`s `direction`, for `WorldEdit` to select it by
/// before the glass is broken: the first pair of corners both empty, or
/// `None` when every pair holds a block.
pub fn with_markers(
    data: &[u8],
    direction: StackDirection,
) -> Result<Option<Vec<u8>>> {
    let blocks = Blocks::read(data)?;
    let [low, high] =
        first_copy(&blocks, step_of(direction)?, direction.faces).sized;
    let corner = |far: [bool; 3]| -> Pos {
        [0, 1, 2].map(|axis| if far[axis] { high[axis] } else { low[axis] })
    };
    let pairs = [
        [false, false, false],
        [true, false, false],
        [false, true, false],
        [false, false, true],
    ]
    .map(|far| (corner(far), corner(far.map(|f| !f))));
    let Some((a, b)) = pairs
        .into_iter()
        .find(|&(a, b)| blocks.get(a) == AIR && blocks.get(b) == AIR)
    else {
        return Ok(None);
    };
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    for pos in [a, b] {
        grid.insert(pos, GLASS.to_owned());
    }
    let mut nbt = schem::decode(data)?;
    let entities = nbt.take_entities();
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)?;
    Ok(Some(schem::encode(nbt)?))
}

/// The layers only the first copy keeps when copies stack a `step` along,
/// which later copies leave out, so each stacks the build's box: from the
/// face the stack starts from, the east for one west and the bottom for
/// one up, each layer whose every block is interface,
/// or is what the copy before already has where it lands, as placed, so
/// however either's power stood and dust on dust joining; never so many
/// that the box is shorter than the step.
pub struct FirstCopy {
    /// How many layers, along each way the stack steps.
    pub layers: usize,
    /// The box later copies stack, its least and greatest corner.
    pub sized: [Pos; 2],
    /// The levers and buttons among them.
    pub switches: usize,
    /// Every block in them.
    pub all: HashSet<Pos>,
}

impl FirstCopy {
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}

fn first_copy(blocks: &Blocks, step: Pos, faces: Option<Faces>) -> FirstCopy {
    if let Some(faces) = faces {
        let sized = faces.corners(blocks.size);
        let all: HashSet<Pos> = blocks
            .iter()
            .filter(|&(pos, state)| state != AIR && !inside(pos, sized))
            .map(|(pos, _)| pos)
            .collect();
        let layers = faces
            .low
            .iter()
            .chain(&faces.high)
            .map(|&n| usize::from(n))
            .sum();
        return FirstCopy {
            layers,
            sized,
            switches: switches_in(blocks, &all),
            all,
        };
    }
    let interface = interface_blocks(blocks);
    let mut all = HashSet::new();
    let mut layers = 0;
    let mut sized = [[0; 3], blocks.size.map(|n| n - 1)];
    for axis in (0..3).filter(|&axis| step[axis] != 0) {
        let size = blocks.size[axis];
        // The first copy sits behind each step.
        let layer = |n: i32| if step[axis] < 0 { size - 1 - n } else { n };
        let mut n = 0;
        while size - n > step[axis].abs() {
            let fits = blocks
                .iter()
                .filter(|&(pos, state)| pos[axis] == layer(n) && state != AIR)
                .all(|(pos, state)| {
                    let under = blocks.get(schem::add(pos, step));
                    interface.contains(&pos)
                        || schem::as_placed(under) == schem::as_placed(state)
                });
            if !fits {
                break;
            }
            all.extend(
                blocks
                    .iter()
                    .filter(|&(pos, state)| {
                        pos[axis] == layer(n) && state != AIR
                    })
                    .map(|(pos, _)| pos),
            );
            n += 1;
        }
        layers += usize::try_from(n).unwrap_or(0);
        if step[axis] < 0 {
            sized[1][axis] -= n;
        } else {
            sized[0][axis] += n;
        }
    }
    FirstCopy {
        layers,
        sized,
        switches: switches_in(blocks, &all),
        all,
    }
}

/// How many of `blocks`' levers and buttons are among `all`.
fn switches_in(blocks: &Blocks, all: &HashSet<Pos>) -> usize {
    blocks
        .switches()
        .iter()
        .filter(|switch| all.contains(&switch.pos))
        .count()
}

/// Whether `pos` is within the box with corners `corners`.
fn inside(pos: Pos, [low, high]: [Pos; 2]) -> bool {
    (0..3).all(|axis| low[axis] <= pos[axis] && pos[axis] <= high[axis])
}

/// The box copies stacked `direction` repeat, found by testing them with
/// `works`: the box rule's first, then, best first, boxes leaving out more
/// of it, the fewest blocks left out first, until one works. What only the
/// first copy keeps drives a control line, as a carry in's lever and what
/// it powers, so a box grows only off a face whose outer layer holds a
/// control line's lever or button, a layer at a time. A box whose two
/// copies lack data bits or control lines the stack needs, copies
/// `along_bits` doubling every data bus and copies across keeping it and
/// adding control lines, can only lack more with more left out, so nothing
/// beyond it is tried; nor is a box shorter than the step. The `Err` is
/// why the box rule's failed.
pub fn find_box<T, E>(
    data: &[u8],
    direction: StackDirection,
    along_bits: bool,
    buses: Buses,
    mut works: impl FnMut(StackDirection) -> Result<T, E>,
) -> Result<Result<(StackDirection, T), E>> {
    let blocks = Blocks::read(data)?;
    let step = step_of(direction)?;
    let rule = first_copy(&blocks, step, None).sized;
    let ruled = StackDirection {
        faces: None,
        ..direction
    };
    let first = match works(ruled) {
        Ok(found) => return Ok(Ok((ruled, found))),
        Err(why) => why,
    };
    let single = data_bits(&blocks);
    let wanted = if along_bits { 2 } else { 1 };
    // Whether two copies repeating this box lack what the stack needs.
    let lacking = |stacked: StackDirection| -> Result<bool> {
        let two = Blocks::read(&along(data, stacked, 2, buses)?)?;
        let (bits, lines) = data_bits(&two);
        Ok(bits < wanted * single.0 || (!along_bits && lines <= single.1))
    };
    let left_out = |corners: [Pos; 2]| -> Vec<Pos> {
        blocks
            .iter()
            .filter(|&(pos, state)| {
                state != AIR && inside(pos, rule) && !inside(pos, corners)
            })
            .map(|(pos, _)| pos)
            .collect()
    };
    // The faces, as an axis and its far end, holding a control line's
    // switch in their outer layer.
    let control = |pos: Pos| {
        blocks.roles.as_ref().and_then(|roles| roles.get(pos))
            == Some(Role::Control)
    };
    let faces: Vec<(usize, bool)> = (0..3)
        .flat_map(|axis| [(axis, false), (axis, true)])
        .filter(|&(axis, far)| {
            let edge = if far { blocks.size[axis] - 1 } else { 0 };
            blocks
                .switches()
                .iter()
                .any(|switch| switch.pos[axis] == edge && control(switch.block))
        })
        .collect();
    let mut queue = std::collections::BinaryHeap::new();
    let mut seen = HashSet::from([rule]);
    let mut tried = HashSet::new();
    let mut grow = |queue: &mut std::collections::BinaryHeap<_>,
                    corners: [Pos; 2]| {
        for &(axis, far) in &faces {
            let mut next = corners;
            if far {
                next[1][axis] -= 1;
            } else {
                next[0][axis] += 1;
            }
            // Copies a step apart leave no gap between their boxes.
            let short = (0..3).any(|axis| {
                next[1][axis] - next[0][axis] + 1 < step[axis].abs().max(1)
            });
            if !short && seen.insert(next) {
                let layers: i32 = (0..3)
                    .map(|axis| {
                        next[0][axis] - rule[0][axis] + rule[1][axis]
                            - next[1][axis]
                    })
                    .sum();
                let cost = left_out(next).len();
                queue.push(std::cmp::Reverse((cost, layers, next)));
            }
        }
    };
    grow(&mut queue, rule);
    while let Some(std::cmp::Reverse((_, _, corners))) = queue.pop() {
        // Boxes that leave out the same blocks stack alike.
        if !tried.insert(left_out(corners)) {
            continue;
        }
        let Some(faces) = Faces::of(corners, blocks.size) else {
            continue;
        };
        let stacked = StackDirection {
            faces: Some(faces),
            ..direction
        };
        if lacking(stacked)? {
            continue;
        }
        if let Ok(found) = works(stacked) {
            return Ok(Ok((stacked, found)));
        }
        grow(&mut queue, corners);
    }
    Ok(Err(first))
}

/// How many data bits a build's inputs and outputs have, and how many
/// control lines its inputs: every bit but those of a labelled port, a
/// status line, or a control line, which are counted apart.
fn data_bits(blocks: &Blocks) -> (usize, usize) {
    let interface = interface::read(blocks);
    let inputs = interface
        .inputs
        .iter()
        .filter(|port| !port.labelled && !port.control());
    let outputs = interface
        .outputs
        .iter()
        .filter(|port| !port.labelled && !port.status());
    let bits = inputs.chain(outputs).map(|port| port.bits.len()).sum();
    let lines = interface
        .inputs
        .iter()
        .filter(|port| port.control())
        .map(|port| port.bits.len())
        .sum();
    (bits, lines)
}

/// A build's interface by the rule for inputs and outputs, colour playing
/// no part: its input blocks, each lever's and button's and the block
/// under each comparator with air behind, with the comparator; the levers,
/// buttons, and signs on any block; the dust, repeaters, and comparators
/// standing on an input block; and each repeater or comparator facing air
/// with the block under it.
fn interface_blocks(blocks: &Blocks) -> HashSet<Pos> {
    let mut found: HashSet<Pos> = HashSet::new();
    for switch in blocks.switches() {
        found.insert(switch.pos);
        found.insert(switch.block);
    }
    for input in repeater_outputs::strength_inputs(blocks) {
        found.insert(input.at);
        found.insert(schem::step(input.at, Direction::Down));
    }
    let inputs = found.clone();
    found.extend(blocks.iter().filter_map(|(pos, state)| {
        (matches!(
            schem::short(state),
            "redstone_wire" | "repeater" | "comparator"
        ) && inputs.contains(&schem::step(pos, Direction::Down)))
        .then_some(pos)
    }));
    for output in repeater_outputs::find(blocks) {
        found.insert(output.at);
        found.insert(output.block());
    }
    for &sign in blocks.signs.keys() {
        found.insert(sign);
        found.extend(blocks.sign_block(sign));
    }
    found
}

/// Whether a block of `role` carries a bus's data, which copies sharing
/// their buses take from the first copy alone: any input but a control
/// line, or any output.
fn bus(role: Role) -> bool {
    role != Role::Control
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
        .filter(|port| port.labelled || port.status())
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
            if schem::short(existing) == "redstone_wire"
                && schem::short(state) == "redstone_wire" =>
        {
            joined(existing, state)
        }
        _ => state.to_owned(),
    };
    grid.insert(pos, placed);
}

/// Whether `state` would replace a different block at `existing`: anything
/// but air or the same block, though in another state of power, which the
/// game works out again, or a building block of the same kind, solid or
/// see-through, whatever its colour, as a copy's input block may be
/// coloured where the copy before has a plain one.
fn replaces(existing: &str, state: &str) -> bool {
    let kind = |state: &str| {
        let name = schem::short(state);
        if state == AIR
            || schem::block::is_component(name)
            || name.ends_with("sign")
            || schem::slots(state).is_some()
        {
            schem::as_placed(state)
        } else if schem::block::see_through(name) {
            "see-through".to_owned()
        } else {
            "solid".to_owned()
        }
    };
    existing != AIR && kind(existing) != kind(state)
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
        let white = "minecraft:white_concrete";
        assert!(!replaces(AIR, white));
        assert!(replaces(white, unlit));
        assert!(!replaces(white, "minecraft:lime_wool"));
        assert!(replaces(
            "minecraft:repeater[delay=1,facing=west]",
            "minecraft:repeater[delay=1,facing=east]"
        ));
    }
}
