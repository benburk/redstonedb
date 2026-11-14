//! Testing a register or a RAM: one or more cells of a CPU's register
//! file, or words of memory. A cell takes the data bus, input A, when its
//! write lever is turned on and off, and puts what it holds on an output
//! bus while its read lever is on, and nothing otherwise, since every cell
//! shares the bus. A dual-read cell has two read levers, each with its own
//! output bus. Only one cell is written or read at a time.
//!
//! Its control lines are all levers: a button's press is no write. A sign
//! naming a line write or read says which it is; else a lever is a write
//! when flicking it, the data bus all ones, changes what the buses show,
//! and a read when it changes nothing. Which read belongs to which write
//! is found by writing each cell alone and seeing which reads show it;
//! when that is unclear, as when reads are not gated, they pair in order,
//! and the cases show what is wrong.
//!
//! Each value, every one up to 8 bits and the test patterns and each byte's
//! values beyond, is written into every cell, each cell offset from the
//! last so that they hold different values, each round starting its writes
//! a cell on, so a cell another's write disturbs shows it. Then the data
//! bus changes, which no cell may take without its write, every read off
//! must leave the buses empty, and each read alone must show its cell's
//! value on its bus and nothing on any other.
//!
//! Copies stack across its bits into a bigger register file, more words
//! sharing the data buses, which are driven and read at the first copy's
//! end, or along them into a wider register, sharing the control lines;
//! see `stacking`.
//!
//! A buffer is a register without reads: each cell shows what it holds on
//! its own output bus all the time. Its control lines are its writes, each
//! a lever, and which bus is whose is found by writing each cell alone. Each value is written into every
//! cell, then the data bus changes, and each bus must still show its
//! cell's value, as it is: a register's read may show its cell inverted,
//! but a buffer's output may not. Its timing counts from a write, as a
//! register's counts from a read.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use self::check::check_on;
use crate::{
    category::{self, test::Runs},
    interface::{self, Interface},
    schem::{Blocks, Pos},
    simulate::{Run, Stacker, Stacking, Step, Timing, Value, bench::mask},
};

mod check;
mod throughput;
mod timing;

pub use throughput::{stacker, stacking, stacks_at, throughput};
pub use timing::time;

/// One control line, a lever, by its port and bit, and where its block is.
#[derive(Clone, Copy, Debug)]
struct Line {
    port: usize,
    bit: usize,
    pos: Pos,
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
/// while read, and may be read while another is written; a RAM, whose
/// words are read or written one at a time; or a buffer, each of whose
/// cells shows on its own bus all the time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Register,
    Ram,
    Buffer,
}

impl Kind {
    /// The category it tests for.
    fn category(self) -> category::Kind {
        match self {
            Kind::Register => category::Kind::Register,
            Kind::Ram => category::Kind::Ram,
            Kind::Buffer => category::Kind::Buffer,
        }
    }

    /// Whether its cells show on a shared bus only while read, as a
    /// register's and a RAM's do, rather than all the time, as a buffer's.
    fn reads(self) -> bool {
        self != Kind::Buffer
    }
}

/// A cell: its write lever, and each read lever that puts it on a bus.
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

/// The lines of `blocks`, a `kind` found to be `register`, named as its
/// category names them, each unsigned control line then a write or a read,
/// as its test found it, numbered in order when there are more; a sign's
/// name stands.
fn named(blocks: &Blocks, register: &Register, kind: Kind) -> Interface {
    let mut named = interface::read(blocks);
    interface::name_lines(kind.category().lines(), &mut named);
    let writes: Vec<Pos> =
        register.cells.iter().map(|cell| cell.write.pos).collect();
    let reads: Vec<Pos> = register.reads.iter().map(|line| line.pos).collect();
    for port in &mut named.inputs {
        let Some(pos) = port.bits.first().map(|bit| bit.pos) else {
            continue;
        };
        if !port.control() || port.labelled {
            continue;
        }
        for (lines, word) in [(&writes, "write"), (&reads, "read")] {
            if let Some(i) = lines.iter().position(|&at| at == pos) {
                port.name = if lines.len() > 1 {
                    format!("{word} {}", i + 1)
                } else {
                    word.to_owned()
                };
            }
        }
    }
    named
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

/// A register's or a buffer's test: each value written into every cell,
/// so passing it is holding state. A register's copies may add words on
/// its shared bus as well as bits.
impl Runs for Kind {
    fn check(&self, blocks: &Blocks, quick: bool) -> Result<Run> {
        check(blocks, *self, quick)
    }

    fn remembers(&self, _blocks: &Blocks) -> Result<Option<bool>> {
        Ok(Some(true))
    }

    fn stacker<'a>(&self, data: &'a [u8]) -> Option<Stacker<'a>> {
        stacker(data, *self)
    }

    fn stacking(&self, data: &[u8]) -> Option<Stacking> {
        stacking(data, *self)
    }

    fn stacks_at(
        &self,
        data: &[u8],
        step: Step,
        copies: usize,
    ) -> Option<Result<(Step, u64), String>> {
        stacks_at(data, *self, step, copies)
            .map(|found| found.map(|(step, cases, _)| (step, cases)))
    }

    fn throughput(
        &self,
        _blocks: &Blocks,
        data: &[u8],
        timing: &Timing,
    ) -> Result<Option<i32>> {
        throughput(data, timing, *self)
    }

    fn lines(&self, blocks: &Blocks) -> Result<Interface> {
        let interface = interface::read(blocks);
        let (_, register) = check::ports(blocks, &interface, *self)?;
        Ok(named(blocks, &register, *self))
    }

    fn time(
        &self,
        blocks: &Blocks,
        _patterned: bool,
        cancel: &AtomicBool,
    ) -> Result<Timing> {
        time(blocks, *self, cancel)
    }
}
