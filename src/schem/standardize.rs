//! Shaping a build's file: fitting its box tightly and bringing a
//! downloaded build into the building conventions' palette.
//!
//! Standardizing needs the build's inputs and outputs already marked, since
//! everything else turns white:
//!
//! - Old corner markers are taken out: any group of plain glass that
//!   touches nothing but air, wherever it sits.
//! - A furnace, cauldron, or cake becomes a barrel giving a comparator the
//!   same signal strength.
//! - Slabs, stairs, and every kind of glass become white stained glass;
//!   every other solid block becomes white concrete. Redstone components
//!   and output blocks are kept, and so is an input: a coloured block with
//!   a lever on it, which becomes that colour's concrete, or input-colour
//!   concrete with dust on top, a signal-strength input.
//! - A status line, a block whose sign names a status output such as
//!   `carry out`, turns pink, as older builds marked them on yellow.
//! - A sign of any wood becomes an oak one, and a sign on a block that is
//!   neither an input nor an output, such as a note of the build's date,
//!   is taken out, as signs carry only labels. Redstone passes a sign by as
//!   it does air. A sign's commas are taken out, as its words are separated
//!   by spaces and lines alone.
//! - The box is fitted tightly around what is left.
//! - Each data input's lever is switched off, every other component coming
//!   to rest with it, as a data input never reads inverted, so its lever
//!   off is its 0. A control line's levers stay as they were, for its test
//!   to find which read inverted; see `levers_at_zero`. A build that cannot
//!   be settled, as one with a clock, keeps its levers as they were.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Result, ensure};

use crate::{
    recognize::{colour, interface},
    schem::{self, AIR, Blocks, Direction, Pos, block},
};

const WHITE: &str = "minecraft:white_concrete";
const WHITE_GLASS: &str = "minecraft:white_stained_glass";
const GLASS: &str = "minecraft:glass";

pub fn standardize(data: &[u8]) -> Result<Vec<u8>> {
    let swapped = barrels(data, &[])?;
    let data = swapped.as_slice();
    let blocks = Blocks::read(data)?;
    let old_markers = loose_glass(&blocks, false);
    let inputs: HashSet<Pos> = interface::feeds(&blocks).into_keys().collect();
    let status = status_lines(&blocks);
    let notes = notes(&blocks, &inputs, &status);
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(pos, state)| {
            *state != AIR && !old_markers.contains(pos) && !notes.contains(pos)
        })
        .map(|(pos, state)| {
            let state = if status.contains(&pos) {
                pink(state)
            } else {
                restyle(state, inputs.contains(&pos))
            };
            (pos, state)
        })
        .collect();
    let restyled = rebox(data, &grid, [0, 0, 0], blocks.size)?;
    let standardized = tidy(&restyled)?;

    let found = interface::read(&Blocks::read(&standardized)?);
    ensure!(
        !found.inputs.is_empty(),
        "no inputs found: mark each input with a lever on lime, light blue, \
         cyan, blue, or magenta concrete, or for a signal-strength input \
         dust on top, before standardizing"
    );
    ensure!(
        !found.outputs.is_empty(),
        "no outputs found: mark each output with yellow, orange, brown, or \
         pink concrete or stained glass before standardizing; if the file \
         should have some, check the selection it was saved from covers \
         the whole build"
    );
    Ok(data_levers_off(&standardized).unwrap_or(standardized))
}

/// The file with the lever of each data input, any input but a magenta
/// control line, switched off, every other lever as it was, and every
/// component at rest with them, as `with_levers` settles them. A file with
/// no data input's lever on comes back as it was.
pub fn data_levers_off(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let data_levers: BTreeSet<Pos> = interface::read(&blocks)
        .inputs
        .iter()
        .filter(|port| !port.block.starts_with(colour::CONTROL))
        .flat_map(|port| &port.bits)
        .filter_map(|bit| Some(schem::step(bit.pos, bit.entry?)))
        .filter(|&pos| schem::short(blocks.get(pos)) == "lever")
        .collect();
    let on: BTreeSet<Pos> = blocks
        .iter()
        .filter(|(_, state)| {
            schem::short(state) == "lever"
                && schem::property(state, "powered") == Some("true")
        })
        .map(|(pos, _)| pos)
        .collect();
    if on.is_disjoint(&data_levers) {
        return Ok(data.to_vec());
    }
    with_levers(data, &(&on - &data_levers))
}

