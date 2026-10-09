//! A build's inputs and outputs, read from its blocks and signs as
//! `conventions.md` lays them out. An input is a block with a lever on it,
//! or for a control line a button, or for a signal-strength input dust on
//! top; an output is a block of an output colour. Each colour is one bus, named by a sign on any of its blocks,
//! except magenta control lines and pink status lines, each one line named
//! by its own sign. Each bus's least significant bit is the one nearest the
//! south-east-bottom corner of the bounding box.

use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// A signal-strength input: each block takes a value from 0 to 15 as
    /// its strength, through the dust on top, rather than a bit from a
    /// lever.
    pub signal_strength: bool,
}

pub struct Bit {
    /// Position in the file.
    pub pos: Pos,
    /// For an input, the side power enters through, such as `south`: the
    /// side its lever is on, or the open end of the dust on top.
    pub entry: Option<String>,
    /// For an input, whether dust touches its block, so only strong power
    /// from a repeater, comparator, or torch drives it.
    pub hard: bool,
    /// For an input, whether it is saved with its lever on.
    pub inverted: bool,
}

/// The bounds, inclusive, of the build itself: its box less any outer
/// layer that holds nothing but its interface, meaning input blocks with
/// their levers, dust, signs, and torches. Such a layer only carries the
/// inputs, so it does not make the build bigger, and a stack's copies share
/// it.
pub fn core(blocks: &Blocks) -> (Pos, Pos) {
    let fed = dust_inputs(blocks);
    let mut low = [0; 3];
    let mut high = blocks.size.map(|n| n - 1);
    for axis in 0..3 {
        while high[axis] > low[axis]
            && interface_only(blocks, &fed, axis, low[axis], low, high)
        {
            low[axis] += 1;
        }
        while high[axis] > low[axis]
            && interface_only(blocks, &fed, axis, high[axis], low, high)
        {
            high[axis] -= 1;
        }
    }
    (low, high)
}

/// Whether the layer at `at` along `axis`, within `low` to `high` on the
/// other axes, holds nothing but the interface. `fed` holds the
/// signal-strength inputs, whose dust is part of the interface.
fn interface_only(
    blocks: &Blocks,
    fed: &HashMap<Pos, Option<String>>,
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
                state == schem::AIR
                    || lint::is_interface(state, support)
                    || (schem::id(state) == DUST
                        && fed.contains_key(&schem::step(pos, "down")))
            })
        })
    })
}

const DUST: &str = "minecraft:redstone_wire";

/// How an input block takes its signal.
pub enum Feed {
    /// A lever, or for a control line a button, on the block.
    Switch(Switch),
    /// Dust on top, for a signal-strength input, entered through the side
    /// its open end points to.
    Dust { entry: Option<String> },
}

/// Every input block, with what feeds it.
pub fn feeds(blocks: &Blocks) -> HashMap<Pos, Feed> {
    let mut feeds: HashMap<Pos, Feed> = dust_inputs(blocks)
        .into_iter()
        .map(|(pos, entry)| (pos, Feed::Dust { entry }))
        .collect();
    feeds.extend(
        blocks
            .switches()
            .into_iter()
            .map(|switch| (switch.block, Feed::Switch(switch))),
    );
    feeds
}

/// The signal-strength inputs: input-colour concrete with no lever or
/// button on it and dust on top, each with the side its signal enters
/// through.
pub fn dust_inputs(blocks: &Blocks) -> HashMap<Pos, Option<String>> {
    let switched: HashSet<Pos> = blocks
        .switches()
        .iter()
        .map(|switch| switch.block)
        .collect();
    blocks
        .iter()
        .filter(|&(pos, state)| {
            lint::is_input(short(state))
                && !switched.contains(&pos)
                && schem::id(blocks.get(schem::step(pos, "up"))) == DUST
        })
        .map(|(pos, _)| (pos, open_end(blocks, schem::step(pos, "up"))))
        .collect()
}

/// The side of the dust at `pos` that leads nowhere, where its driver
/// goes; `None` unless exactly one side does.
fn open_end(blocks: &Blocks, pos: Pos) -> Option<String> {
    let state = blocks.get(pos);
    let mut open =
        ["north", "south", "west", "east"]
            .into_iter()
            .filter(|&side| {
                let next = schem::step(pos, side);
                schem::property(state, side) == Some("side")
                    && blocks.get(next) == schem::AIR
                    && schem::id(blocks.get(schem::step(next, "down"))) != DUST
            });
    match (open.next(), open.next()) {
        (Some(side), None) => Some(side.to_owned()),
        _ => None,
    }
}

