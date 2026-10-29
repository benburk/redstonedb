//! Testing a register: one or more cells of a CPU's register file. A cell
//! takes the data bus, lime input A, when its write button is pressed, and
//! puts what it holds on an output bus, yellow, while its read lever is
//! on, and nothing otherwise, since every cell shares the bus. A dual-read
//! cell has two read levers, each with its own output bus.
//!
//! Every control line is magenta: one with a button is a write, and one
//! with a lever a read. Which read belongs to which write is found by
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
//! its own output bus all the time. Its magenta lines are writes, by a
//! button, or by a lever when none has a button, and which bus is whose is
//! found by writing each cell alone. Each value is written into every
//! cell, then the data bus changes, and each bus must still show its
//! cell's value. Its timing counts from a write, as a register's counts
//! from a read.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};

use super::{
    Action, Broken, CellControls, Run, Signal, Wrong,
    bench::{mask, patterns},
    circuit::{Circuit, MAX_TICKS, output_bits},
    stacking::{Stacking, Step},
    timing::{BitTiming, InputTiming, OutputTiming, Start, Takes, Timing},
};
use crate::{
    interface::{self, Interface},
    lint,
    models::schematic::Orientation,
    schem::{Blocks, Pos},
    stack,
};

/// One magenta line, by its port and bit, where its block is, and whether
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
    fn name(self) -> &'static str {
        match self {
            Kind::Register => "register",
            Kind::Buffer => "buffer",
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
    fn shows(&self, held: u64, width: usize) -> u64 {
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
    idle: Vec<u64>,
}

/// The offset between the values one round writes into successive cells:
/// odd, so each cell takes every value as the round's does.
const STRIDE: u64 = 0x9d;

/// The most copies a register file is stacked to; one that works with
/// this many is taken to stack any number of times.
const MOST_COPIES: usize = 16;

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

/// The run, and the shape of the register it found.
fn check_on(blocks: &Blocks, kind: Kind, quick: bool) -> Result<(Run, Shape)> {
    let interface = interface::read(blocks);
    let (mut circuit, register) = ports(blocks, &interface, kind)?;
    let (values, exhaustive) = if quick {
        (telling(register.width), false)
    } else {
        values(register.width)
    };
    let all = mask(register.width);
    let mut tally = Tally::new(&interface, &register.buses);
    let mut ungated = 0;
    let cells = register.cells.len();
    for (round, &value) in values.iter().enumerate() {
        let stored: Vec<u64> = (0..cells as u64)
            .map(|cell| value.wrapping_add(cell * STRIDE) & all)
            .collect();
        // Each round starts its writes a cell on, so every cell is written
        // after every other, and one a later write disturbs shows it.
        for i in 0..cells {
            let cell = (round + i) % cells;
            write(
                &mut circuit,
                &register,
                register.cells[cell].write,
                stored[cell],
            )?;
        }
        // No cell may take the bus without its button.
        circuit.set(register.data, !value & all);
        circuit.settle()?;
        let none = shown(&register, &stored);
        let got = buses(&circuit, &register);
        ungated += u64::from(got != none);
        let off = match kind {
            Kind::Register => "every read off",
            Kind::Buffer => "data in changed",
        };
        tally.check(|| off.to_owned(), value, &none, got);
        for (i, (cell, &held)) in register.cells.iter().zip(&stored).enumerate()
        {
            let levers: Vec<&Read> = cell
                .reads
                .iter()
                .filter(|read| read.line.is_some())
                .collect();
            for read_line in &levers {
                read(&mut circuit, &register, read_line.line.as_slice())?;
                let mut expected = none.clone();
                expected[read_line.bus] = read_line.shows(held, register.width);
                let got = buses(&circuit, &register);
                tally.check(
                    || format!("cell {} read", i + 1),
                    held,
                    &expected,
                    got,
                );
            }
            if levers.len() > 1 {
                let lines: Vec<Line> =
                    levers.iter().filter_map(|read| read.line).collect();
                read(&mut circuit, &register, &lines)?;
                let mut expected = none.clone();
                for read_line in &levers {
                    expected[read_line.bus] =
                        read_line.shows(held, register.width);
                }
                let got = buses(&circuit, &register);
                tally.check(
                    || format!("cell {}, every read", i + 1),
                    held,
                    &expected,
                    got,
                );
            }
            read(&mut circuit, &register, &[])?;
        }
    }
    let shape = Shape {
        cells: register.cells.len(),
        width: register.width,
    };
    let mut run = tally.run(&interface, &register, kind, exhaustive);
    if !exhaustive {
        run.patterns = patterns(register.width).len() as u64;
    }
    if kind == Kind::Register && ungated == values.len() as u64 && run.wrong > 0
    {
        run.hint = Some(
            "With every read lever off, the output still shows what the cells \
             hold, so its reads do not keep it off the bus."
                .to_owned(),
        );
    }
    if !quick && run.wrong > 0 {
        // The faults explain the cases; a fault search that fails, as on a
        // build that never settles, leaves the cases to speak for it.
        if let Ok((cells, faults)) = faults(blocks, &interface, kind) {
            run.cells = cells;
            run.faults = faults;
        }
    }
    Ok((run, shape))
}

/// The cells' controls, and each fault the cells show from the build with
/// its levers off, as the writes and reads that show it: a cell that reads wrong
/// after a write into it alone, and a cell that reads differently after
/// another is written. The value written into another cell is narrowed to
/// a single bit set, or else clear, so the steps point at the bit that
/// disturbs it.
fn faults(
    blocks: &Blocks,
    interface: &Interface,
    kind: Kind,
) -> Result<(Vec<CellControls>, Vec<Vec<Action>>)> {
    let (mut circuit, register) = ports(blocks, interface, kind)?;
    let width = register.width;
    let all = mask(width);
    let controls = register
        .cells
        .iter()
        .map(|cell| CellControls {
            write: cell.write.pos,
            read: cell
                .reads
                .first()
                .and_then(|read| read.line)
                .map(|line| line.pos),
        })
        .collect();
    let mut faults = Vec::new();
    for cell in 0..register.cells.len() {
        for value in telling(width) {
            let steps = vec![
                write_step(&mut circuit, &register, cell, value)?,
                read_step(&mut circuit, &register, cell, value)?,
            ];
            if !steps.iter().all(Action::right) {
                faults.push(steps);
                break;
            }
        }
    }
    // Single bits set, then single bits clear, so the first write found
    // to disturb a cell is the smallest.
    let single: Vec<u64> = (0..width)
        .map(|bit| 1 << bit)
        .chain((0..width).map(|bit| all ^ 1 << bit))
        .collect();
    for cell in 0..register.cells.len() {
        for other in (0..register.cells.len()).filter(|&other| other != cell) {
            // The telling values find a disturbance quickly; only then do
            // single bits find the smallest write that causes it.
            let pair = (cell, other);
            if disturbed(&mut circuit, &register, pair, &telling(width))?
                .is_none()
            {
                continue;
            }
            if let Some(steps) =
                disturbed(&mut circuit, &register, pair, &single)?
            {
                faults.push(steps);
            }
        }
    }
    Ok((controls, faults))
}

/// The first of `others` that, written into the cell `pair.1` after the
/// cell `pair.0` is written all ones or none, changes what `pair.0` reads,
/// as its steps.
fn disturbed(
    circuit: &mut Circuit,
    register: &Register,
    (cell, other): (usize, usize),
    others: &[u64],
) -> Result<Option<Vec<Action>>> {
    for held in [mask(register.width), 0] {
        for &value in others {
            let steps = vec![
                write_step(circuit, register, cell, held)?,
                read_step(circuit, register, cell, held)?,
                write_step(circuit, register, other, value)?,
                read_step(circuit, register, cell, held)?,
            ];
            if steps[1].got() != steps[3].got() {
                return Ok(Some(steps));
            }
        }
    }
    Ok(None)
}

/// Writes `value` into the cell `cell`, leaving data in on it.
fn write_step(
    circuit: &mut Circuit,
    register: &Register,
    cell: usize,
    value: u64,
) -> Result<Action> {
    write(circuit, register, register.cells[cell].write, value)?;
    Ok(Action::Write { cell, value })
}

/// Reads the cell `cell`, which was written `held`, through its first
/// read, then turns every read off.
fn read_step(
    circuit: &mut Circuit,
    register: &Register,
    cell: usize,
    held: u64,
) -> Result<Action> {
    let Some(&first) = register.cells[cell].reads.first() else {
        bail!("cell {} has no read", cell + 1);
    };
    read(circuit, register, first.line.as_slice())?;
    let got = buses(circuit, register)[first.bus];
    read(circuit, register, &[])?;
    Ok(Action::Read {
        cell,
        bus: first.bus,
        expected: first.shows(held, register.width),
        got,
    })
}

/// What the buses show with every read off, the cells holding `stored`:
/// nothing, but for a buffer's cells, which show all the time.
fn shown(register: &Register, stored: &[u64]) -> Vec<u64> {
    let mut shown = register.idle.clone();
    for (cell, &held) in register.cells.iter().zip(stored) {
        for read in cell.reads.iter().filter(|read| read.line.is_none()) {
            shown[read.bus] = read.shows(held, register.width);
        }
    }
    shown
}

/// Every value of `width` bits up to 8, with whether that is all of them;
/// beyond, the test patterns and every value of each byte.
fn values(width: usize) -> (Vec<u64>, bool) {
    if width <= 8 {
        return ((0..1 << width).collect(), true);
    }
    let mut values = patterns(width);
    for low in (0..width).step_by(8) {
        values.extend((0..256u64).map(|byte| byte << low & mask(width)));
    }
    values.sort_unstable();
    values.dedup();
    (values, false)
}

/// A few values that show whether each bit stores and reads: none and all
/// set, and alternate bits each way.
fn telling(width: usize) -> Vec<u64> {
    let all = mask(width);
    let alternate = 0x5555_5555_5555_5555 & all;
    let mut values = vec![all, 0, alternate, !alternate & all];
    values.dedup();
    values
}

/// The register's ports, and its circuit settled with its levers off,
/// with its cells found.
fn ports(
    blocks: &Blocks,
    interface: &Interface,
    kind: Kind,
) -> Result<(Circuit, Register)> {
    let what = kind.name();
    let control = format!("{} concrete", lint::CONTROL);
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| interface.inputs[i].block != control)
        .collect();
    let &[data] = operands.as_slice() else {
        broken!(
            "A {what} takes one data bus, but this has {} inputs besides its \
             magenta lines.",
            operands.len()
        );
    };
    if interface.inputs[data].signal_strength {
        bail!(
            "Input {} takes a signal strength, which the {what} test does \
             not drive yet.",
            interface.inputs[data].name
        );
    }
    let width = interface.inputs[data].bits.len();
    let buses: Vec<usize> = (0..interface.outputs.len()).collect();
    if buses.is_empty() {
        broken!("A {what} puts its cells on an output bus, but this has none.");
    }
    for &bus in &buses {
        let bits = output_bits(interface, &interface.outputs[bus]);
        if bits != width {
            broken!(
                "Its data bus has {width} bits, but output {} has {bits}.",
                interface.outputs[bus].name
            );
        }
    }
    let controls: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| interface.inputs[i].block == control)
        .collect();
    let mut circuit = Circuit::new(blocks, interface)?;
    circuit.settle()?;
    let any_button = controls.iter().any(|&port| {
        (0..interface.inputs[port].bits.len())
            .any(|bit| circuit.is_button(port, bit))
    });
    let (mut writes, mut reads) = (Vec::new(), Vec::new());
    for &port in &controls {
        // A sign says which a line is; else a button writes and a lever
        // reads, but in a buffer with no button, which reads nothing, a
        // lever writes.
        let says =
            |word: &str| interface.inputs[port].words.iter().any(|w| w == word);
        for (bit, at) in interface.inputs[port].bits.iter().enumerate() {
            let line = Line {
                port,
                bit,
                pos: at.pos,
                button: circuit.is_button(port, bit),
                active_low: false,
            };
            let writing = line.button || (kind == Kind::Buffer && !any_button);
            if says("write") || (writing && !says("read")) {
                writes.push(line);
            } else {
                reads.push(line);
            }
        }
    }
    if writes.is_empty() {
        broken!(
            "A {what}'s cells are written by a button on a magenta line, or a \
             line signed write, but this has none."
        );
    }
    match kind {
        Kind::Register if reads.is_empty() => broken!(
            "A register's cells are read by a lever on a magenta line, which \
             keeps them off the shared bus until then, but this has none, so \
             it may be a buffer."
        ),
        Kind::Buffer if !reads.is_empty() => broken!(
            "A buffer shows what it holds all the time, so it has no reads, \
             but this has {}, so it may be a register.",
            reads.len()
        ),
        _ => {}
    }
    let mut register = Register {
        data,
        width,
        buses,
        controls,
        reads,
        cells: Vec::new(),
        idle: Vec::new(),
    };
    register.cells = match kind {
        Kind::Register => cells(&mut circuit, &mut register, &writes)?,
        Kind::Buffer => held(&mut circuit, interface, &register, &writes)?,
    };
    read(&mut circuit, &register, &[])?;
    register.idle = match kind {
        Kind::Register => self::buses(&circuit, &register),
        Kind::Buffer => vec![0; register.buses.len()],
    };
    Ok((circuit, register))
}

