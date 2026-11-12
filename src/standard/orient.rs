//! Laying a build the standard way, as `docs/conventions.md` lays it out:
//! A's inputs entered from the south, and its least significant bits at the
//! south-east-bottom corner, so its buses count up as they should.
//!
//! A build turns about the vertical until A's levers are on the south side
//! of their blocks. It is then mirrored east for west, which keeps them
//! south, when that makes more of the quick cases of the test it seems to
//! be come out right, as it does for a build whose least significant bits
//! sit at its west end. A build whose least significant bits sit at its
//! top says so with an `lsb` sign.

use anyhow::Result;

use crate::{
    category, interface,
    schem::{Blocks, Direction, turn::Turn},
};

/// The build in `data` laid the standard way, with how it was turned; the
/// file is returned unchanged when it already lies so.
pub fn orient(data: &[u8]) -> Result<(Vec<u8>, Turn)> {
    let (laid, turn) = oriented(&Blocks::read(data)?)?;
    let file = match laid {
        Some(laid) => laid.encode()?,
        None => data.to_vec(),
    };
    Ok((file, turn))
}

/// `blocks` laid the standard way, with how they were turned; `None` for
/// the blocks when they already lie so.
pub fn oriented(blocks: &Blocks) -> Result<(Option<Blocks>, Turn)> {
    let entered = entered(blocks);
    // Each way of mirroring, turned so A is entered from the south.
    let candidates = [false, true].map(|mirrored| {
        let quarters = entered.map_or(0, |side| {
            let side = if mirrored { side.mirrored() } else { side };
            quarters_to_south(side)
        });
        Turn { mirrored, quarters }
    });
    let laid = |turn: Turn| -> Result<Option<Blocks>> {
        if turn.is_none() {
            Ok(None)
        } else {
            blocks.turned(turn).map(Some)
        }
    };
    let wrong = |laid: &Option<Blocks>| wrong(laid.as_ref().unwrap_or(blocks));
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
    category::guess(blocks).map(|guess| guess.wrong_share())
}

/// The side A's levers are on, when every one is on the same side, north,
/// south, east, or west; for a build with no A, as a T flip-flop, the side
/// its every other input's lever or button is on. A signal-strength A has
/// none, as its dust may be entered from any side.
fn entered(blocks: &Blocks) -> Option<Direction> {
    let found = interface::read(blocks);
    let a = found
        .inputs
        .iter()
        .find(|port| !port.labelled && port.a() && !port.signal_strength);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_side_turns_round_to_the_south() {
        assert_eq!(quarters_to_south(Direction::South), 0);
        assert_eq!(quarters_to_south(Direction::East), 1);
        assert_eq!(quarters_to_south(Direction::North), 2);
        assert_eq!(quarters_to_south(Direction::West), 3);
    }

    /// A T flip-flop has no A, so it is turned by its toggle's button,
    /// which ends up on the south side of its block whichever way it came.
    #[test]
    fn a_build_with_no_a_turns_by_its_control_line() {
        let data = std::fs::read("tests/fixtures/t-flip-flop.schem").unwrap();
        let probed = crate::recognize::probe::recognize(&data).unwrap();
        assert!(probed.found.is_some(), "{:?}", probed.problems);
        for quarters in 0..4 {
            let turn = Turn {
                mirrored: false,
                quarters,
            };
            let (laid, _) = orient(
                &crate::schem::turn::turned(&probed.file, turn).unwrap(),
            )
            .unwrap();
            let blocks = Blocks::read(&laid).unwrap();
            assert_eq!(entered(&blocks), Some(Direction::South), "{quarters}");
        }
    }
}