/// The signs on a block that is neither an input, one of `inputs`, nor an
/// output or one of the `status` lines, so label nothing.
fn notes(
    blocks: &Blocks,
    inputs: &HashSet<Pos>,
    status: &HashSet<Pos>,
) -> HashSet<Pos> {
    blocks
        .signs
        .keys()
        .copied()
        .filter(|&sign| {
            blocks.sign_block(sign).is_none_or(|on| {
                !inputs.contains(&on)
                    && !status.contains(&on)
                    && !colour::is_output(schem::short(blocks.get(on)))
            })
        })
        .collect()
}

/// The file with each status line pink: a block with no lever whose sign
/// names a status output, such as `carry out`, as older builds marked them
/// on yellow. A file with none comes back as it was.
pub fn pink_status(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let status = status_lines(&blocks);
    if status.is_empty() {
        return Ok(data.to_vec());
    }
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| {
            let state = if status.contains(&pos) {
                pink(state)
            } else {
                state.to_owned()
            };
            (pos, state)
        })
        .collect();
    rebox(data, &grid, [0, 0, 0], blocks.size)
}

/// The blocks that are status lines but not yet pink: not inputs, and a
/// sign naming a status output.
fn status_lines(blocks: &Blocks) -> HashSet<Pos> {
    let inputs = interface::feeds(blocks);
    blocks
        .sign_texts()
        .into_iter()
        .filter(|(pos, lines)| {
            !inputs.contains_key(pos)
                && interface::sign_name(lines)
                    .is_some_and(|name| colour::is_status_name(&name))
                && !blocks.get(*pos).starts_with("minecraft:pink_")
        })
        .map(|(pos, _)| pos)
        .collect()
}

/// A status line's block: pink stained glass where it was glass, else pink
/// concrete.
fn pink(state: &str) -> String {
    if schem::id(state).ends_with("glass") {
        "minecraft:pink_stained_glass".to_owned()
    } else {
        "minecraft:pink_concrete".to_owned()
    }
}

/// The file with any corner markers taken out, its box fitted tightly
/// around the blocks left, and its bytes written the one way the site
/// writes them, so the same build always gives the same file. A marker is
/// loose glass, as [`loose_glass`] finds it, lying on the box's edges: the
/// old corner markers, a corner and its three neighbours, or `WorldEdit`'s
/// single corner blocks.
pub fn tidy(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let markers = loose_glass(&blocks, true);
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(pos, state)| *state != AIR && !markers.contains(pos))
        .map(|(pos, state)| (pos, facing_as_usual(&blocks, pos, state)))
        .collect();
    ensure!(!grid.is_empty(), "the schematic has no blocks");
    let bound = |axis: usize, pick: fn(i32, i32) -> i32| {
        grid.keys().map(|pos| pos[axis]).reduce(pick).unwrap_or(0)
    };
    let low = [0, 1, 2].map(|axis| bound(axis, i32::min));
    let high = [0, 1, 2].map(|axis| bound(axis, i32::max));
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    rebox(data, &grid, low, size)
}

/// The file with each block of `changes` put in place, air taking one out,
/// its box and paste offset as they were, so `tidy` fits it afterwards.
pub fn edit(data: &[u8], changes: &BTreeMap<Pos, String>) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    for (&pos, state) in changes {
        if state == AIR {
            grid.remove(&pos);
        } else {
            grid.insert(pos, state.clone());
        }
    }
    rebox(data, &grid, [0, 0, 0], blocks.size)
}

/// The file with each input at logical 0, as every stored build is: the
/// levers of each line its test found reads `inverted`, by name, as a
/// build of `category` names its lines, on, every other lever off, and
/// every component at rest with them.
pub fn levers_at_zero(
    data: &[u8],
    category: &str,
    inverted: &[String],
) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let mut lines = interface::read(&blocks);
    interface::name_lines(category, &blocks, &mut lines);
    let on: BTreeSet<Pos> = lines
        .inputs
        .iter()
        .filter(|port| inverted.contains(&port.name))
        .flat_map(|port| &port.bits)
        .filter_map(|bit| Some(schem::step(bit.pos, bit.entry?)))
        .filter(|&pos| schem::short(blocks.get(pos)) == "lever")
        .collect();
    with_levers(data, &on)
}

/// The file with every lever off but those at `on`, and every component
/// at rest with them, as the game settles it: each dust's power, each
/// torch, repeater, comparator, and lamp, and each comparator's strength.
pub fn with_levers(data: &[u8], on: &BTreeSet<Pos>) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let (changes, strengths) = crate::simulate::settled(&blocks, on)?;
    if changes.is_empty() && strengths.is_empty() {
        return Ok(data.to_vec());
    }
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    grid.extend(changes);
    rebox_with(data, &grid, [0, 0, 0], blocks.size, &strengths)
}