/// The cells, each write with the reads that show what it wrote, and
/// how. With every cell holding 0 and then each alone holding all ones,
/// the buses are read with every read lever off and with each on alone.
/// A read whose lever changes how the cell shows is the cell's, on the
/// bus where it does: a plain one when it shows the cell with its lever
/// on, and an inverted one, `active_low`, when it shows it off. It is
/// an inverted output when the cell's all ones show as all zeros. When a
/// read shows more than one cell or none, as when reads are not gated,
/// writes and reads pair in order, if they number the same.
fn cells(
    circuit: &mut Circuit,
    register: &mut Register,
    writes: &[Line],
) -> Result<Vec<Cell>> {
    for &write_line in writes {
        write(circuit, register, write_line, 0)?;
    }
    let all = mask(register.width);
    let (saved_before, flipped_before) = look(circuit, register)?;
    let mut shows: Vec<Vec<(usize, Read)>> =
        vec![Vec::new(); register.reads.len()];
    for (w, &write_line) in writes.iter().enumerate() {
        write(circuit, register, write_line, all)?;
        circuit.set(register.data, 0);
        circuit.settle()?;
        let (saved, flipped) = look(circuit, register)?;
        let changed = |a: &[u64], b: &[u64]| -> Vec<u64> {
            a.iter().zip(b).map(|(a, b)| a ^ b).collect()
        };
        let as_saved = changed(&saved_before, &saved);
        for (r, &line) in register.reads.iter().enumerate() {
            let as_flipped = changed(&flipped_before[r], &flipped[r]);
            let Some(bus) = (0..as_saved.len())
                .find(|&bus| as_saved[bus] != as_flipped[bus])
            else {
                continue;
            };
            let active_low = as_saved[bus] != 0;
            let reading = if active_low { &saved } else { &flipped[r] };
            shows[r].push((
                w,
                Read {
                    line: Some(Line { active_low, ..line }),
                    bus,
                    inverted: reading[bus] == 0,
                },
            ));
        }
        write(circuit, register, write_line, 0)?;
    }
    if shows.iter().all(|found| found.len() == 1) {
        let mut cells: Vec<Cell> = writes
            .iter()
            .map(|&write| Cell {
                write,
                reads: Vec::new(),
            })
            .collect();
        for (r, found) in shows.iter().enumerate() {
            let (w, read) = found[0];
            register.reads[r] = read.line.expect("a read found has its lever");
            cells[w].reads.push(read);
        }
        if let Some(cell) = cells.iter().find(|cell| cell.reads.is_empty()) {
            let [x, y, z] = cell.write.pos;
            broken!(
                "No read lever shows the cell the write button at ({x}, {y}, \
                 {z}) writes."
            );
        }
        return Ok(cells);
    }
    if writes.len() == register.reads.len() && register.buses.len() == 1 {
        return Ok(writes
            .iter()
            .zip(&register.reads)
            .map(|(&write, &line)| Cell {
                write,
                reads: vec![Read {
                    line: Some(line),
                    bus: 0,
                    inverted: false,
                }],
            })
            .collect());
    }
    broken!(
        "Its {} read levers could not be matched to its {} write buttons: \
         each read should show one cell.",
        register.reads.len(),
        writes.len()
    );
}

