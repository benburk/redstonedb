//! Turning and mirroring a build so it lies the standard way, as
//! `conventions.md` lays it out: A's inputs entered from the south, and its
//! least significant bits at the south-east-bottom corner, so its buses
//! count up as they should.
//!
//! A build turns about the vertical until A's levers are on the south side
//! of their blocks. It is
//! then mirrored east for west, which keeps them south, when that makes
//! more of the quick cases of the test it seems to be come out right, as
//! it does for a build whose least significant bits sit at its west end. It
//! never turns upside down, which redstone does not survive; a build whose
//! least significant bits sit at its top says so with an `lsb` sign.
//!
//! Redpiler, and so the simulator, works a mirrored build as it does the
//! original. Turning and mirroring rewrite every block's `facing`, `axis`,
//! the sides dust and walls join on, a standing sign's `rotation`, a door's
//! `hinge`, and a stair's corner `shape`.

use std::collections::HashMap;

use anyhow::Result;

use crate::{
    interface,
    schem::{self, AIR, Blocks, Direction, Pos},
    simulate,
};

/// How a build was laid down the standard way: mirrored east for west
/// first, then turned clockwise, seen from above, a quarter at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Turn {
    pub mirrored: bool,
    /// Quarter turns, 0 to 3.
    pub quarters: u8,
}

impl Turn {
    pub fn is_none(self) -> bool {
        self == Turn::default()
    }

    /// What was done, as a sentence begins, such as `Mirrored east for west
    /// and turned a half turn`; empty when nothing was.
    pub fn words(self) -> String {
        let turned = match self.quarters {
            1 => Some("turned a quarter turn clockwise"),
            2 => Some("turned a half turn"),
            3 => Some("turned a quarter turn anticlockwise"),
            _ => None,
        };
        let mirrored = self.mirrored.then_some("mirrored east for west");
        let words = [mirrored, turned]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" and ");
        let mut chars = words.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    }

    fn direction(self, mut d: Direction) -> Direction {
        if self.mirrored {
            d = d.mirrored();
        }
        for _ in 0..self.quarters {
            d = d.turned();
        }
        d
    }
}

/// The build in `data` laid the standard way, with how it was turned; the
/// file is returned unchanged when it already lies so.
pub fn orient(data: &[u8]) -> Result<(Vec<u8>, Turn)> {
    let blocks = Blocks::read(data)?;
    let entered = entered(&blocks);
    // Each way of mirroring, turned so A is entered from the south.
    let candidates = [false, true].map(|mirrored| {
        let quarters = entered.map_or(0, |side| {
            let side = if mirrored { side.mirrored() } else { side };
            quarters_to_south(side)
        });
        Turn { mirrored, quarters }
    });
    let laid = |turn: Turn| -> Result<Vec<u8>> {
        if turn.is_none() {
            Ok(data.to_vec())
        } else {
            turned(data, turn)
        }
    };
    let wrong = |file: &[u8]| Blocks::read(file).ok().and_then(|b| wrong(&b));
    let plain = laid(candidates[0])?;
    let plain_wrong = wrong(&plain);
    if plain_wrong == Some(0) {
        return Ok((plain, candidates[0]));
    }
    let mirrored = laid(candidates[1])?;
    let better = match (plain_wrong, wrong(&mirrored)) {
        (None, Some(_)) => true,
        (Some(plain), Some(mirrored)) => mirrored < plain,
        _ => false,
    };
    Ok(if better {
        (mirrored, candidates[1])
    } else {
        (plain, candidates[0])
    })
}

/// The share of quick cases, in thousandths, the build in `blocks` gets
/// wrong as what it seems to be; `None` for a build not recognized, which
/// is only turned.
fn wrong(blocks: &Blocks) -> Option<u64> {
    let guess = simulate::guess(blocks)?;
    Some(guess.wrong * 1000 / guess.cases.max(1))
}

