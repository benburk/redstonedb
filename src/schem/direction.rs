//! The six ways from a block to a neighbour, and the neighbour each one
//! leads to.

use super::{Pos, blocks::property};

/// One of the six ways from a block to a neighbour, as a block state's
/// `facing` and the side a lever or sign is on name them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[must_use]
pub enum Direction {
    North,
    South,
    West,
    East,
    Up,
    Down,
}

impl Direction {
    /// Every direction.
    pub const ALL: [Direction; 6] = [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
        Direction::Up,
        Direction::Down,
    ];

    /// The four level directions, the sides of a block as dust joins them.
    pub const SIDES: [Direction; 4] = [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    /// The direction a block state's name for it gives, such as `north`.
    pub fn parse(name: &str) -> Option<Self> {
        Direction::ALL.into_iter().find(|d| d.name() == name)
    }

    /// The way the block in `state` faces, if it has a `facing`.
    pub fn facing(state: &str) -> Option<Self> {
        property(state, "facing").and_then(Direction::parse)
    }

    /// Its name in a block state, such as `north`.
    pub fn name(self) -> &'static str {
        match self {
            Direction::North => "north",
            Direction::South => "south",
            Direction::West => "west",
            Direction::East => "east",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    /// One block toward it: north is toward negative z, west toward
    /// negative x.
    pub fn offset(self) -> Pos {
        match self {
            Direction::North => [0, 0, -1],
            Direction::South => [0, 0, 1],
            Direction::West => [-1, 0, 0],
            Direction::East => [1, 0, 0],
            Direction::Up => [0, 1, 0],
            Direction::Down => [0, -1, 0],
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }

    /// A quarter turn clockwise seen from above; up and down stay.
    pub fn turned(self) -> Self {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            other => other,
        }
    }

    /// Mirrored east for west.
    pub fn mirrored(self) -> Self {
        match self {
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            other => other,
        }
    }

    /// Whether it runs north and south rather than east and west, up, or
    /// down.
    pub fn north_south(self) -> bool {
        matches!(self, Direction::North | Direction::South)
    }
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The neighbour of `pos` one block toward `direction`.
pub fn step(pos: Pos, direction: Direction) -> Pos {
    add(pos, direction.offset())
}

/// `a` moved by `b`.
pub fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// How many blocks `a` is from `b`, stepping along the axes.
pub fn distance(a: Pos, b: Pos) -> i32 {
    (0..3).map(|axis| (a[axis] - b[axis]).abs()).sum()
}
