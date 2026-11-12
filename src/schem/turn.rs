//! Turning and mirroring a build's file about the vertical. It never turns
//! upside down, which redstone does not survive.
//!
//! Redpiler, and so the simulator, works a mirrored build as it does the
//! original. Turning and mirroring rewrite every block's `facing`, `axis`,
//! the sides dust and walls join on, a standing sign's `rotation`, a door's
//! `hinge`, and a stair's corner `shape`.

use std::collections::HashMap;

use anyhow::Result;

use super::{self as schem, AIR, Blocks, Direction, Pos};

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
    nbt.move_roles(|pos| Some(place(pos)));
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
}