pub fn read(blocks: &Blocks) -> Interface {
    let feeds = feeds(blocks);
    let signs = blocks.sign_texts();
    let outputs: Vec<Pos> = blocks
        .iter()
        .filter(|(pos, state)| {
            !feeds.contains_key(pos) && lint::is_output(short(state))
        })
        .map(|(pos, _)| pos)
        .collect();
    Interface {
        inputs: ports(blocks, feeds.keys().copied(), &signs, Some(&feeds)),
        outputs: ports(blocks, outputs, &signs, None),
    }
}

/// Groups `members` into ports, `feeds` being given for inputs. Each
/// control or status line with a name sign is a line of its own. Every
/// other colour is one bus, named by a sign on any of its blocks, or else
/// by the next letter for an input, A for lime, or by its place among the
/// output colours.
fn ports(
    blocks: &Blocks,
    members: impl IntoIterator<Item = Pos>,
    signs: &HashMap<Pos, &[String]>,
    feeds: Option<&HashMap<Pos, Feed>>,
) -> Vec<Port> {
    let (order, line): (&[&str], _) = if feeds.is_some() {
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
                let mut port = port(blocks, name, vec![pos], lines, feeds);
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
                let what = if feeds.is_some() { "control" } else { "status" };
                format!("unnamed {what}")
            } else if let Some(given) = given {
                given
            } else if feeds.is_some() {
                letters.next().map(String::from).unwrap_or_default()
            } else if output_count == 1 {
                "output".to_owned()
            } else {
                format!("output {}", i + 1)
            };
            port(blocks, name, members, &lines, feeds)
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
    feeds: Option<&HashMap<Pos, Feed>>,
) -> Port {
    let [width, _, length] = blocks.size;
    let corner = [width - 1, 0, length - 1];
    if let Some(&lsb) = members.iter().min_by_key(|&&pos| distance(pos, corner))
    {
        members.sort_by_key(|&pos| distance(pos, lsb));
    }
    let inverted_line = lines.iter().any(|l| l.trim() == "inverted");
    let feed = |pos: Pos| feeds.and_then(|feeds| feeds.get(&pos));
    let signal_strength = !members.is_empty()
        && members
            .iter()
            .all(|&pos| matches!(feed(pos), Some(Feed::Dust { .. })));
    let bits = members
        .iter()
        .map(|&pos| {
            let lever = match feed(pos) {
                Some(Feed::Switch(lever)) => Some(lever),
                _ => None,
            };
            Bit {
                pos,
                entry: match feed(pos) {
                    Some(Feed::Switch(lever)) => Some(lever.side.clone()),
                    Some(Feed::Dust { entry }) => entry.clone(),
                    None => None,
                },
                hard: lever.is_some()
                    && DIRECTIONS.iter().any(|&d| {
                        schem::id(blocks.get(schem::step(pos, d))) == DUST
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
        signal_strength,
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

/// The port a sign names: its first line with a name on it, so a name
/// written on any line counts, as people often centre one word on the
/// second. A token is no name: older builds marked their least significant
/// bit with an `lsb` sign, which the corner rule replaces, and `4ss` gives a
/// strength.
pub fn sign_name(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .map(|line| line.trim().to_lowercase())
        .find(|line| {
            !line.is_empty() && line != "lsb" && strength(line).is_none()
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: [&str; 4]) -> Vec<String> {
        text.map(str::to_owned).to_vec()
    }

    #[test]
    fn a_sign_names_on_whichever_line_has_a_name() {
        assert_eq!(
            sign_name(&lines(["Next", "", "", ""])).as_deref(),
            Some("next")
        );
        assert_eq!(
            sign_name(&lines(["", "next", "", ""])).as_deref(),
            Some("next")
        );
        assert_eq!(
            sign_name(&lines(["lsb", "addend", "", ""])).as_deref(),
            Some("addend")
        );
        assert_eq!(sign_name(&lines(["", "4ss", "", ""])), None);
        assert_eq!(sign_name(&lines(["", "", "", ""])), None);
    }
}