/// A buffer's cells, each write with the buses that show what it wrote,
/// all the time. With every cell holding 0 and then each alone holding all
/// ones, a bus that changes is the cell's: plain when it shows all ones,
/// and inverted when it shows them as zeros. A write that changes the same
/// buses as one before is a second button on that cell's line, as each
/// copy of a stacked buffer keeps its own, and the first stands for it.
/// Each bus must show one cell.
fn held(
    circuit: &mut Circuit,
    interface: &Interface,
    register: &Register,
    writes: &[Line],
) -> Result<Vec<Cell>> {
    for &write_line in writes {
        write(circuit, register, write_line, 0)?;
    }
    let all = mask(register.width);
    let before = buses(circuit, register);
    let mut owned = vec![false; register.buses.len()];
    let mut cells: Vec<Cell> = Vec::with_capacity(writes.len());
    for &write_line in writes {
        write(circuit, register, write_line, all)?;
        circuit.set(register.data, 0);
        circuit.settle()?;
        let after = buses(circuit, register);
        write(circuit, register, write_line, 0)?;
        let reads: Vec<Read> = (0..after.len())
            .filter(|&bus| after[bus] != before[bus])
            .map(|bus| Read {
                line: None,
                bus,
                inverted: after[bus] == 0,
            })
            .collect();
        if reads.is_empty() {
            let [x, y, z] = write_line.pos;
            broken!(
                "No output shows the cell the write at ({x}, {y}, {z}) writes."
            );
        }
        let same = |cell: &Cell| {
            cell.reads.len() == reads.len()
                && cell.reads.iter().zip(&reads).all(|(a, b)| a.bus == b.bus)
        };
        if cells.iter().any(same) {
            continue;
        }
        for read in &reads {
            if std::mem::replace(&mut owned[read.bus], true) {
                broken!(
                    "Output {} shows more than one cell.",
                    interface.outputs[register.buses[read.bus]].name
                );
            }
        }
        cells.push(Cell {
            write: write_line,
            reads,
        });
    }
    if let Some(bus) = owned.iter().position(|&owned| !owned) {
        broken!(
            "Output {} shows no cell.",
            interface.outputs[register.buses[bus]].name
        );
    }
    Ok(cells)
}

/// What the buses show with every read lever off, and with each on
/// alone.
fn look(
    circuit: &mut Circuit,
    register: &Register,
) -> Result<(Vec<u64>, Vec<Vec<u64>>)> {
    read(circuit, register, &[])?;
    let saved = buses(circuit, register);
    let mut flipped = Vec::with_capacity(register.reads.len());
    for &line in &register.reads {
        read(circuit, register, &[line])?;
        flipped.push(buses(circuit, register));
    }
    read(circuit, register, &[])?;
    Ok((saved, flipped))
}

/// Writes `value` into the cell `line` writes: sets the data bus, presses
/// the button, or flicks the lever on and off, and waits until nothing
/// changes.
fn write(
    circuit: &mut Circuit,
    register: &Register,
    line: Line,
    value: u64,
) -> Result<()> {
    circuit.set(register.data, value);
    circuit.settle()?;
    if line.button {
        circuit.press(line.port, line.bit);
    } else {
        read(circuit, register, &[line])?;
        set_reads(circuit, register, &[]);
    }
    circuit.settle()?;
    Ok(())
}

/// Turns on the levers `on`, reads or a write, and the rest off, without
/// settling. An inverted read is on with its lever off.
fn set_reads(circuit: &mut Circuit, register: &Register, on: &[Line]) {
    let wanted = |line: &Line| on.iter().any(|other| other.is(line));
    for &port in &register.controls {
        let reads = register.reads.iter().filter(|line| {
            line.port == port && wanted(line) != line.active_low
        });
        let others = on.iter().filter(|line| {
            line.port == port
                && !register.reads.iter().any(|read| read.is(line))
        });
        let value = reads
            .chain(others)
            .fold(0, |value, line| value | 1 << line.bit);
        circuit.set(port, value);
    }
}

/// Turns on the levers `on` and the rest off, and settles.
fn read(circuit: &mut Circuit, register: &Register, on: &[Line]) -> Result<()> {
    set_reads(circuit, register, on);
    circuit.settle()?;
    Ok(())
}

/// What each output bus shows.
fn buses(circuit: &Circuit, register: &Register) -> Vec<u64> {
    register.buses.iter().map(|&bus| circuit.get(bus)).collect()
}

/// The cases checked so far, and what was wrong in them.
struct Tally {
    cases: u64,
    wrong: u64,
    /// Each bus bit's label, such as `output bit 3`, and how many cases
    /// it was wrong in, bus by bus.
    bits: Vec<Vec<(String, u64)>>,
    examples: Vec<Wrong>,
}

impl Tally {
    fn new(interface: &Interface, buses: &[usize]) -> Self {
        let bits = buses
            .iter()
            .map(|&bus| {
                let port = &interface.outputs[bus];
                let width = output_bits(interface, port);
                (0..width)
                    .map(|bit| match width {
                        1 => (port.name.clone(), 0),
                        _ => (format!("{} bit {bit}", port.name), 0),
                    })
                    .collect()
            })
            .collect();
        Tally {
            cases: 0,
            wrong: 0,
            bits,
            examples: Vec::new(),
        }
    }

    /// Counts one case, written `value`, whose buses should show
    /// `expected` and showed `got`; `op` names it if it was wrong.
    fn check(
        &mut self,
        op: impl FnOnce() -> String,
        value: u64,
        expected: &[u64],
        got: Vec<u64>,
    ) {
        self.cases += 1;
        if got == expected {
            return;
        }
        self.wrong += 1;
        for ((bus, &want), &showed) in
            self.bits.iter_mut().zip(expected).zip(&got)
        {
            for (bit, (_, count)) in bus.iter_mut().enumerate() {
                *count += (want ^ showed) >> bit & 1;
            }
        }
        if self.examples.len() < 5 {
            self.examples.push(Wrong {
                op: Some(op()),
                inputs: vec![value],
                expected: expected.to_vec(),
                got,
            });
        }
    }