/// The side A's levers are on, when every one is on the same side, north,
/// south, east, or west; for a build with no A, as a T flip-flop, the side
/// its every other input's lever or button is on. A signal-strength A has
/// none, as its dust may be entered from any side.
fn entered(blocks: &Blocks) -> Option<Direction> {
    let found = interface::read(blocks);
    let a = found.inputs.iter().find(|port| {
        !port.labelled
            && port.block.starts_with("lime")
            && !port.signal_strength
    });
    let ports: Vec<_> = match a {
        Some(a) => vec![a],
        None => found
            .inputs
            .iter()
            .filter(|port| !port.signal_strength)
            .collect(),
    };
    let mut sides = ports
        .iter()
        .flat_map(|port| &port.bits)
        .map(|bit| bit.entry);
    let first = sides.next()??;
    (sides.all(|side| side == Some(first))
        && !matches!(first, Direction::Up | Direction::Down))
    .then_some(first)
}

/// The quarter turns clockwise that bring `side` round to the south.
fn quarters_to_south(side: Direction) -> u8 {
    let mut d = side;
    let mut quarters = 0;
    while d != Direction::South && quarters < 4 {
        d = d.turned();
        quarters += 1;
    }
    quarters % 4
}

/// The build in `data` turned as `turn` says, its box fitted to it.
pub fn turned(data: &[u8], turn: Turn) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let [width, height, length] = blocks.size;
    let place = |pos: Pos| -> Pos {
        let [mut x, y, mut z] = pos;
        let (mut across, mut along) = (width, length);
        if turn.mirrored {
            x = across - 1 - x;
        }
        for _ in 0..turn.quarters {
            // A quarter clockwise from above takes north to east.
            (x, z) = (along - 1 - z, x);
            (across, along) = (along, across);
        }
        [x, y, z]
    };
    let size = if turn.quarters % 2 == 1 {
        [length, height, width]
    } else {
        [width, height, length]
    };
    let grid: HashMap<Pos, String> = blocks
        .iter()
        .filter(|(_, state)| *state != AIR)
        .map(|(pos, state)| (place(pos), turned_state(state, turn)))
        .collect();
    let mut nbt = schem::decode(data)?;
    let entities = nbt
        .take_entities()
        .iter()
        .filter_map(|entity| {
            Some(schem::with_pos(entity, place(schem::entity_pos(entity)?)))
        })
        .collect();
    nbt.write_blocks([0, 0, 0], size, &grid, entities)?;
    schem::encode(nbt)
}

/// A block state turned as `turn` says: its facing, axis, joining sides,
/// rotation, hinge, and stair corner.
fn turned_state(state: &str, turn: Turn) -> String {
    let Some((id, properties)) = state.split_once('[') else {
        return state.to_owned();
    };
    let quarter_turned = turn.quarters % 2 == 1;
    let stairs = id.ends_with("_stairs");
    let turned: Vec<String> = properties
        .trim_end_matches(']')
        .split(',')
        .map(|pair| {
            let Some((key, value)) = pair.split_once('=') else {
                return pair.to_owned();
            };
            let value = match key {
                "facing" => Direction::parse(value)
                    .map_or(value, |d| turn.direction(d).name())
                    .to_owned(),
                "axis" if quarter_turned => match value {
                    "x" => "z".to_owned(),
                    "z" => "x".to_owned(),
                    other => other.to_owned(),
                },
                "rotation" => {
                    value.parse::<u8>().map_or(value.to_owned(), |r| {
                        let r = if turn.mirrored { (16 - r) % 16 } else { r };
                        ((r + 4 * turn.quarters) % 16).to_string()
                    })
                }
                "hinge" | "shape"
                    if turn.mirrored && (stairs || key == "hinge") =>
                {
                    swap_hand(value)
                }
                _ => value.to_owned(),
            };
            // Dust, walls, and panes join on their sides, which turn too.
            let key = Direction::parse(key)
                .map_or(key, |side| turn.direction(side).name());
            format!("{key}={value}")
        })
        .collect();
    let mut turned = turned;
    // In the game's order, by name, as the sides moved.
    turned.sort();
    format!("{id}[{}]", turned.join(","))
}

