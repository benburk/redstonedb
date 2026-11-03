//! Testing a register: one or more cells of a CPU's register file. A cell
//! takes the data bus, input A, when its write button is pressed, and
//! puts what it holds on an output bus while its read lever is on, and
//! nothing otherwise, since every cell shares the bus. A dual-read cell has
//! two read levers, each with its own output bus.
//!
//! Of its control lines, one with a button is a write, and one with a
//! lever a read. Which read belongs to which write is found by
//! writing each cell alone and seeing which reads show it; when that is
//! unclear, as when reads are not gated, they pair in order, and the cases
//! show what is wrong.
//!
//! Each value, every one up to 8 bits and the test patterns and each byte's
//! values beyond, is written into every cell, each cell offset from the
//! last so that they hold different values, each round starting its writes
//! a cell on, so a cell another's write disturbs shows it. Then the data
//! bus changes, which no cell may take without its button, every read off
//! must leave the buses empty, and each read alone must show its cell's
//! value on its bus and nothing on any other.
//!
//! Copies stack west into a bigger register file, sharing the data buses,
//! which are driven and read at the first copy's end, or into a wider
//! register, sharing the control lines; see `stacking`.
//!
//! A buffer is a register without reads: each cell shows what it holds on
//! its own output bus all the time. Its control lines are writes, by a
//! button, or by a lever when none has a button, and which bus is whose is
//! found by writing each cell alone. Each value is written into every
//! cell, then the data bus changes, and each bus must still show its
//! cell's value, as it is: a register's read may show its cell inverted,
//! but a buffer's output may not. Its timing counts from a write, as a
//! register's counts from a read.

use anyhow::Result;

use self::check::check_on;
use super::{Run, Value, bench::mask};
use crate::{
    category,
    schem::{Blocks, Pos},
};

mod check;
mod throughput;
mod timing;

pub use throughput::{stacking, stacks_at, throughput};
pub use timing::time;

/// One control line, by its port and bit, where its block is, and whether
/// a button drives it rather than a lever.
#[derive(Clone, Copy, Debug)]
struct Line {
    port: usize,
    bit: usize,
    pos: Pos,
    button: bool,
    /// A read that reads with its lever off, and stops when it is on:
    /// inverted.
    active_low: bool,
}

impl Line {
    fn is(&self, other: &Line) -> bool {
        (self.port, self.bit) == (other.port, other.bit)
    }
}

/// Which a build is: a register, whose cells show on a shared bus only
/// while read, or a buffer, each of whose cells shows on its own bus all
/// the time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Register,
    Buffer,
}

impl Kind {
    /// The category it tests for.
    fn category(self) -> category::Kind {
        match self {
            Kind::Register => category::Kind::Register,
            Kind::Buffer => category::Kind::Buffer,
        }
    }
}

/// A cell: its write button, and each read lever that puts it on a bus.
struct Cell {
    write: Line,
    reads: Vec<Read>,
}

/// A read lever, the bus it puts its cell on, by its place among the
/// register's, and whether it puts the cell's value there inverted. A
/// buffer's cell has no lever, `None`, showing on its bus all the time.
#[derive(Clone, Copy)]
struct Read {
    line: Option<Line>,
    bus: usize,
    inverted: bool,
}

impl Read {
    /// What its bus shows of a cell holding `held`, of `width` bits.
    fn shows(&self, held: Value, width: usize) -> Value {
        if self.inverted {
            !held & mask(width)
        } else {
            held
        }
    }
}

/// A register's ports, set up in its circuit.
struct Register {
    /// The data input, by its place in the interface.
    data: usize,
    width: usize,
    /// The output buses, by their place in the interface.
    buses: Vec<usize>,
    /// The control ports, by their place in the interface, set together.
    controls: Vec<usize>,
    /// Every read line, each held where it does not read.
    reads: Vec<Line>,
    cells: Vec<Cell>,
    /// What each bus shows with no read on, which no cell may change: 0,
    /// or all ones for an inverted bus.
    idle: Vec<Value>,
}

/// The offset between the values one round writes into successive cells:
/// odd, so each cell takes every value as the round's does.
const STRIDE: Value = 0x9d;

/// Runs the register or buffer in `blocks` through its every case, or when
/// `quick` a few telling ones, for a guess at what a build is.
pub fn check(blocks: &Blocks, kind: Kind, quick: bool) -> Result<Run> {
    Ok(check_on(blocks, kind, quick)?.0)
}

/// How many cells a register has, and of how many bits.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shape {
    cells: usize,
    width: usize,
}
