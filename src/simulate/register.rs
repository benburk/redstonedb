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
//! Each value, every one up to 8 bits and the test patterns and each
//! byte's values beyond, is written into every cell, each cell offset from
//! the last so that they hold different values. Then the data bus changes,
//! which no cell may take without its button, every read off must leave
//! the buses empty, and each read alone must show its cell's value on its
//! bus and nothing on any other.
//!
//! Copies stack west into a bigger register file, sharing the data buses,
//! which are driven and read at the first copy's end, or into a wider
//! register, sharing the control lines; see `stacking`.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};

use super::{
    Broken, Circuit, MAX_TICKS, Run, Signal, Wrong, mask, output_bits,
    patterns,
    stacking::{Stacking, Step},
    timing::{BitTiming, InputTiming, OutputTiming, Takes, Timing},
};
use crate::{
    interface::{self, Interface},
    lint,
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
    /// A read that reads as saved, and stops when its lever is flipped:
    /// inverted, though saved as if not.
    active_low: bool,
}

impl Line {
    fn is(&self, other: &Line) -> bool {
        (self.port, self.bit) == (other.port, other.bit)
    }
}

/// A cell: its write button, and each read lever that puts it on a bus.
struct Cell {
    write: Line,
    reads: Vec<Read>,
}

/// A read lever, the bus it puts its cell on, by its place among the
/// register's, and whether it puts the cell's value there inverted.
#[derive(Clone, Copy)]
struct Read {
    line: Line,
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

/// Runs the register in `blocks` through its every case.
pub fn check(blocks: &Blocks) -> Result<Run> {
    Ok(check_on(blocks, false)?.0)
}

/// `check` on a few telling values, for a guess at what a build is.
pub fn check_quick(blocks: &Blocks) -> Result<Run> {
    Ok(check_on(blocks, true)?.0)
}

/// How many cells a register has, and of how many bits.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shape {
    cells: usize,
    width: usize,
}