/// The file with each block of `changes` put in place, as `edit` does, its
/// box grown to hold any outside it; and how far every old position moved.
pub fn edit_grown(
    data: &[u8],
    changes: &BTreeMap<Pos, String>,
) -> Result<(Vec<u8>, Pos)> {
    let blocks = Blocks::read(data)?;
    let mut grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let (mut low, mut high) = ([0; 3], blocks.size.map(|n| n - 1));
    for (&pos, state) in changes {
        for axis in 0..3 {
            low[axis] = low[axis].min(pos[axis]);
            high[axis] = high[axis].max(pos[axis]);
        }
        if state == AIR {
            grid.remove(&pos);
        } else {
            grid.insert(pos, state.clone());
        }
    }
    let size = [0, 1, 2].map(|axis| high[axis] - low[axis] + 1);
    Ok((rebox(data, &grid, low, size)?, low.map(|n| -n)))
}

const BARREL: &str = "minecraft:barrel[facing=up,open=false]";

/// `state` at `pos` facing the usual way where its facing does nothing, so
/// files that differ only in that compare alike: a barrel up, closed, and
/// a hopper down, unless it points into a container, which it would fill,
/// or down would. Any other block stays as it is.
fn facing_as_usual(blocks: &Blocks, pos: Pos, state: &str) -> String {
    match schem::short(state) {
        "barrel" => BARREL.to_owned(),
        "hopper" => {
            let into = |side: Direction| {
                is_container(schem::short(blocks.get(schem::step(pos, side))))
            };
            let facing = Direction::facing(state).unwrap_or(Direction::Down);
            if into(facing) || into(Direction::Down) {
                state.to_owned()
            } else {
                state.replace(&format!("facing={facing}"), "facing=down")
            }
        }
        _ => state.to_owned(),
    }
}

/// Whether a block id without its namespace holds items a hopper could
/// push into.
fn is_container(name: &str) -> bool {
    matches!(
        name,
        "barrel"
            | "chest"
            | "trapped_chest"
            | "hopper"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "dropper"
            | "dispenser"
            | "brewing_stand"
            | "crafter"
    ) || name.ends_with("shulker_box")
}

/// The file with each furnace, cauldron, and cake, and each container whose
/// id is in `also`, such as `hopper`, swapped for a barrel holding redstone
/// dust that gives a comparator the same signal strength, and each barrel
/// holding anything else, such as wooden shovels, given that dust in its
/// place. Barrels are how a build takes a fixed strength; a hopper, which
/// does not conduct as a barrel does, stays unless its builder asks.
pub fn barrels(data: &[u8], also: &[&str]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let swapped: BTreeMap<Pos, u8> = blocks
        .iter()
        .filter_map(|(pos, state)| {
            let name = schem::short(state);
            let strength = if let Some(strength) = stand_in_strength(state) {
                strength
            } else if name == "furnace" || also.contains(&name) {
                let items = blocks.containers.get(&pos);
                schem::container_strength(
                    items.map_or(&[], Vec::as_slice),
                    schem::slots(state)?,
                )
            } else if name == "barrel" {
                let items =
                    blocks.containers.get(&pos).map_or(&[][..], Vec::as_slice);
                let strength = schem::container_strength(items, 27);
                let dust: Vec<(String, u32)> = schem::barrel_redstone(strength)
                    .into_iter()
                    .map(|count| ("minecraft:redstone".to_owned(), count))
                    .collect();
                let mut held = items.to_vec();
                held.sort();
                let mut standard = dust;
                standard.sort();
                if held == standard {
                    return None;
                }
                strength
            } else {
                return None;
            };
            Some((pos, strength))
        })
        .collect();
    if swapped.is_empty() {
        return Ok(data.to_vec());
    }
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| {
            let state = if swapped.contains_key(&pos) {
                BARREL
            } else {
                state
            };
            (pos, state.to_owned())
        })
        .collect();
    let mut nbt = schem::decode(data)?;
    let mut entities: Vec<_> = nbt
        .take_entities()
        .into_iter()
        .filter(|entity| {
            schem::entity_pos(entity)
                .is_none_or(|pos| !swapped.contains_key(&pos))
        })
        .collect();
    entities.extend(swapped.iter().map(|(&pos, &strength)| {
        nbt.barrel_entity(pos, &schem::barrel_redstone(strength))
    }));
    nbt.write_blocks([0, 0, 0], blocks.size, &grid, entities)?;
    schem::encode(nbt)
}

