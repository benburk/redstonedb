//! Shaping a build's file: fitting its box tightly and taking out what is
//! no part of it, its blocks as its author made them. What each line is
//! for goes with the file, see [`crate::schem::lines`], and a theme colours
//! it when the build is shown or downloaded; see
//! [`crate::recognize::palette`]. Its inputs and outputs themselves are its
//! levers, buttons, and comparators with air behind, and its repeaters and
//! comparators facing air, whatever their colour:
//!
//! - Old corner markers are taken out: any group of plain glass that
//!   touches nothing but air, wherever it sits.
//! - A furnace, cauldron, or cake becomes a barrel giving a comparator the
//!   same signal strength.
//! - A sign on a block that is neither an input nor an output, such as a
//!   note of the build's date, is taken out, as signs carry only labels.
//!   Redstone passes a sign by as it does air. A sign's commas are taken
//!   out, as its words are separated by spaces and lines alone.
//! - The box is fitted tightly around what is left.
//! - Each data input's lever is switched off, every other component coming
//!   to rest with it, as a data input never reads inverted, so its lever
//!   off is its 0. A control line's levers stay as they were, for its test
//!   to find which read inverted; see `levers_at_zero`. A build that cannot
//!   be settled, as one with a clock, keeps its levers as they were.
//!
//! Every other block keeps its kind and colour.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Result, ensure};

use crate::{
    recognize::{colour, interface, repeater_outputs},
    schem::{self, AIR, Blocks, Direction, Pos},
};

/// Whether `state` is glass of any kind, plain, stained, or tinted, as a
/// corner marker may be.
fn is_glass(state: &str) -> bool {
    schem::id(state).ends_with("glass")
}

pub fn standardize(data: &[u8]) -> Result<Vec<u8>> {
    let swapped = barrels(data, &[])?;
    let data = swapped.as_slice();
    let blocks = Blocks::read(data)?;
    let old_markers = loose_glass(&blocks, false);
    let inputs: HashSet<Pos> = interface::feeds(&blocks).into_keys().collect();
    let notes = notes(&blocks, &inputs);
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(pos, state)| {
            *state != AIR && !old_markers.contains(pos) && !notes.contains(pos)
        })
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect();
    let restyled = schem::rebox(data, &grid, [0, 0, 0], blocks.size)?;
    let standardized = tidy(&restyled)?;

    let found = interface::read(&Blocks::read(&standardized)?);
    ensure!(
        !found.inputs.is_empty(),
        "inputs: none found — mark each with a lever or button, or a \
         comparator with air behind for a signal strength"
    );
    ensure!(
        !found.outputs.is_empty(),
        "outputs: none found — mark each with a repeater or comparator \
         facing air; if the file should have some, check the selection it \
         was saved from covers the whole build"
    );
    Ok(data_levers_off(&standardized).unwrap_or(standardized))
}

/// The file with the lever of each data input, any input but a control
/// line, switched off, every other lever as it was, and every
/// component at rest with them, as `with_levers` settles them. A file with
/// no data input's lever on comes back as it was.
pub fn data_levers_off(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let data_levers: BTreeSet<Pos> = interface::read(&blocks)
        .inputs
        .iter()
        .filter(|port| !port.control())
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

/// The signs that label nothing: each but those on an input, one of
/// `inputs`, and each output's own, on the face of its block under its
/// front, see [`repeater_outputs::signs`], as one on another face of an
/// output's block is a note too.
fn notes(blocks: &Blocks, inputs: &HashSet<Pos>) -> HashSet<Pos> {
    let named: HashSet<Pos> = repeater_outputs::find(blocks)
        .into_iter()
        .filter(|output| {
            let place = output.sign_place();
            blocks.signs.contains_key(&place)
                && blocks.sign_block(place) == Some(output.block())
        })
        .map(repeater_outputs::Output::sign_place)
        .collect();
    blocks
        .signs
        .keys()
        .copied()
        .filter(|&sign| {
            !named.contains(&sign)
                && blocks
                    .sign_block(sign)
                    .is_none_or(|on| !inputs.contains(&on))
        })
        .collect()
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
    schem::rebox(data, &grid, low, size)
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
    interface::name_lines(
        crate::category::Kind::named(category),
        &blocks,
        &mut lines,
    );
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
    schem::edit::rebox_with(
        data,
        &grid,
        [0, 0, 0],
        blocks.size,
        &strengths,
        &HashMap::new(),
    )
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

/// The file without its trapdoors, which builders hang on an output to show
/// it flipping, playing no part in the circuit; as it is when it has none.
pub fn without_trapdoors(data: &[u8]) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let gone: BTreeMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| schem::short(state).ends_with("_trapdoor"))
        .map(|(pos, _)| (pos, schem::AIR.to_owned()))
        .collect();
    if gone.is_empty() {
        return Ok(data.to_vec());
    }
    schem::edit(data, &gone)
}

/// The file with each furnace, cauldron, and cake, and each container whose
/// id is in `also`, such as `hopper`, swapped for a barrel holding redstone
/// dust that gives a comparator the same signal strength, and each barrel
/// holding anything else, such as wooden shovels, given that dust in its
/// place. Barrels are how a build takes a fixed strength; a hopper, which
/// does not conduct as a barrel does, stays unless its author asks.
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

/// Glass that is no part of the build, of any colour: old corner markers,
/// or `WorldEdit`'s, as a player places them in whatever glass. A group
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
        .filter(|&(_, state)| is_glass(state))
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
