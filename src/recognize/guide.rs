//! The example builds the How builds are recognized page pictures, one or two for
//! each rule a build must keep, each kept or broken as its caption says.
//! A test holds every one to the site's own check, so a picture can't say
//! what the check doesn't.

use std::collections::HashMap;

use crate::schem::{Blocks, Pos};

/// One example: what it is called, what its caption says, whether it keeps
/// the rule, the rule's name in the check's report when it breaks it, and
/// its blocks.
pub struct Example {
    pub name: &'static str,
    pub caption: &'static str,
    pub kept: bool,
    pub breaks: Option<&'static str>,
    pub blocks: Vec<(Pos, &'static str)>,
}

impl Example {
    /// The example as a schematic, its box fitted to its blocks, its dust
    /// shaped as the game places it.
    pub fn schematic(&self) -> Blocks {
        let size = [0, 1, 2].map(|axis| {
            self.blocks
                .iter()
                .map(|(pos, _)| pos[axis])
                .max()
                .unwrap_or(0)
                + 1
        });
        let mut states: HashMap<Pos, String> = self
            .blocks
            .iter()
            .map(|&(pos, state)| (pos, format!("minecraft:{state}")))
            .collect();
        let placed = Blocks::from_grid(size, &states);
        states.extend(crate::schem::dust::reshaped(&placed));
        Blocks::from_grid(size, &states)
    }
}

const WHITE: &str = "white_concrete";
const YELLOW: &str = "yellow_concrete";
const LEVER: &str = "lever[face=wall,facing=south,powered=true]";
const DUST_NS: &str =
    "redstone_wire[east=none,north=side,power=15,south=side,west=none]";
const DUST_EW: &str =
    "redstone_wire[east=side,north=none,power=15,south=none,west=side]";
const TORCH_EAST: &str = "redstone_wall_torch[facing=east,lit=false]";
const REPEATER_IN: &str =
    "repeater[delay=1,facing=south,locked=false,powered=true]";

/// An input at `x`, `y`: its block, its lever on the south side, and the
/// dust it drives to the north, on a block of its own.
fn input(x: i32, y: i32) -> Vec<(Pos, &'static str)> {
    coloured_input(x, y, WHITE)
}

/// An input as `input` makes it, its block `block`, with dust on top.
fn dust_on_top(block: &'static str) -> Vec<(Pos, &'static str)> {
    [coloured_input(1, 1, block), vec![([1, 2, 1], DUST_EW)]].concat()
}

/// An input as `input` makes it, its block `block`.
fn coloured_input(
    x: i32,
    y: i32,
    block: &'static str,
) -> Vec<(Pos, &'static str)> {
    vec![
        ([x, y, 1], block),
        ([x, y, 2], LEVER),
        ([x, y - 1, 0], WHITE),
        ([x, y, 0], DUST_NS),
    ]
}

/// An output: dust coming from the west along white blocks onto a block
/// of `colour` at `x`.
fn output(x: i32, z: i32, colour: &'static str) -> Vec<(Pos, &'static str)> {
    vec![
        ([x - 1, 0, z], WHITE),
        ([x - 1, 1, z], DUST_EW),
        ([x, 0, z], colour),
        ([x, 1, z], DUST_EW),
    ]
}

/// Every example, in the order the page shows them.
pub fn examples() -> Vec<Example> {
    let bus = |xs: &[i32], ys: &[i32]| -> Vec<(Pos, &'static str)> {
        xs.iter().zip(ys).flat_map(|(&x, &y)| input(x, y)).collect()
    };
    vec![
        Example {
            name: "input",
            caption: "A lever on the side of a solid block, which drives one dust and a torch.",
            kept: true,
            breaks: None,
            blocks: [input(1, 1), vec![([2, 1, 1], TORCH_EAST)]].concat(),
        },
        Example {
            name: "input-dust-below",
            caption: "Dust under an input's block, which takes its power as dust beside it does.",
            kept: true,
            breaks: None,
            blocks: [
                input(1, 2),
                vec![([1, 1, 1], DUST_EW), ([1, 0, 1], WHITE)],
            ]
            .concat(),
        },
        Example {
            name: "repeater",
            caption: "Where the lever was, a repeater on a block drives the input as the lever did.",
            kept: true,
            breaks: None,
            blocks: vec![
                ([1, 1, 1], WHITE),
                ([1, 0, 2], WHITE),
                ([1, 1, 2], REPEATER_IN),
                ([1, 0, 0], WHITE),
                ([1, 1, 0], DUST_NS),
            ],
        },
        Example {
            name: "data-dust-on-top",
            caption: "Dust on top of an input's block.",
            kept: true,
            breaks: None,
            blocks: dust_on_top("lime_concrete"),
        },
        Example {
            name: "output",
            caption: "Dust on a block that no block like it touches.",
            kept: true,
            breaks: None,
            blocks: output(1, 0, YELLOW),
        },
        Example {
            name: "outputs-touching",
            caption: "Two outputs of one block touching.",
            kept: false,
            breaks: Some("no outputs"),
            blocks: [output(1, 0, YELLOW), output(1, 1, YELLOW)].concat(),
        },
        Example {
            name: "bus-straight",
            caption: "A bus in a straight line, evenly spaced.",
            kept: true,
            breaks: None,
            blocks: bus(&[0, 2, 4, 6], &[1, 1, 1, 1]),
        },
        Example {
            name: "bus-diagonal",
            caption: "A diagonal bus, rising one block for each block across.",
            kept: true,
            breaks: None,
            blocks: bus(&[0, 1, 2, 3], &[1, 2, 3, 4]),
        },
        Example {
            name: "bus-alternating",
            caption: "A bus of two lines, its bits alternating between them.",
            kept: true,
            breaks: None,
            blocks: bus(&[0, 1, 2, 3, 4, 5], &[1, 2, 1, 2, 1, 2]),
        },
        Example {
            name: "bus-uneven",
            caption: "A bus spaced unevenly.",
            kept: false,
            breaks: None,
            blocks: bus(&[0, 2, 5, 7], &[1, 1, 1, 1]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recognize::inspect;

    /// Each kept example passes the rule it shows; each broken one fails
    /// it, by name where the check names it.
    #[test]
    fn every_example_shows_what_the_check_does() {
        for example in examples() {
            let report = inspect::inspect(&example.schematic());
            let rules: Vec<&str> =
                report.errors.iter().map(|p| p.rule).collect();
            if let Some(rule) = example.breaks {
                assert!(rules.contains(&rule), "{}: {rules:?}", example.name);
            }
            if example.kept {
                for rule in [
                    "input block wiring",
                    "no room for a repeater",
                    "switch touches redstone",
                ] {
                    assert!(
                        !rules.contains(&rule),
                        "{} breaks {rule}",
                        example.name
                    );
                }
            }
        }
    }

    /// Every example has its picture built into the site.
    #[test]
    fn every_example_is_pictured() {
        for example in examples() {
            assert!(
                crate::web::guide_image(example.name).is_some(),
                "{} has no picture",
                example.name
            );
        }
    }

    /// A kept bus is found as one run of levers; an uneven one isn't.
    #[test]
    fn buses_are_found_as_they_are_drawn() {
        let runs = |name: &str| {
            let example =
                examples().into_iter().find(|e| e.name == name).unwrap();
            let mut sizes: Vec<usize> = inspect::inspect(&example.schematic())
                .inputs
                .iter()
                .map(|run| run.bits.len())
                .collect();
            sizes.sort_unstable();
            sizes
        };
        assert_eq!(runs("bus-straight"), [4]);
        assert_eq!(runs("bus-diagonal"), [4]);
        assert_eq!(runs("bus-alternating"), [3, 3]);
        assert_ne!(runs("bus-uneven"), [4]);
    }
}
