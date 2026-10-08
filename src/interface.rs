//! A build's inputs and outputs, read from its blocks and signs as
//! `conventions.md` lays them out. An input is a block with a lever on it,
//! or for a control line a button; an output is a block of an output
//! colour. Each colour is one bus, named by a sign on any of its blocks,
//! except magenta control lines and pink status lines, each one line named
//! by its own sign. Each bus's least significant bit is the one nearest the
//! south-east-bottom corner of the bounding box.

use std::collections::{BTreeMap, HashMap};

use crate::{
    lint,
    schem::{self, Blocks, DIRECTIONS, Pos, Switch},
};

pub struct Interface {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}

/// A bus, or one named line such as `carry in`.
pub struct Port {
    pub name: String,
    /// The block marking each bit, such as `lime concrete`.
    pub block: String,
    /// Whether it is a line named on its own, a control line such as `carry
    /// in` or a status line such as `carry out`, rather than a bus.
    pub labelled: bool,
    /// Least significant first.
    pub bits: Vec<Bit>,
    /// The weakest signal strength that drives an input correctly, from a
    /// sign token such as `4ss` on any of its bits.
    pub strength: u8,
}

pub struct Bit {
    /// Position in the file.
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

/// The bounds, inclusive, of the build itself: its box less any outer
/// layer that holds nothing but its interface, meaning input blocks with
/// their levers, signs, and torches. Such a layer only carries the inputs,
/// so it does not make the build bigger, and a stack's copies share it.
pub fn core(blocks: &Blocks) -> (Pos, Pos) {
    let mut low = [0; 3];
    let mut high = blocks.size.map(|n| n - 1);
    for axis in 0..3 {
        while high[axis] > low[axis]
            && interface_only(blocks, axis, low[axis], low, high)
        {
            low[axis] += 1;
        }
        while high[axis] > low[axis]
            && interface_only(blocks, axis, high[axis], low, high)
        {
            high[axis] -= 1;
        }
    }
    (low, high)
}

/// Whether the layer at `at` along `axis`, within `low` to `high` on the
/// other axes, holds nothing but the interface.
fn interface_only(
    blocks: &Blocks,
    axis: usize,
    at: i32,
    low: Pos,
    high: Pos,
) -> bool {
    let (mut low, mut high) = (low, high);
    low[axis] = at;
    high[axis] = at;
    (low[0]..=high[0]).all(|x| {
        (low[1]..=high[1]).all(|y| {
            (low[2]..=high[2]).all(|z| {
                let pos = [x, y, z];
                let state = blocks.get(pos);
                let support = schem::torch_block(pos, state)
                    .map(|block| blocks.get(block));
                state == schem::AIR || lint::is_interface(state, support)
            })
        })
    })
}

pub fn read(blocks: &Blocks) -> Interface {
    let levers: HashMap<Pos, Switch> = blocks
        .switches()
        .into_iter()
        .map(|lever| (lever.block, lever))
        .collect();
    let signs = blocks.sign_texts();
    let outputs: Vec<Pos> = blocks
        .iter()
        .filter(|(pos, state)| {
            !levers.contains_key(pos) && lint::is_output(short(state))
        })
        .map(|(pos, _)| pos)
        .collect();
    Interface {
        inputs: ports(blocks, levers.keys().copied(), &signs, Some(&levers)),
        outputs: ports(blocks, outputs, &signs, None),
    }
}

/// Groups `members` into ports, `levers` being given for inputs. Each
/// control or status line with a name sign is a line of its own. Every
/// other colour is one bus, named by a sign on any of its blocks, or else
/// by the next letter for an input, A for lime, or by its place among the
/// output colours.
fn ports(
    blocks: &Blocks,
    members: impl IntoIterator<Item = Pos>,
    signs: &HashMap<Pos, &[String]>,
    levers: Option<&HashMap<Pos, Switch>>,
) -> Vec<Port> {
    let (order, line): (&[&str], _) = if levers.is_some() {
        (&lint::INPUT_COLOURS, lint::CONTROL)
    } else {
        (&lint::OUTPUT_COLOURS, lint::STATUS)
    };
    let mut named = Vec::new();
    let mut buses: BTreeMap<&str, Vec<Pos>> = BTreeMap::new();
    for pos in members {
        let lines = signs.get(&pos).copied().unwrap_or_default();
        let colour = colour(blocks.get(pos));
        match sign_name(lines).filter(|_| colour == line) {
            Some(name) => {
                let mut port = port(blocks, name, vec![pos], lines, levers);
                port.labelled = true;
                named.push(port);
            }
            None => buses.entry(colour).or_default().push(pos),
        }
    }
    let rank = |colour: &str| {
        order
            .iter()
            .position(|c| *c == colour)
            .unwrap_or(usize::MAX)
    };
    let mut colours: Vec<&str> = buses.keys().copied().collect();
    colours.sort_by_key(|colour| (rank(colour), *colour));
    let mut letters = 'A'..='Z';
    let output_count = colours.iter().filter(|c| **c != line).count();
    let mut result: Vec<Port> = colours
        .into_iter()
        .enumerate()
        .map(|(i, colour)| {
            let members = buses.remove(colour).unwrap_or_default();
            // A sign on any bit can carry the bus's strength.
            let lines: Vec<String> = members
                .iter()
                .filter_map(|pos| signs.get(pos))
                .flat_map(|lines| lines.iter().cloned())
                .collect();
            let given = members
                .iter()
                .find_map(|pos| signs.get(pos).and_then(|l| sign_name(l)));
            let name = if colour == line {
                let what = if levers.is_some() {
                    "control"
                } else {
                    "status"
                };
                format!("unnamed {what}")
            } else if let Some(given) = given {
                given
            } else if levers.is_some() {
                letters.next().map(String::from).unwrap_or_default()
            } else if output_count == 1 {
                "output".to_owned()
            } else {
                format!("output {}", i + 1)
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
    levers: Option<&HashMap<Pos, Switch>>,
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
                pos,
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
        labelled: false,
        bits,
        strength: lines.iter().find_map(|l| strength(l)).unwrap_or(1),
    }
}

/// The colour an input or output block marks, such as `yellow` for yellow
/// stained glass. Any other block is its own kind.
fn colour(state: &str) -> &str {
    let block = short(state);
    lint::output_colour(block)
        .or_else(|| block.strip_suffix("_concrete"))
        .unwrap_or(block)
}

/// A block state's id without its namespace, such as `yellow_concrete`.
fn short(state: &str) -> &str {
    let id = schem::id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// The port a sign names: its first line, unless that is empty or a token.
/// Older builds marked their least significant bit with an `lsb` sign,
/// which the corner rule replaces.
pub fn sign_name(lines: &[String]) -> Option<String> {
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