/// The run, and the shape of the register it found.
fn check_on(blocks: &Blocks, quick: bool) -> Result<(Run, Shape)> {
    let interface = interface::read(blocks);
    let (mut circuit, register) = ports(blocks, &interface)?;
    let (values, exhaustive) = if quick {
        (telling(register.width), false)
    } else {
        values(register.width)
    };
    let all = mask(register.width);
    let mut tally = Tally::new(&interface, &register.buses);
    let mut ungated = 0;
    for &value in &values {
        let stored: Vec<u64> = (0..register.cells.len() as u64)
            .map(|cell| value.wrapping_add(cell * STRIDE) & all)
            .collect();
        for (cell, &held) in register.cells.iter().zip(&stored) {
            write(&mut circuit, &register, cell.write, held)?;
        }
        // No cell may take the bus without its button.
        circuit.set(register.data, !value & all);
        circuit.settle()?;
        let none = register.idle.clone();
        let got = buses(&circuit, &register);
        ungated += u64::from(got != none);
        tally.check(|| "every read off".to_owned(), value, &none, got);
        for (i, (cell, &held)) in register.cells.iter().zip(&stored).enumerate()
        {
            for read_line in &cell.reads {
                read(&mut circuit, &register, &[read_line.line])?;
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
            if cell.reads.len() > 1 {
                let lines: Vec<Line> =
                    cell.reads.iter().map(|read| read.line).collect();
                read(&mut circuit, &register, &lines)?;
                let mut expected = none.clone();
                for read_line in &cell.reads {
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
    let mut run = tally.run(&interface, &register, exhaustive);
    if !exhaustive {
        run.patterns = patterns(register.width).len() as u64;
    }
    if ungated == values.len() as u64 && run.wrong > 0 {
        run.hint = Some(
            "With every read lever off, the output still shows what the cells \
             hold, so its reads do not keep it off the bus."
                .to_owned(),
        );
    }
    Ok((run, shape))
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

/// The register's ports, and its circuit settled as saved, with its cells
/// found.
fn ports(
    blocks: &Blocks,
    interface: &Interface,
) -> Result<(Circuit, Register)> {
    let control = format!("{} concrete", lint::CONTROL);
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| interface.inputs[i].block != control)
        .collect();
    let &[data] = operands.as_slice() else {
        broken!(
            "A register takes one data bus, but this has {} inputs besides \
             its magenta lines.",
            operands.len()
        );
    };
    if interface.inputs[data].signal_strength {
        bail!(
            "Input {} takes a signal strength, which the register test does \
             not drive yet.",
            interface.inputs[data].name
        );
    }
    let width = interface.inputs[data].bits.len();
    let buses: Vec<usize> = (0..interface.outputs.len()).collect();
    if buses.is_empty() {
        broken!(
            "A register puts its cells on an output bus, but this has none."
        );
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
    let (mut writes, mut reads) = (Vec::new(), Vec::new());
    for &port in &controls {
        // A sign says which a line is; else a button writes and a lever
        // reads.
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
            if says("write") || (line.button && !says("read")) {
                writes.push(line);
            } else {
                reads.push(line);
            }
        }
    }
    if writes.is_empty() {
        broken!(
            "A register's cells are written by a button on a magenta line, \
             or a line signed write, but this has none."
        );
    }
    if reads.is_empty() {
        broken!(
            "A register's cells are read by a lever on a magenta line, which \
             keeps them off the shared bus until then, but this has none."
        );
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
    register.cells = cells(&mut circuit, &mut register, &writes)?;
    read(&mut circuit, &register, &[])?;
    register.idle = self::buses(&circuit, &register);
    Ok((circuit, register))
}

/// The cells, each write with the reads that show what it wrote, and
/// how. With every cell holding 0 and then each alone holding all ones,
/// the buses are read with every read as saved and with each flipped
/// alone. A read whose flipping changes how the cell shows is the cell's,
/// on the bus where it does: a plain one when it shows the cell flipped,
/// and an inverted one, `active_low`, when it shows it as saved. It is
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
                    line: Line { active_low, ..line },
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
            register.reads[r] = read.line;
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
                    line,
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

/// What the buses show with every read as saved, and with each flipped
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
/// settling. An inverted read is on as saved, so off is flipped.
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
        exhaustive: bool,
    ) -> Run {
        let cells = register.cells.len();
        Run {
            pattern: {
                let inverted = register
                    .cells
                    .iter()
                    .any(|cell| cell.reads.iter().any(|read| read.inverted));
                let read = if inverted { "read inverted" } else { "read" };
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
            timing: None,
            stacking: None,
        }
    }
}

/// When a register's reads settle: from a read lever turning on, with its
/// cell holding each value in turn, until its bus shows it, the bits'
/// changes tick by tick. Its writes too: from a button's press, with the
/// cell's read held on, until the bus shows the new value.
pub fn time(blocks: &Blocks, cancel: &AtomicBool) -> Result<Timing> {
    let interface = interface::read(blocks);
    let (mut circuit, register) = ports(blocks, &interface)?;
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
    for cell in &register.cells {
        for &Read { line, bus, .. } in &cell.reads {
            for &value in &values {
                if cancel.load(Ordering::Relaxed) {
                    bail!("The timing was cancelled.");
                }
                write(&mut circuit, &register, cell.write, value)?;
                circuit.set(register.data, !value & all);
                circuit.settle()?;
                set_reads(&mut circuit, &register, &[line]);
                let changes = watch(&mut circuit, &register)?;
                let last =
                    stats[bus].add(&changes[bus], carried[register.buses[bus]]);
                settled.push((last, value));
                read(&mut circuit, &register, &[])?;
            }
        }
    }
    let takes = if takes_data_closing(&mut circuit, &register)? {
        Takes::Closing
    } else {
        Takes::Open
    };
    let closing = takes == Takes::Closing;
    let write_ticks = write_latency(&mut circuit, &register, closing)?;
    let data_ticks = data_latency(&mut circuit, &register, &carried, closing)?;
    let synced = stats
        .iter()
        .all(|bus| bus.glitches == 0 && bus.staggered == 0);
    let mut ticks: Vec<i32> = settled.iter().map(|&(tick, _)| tick).collect();
    ticks.sort_unstable();
    let at =
        |share: usize| ticks[(ticks.len() * share / 100).min(ticks.len() - 1)];
    let (slowest, slowest_value) = settled
        .iter()
        .copied()
        .max_by_key(|&(tick, _)| tick)
        .unwrap_or_default();
    Ok(Timing {
        inputs: vec![InputTiming {
            name: "read".to_owned(),
            tick: Some(0),
        }],
        outputs: stats
            .into_iter()
            .zip(&register.buses)
            .map(|(bus, &port)| {
                bus.timing(&interface.outputs[port].name, carried[port])
            })
            .collect(),
        synced,
        median: at(50),
        p99: at(99),
        slowest,
        slowest_case: vec![slowest_value],
        changes: settled.len() as u64,
        patterns: !exhaustive,
        from_read: true,
        write: Some(write_ticks),
        data: data_ticks,
        takes,
    })
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

/// Whether the register's cells take their data as their write levers
/// turn off: some cell, its read on and its write held open, keeps a value
/// it was not given off its bus, as a cell whose read is cut while it
/// writes does, and shows it once its lever turns off.
fn takes_data_closing(
    circuit: &mut Circuit,
    register: &Register,
) -> Result<bool> {
    let all = mask(register.width);
    for cell in &register.cells {
        let Some(&read_line) = cell.reads.first() else {
            continue;
        };
        if cell.write.button {
            continue;
        }
        for value in telling(register.width) {
            write(circuit, register, cell.write, !value & all)?;
            circuit.set(register.data, value);
            read(circuit, register, &[read_line.line, cell.write])?;
            let shows = |circuit: &Circuit| {
                buses(circuit, register)[read_line.bus]
                    == read_line.shows(value, register.width)
            };
            let held = shows(circuit);
            read(circuit, register, &[read_line.line])?;
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
/// lever turns off, changing as the lever does, the latest it can. `None`
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
            read(circuit, register, &[read_line.line])?;
            circuit.set(register.data, value);
            circuit.settle()?;
            let shows = |circuit: &Circuit, held: u64| {
                buses(circuit, register)[read_line.bus]
                    == read_line.shows(held, register.width)
            };
            // Open the write, and wait until the cell passes its data.
            if cell.write.button {
                circuit.press(cell.write.port, cell.write.bit);
            } else {
                set_reads(circuit, register, &[read_line.line, cell.write]);
            }
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

/// `data_latency` for cells that take their data as their write levers
/// turn off: from the data and the lever changing on the same tick, the
/// write held open before with the old value, until the bus shows the new
/// one and keeps it.
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
        for value in telling(register.width) {
            let old = !value & all;
            write(circuit, register, cell.write, old)?;
            circuit.set(register.data, old);
            read(circuit, register, &[read_line.line, cell.write])?;
            circuit.set(register.data, value);
            set_reads(circuit, register, &[read_line.line]);
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

/// The most ticks from pressing a cell's write button, its read held on,
/// until its bus shows the value written and keeps it; for a lever, from
/// turning it on, or for cells `closing`, from turning it off, their data
/// set while it was on.
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
            read(circuit, register, &[line])?;
            circuit.set(register.data, value);
            circuit.settle()?;
            if cell.write.button {
                circuit.press(cell.write.port, cell.write.bit);
            } else if closing {
                read(circuit, register, &[line, cell.write])?;
                set_reads(circuit, register, &[line]);
            } else {
                set_reads(circuit, register, &[line, cell.write]);
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
/// a shared line that runs too far fails. `None` when it is not a
/// register.
pub fn stacking(data: &[u8]) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let (_, single) = check_on(&blocks, true).ok()?;
    let (low, high) = interface::measured(&blocks);
    let reach = i64::from(high[0] - low[0] + 1);
    let works = |step: Step, copies: usize, bits: bool, quick: bool| {
        // Copies along a register's bits share its control lines; across
        // them, its data buses.
        let orientation = if bits { "horz" } else { "vert" };
        let buses = stack::Buses::of("register", orientation);
        let (stacked, _) =
            stack::stack(data, &[(step.direction(), copies)], buses).ok()?;
        let blocks = Blocks::read(&stacked).ok()?;
        let (run, shape) = check_on(&blocks, quick).ok()?;
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
        .find(|&(step, bits)| {
            works(step, 2, bits, true).is_some()
                && works(step, 2, bits, false).is_some()
        });
    let Some((step, bits)) = found else {
        return Some(Stacking::Not {
            step: None,
            reason: "No step west, up to its width, makes copies whose every \
                     cell stores and reads right, as more cells or more bits."
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