    fn run(
        self,
        interface: &Interface,
        register: &Register,
        kind: Kind,
        exhaustive: bool,
    ) -> Run {
        let cells = register.cells.len();
        Run {
            pattern: {
                let inverted = register
                    .cells
                    .iter()
                    .any(|cell| cell.reads.iter().any(|read| read.inverted));
                let read = match (kind, inverted) {
                    (Kind::Register, false) => "read",
                    (Kind::Register, true) => "read inverted",
                    (Kind::Buffer, false) => "shown",
                    (Kind::Buffer, true) => "shown inverted",
                };
                match cells {
                    1 => format!("written, then {read}"),
                    n => format!("each of {n} cells written, then {read}"),
                }
            },
            cases: self.cases,
            exhaustive,
            patterns: 0,
            by_digit: false,
            wrong: self.wrong,
            inputs: vec![Signal {
                name: interface.inputs[register.data].name.clone(),
                bits: register.width,
            }],
            outputs: register
                .buses
                .iter()
                .map(|&bus| Signal {
                    name: interface.outputs[bus].name.clone(),
                    bits: output_bits(interface, &interface.outputs[bus]),
                })
                .collect(),
            wrong_bits: self
                .bits
                .into_iter()
                .flatten()
                .filter(|&(_, count)| count > 0)
                .collect(),
            examples: self.examples,
            hint: None,
            // A read that reads with its lever off reads inverted.
            inverted: {
                let mut reads: Vec<String> = register
                    .cells
                    .iter()
                    .flat_map(|cell| &cell.reads)
                    .filter_map(|read| read.line.filter(|line| line.active_low))
                    .map(|line| interface.inputs[line.port].name.clone())
                    .collect();
                reads.dedup();
                reads
            },
            cells: Vec::new(),
            faults: Vec::new(),
            timing: None,
            stacking: None,
        }
    }
}

/// When a register's reads settle: from a read lever turning on, with its
/// cell holding each value in turn, until its bus shows it, the bits'
/// changes tick by tick. Its writes too: from a button's press, with the
/// cell's read held on, until the bus shows the new value. A buffer, which
/// has no reads, is timed from its writes: from a write opening, or for a
/// cell that takes its data as its write closes, from it closing, its cell
/// holding each value's complement, until its bus shows the value.
pub fn time(
    blocks: &Blocks,
    kind: Kind,
    cancel: &AtomicBool,
) -> Result<Timing> {
    let interface = interface::read(blocks);
    let (mut circuit, register) = ports(blocks, &interface, kind)?;
    let (values, exhaustive) = values(register.width);
    let carried = circuit.carried();
    let widths: Vec<usize> = register
        .buses
        .iter()
        .map(|&bus| output_bits(&interface, &interface.outputs[bus]))
        .collect();
    let mut stats: Vec<BusStats> =
        widths.iter().map(|&width| BusStats::new(width)).collect();
    let mut settled: Vec<(i32, u64)> = Vec::new();
    let all = mask(register.width);
    let takes = if takes_data_closing(&mut circuit, &register)? {
        Takes::Closing
    } else {
        Takes::Open
    };
    let closing = takes == Takes::Closing;
    // A register is timed from its data changing, its controls where they
    // let it pass soonest; a buffer, or a cell taking its data as its
    // write closes, from its write.
    if kind == Kind::Register
        && !closing
        && let Some(timing) = from_data(
            &mut circuit,
            (blocks, &interface),
            &register,
            &carried,
            (&values, exhaustive),
        )?
    {
        return Ok(timing);
    }
    for cell in &register.cells {
        for &Read { line, bus, .. } in &cell.reads {
            for &value in &values {
                if cancel.load(Ordering::Relaxed) {
                    bail!("The timing was cancelled.");
                }
                if let Some(line) = line {
                    write(&mut circuit, &register, cell.write, value)?;
                    circuit.set(register.data, !value & all);
                    circuit.settle()?;
                    set_reads(&mut circuit, &register, &[line]);
                } else {
                    write(&mut circuit, &register, cell.write, !value & all)?;
                    circuit.set(register.data, value);
                    circuit.settle()?;
                    open_write(&mut circuit, &register, cell.write, None);
                    if closing {
                        hold_write(&mut circuit, cell.write, |_| false)?;
                        close_write(&mut circuit, &register, cell.write, None);
                    }
                }
                let changes = watch(&mut circuit, &register)?;
                let last =
                    stats[bus].add(&changes[bus], carried[register.buses[bus]]);
                settled.push((last, value));
                read(&mut circuit, &register, &[])?;
            }
        }
    }
    let write_ticks = write_latency(&mut circuit, &register, closing)?;
    let data_ticks = data_latency(&mut circuit, &register, &carried, closing)?;
    let synced = stats
        .iter()
        .all(|bus| bus.glitches == 0 && bus.staggered == 0);
    let (median, p99, slowest, slowest_value) = spread(&settled);
    Ok(Timing {
        inputs: vec![InputTiming {
            name: match kind {
                Kind::Register => "read",
                Kind::Buffer => "write",
            }
            .to_owned(),
            tick: Some(0),
            earliest: None,
        }],
        outputs: stats
            .into_iter()
            .zip(&register.buses)
            .map(|(bus, &port)| {
                bus.timing(&interface.outputs[port].name, carried[port])
            })
            .collect(),
        synced,
        median,
        p99,
        slowest,
        slowest_case: vec![slowest_value],
        changes: settled.len() as u64,
        patterns: !exhaustive,
        start: match kind {
            Kind::Register => Start::Read,
            Kind::Buffer => Start::Write,
        },
        write: Some(write_ticks),
        data: data_ticks,
        takes,
        pulse: None,
    })
}

/// How far either side of the data changing a control's offset is
/// searched, in ticks.
const REACH: i32 = 20;

/// When a register's controls switch, in ticks from its data in changing:
/// the window its write may open in and still store the new value; the
/// latest of those whose bus shows it soonest, `write`; and the latest its
/// read may turn on and still show it then, `read`, with `latency` that
/// tick.
struct Offsets {
    window: (i32, i32),
    write: i32,
    read: Option<i32>,
    latency: i32,
}

/// One cell's write and read, timed from the data changing to `value` at
/// t = 0, the cell holding its opposite: the write opened at `write`, the
/// read turned on at `read`, or on throughout when `None`. Whether the bus
/// ends showing `value`, and each bus bit's change ticks.
fn timed(
    circuit: &mut Circuit,
    register: &Register,
    cell: &Cell,
    read_line: Read,
    value: u64,
    write_at: i32,
    read_at: Option<i32>,
) -> Result<(bool, Vec<Vec<i32>>)> {
    let all = mask(register.width);
    write(circuit, register, cell.write, !value & all)?;
    read(
        circuit,
        register,
        if read_at.is_none() {
            read_line.line.as_slice()
        } else {
            &[]
        },
    )?;
    let start = write_at.min(read_at.unwrap_or(0)).min(0);
    let end = write_at.max(read_at.unwrap_or(0)).max(0);
    let mut before = buses(circuit, register)[read_line.bus];
    let mut changes = vec![Vec::new(); register.width];
    let mut reading = read_at.is_none();
    let mut t = start;
    loop {
        if t == 0 {
            circuit.set(register.data, value);
        }
        if Some(t) == read_at {
            reading = true;
        }
        if t == write_at {
            open_write(
                circuit,
                register,
                cell.write,
                read_line.line.filter(|_| reading),
            );
        } else if Some(t) == read_at {
            let open = !cell.write.button && t > write_at;
            let on: Vec<Line> = read_line
                .line
                .into_iter()
                .chain(open.then_some(cell.write))
                .collect();
            set_reads(circuit, register, &on);
        }
        let down = cell.write.button
            && circuit.pressed(cell.write.port, cell.write.bit);
        if t >= end && !down && !circuit.pending() {
            break;
        }
        if t - start > i32::try_from(MAX_TICKS).unwrap_or(i32::MAX) {
            bail!("It was still changing {MAX_TICKS} ticks after its data.");
        }
        circuit.tick();
        t += 1;
        let now = buses(circuit, register)[read_line.bus];
        let flipped = before ^ now;
        for (bit, ticks) in changes.iter_mut().enumerate() {
            if flipped >> bit & 1 == 1 {
                ticks.push(t);
            }
        }
        before = now;
    }
    // A lever's write stays open until now; it closes holding the value.
    close_write(circuit, register, cell.write, read_line.line);
    circuit.settle()?;
    let stored = buses(circuit, register)[read_line.bus]
        == read_line.shows(value, register.width);
    read(circuit, register, &[])?;
    Ok((stored, changes))
}