/// `left` for `right` and back, within a value such as `inner_left`.
fn swap_hand(value: &str) -> String {
    if let Some(rest) = value.strip_suffix("left") {
        format!("{rest}right")
    } else if let Some(rest) = value.strip_suffix("right") {
        format!("{rest}left")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF: Turn = Turn {
        mirrored: false,
        quarters: 2,
    };

    #[test]
    fn a_turn_turns_facings_and_the_sides_dust_joins_on() {
        let quarter = Turn {
            mirrored: false,
            quarters: 1,
        };
        assert_eq!(
            turned_state(
                "minecraft:repeater[delay=1,facing=north,locked=false,powered=false]",
                quarter
            ),
            "minecraft:repeater[delay=1,facing=east,locked=false,powered=false]"
        );
        assert_eq!(
            turned_state(
                "minecraft:redstone_wire[east=none,north=side,power=0,south=up,west=none]",
                quarter
            ),
            "minecraft:redstone_wire[east=side,north=none,power=0,south=none,west=up]"
        );
        assert_eq!(
            turned_state("minecraft:oak_log[axis=x]", quarter),
            "minecraft:oak_log[axis=z]"
        );
        assert_eq!(
            turned_state(
                "minecraft:oak_sign[rotation=0,waterlogged=false]",
                HALF
            ),
            "minecraft:oak_sign[rotation=8,waterlogged=false]"
        );
    }

    #[test]
    fn a_mirror_swaps_east_and_west_and_left_and_right() {
        let mirror = Turn {
            mirrored: true,
            quarters: 0,
        };
        assert_eq!(
            turned_state(
                "minecraft:lever[face=wall,facing=east,powered=false]",
                mirror
            ),
            "minecraft:lever[face=wall,facing=west,powered=false]"
        );
        assert_eq!(
            turned_state(
                "minecraft:lever[face=wall,facing=south,powered=false]",
                mirror
            ),
            "minecraft:lever[face=wall,facing=south,powered=false]"
        );
        assert_eq!(
            turned_state(
                "minecraft:oak_stairs[facing=north,half=bottom,shape=inner_left,waterlogged=false]",
                mirror
            ),
            "minecraft:oak_stairs[facing=north,half=bottom,shape=inner_right,waterlogged=false]"
        );
        assert_eq!(
            turned_state(
                "minecraft:oak_sign[rotation=4,waterlogged=false]",
                mirror
            ),
            "minecraft:oak_sign[rotation=12,waterlogged=false]"
        );
    }

    #[test]
    fn a_side_turns_round_to_the_south() {
        assert_eq!(quarters_to_south(Direction::South), 0);
        assert_eq!(quarters_to_south(Direction::East), 1);
        assert_eq!(quarters_to_south(Direction::North), 2);
        assert_eq!(quarters_to_south(Direction::West), 3);
    }

    #[test]
    fn words_say_what_was_done() {
        assert_eq!(Turn::default().words(), "");
        assert_eq!(
            Turn {
                mirrored: true,
                quarters: 2
            }
            .words(),
            "Mirrored east for west and turned a half turn"
        );
    }

    /// A T flip-flop has no A, so it is turned by its toggle's button,
    /// which ends up on the south side of its block whichever way it came.
    #[test]
    fn a_build_with_no_a_turns_by_its_control_line() {
        let data = std::fs::read("tests/fixtures/t-flip-flop.schem").unwrap();
        let probed = crate::probe::recognize(&data).unwrap();
        assert!(probed.found.is_some(), "{:?}", probed.problems);
        for quarters in 0..4 {
            let turn = Turn {
                mirrored: false,
                quarters,
            };
            let (laid, _) =
                orient(&turned(&probed.file, turn).unwrap()).unwrap();
            let blocks = Blocks::read(&laid).unwrap();
            assert_eq!(entered(&blocks), Some(Direction::South), "{quarters}");
        }
    }
}