/// What `barrels` would swap in a build.
pub struct Swappable {
    /// Each kind of block a barrel always stands in for, such as `water
    /// cauldron`, with how many the build has, in name order.
    pub stand_ins: Vec<(String, usize)>,
    /// How many hoppers it has, which a barrel stands in for when asked.
    pub hoppers: usize,
}

pub fn swappable(blocks: &Blocks) -> Swappable {
    let mut stand_ins: BTreeMap<String, usize> = BTreeMap::new();
    let mut hoppers = 0;
    for (_, state) in blocks.iter() {
        match schem::short(state) {
            name if colour::barrel_stands_in(name) => {
                *stand_ins.entry(name.replace('_', " ")).or_default() += 1;
            }
            "hopper" => hoppers += 1,
            _ => {}
        }
    }
    Swappable {
        stand_ins: stand_ins.into_iter().collect(),
        hoppers,
    }
}

/// What a comparator reads from a block a barrel stands in for: a
/// cauldron's level, or 3 when it holds lava; a cake's 14, less 2 for each
/// bite taken. `None` for any other block.
fn stand_in_strength(state: &str) -> Option<u8> {
    let id = schem::id(state);
    let number = |key: &str| {
        schem::property(state, key).and_then(|n| n.parse::<u8>().ok())
    };
    Some(match id.strip_prefix("minecraft:").unwrap_or(id) {
        "lava_cauldron" => 3,
        "cake" => 14u8.saturating_sub(2 * number("bites").unwrap_or(0)),
        name if name.ends_with("candle_cake") => 14,
        name if name.ends_with("cauldron") => number("level").unwrap_or(0),
        _ => return None,
    })
}

/// `data` as the 3D viewer, schematic-renderer, should be given it: it
/// turns a wall sign facing east or west the wrong way round, onto the far
/// side of its own space, a block from what it hangs on, so each such sign
/// is given facing the other way.
pub fn for_viewer(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let mut swapped = false;
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| {
            let wall_sign = schem::id(state).ends_with("_wall_sign");
            let facing = schem::property(state, "facing");
            let turned = match facing {
                Some("east") if wall_sign => {
                    state.replace("facing=east", "facing=west")
                }
                Some("west") if wall_sign => {
                    state.replace("facing=west", "facing=east")
                }
                _ => return (pos, state.to_owned()),
            };
            swapped = true;
            (pos, turned)
        })
        .collect();
    if !swapped {
        return Ok(data.to_vec());
    }
    rebox(data, &grid, [0, 0, 0], blocks.size)
}

/// Writes `grid`, in the file's own positions, into a box of `size` whose
/// first block is at `low`, moving signs and other block entities along,
/// each sign's lines plain text; see `schem::plain_sign`.
/// Entities left outside the box are dropped.
fn rebox(
    data: &[u8],
    grid: &HashMap<Pos, String>,
    low: Pos,
    size: Pos,
) -> Result<Vec<u8>> {
    rebox_with(data, grid, low, size, &HashMap::new())
}

/// As `rebox`, each comparator at a position of `strengths`, in the old
/// box, putting out its strength.
fn rebox_with(
    data: &[u8],
    grid: &HashMap<Pos, String>,
    low: Pos,
    size: Pos,
    strengths: &HashMap<Pos, u8>,
) -> Result<Vec<u8>> {
    let shift = |pos: Pos| [0, 1, 2].map(|axis| pos[axis] - low[axis]);
    let inside =
        |pos: Pos| (0..3).all(|axis| (0..size[axis]).contains(&pos[axis]));
    let placed: HashMap<Pos, String> = grid
        .iter()
        .map(|(&pos, state)| (shift(pos), state.clone()))
        .collect();
    let mut nbt = schem::decode(data)?;
    // A block entity whose block was taken out goes with it, and a sign's
    // lines are written plain.
    let entities = nbt
        .take_entities()
        .iter()
        .filter_map(|entity| {
            let was = schem::entity_pos(entity)?;
            let pos = shift(was);
            let entity = match strengths.get(&was) {
                Some(&strength) => schem::with_output_signal(entity, strength),
                None => entity.clone(),
            };
            (inside(pos) && placed.contains_key(&pos))
                .then(|| schem::with_pos(&schem::plain_sign(&entity), pos))
        })
        .collect();
    nbt.write_blocks(low, size, &placed, entities)?;
    schem::encode(nbt)
}

