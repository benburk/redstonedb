//! When a register's reads and writes settle: each read timed from its
//! lever turning on, or a buffer's write from it opening or closing, and
//! the ticks for data and for a write to pass through a cell to its bus. A
//! register whose controls can be timed against its data is timed from
//! its data changing instead.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};

use super::{
    Cell, Kind, Line, Read, Register,
    check::{buses, ports, read, set_reads, telling, values, write},
};
use crate::{
    category,
    interface::{self, Interface},
    schem::{Blocks, Pos},
    simulate::{
        Value,
        bench::mask,
        circuit::{Carried, Circuit, MAX_TICKS, output_bits},
        timing::{
            InputTiming, OutputStats, OutputTiming, Start, Takes, Timing,
        },
    },
};

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
    let mut stats: Vec<OutputStats> = widths
        .iter()
        .map(|&width| OutputStats::new(width))
        .collect();
    let mut settled: Vec<(i32, Value)> = Vec::new();
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
                let last = stats[bus]
                    .add(&carried.arrived(register.buses[bus], &changes[bus]));
                settled.push((last, value));
                read(&mut circuit, &register, &[])?;
            }
        }
    }
    let write_ticks =
        write_latency(&mut circuit, &register, &carried, closing)?;
    let data_ticks = data_latency(&mut circuit, &register, &carried, closing)?;
    let synced = stats
        .iter()
        .all(|bus| bus.glitches == 0 && bus.staggered == 0);
    let (median, p99, slowest, slowest_value) = spread(&settled);
    let (opens, start) = opened(kind);
    Ok(Timing {
        inputs: vec![InputTiming {
            name: opens.to_owned(),
            tick: Some(0),
            earliest: None,
        }],
        outputs: bus_timings(&stats, &register, &interface, &carried),
        synced,
        median,
        p99,
        slowest,
        slowest_case: vec![slowest_value],
        changes: settled.len() as u64,
        patterns: !exhaustive,
        start,
        write: Some(write_ticks),
        data: data_ticks,
        takes,
        pulse: None,
        free_running: None,
    })
}

/// The line a `kind`'s timing starts from, by name, and its start: a
/// register's read, a buffer's write.
fn opened(kind: Kind) -> (&'static str, Start) {
    match kind {
        Kind::Register => ("read", Start::Read),
        Kind::Buffer => ("write", Start::Write),
    }
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
    value: Value,
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
    carried: &Carried,
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
            latest = latest.max(last(&carried.arrived(port, &changes)));
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
    carried: &Carried,
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
    carried: &Carried,
    values: (&[Value], bool),
) -> Result<Option<Timing>> {
    let Some(found) = offsets(circuit, register, carried)? else {
        return Ok(None);
    };
    // The lines as a register's are named, to say which is which.
    let mut named = interface::read(blocks);
    interface::name_lines(category::Kind::Register.lines(), blocks, &mut named);
    let mut timing = aligned(
        circuit,
        (interface, &named),
        register,
        carried,
        &found,
        values,
    )?;
    timing.write = Some(write_latency(circuit, register, carried, false)?);
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
    carried: &Carried,
    found: &Offsets,
    (values, exhaustive): (&[Value], bool),
) -> Result<Timing> {
    let widths: Vec<usize> = register
        .buses
        .iter()
        .map(|&bus| output_bits(interface, &interface.outputs[bus]))
        .collect();
    let mut stats: Vec<OutputStats> = widths
        .iter()
        .map(|&width| OutputStats::new(width))
        .collect();
    let mut settled: Vec<(i32, Value)> = Vec::new();
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
            let port = register.buses[read_line.bus];
            let last =
                stats[read_line.bus].add(&carried.arrived(port, &changes));
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
        outputs: bus_timings(&stats, register, named, carried),
        synced,
        median,
        p99,
        slowest,
        slowest_case: vec![slowest_value],
        changes: settled.len() as u64,
        patterns: !exhaustive,
        start: Start::Input,
        data: Some(found.latency),
        takes: Takes::Open,
        ..Timing::default()
    })
}

/// Each bus's timing from `stats`, by the name `interface` gives it, with
/// the ticks `carried` its repeaters or comparators take to carry it out.
fn bus_timings(
    stats: &[OutputStats],
    register: &Register,
    interface: &Interface,
    carried: &Carried,
) -> Vec<OutputTiming> {
    stats
        .iter()
        .zip(&register.buses)
        .map(|(bus, &port)| {
            bus.timing(&interface.outputs[port].name, false, carried.port(port))
        })
        .collect()
}

/// Of reads that each settled on a tick with a value, the tick half of them
/// settled by, 99% by, and the slowest, with its value.
fn spread(settled: &[(i32, Value)]) -> (i32, i32, i32, Value) {
    let mut ticks: Vec<i32> = settled.iter().map(|&(tick, _)| tick).collect();
    ticks.sort_unstable();
    let at = |share: usize| crate::simulate::timing::percentile(&ticks, share);
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
    let widths = vec![register.width; register.buses.len()];
    crate::simulate::timing::watch(circuit, &widths, "a read", |circuit| {
        buses(circuit, register)
    })
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
    carried: &Carried,
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
            let shows = |circuit: &Circuit, held: Value| {
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
            // The data changes while the write is still open, and passes
            // once its bus shows it as it arrives, its every bit by then.
            circuit.set(register.data, next);
            let before = buses(circuit, register)[read_line.bus];
            let mut steps = Vec::new();
            let mut tick = 0;
            while circuit.step() {
                tick += 1;
                if tick > MAX_TICKS.cast_signed() {
                    bail!(
                        "It was still changing {MAX_TICKS} ticks after its \
                         data."
                    );
                }
                steps.push((tick, buses(circuit, register)[read_line.bus]));
            }
            let want = read_line.shows(next, register.width);
            let passed = carried
                .arrived_steps(port, before, &steps)
                .into_iter()
                .find(|&(_, shown)| shown == want);
            let Some((ticks, _)) = passed else {
                return Ok(None);
            };
            slowest = Some(slowest.map_or(ticks, |s: i32| s.max(ticks)));
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
    carried: &Carried,
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
            let ticks = last(&carried.arrived(port, &changes[read_line.bus]));
            slowest = Some(slowest.map_or(ticks, |s: i32| s.max(ticks)));
            read(circuit, register, &[])?;
        }
    }
    Ok(slowest)
}

/// The most ticks from opening a cell's write, pressing its button or
/// turning its lever on, its read held on, until its bus shows the value
/// written and keeps it, less the ticks `carried` its repeaters take to
/// carry it out, as every other latency is; or for cells `closing`, from
/// the write closing, their data set while it was open.
fn write_latency(
    circuit: &mut Circuit,
    register: &Register,
    carried: &Carried,
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
            let port = register.buses[bus];
            slowest = slowest.max(last(&carried.arrived(port, &changes[bus])));
            read(circuit, register, &[])?;
        }
    }
    Ok(slowest)
}

/// The last tick any bit of `changes`, each bit's ticks, changed on; 0 for
/// none.
fn last(changes: &[Vec<i32>]) -> i32 {
    changes.iter().flatten().max().copied().unwrap_or(0)
}
