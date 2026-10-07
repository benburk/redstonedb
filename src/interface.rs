//! A build's inputs and outputs, read from its blocks and signs as
//! `conventions.md` lays them out. An input is a block with a lever on it;
//! an output is a lamp, or a block without a lever that carries a sign or
//! matches the block an output `lsb` sign is on. `lsb` signs order the bits.

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
    /// Least significant first, when an `lsb` sign says which that is.
    pub bits: Vec<Bit>,
    pub lsb_known: bool,
    /// The weakest signal strength that drives an input correctly, from a
    /// sign token such as `4ss`.
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
            !levers.contains_key(*pos) && label(lines) == "lsb"
        })
        .map(|(&pos, _)| schem::id(blocks.get(pos)))
        .chain(["minecraft:redstone_lamp"])
        .collect();
    let outputs: Vec<Pos> = blocks
        .iter()
        .filter(|(pos, state)| {
            !levers.contains_key(pos)
                && (output_kinds.contains(schem::id(state))
                    || signs.contains_key(pos))
        })
        .map(|(pos, _)| pos)
        .collect();
    Interface {
        inputs: ports(blocks, levers.keys().copied(), &signs, Some(&levers)),
        outputs: ports(blocks, outputs, &signs, None),
    }
}

/// Groups `members` into ports: one per named sign, then one bus per block
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
        match label(lines).as_str() {
            "" | "lsb" => {
                buses
                    .entry(schem::id(blocks.get(pos)))
                    .or_default()
                    .push(pos);
            }
            name => named.push(port(
                blocks,
                name.to_owned(),
                vec![pos],
                Some(pos),
                lines,
                levers,
            )),
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
            let lsb = members
                .iter()
                .copied()
                .find(|pos| signs.get(pos).is_some_and(|l| label(l) == "lsb"));
            let lines = lsb.and_then(|pos| signs.get(&pos).copied());
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
            port(
                blocks,
                name,
                members,
                lsb,
                lines.unwrap_or_default(),
                levers,
            )
        })
        .collect();
    result.extend(named);
    result
}

fn port(
    blocks: &Blocks,
    name: String,
    mut members: Vec<Pos>,
    lsb: Option<Pos>,
    lines: &[String],
    levers: Option<&HashMap<Pos, Lever>>,
) -> Port {
    match lsb {
        Some(lsb) => members.sort_by_key(|pos| distance(*pos, lsb)),
        None => members.sort_unstable(),
    }
    let inverted_line = lines.iter().any(|l| l.trim() == "inverted");
    let bits = members
        .iter()
        .map(|&pos| {
            let lever = levers.and_then(|levers| levers.get(&pos));
            Bit {
                pos: pos.map(|n| n - 1),
                entry: lever.map(|lever| lever.facing.clone()),
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
        lsb_known: lsb.is_some(),
        strength: lines.iter().find_map(|l| strength(l)).unwrap_or(1),
    }
}

/// A sign's first line, which names its port or says `lsb`.
fn label(lines: &[String]) -> String {
    lines
        .first()
        .map(|line| line.trim().to_lowercase())
        .unwrap_or_default()
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
