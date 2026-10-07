//! A build's inputs and outputs, read from its blocks and signs as
//! `conventions.md` lays them out. An input is a block with a lever on it;
//! an output is a yellow concrete block, or any block without a lever that
//! carries a name sign or matches a block that does. Each bus's least significant bit is the one
//! nearest the south-east-bottom corner of the bounding box.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::schem::{self, Blocks, DIRECTIONS, Lever, Pos};

pub struct Interface {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}

/// A bus, or one named line such as `carry in`.
pub struct Port {
    pub name: String,
    /// The block marking each bit, such as `lime concrete`.
    pub block: String,
    /// Least significant first.
    pub bits: Vec<Bit>,
    /// The weakest signal strength that drives an input correctly, from a
    /// sign token such as `4ss` on any of its bits.
    pub strength: u8,
}

pub struct Bit {
    /// Position inside the corner markers.
    pub pos: Pos,
    /// For an input, the side power enters through: the side its lever is
    /// on, such as `south`.
    pub entry: Option<String>,
    /// For an input, whether dust touches its block, so only strong power
    /// from a repeater, comparator, or torch drives it.
    pub hard: bool,
    /// For an input, whether it is saved with its lever on.
    pub inverted: bool,
}

pub fn read(blocks: &Blocks) -> Interface {
    let levers: HashMap<Pos, Lever> = blocks
        .levers()
        .into_iter()
        .map(|lever| (lever.block, lever))
        .collect();
    let signs = blocks.sign_texts();
    let output_kinds: HashSet<&str> = signs
        .iter()
        .filter(|(pos, lines)| {
            !levers.contains_key(*pos) && name(lines).is_some()
        })
        .map(|(&pos, _)| schem::id(blocks.get(pos)))
        .chain(["minecraft:yellow_concrete"])
        .collect();
    let outputs: Vec<Pos> = blocks
        .iter()
        .filter(|(pos, state)| {
            !levers.contains_key(pos) && output_kinds.contains(schem::id(state))
        })
        .map(|(pos, _)| pos)
        .collect();
    Interface {
        inputs: ports(blocks, levers.keys().copied(), &signs, Some(&levers)),
        outputs: ports(blocks, outputs, &signs, None),
    }
}

/// Groups `members` into ports: one per name sign, then one bus per block
/// kind. `levers` is given for inputs.
fn ports(
    blocks: &Blocks,
    members: impl IntoIterator<Item = Pos>,
    signs: &HashMap<Pos, &[String]>,
    levers: Option<&HashMap<Pos, Lever>>,
) -> Vec<Port> {
    let mut named = Vec::new();
    let mut buses: BTreeMap<&str, Vec<Pos>> = BTreeMap::new();
    for pos in members {
        let lines = signs.get(&pos).copied().unwrap_or_default();
        match name(lines) {
            Some(name) => {
                named.push(port(blocks, name, vec![pos], lines, levers));
            }
            None => {
                buses
                    .entry(schem::id(blocks.get(pos)))
                    .or_default()
                    .push(pos);
            }
        }
    }
    // Operands take letters in palette order: lime is A, light blue B.
    let rank = |kind: &str| match kind {
        "minecraft:lime_concrete" => 0,
        "minecraft:light_blue_concrete" => 1,
        _ => 2,
    };
    let mut kinds: Vec<&str> = buses.keys().copied().collect();
    kinds.sort_by_key(|kind| rank(kind));
    let mut letters = 'A'..='Z';
    let output_count = kinds.len();
    let mut result: Vec<Port> = kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| {
            let members = buses.remove(kind).unwrap_or_default();
            // A sign on any bit can carry the bus's strength.
            let lines: Vec<String> = members
                .iter()
                .filter_map(|pos| signs.get(pos))
                .flat_map(|lines| lines.iter().cloned())
                .collect();
            let name = if levers.is_none() {
                if output_count == 1 {
                    "output".to_owned()
                } else {
                    format!("output {}", i + 1)
                }
            } else if kind == "minecraft:magenta_concrete" {
                "unnamed control".to_owned()
            } else {
                letters.next().map(String::from).unwrap_or_default()
            };
            port(blocks, name, members, &lines, levers)
        })
        .collect();
    result.extend(named);
    result
}

/// A port with its bits ordered from the one nearest the south-east-bottom
/// corner of the bounding box.
fn port(
    blocks: &Blocks,
    name: String,
    mut members: Vec<Pos>,
    lines: &[String],
    levers: Option<&HashMap<Pos, Lever>>,
) -> Port {
    let [width, _, length] = blocks.size;
    let corner = [width - 1, 0, length - 1];
    if let Some(&lsb) = members.iter().min_by_key(|&&pos| distance(pos, corner))
    {
        members.sort_by_key(|&pos| distance(pos, lsb));
    }
    let inverted_line = lines.iter().any(|l| l.trim() == "inverted");
    let bits = members
        .iter()
        .map(|&pos| {
            let lever = levers.and_then(|levers| levers.get(&pos));
            Bit {
                pos: pos.map(|n| n - 1),
                entry: lever.map(|lever| lever.side.clone()),
                hard: lever.is_some()
                    && DIRECTIONS.iter().any(|&d| {
                        schem::id(blocks.get(schem::step(pos, d)))
                            == "minecraft:redstone_wire"
                    }),
                inverted: lever.is_some_and(|lever| lever.on) || inverted_line,
            }
        })
        .collect();
    Port {
        name,
        block: members
            .first()
            .map(|&pos| block_label(blocks.get(pos)))
            .unwrap_or_default(),
        bits,
        strength: lines.iter().find_map(|l| strength(l)).unwrap_or(1),
    }
}

/// The port a sign names: its first line, unless that is empty or a token.
/// Older builds marked their least significant bit with an `lsb` sign,
/// which the corner rule replaces.
fn name(lines: &[String]) -> Option<String> {
    let first = lines.first()?.trim().to_lowercase();
    (!first.is_empty() && first != "lsb" && strength(&first).is_none())
        .then_some(first)
}

/// A strength token such as `4ss`.
fn strength(line: &str) -> Option<u8> {
    line.trim().strip_suffix("ss")?.parse().ok()
}

fn distance(a: Pos, b: Pos) -> i32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).sum()
}

/// A block state as words, such as `lime concrete`.
pub fn block_label(state: &str) -> String {
    let id = schem::id(state);
    id.strip_prefix("minecraft:")
        .unwrap_or(id)
        .replace('_', " ")
}