/// Plain glass that is no part of the build: old corner markers. A group
/// of glass is loose when it touches nothing but air and levers and signs
/// hanging on other blocks; glass touching anything else may carry dust, so
/// it stays. With `on_edges`, only groups lying wholly on the box's edges
/// count, as markers do; without, misplaced ones count too.
fn loose_glass(blocks: &Blocks, on_edges: bool) -> HashSet<Pos> {
    let size = blocks.size;
    // On an edge of the box: at its first or last block along two axes.
    let on_edge = |pos: Pos| {
        (0..3)
            .filter(|&axis| pos[axis] == 0 || pos[axis] == size[axis] - 1)
            .count()
            >= 2
    };
    let glass: HashSet<Pos> = blocks
        .iter()
        .filter(|&(_, state)| state == GLASS)
        .map(|(pos, _)| pos)
        .collect();
    // The blocks a lever or sign hangs on, which must stay.
    let mut held: HashSet<Pos> = blocks
        .switches()
        .into_iter()
        .map(|lever| lever.block)
        .collect();
    held.extend(
        blocks
            .signs
            .keys()
            .filter_map(|&sign| blocks.sign_block(sign)),
    );
    let harmless = |state: &str| {
        let id = schem::id(state);
        state == AIR || id == "minecraft:lever" || id.ends_with("sign")
    };
    let mut loose = HashSet::new();
    let mut seen = HashSet::new();
    for &start in &glass {
        if !seen.insert(start) {
            continue;
        }
        // Gather the group of glass connected to `start`.
        let mut group = vec![start];
        let mut touches_build = false;
        let mut next = 0;
        while next < group.len() {
            let pos = group[next];
            next += 1;
            for direction in Direction::ALL {
                let neighbour = schem::step(pos, direction);
                if glass.contains(&neighbour) {
                    if seen.insert(neighbour) {
                        group.push(neighbour);
                    }
                } else if !harmless(blocks.get(neighbour)) {
                    touches_build = true;
                }
            }
        }
        let carries = group.iter().any(|pos| held.contains(pos));
        if !touches_build
            && !carries
            && (!on_edges || group.iter().all(|&p| on_edge(p)))
        {
            loose.extend(group);
        }
    }
    loose
}

/// The block a standardized build uses in place of `state`. A coloured
/// block with a lever or signal-strength dust on it, an `input`, becomes
/// that colour's concrete; without one, its colour means nothing and it
/// turns white.
fn restyle(state: &str, input: bool) -> String {
    let id = schem::id(state);
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    if input && let Some(colour) = colour::input_colour(name) {
        format!("minecraft:{colour}_concrete")
    } else if colour::keeps(name) {
        state.to_owned()
    } else if name.ends_with("_sign") && !name.contains("hanging") {
        let wall = if name.ends_with("_wall_sign") {
            "wall_"
        } else {
            ""
        };
        let properties = &state[id.len()..];
        format!("minecraft:oak_{wall}sign{properties}")
    } else if block::see_through(name) {
        WHITE_GLASS.to_owned()
    } else {
        WHITE.to_owned()
    }
}

/// What the block id `name`, without its namespace, counts as among a
/// build's blocks: a redstone part by what it is, a torch on a wall or on
/// the floor alike and dust whatever its shape; every see-through block as
/// one, and every other solid block as one. `None` for air and for signs,
/// which carry only labels.
pub fn block_kind(name: &str) -> Option<&'static str> {
    Some(match name {
        "air" | "cave_air" | "void_air" => return None,
        _ if name.ends_with("_sign") => return None,
        "redstone_wire" => "Dust",
        "redstone_torch" | "redstone_wall_torch" => "Torch",
        "repeater" => "Repeater",
        "comparator" => "Comparator",
        "target" => "Target",
        "barrel" => "Barrel",
        "redstone_block" => "Redstone block",
        "lever" => "Lever",
        _ if name.ends_with("_button") => "Button",
        _ if name.ends_with("_pressure_plate") => "Pressure plate",
        "observer" => "Observer",
        "piston" | "sticky_piston" | "piston_head" | "moving_piston" => {
            "Piston"
        }
        "redstone_lamp" => "Lamp",
        "hopper" => "Hopper",
        "furnace" => "Furnace",
        "cake" => "Cake",
        _ if name.ends_with("cauldron") => "Cauldron",
        _ if block::see_through(name) => "Transparent",
        _ => "Solid",
    })
}

/// How many of each kind of block, as `block_kind` counts them, `blocks`
/// holds, most first.
pub fn block_counts(blocks: &Blocks) -> Vec<(&'static str, usize)> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for (_, state) in blocks.iter() {
        if let Some(kind) = block_kind(schem::short(state)) {
            *counts.entry(kind).or_default() += 1;
        }
    }
    let mut counts: Vec<(&'static str, usize)> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    counts
}