/// Over every cell's first read and the telling values, whether timing its
/// write at `write_at` and its read at `read_at` stores each value, and the
/// latest tick its bus settles, less what carries it out.
fn timed_all(
    circuit: &mut Circuit,
    register: &Register,
    carried: &[i32],
    write_at: i32,
    read_at: Option<i32>,
) -> Result<Option<i32>> {
    let mut latest = 0;
    for cell in &register.cells {
        let Some(&read_line) = cell.reads.first() else {
            continue;
        };
        for value in telling(register.width) {
            let (stored, changes) = timed(
                circuit, register, cell, read_line, value, write_at, read_at,
            )?;
            if !stored {
                return Ok(None);
            }
            let port = register.buses[read_line.bus];
            let last = changes.iter().flatten().max().copied().unwrap_or(0);
            latest = latest.max(last - carried[port]);
        }
    }
    Ok(Some(latest))
}

/// Where the register's controls switch, as `Offsets` says, searched
/// `REACH` either side of its data changing; `None` when no write in reach
/// stores the new value.
fn offsets(
    circuit: &mut Circuit,
    register: &Register,
    carried: &[i32],
) -> Result<Option<Offsets>> {
    let mut stores: Vec<(i32, i32)> = Vec::new();
    for write_at in -REACH..=REACH {
        if let Some(settles) =
            timed_all(circuit, register, carried, write_at, None)?
        {
            stores.push((write_at, settles));
        }
    }
    let (Some(&(earliest, _)), Some(&(latest, _))) =
        (stores.first(), stores.last())
    else {
        return Ok(None);
    };
    let soonest = stores
        .iter()
        .map(|&(_, settles)| settles)
        .min()
        .unwrap_or(0);
    let write = stores
        .iter()
        .filter(|&&(_, settles)| settles == soonest)
        .map(|&(at, _)| at)
        .max()
        .unwrap_or(earliest);
    let mut read = None;
    if register
        .cells
        .iter()
        .any(|cell| cell.reads.iter().any(|r| r.line.is_some()))
    {
        for read_at in -REACH..=soonest {
            let settles =
                timed_all(circuit, register, carried, write, Some(read_at))?;
            if settles.is_some_and(|settles| settles <= soonest) {
                read = Some(read_at);
            }
        }
    }
    Ok(Some(Offsets {
        window: (earliest, latest),
        write,
        read,
        latency: soonest,
    }))
}

/// The register's timing from its data changing, as `aligned` takes it at
/// the offsets its controls let it pass soonest, with its write latency
/// from the button; `None` when no write in reach stores its data.
fn from_data(
    circuit: &mut Circuit,
    (blocks, interface): (&Blocks, &Interface),
    register: &Register,
    carried: &[i32],
    values: (&[u64], bool),
) -> Result<Option<Timing>> {
    let Some(found) = offsets(circuit, register, carried)? else {
        return Ok(None);
    };
    // The lines as a register's are named, to say which is which.
    let mut named = interface::read(blocks);
    interface::name_lines("register", blocks, &mut named);
    let mut timing = aligned(
        circuit,
        (interface, &named),
        register,
        carried,
        &found,
        values,
    )?;
    timing.write = Some(write_latency(circuit, register, false)?);
    Ok(Some(timing))
}

/// The register's timing from its data changing, its controls switched at
/// `found`'s offsets: each value of `values`, `exhaustive` or not, written
/// and read through every cell's first read, each bus's changes taken. Its
/// lines go by their names in `named`, the interface as its category names
/// it, matched to `interface`'s by where they are.
fn aligned(
    circuit: &mut Circuit,
    (interface, named): (&Interface, &Interface),
    register: &Register,
    carried: &[i32],
    found: &Offsets,
    (values, exhaustive): (&[u64], bool),
) -> Result<Timing> {
    let widths: Vec<usize> = register
        .buses
        .iter()
        .map(|&bus| output_bits(interface, &interface.outputs[bus]))
        .collect();
    let mut stats: Vec<BusStats> =
        widths.iter().map(|&width| BusStats::new(width)).collect();
    let mut settled: Vec<(i32, u64)> = Vec::new();
    for cell in &register.cells {
        let Some(&read_line) = cell.reads.first() else {
            continue;
        };
        for &value in values {
            let (_, changes) = timed(
                circuit,
                register,
                cell,
                read_line,
                value,
                found.write,
                found.read,
            )?;
            let last = stats[read_line.bus]
                .add(&changes, carried[register.buses[read_line.bus]]);
            settled.push((last, value));
        }
    }
    let synced = stats
        .iter()
        .all(|bus| bus.glitches == 0 && bus.staggered == 0);
    let (median, p99, slowest, slowest_value) = spread(&settled);
    // The data in first, t = 0; then each control by its name, a write
    // with its window, a read by the latest it may turn on.
    let name_at = |pos: Pos| {
        named
            .inputs
            .iter()
            .find(|port| port.bits.iter().any(|bit| bit.pos == pos))
            .map_or_else(String::new, |port| port.name.clone())
    };
    let data = interface.inputs[register.data]
        .bits
        .first()
        .map(|bit| bit.pos);
    let mut inputs = vec![InputTiming {
        name: data.map_or_else(|| "data in".to_owned(), name_at),
        tick: Some(0),
        earliest: None,
    }];
    // A write may come as late as it lets the data pass soonest, and as
    // early as a button's press still holds when the data arrives.
    for cell in &register.cells {
        inputs.push(InputTiming {
            name: name_at(cell.write.pos),
            tick: Some(found.write),
            earliest: cell.write.button.then_some(found.window.0),
        });
        for read in cell.reads.iter().filter_map(|read| read.line) {
            inputs.push(InputTiming {
                name: name_at(read.pos),
                tick: found.read,
                earliest: None,
            });
        }
    }
    Ok(Timing {
        inputs,
        outputs: stats
            .into_iter()
            .zip(&register.buses)
            .map(|(bus, &port)| {
                bus.timing(&named.outputs[port].name, carried[port])
            })
            .collect(),
        synced,
        median,
        p99,
        slowest,
        slowest_case: vec![slowest_value],
        changes: settled.len() as u64,
        patterns: !exhaustive,
        start: Start::Input,
        write: None,
        data: Some(found.latency),
        takes: Takes::Open,
        pulse: None,
    })
}

/// Of reads that each settled on a tick with a value, the tick half of them
/// settled by, 99% by, and the slowest, with its value.
fn spread(settled: &[(i32, u64)]) -> (i32, i32, i32, u64) {
    let mut ticks: Vec<i32> = settled.iter().map(|&(tick, _)| tick).collect();
    ticks.sort_unstable();
    let at = |share: usize| {
        ticks
            .get((ticks.len() * share / 100).min(ticks.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0)
    };
    let (slowest, value) = settled
        .iter()
        .copied()
        .max_by_key(|&(tick, _)| tick)
        .unwrap_or_default();
    (at(50), at(99), slowest, value)
}

/// Runs the circuit until nothing changes, each bus bit's change ticks.
fn watch(
    circuit: &mut Circuit,
    register: &Register,
) -> Result<Vec<Vec<Vec<i32>>>> {
    let mut before = buses(circuit, register);
    let mut changes =
        vec![vec![Vec::new(); register.width]; register.buses.len()];
    let mut tick = 0;
    while circuit.step() {
        tick += 1;
        if tick > MAX_TICKS {
            bail!("It was still changing {MAX_TICKS} ticks after a read.");
        }
        let now = buses(circuit, register);
        for (bus, (&old, &new)) in before.iter().zip(&now).enumerate() {
            let flipped = old ^ new;
            for (bit, ticks) in changes[bus].iter_mut().enumerate() {
                if flipped >> bit & 1 == 1 {
                    ticks.push(i32::try_from(tick).unwrap_or(i32::MAX));
                }
            }
        }
        before = now;
    }
    Ok(changes)
}

/// Opens `write` with `read`, if any, on: turns its lever on with the
/// read, or presses its button. Settles nothing.
fn open_write(
    circuit: &mut Circuit,
    register: &Register,
    write: Line,
    read: Option<Line>,
) {
    if write.button {
        circuit.press(write.port, write.bit);
    } else {
        let on: Vec<Line> = read.into_iter().chain([write]).collect();
        set_reads(circuit, register, &on);
    }
}

/// Holds a write `open_write` opened until it may close, and says whether
/// `shows` held at any point while it was open: a lever's write settled,
/// or a button's every tick until it springs back.
fn hold_write(
    circuit: &mut Circuit,
    write: Line,
    shows: impl Fn(&Circuit) -> bool,
) -> Result<bool> {
    if !write.button {
        circuit.settle()?;
        return Ok(shows(circuit));
    }
    let mut seen = shows(circuit);
    let mut ticks = 0;
    while circuit.pressed(write.port, write.bit) {
        if ticks > MAX_TICKS {
            bail!("A write button stayed down {MAX_TICKS} ticks.");
        }
        circuit.tick();
        ticks += 1;
        seen |= shows(circuit);
    }
    Ok(seen)
}

/// How many ticks the button on `write` stays down once pressed.
fn press_ticks(circuit: &mut Circuit, write: Line) -> Result<u32> {
    circuit.press(write.port, write.bit);
    let mut ticks = 0;
    while circuit.pressed(write.port, write.bit) {
        if ticks > MAX_TICKS {
            bail!("A write button stayed down {MAX_TICKS} ticks.");
        }
        circuit.tick();
        ticks += 1;
    }
    circuit.settle()?;
    Ok(ticks)
}

/// Closes a write `hold_write` held, with `read`, if any, left on: turns
/// its lever off; a button has sprung back already. Settles nothing.
fn close_write(
    circuit: &mut Circuit,
    register: &Register,
    write: Line,
    read: Option<Line>,
) {
    if !write.button {
        set_reads(circuit, register, read.as_slice());
    }
}

/// Whether the register's cells take their data as their writes close,
/// their lever turning off or their button springing back: some cell, its
/// read on and its write open, never shows a value it was given, as a cell
/// whose read is cut while it writes does, and shows it once its write
/// closes.
fn takes_data_closing(
    circuit: &mut Circuit,
    register: &Register,
) -> Result<bool> {
    let all = mask(register.width);
    for cell in &register.cells {
        let Some(&read_line) = cell.reads.first() else {
            continue;
        };
        for value in telling(register.width) {
            write(circuit, register, cell.write, !value & all)?;
            circuit.set(register.data, value);
            read(circuit, register, read_line.line.as_slice())?;
            let shows = |circuit: &Circuit| {
                buses(circuit, register)[read_line.bus]
                    == read_line.shows(value, register.width)
            };
            open_write(circuit, register, cell.write, read_line.line);
            let held = hold_write(circuit, cell.write, shows)?;
            close_write(circuit, register, cell.write, read_line.line);
            circuit.settle()?;
            let closed = shows(circuit);
            read(circuit, register, &[])?;
            if !held && closed {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The most ticks for data to pass through a cell, its read on: from its
/// data in changing until its bus shows the new value, as when a data
/// loop's ALU feeds it, the control lines no part of it. With its write
/// held open; or, for a cell `closing`, which takes its data as its write
/// closes, changing as it closes, the latest it can. `None`
/// when a cell takes its data only as its write opens, so data changed
/// while it is held does not pass.
fn data_latency(
    circuit: &mut Circuit,
    register: &Register,
    carried: &[i32],
    closing: bool,
) -> Result<Option<i32>> {
    if closing {
        return closing_data_latency(circuit, register, carried);
    }
    let all = mask(register.width);
    let mut slowest = None;
    for cell in &register.cells {
        let Some(&read_line) = cell.reads.first() else {
            continue;
        };
        let port = register.buses[read_line.bus];
        for value in telling(register.width) {
            let next = !value & all;
            write(circuit, register, cell.write, next)?;
            read(circuit, register, read_line.line.as_slice())?;
            circuit.set(register.data, value);
            circuit.settle()?;
            let shows = |circuit: &Circuit, held: u64| {
                buses(circuit, register)[read_line.bus]
                    == read_line.shows(held, register.width)
            };
            // Open the write, and wait until the cell passes its data.
            open_write(circuit, register, cell.write, read_line.line);
            let mut open = false;
            while !open && circuit.step() {
                open = shows(circuit, value);
            }
            if !open {
                return Ok(None);
            }
            // The data changes while the write is still open.
            circuit.set(register.data, next);
            let mut ticks = 0;
            let mut passed = false;
            while !passed && circuit.step() {
                ticks += 1;
                passed = shows(circuit, next);
            }
            if !passed {
                return Ok(None);
            }
            let ticks = ticks - carried[port];
            slowest = Some(slowest.map_or(ticks, |s: i32| s.max(ticks)));
            circuit.settle()?;
            read(circuit, register, &[])?;
        }
    }
    Ok(slowest)
}

/// `data_latency` for cells that take their data as their writes close:
/// from the data changing as the write closes, its lever turning off or
/// its button springing back, the write held open before with the old
/// value, until the bus shows the new one and keeps it.
fn closing_data_latency(
    circuit: &mut Circuit,
    register: &Register,
    carried: &[i32],
) -> Result<Option<i32>> {
    let all = mask(register.width);
    let mut slowest = None;
    for cell in &register.cells {
        let Some(&read_line) = cell.reads.first() else {
            continue;
        };
        let port = register.buses[read_line.bus];
        // A button springs back a set time after its press, so the data
        // changes just before the tick it does, as with a lever turned off.
        let down = if cell.write.button {
            Some(press_ticks(circuit, cell.write)?)
        } else {
            None
        };
        for value in telling(register.width) {
            let old = !value & all;
            write(circuit, register, cell.write, old)?;
            circuit.set(register.data, old);
            read(circuit, register, read_line.line.as_slice())?;
            open_write(circuit, register, cell.write, read_line.line);
            match down {
                Some(ticks) => {
                    for _ in 1..ticks {
                        circuit.tick();
                    }
                }
                None => {
                    circuit.settle()?;
                }
            }
            circuit.set(register.data, value);
            close_write(circuit, register, cell.write, read_line.line);
            let changes = watch(circuit, register)?;
            if buses(circuit, register)[read_line.bus]
                != read_line.shows(value, register.width)
            {
                return Ok(None);
            }
            let last = changes[read_line.bus]
                .iter()
                .flatten()
                .max()
                .copied()
                .unwrap_or(0);
            let ticks = last - carried[port];
            slowest = Some(slowest.map_or(ticks, |s: i32| s.max(ticks)));
            read(circuit, register, &[])?;
        }
    }
    Ok(slowest)
}

/// The most ticks from opening a cell's write, pressing its button or
/// turning its lever on, its read held on, until its bus shows the value
/// written and keeps it; or for cells `closing`, from the write closing,
/// their data set while it was open.
fn write_latency(
    circuit: &mut Circuit,
    register: &Register,
    closing: bool,
) -> Result<i32> {
    let all = mask(register.width);
    let mut slowest = 0;
    for cell in &register.cells {
        let Some(&Read { line, bus, .. }) = cell.reads.first() else {
            continue;
        };
        for value in telling(register.width) {
            write(circuit, register, cell.write, !value & all)?;
            read(circuit, register, line.as_slice())?;
            circuit.set(register.data, value);
            circuit.settle()?;
            open_write(circuit, register, cell.write, line);
            if closing {
                hold_write(circuit, cell.write, |_| false)?;
                close_write(circuit, register, cell.write, line);
            }
            let changes = watch(circuit, register)?;
            let last =
                changes[bus].iter().flatten().max().copied().unwrap_or(0);
            slowest = slowest.max(last);
            read(circuit, register, &[])?;
        }
    }
    Ok(slowest)
}

/// One bus's changes over the reads timed.
struct BusStats {
    bits: Vec<BitTiming>,
    /// Reads in which a bit changed more than once.
    glitches: u64,
    /// Reads in which its bits each changed once but on different ticks.
    staggered: u64,
    latency: i32,
    first_change: i32,
}

impl BusStats {
    fn new(width: usize) -> Self {
        BusStats {
            bits: vec![
                BitTiming {
                    first_change: i32::MAX,
                    latency: 0,
                    glitches: 0,
                };
                width
            ],
            glitches: 0,
            staggered: 0,
            latency: 0,
            first_change: i32::MAX,
        }
    }

    /// Adds one read's changes, each bit's ticks, less `carried`; the tick
    /// the bus settled by.
    fn add(&mut self, changes: &[Vec<i32>], carried: i32) -> i32 {
        let mut last = 0;
        let mut once: Vec<i32> = Vec::new();
        let mut glitched = false;
        for (bit, ticks) in self.bits.iter_mut().zip(changes) {
            let (Some(&first), Some(&end)) = (ticks.first(), ticks.last())
            else {
                continue;
            };
            let (first, end) = (first - carried, end - carried);
            bit.first_change = bit.first_change.min(first);
            bit.latency = bit.latency.max(end);
            if ticks.len() > 1 {
                bit.glitches += 1;
                glitched = true;
            } else {
                once.push(end);
            }
            self.first_change = self.first_change.min(first);
            last = last.max(end);
        }
        self.latency = self.latency.max(last);
        if glitched {
            self.glitches += 1;
        } else if once.iter().any(|&tick| Some(&tick) != once.first()) {
            self.staggered += 1;
        }
        last
    }

    fn timing(self, name: &str, carried: i32) -> OutputTiming {
        let unchanged = |tick: i32| if tick == i32::MAX { 0 } else { tick };
        OutputTiming {
            name: name.to_owned(),
            status: false,
            latency: self.latency,
            first_change: unchanged(self.first_change),
            carried,
            glitches: self.glitches,
            staggered: self.staggered,
            glitch: None,
            bits: self
                .bits
                .into_iter()
                .map(|bit| BitTiming {
                    first_change: unchanged(bit.first_change),
                    ..bit
                })
                .collect(),
        }
    }
}

/// Whether copies of the register in `data`, a file whose own cases came
/// out right, stack west into a bigger register: the shortest step west,
/// up to its width, at which two copies all store and read right, either
/// as more cells sharing the data buses, driven and read at the first
/// copy's end, or as more bits sharing the control lines, driven at the
/// first copy's end; then the most copies that do, up to `MOST_COPIES`, as
/// a shared line that runs too far fails. A buffer's copies only add bits,
/// as its cells show all the time and so cannot share a bus. `None` when
/// it is not of its `kind`.
pub fn stacking(data: &[u8], kind: Kind) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let (_, single) = check_on(&blocks, kind, true).ok()?;
    let (low, high) = interface::measured(&blocks);
    let reach = i64::from(high[0] - low[0] + 1);
    let works = |step: Step, copies: usize, bits: bool, quick: bool| {
        // Copies along a register's bits share its control lines; across
        // them, its data buses.
        let orientation = if bits {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        let buses = stack::Buses::of(kind.name(), orientation);
        let (stacked, _) =
            stack::stack(data, &[(step.direction(), copies)], buses).ok()?;
        let blocks = Blocks::read(&stacked).ok()?;
        let (run, shape) = check_on(&blocks, kind, quick).ok()?;
        let made = if bits {
            Shape {
                width: single.width * copies,
                ..single
            }
        } else {
            Shape {
                cells: single.cells * copies,
                ..single
            }
        };
        (run.wrong == 0 && shape == made).then_some(run.cases)
    };
    let found = (1..=reach)
        .map(|west| Step {
            west,
            up: 0,
            north: 0,
        })
        .flat_map(|step| [(step, false), (step, true)])
        .filter(|&(_, bits)| bits || kind == Kind::Register)
        .find(|&(step, bits)| {
            works(step, 2, bits, true).is_some()
                && works(step, 2, bits, false).is_some()
        });
    let Some((step, bits)) = found else {
        return Some(Stacking::Not {
            step: None,
            reason: match kind {
                Kind::Register => {
                    "No step west, up to its width, makes copies whose every \
                     cell stores and reads right, as more cells or more bits."
                }
                Kind::Buffer => {
                    "No step west, up to its width, makes copies whose every \
                     cell stores and shows right, as more bits."
                }
            }
            .to_owned(),
        });
    };
    let mut most = None;
    let mut copies = 2;
    while copies < MOST_COPIES {
        if works(step, copies + 1, bits, true).is_none() {
            most = Some(copies);
            break;
        }
        copies += 1;
    }
    // The most that pass quickly may yet get a case wrong.
    let cases = loop {
        if let Some(cases) = works(step, copies, bits, false) {
            break cases;
        }
        copies -= 1;
        most = Some(copies);
    };
    Some(if bits {
        Stacking::Wider {
            step,
            copies,
            bits: single.width * copies,
            cases,
            most,
        }
    } else {
        Stacking::Registers {
            step,
            copies,
            registers: single.cells * copies,
            cases,
            most,
        }
    })
}

/// The fewest steps a register's throughput is found on, and the many it
/// is checked on.
const STEPS: usize = 64;
const LONG_STEPS: usize = 1024;

/// How often the register in `data`, timed as `timing`, takes a new
/// access, as a CPU's clock drives it: each period the data bus takes a
/// new value, and a cell is read, or written, its write open for some
/// ticks of the period, timed against the data, both as works best, as a
/// control line is. Its write buttons are driven as levers, as a clock's
/// pulse is not a press. Each read must show what was last written into
/// its cell, some ticks after it starts, the same for every read, no
/// sooner than its bus's latency: a read that follows another may take
/// longer, as the bus lets go of the last. The shortest period that holds, at some timing of its
/// writes, on a short stream and then a long one; `None` when none up to
/// twice its slowest read, write, or data does. A buffer, which has no
/// reads, is written every period, a cell at random, and its cell's bus
/// must show what it was given, as its timing counts from a write.
pub fn throughput(
    data: &[u8],
    timing: &Timing,
    kind: Kind,
) -> Result<Option<i32>> {
    let blocks = Blocks::read(data)?;
    let interface = interface::read(&blocks);
    let (_, register) = ports(&blocks, &interface, kind)?;
    // Its writes' buttons as levers on the same blocks.
    let writes: Vec<Pos> = register
        .cells
        .iter()
        .filter(|cell| cell.write.button)
        .map(|cell| cell.write.pos)
        .collect();
    let levers: std::collections::BTreeMap<Pos, String> = blocks
        .switches()
        .into_iter()
        .filter(|switch| switch.button && writes.contains(&switch.block))
        .map(|switch| {
            let state = blocks.get(switch.pos);
            let properties = state.split_once('[').map_or("", |(_, p)| p);
            (switch.pos, format!("minecraft:lever[{properties}"))
        })
        .collect();
    let levered = Blocks::read(&crate::standardize::edit(data, &levers)?)?;
    let latency: Vec<i32> = timing
        .outputs
        .iter()
        .map(|o| (o.latency + o.carried).max(0))
        .collect();
    if latency.len() != register.buses.len() {
        return Ok(None);
    }
    // A write's data must pass into its cell, and a read come out, so the
    // slowest of those bounds the periods tried.
    let slowest = latency
        .iter()
        .copied()
        .chain(timing.write)
        .chain(timing.data)
        .max()
        .unwrap_or(1)
        .max(1);
    let short = accesses(&register, STEPS, kind);
    let long = accesses(&register, LONG_STEPS, kind);
    let right = |stream: Stream, steps: &[Access]| -> Result<Vec<i32>> {
        let interface = interface::read(&levered);
        let mut circuit = Circuit::new(&levered, &interface)?;
        stream.right(&mut circuit, steps)
    };
    for period in 2..=2 * slowest {
        // Its write open for a tick up to all but one of the period, and
        // opening from as early to as late against its data as one period
        // allows.
        for open in 1..period {
            for offset in -(period - 1)..period {
                let stream = Stream {
                    register: &register,
                    latency: &latency,
                    period,
                    offset,
                    open,
                };
                let found = right(stream, &short)?;
                if found.is_empty() {
                    continue;
                }
                let held = right(stream, &long)?;
                if found.iter().any(|delay| held.contains(delay)) {
                    return Ok(Some(period));
                }
            }
        }
    }
    Ok(None)
}

/// One period of a register's stream: the value the data bus takes, and
/// the cell written or the cell and read read.
struct Access {
    value: u64,
    write: Option<usize>,
    read: Option<(usize, usize)>,
}

/// A fixed stream of `count` accesses to `register`, from a simple
/// generator: each writes a cell or reads one, at random, as a register
/// file may not read one cell while it writes another; a buffer's each
/// write a cell and look at what it shows.
fn accesses(register: &Register, count: usize, kind: Kind) -> Vec<Access> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let pick = |n: usize, r: u64| usize::try_from(r % n as u64).unwrap_or(0);
    (0..count)
        .map(|_| {
            let value = next() & mask(register.width);
            let cell = pick(register.cells.len(), next());
            if kind == Kind::Buffer {
                return Access {
                    value,
                    write: Some(cell),
                    read: Some((cell, 0)),
                };
            }
            if next() & 1 == 0 {
                Access {
                    value,
                    write: Some(cell),
                    read: None,
                }
            } else {
                let reads = register.cells[cell].reads.len();
                Access {
                    value,
                    write: None,
                    read: Some((cell, pick(reads, next()))),
                }
            }
        })
        .collect()
}

/// Sets every control line of `register`: the write `write` open, the
/// read `read` on, and the rest as they rest.
fn drive(
    circuit: &mut Circuit,
    register: &Register,
    write: Option<Line>,
    read: Option<Line>,
) {
    for &port in &register.controls {
        let mut value = 0;
        for line in register.reads.iter().filter(|line| line.port == port) {
            let on = read.is_some_and(|read| read.is(line));
            if on != line.active_low {
                value |= 1 << line.bit;
            }
        }
        if let Some(write) = write.filter(|write| write.port == port) {
            value |= 1 << write.bit;
        }
        circuit.set(port, value);
    }
}

/// A register's stream: an access every `period` ticks, each write
/// opening `offset` ticks after its data, for `open` ticks.
#[derive(Clone, Copy)]
struct Stream<'a> {
    register: &'a Register,
    latency: &'a [i32],
    period: i32,
    offset: i32,
    open: i32,
}

impl Stream<'_> {
    /// The read delays, from its bus's latency on, at which `steps`, after
    /// every cell is first written at leisure, read right: each read's bus
    /// shows what was last written into its cell.
    fn right(
        &self,
        circuit: &mut Circuit,
        steps: &[Access],
    ) -> Result<Vec<i32>> {
        let register = self.register;
        let mut held =
            vec![STRIDE & mask(register.width); register.cells.len()];
        for cell in &register.cells {
            circuit.set(register.data, held[0]);
            drive(circuit, register, Some(cell.write), None);
            circuit.settle()?;
            drive(circuit, register, None, None);
            circuit.settle()?;
        }
        // Each read's start, bus, and what it should show, from what was
        // written before it, or a buffer's, which shows what it is given,
        // with it.
        let mut reads: Vec<(i32, usize, u64)> = Vec::new();
        for (i, step) in steps.iter().enumerate() {
            if let (Some(cell), None) = (step.write, register.reads.first()) {
                held[cell] = step.value;
            }
            if let Some((cell, read)) = step.read {
                let read = register.cells[cell].reads[read];
                reads.push((
                    self.period * i32::try_from(i)? + self.latency[read.bus],
                    read.bus,
                    read.shows(held[cell], register.width),
                ));
            }
            if let Some(cell) = step.write {
                held[cell] = step.value;
            }
        }
        // As late as a read may come, a period on.
        let late = self.period;
        let end = self.period * i32::try_from(steps.len())?
            + late
            + self.latency.iter().copied().max().unwrap_or(0);
        let mut shown: Vec<Vec<u64>> = Vec::new();
        for tick in 0..=end {
            let at = tick.div_euclid(self.period);
            let step = usize::try_from(at).ok().and_then(|at| steps.get(at));
            if let Some(step) = step {
                circuit.set(register.data, step.value);
            }
            let since = tick - self.offset;
            let writing = (since.rem_euclid(self.period) < self.open)
                .then(|| usize::try_from(since.div_euclid(self.period)).ok())
                .flatten()
                .and_then(|i| steps.get(i))
                .and_then(|step| step.write)
                .map(|cell| register.cells[cell].write);
            let reading = step
                .and_then(|step| step.read)
                .and_then(|(cell, read)| register.cells[cell].reads[read].line);
            drive(circuit, register, writing, reading);
            shown.push(buses(circuit, register));
            circuit.tick();
        }
        Ok((0..late)
            .filter(|&delay| {
                reads.iter().all(|&(at, bus, shows)| {
                    usize::try_from(at + delay)
                        .ok()
                        .and_then(|t| shown.get(t))
                        .is_some_and(|buses| buses[bus] == shows)
                })
            })
            .collect())
    }
}
