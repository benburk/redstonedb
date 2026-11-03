//! Whether copies of a register stack into a bigger one, and how often it
//! takes a new access as a clock drives it: a stream of writes and reads a
//! period apart, each read showing what was last written into its cell.

use anyhow::Result;

use super::{
    Kind, Line, Register, STRIDE, Shape,
    check::{buses, check_on, ports},
};
use crate::{
    models::schematic::Orientation,
    recognize::interface,
    schem::{Blocks, Pos, stack},
    simulate::{
        Value,
        bench::mask,
        circuit::Circuit,
        rng::Rng,
        stacking::{Stacking, Step},
        timing::Timing,
    },
};

/// The most copies a register file is stacked to; one that works with
/// this many is taken to stack any number of times.
const MOST_COPIES: usize = 16;

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
        copies_work(data, kind, &single, (step, copies), bits, quick)
    };
    let found = (1..=reach)
        .map(|west| Step::new(west, 0, 0))
        .flat_map(|step| [(step, false), (step, true)])
        .filter(|&(_, bits)| bits || kind == Kind::Register)
        .find_map(|(step, bits)| {
            let orientation = if bits {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            };
            let buses = stack::Buses::of(kind.category(), orientation);
            let found = step.find(data, bits, buses, |step| {
                (works(step, 2, bits, true).is_some()
                    && works(step, 2, bits, false).is_some())
                .then_some(())
                .ok_or(())
            })?;
            found.ok().map(|(step, ())| (step, bits))
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
            also: Vec::new(),
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

/// How a register's copies share their lines: along its bits, its control
/// lines; across them, its data buses.
fn buses_of(kind: Kind, bits: bool) -> stack::Buses {
    let orientation = if bits {
        Orientation::Horizontal
    } else {
        Orientation::Vertical
    };
    stack::Buses::of(kind.category(), orientation)
}

/// The cases `copies` of the register in `data`, one copy shaped `single`,
/// run when stacked `step` and every cell stores and reads right, as more
/// bits with `bits`, else as more cells; quickly with `quick`.
fn copies_work(
    data: &[u8],
    kind: Kind,
    single: &Shape,
    (step, copies): (Step, usize),
    bits: bool,
    quick: bool,
) -> Option<u64> {
    let buses = buses_of(kind, bits);
    let (stacked, _) =
        stack::stack(data, &[(step.direction(), copies)], buses).ok()?;
    let blocks = Blocks::read(&stacked).ok()?;
    let (run, shape) = check_on(&blocks, kind, quick).ok()?;
    let made = if bits {
        Shape {
            width: single.width * copies,
            ..*single
        }
    } else {
        Shape {
            cells: single.cells * copies,
            ..*single
        }
    };
    (run.wrong == 0 && shape == made).then_some(run.cases)
}

/// Whether `copies` of the register in `data` stacked `step` store and
/// read right, as more cells or, for a buffer only, more bits, as
/// [`stacking`] tries each step: the box rule's box first, then boxes
/// leaving out more of the first copy. The step with its box, the cases
/// run, and whether copies add bits; or why none works. `None` when it is
/// not of its `kind`.
pub fn stacks_at(
    data: &[u8],
    kind: Kind,
    step: Step,
    copies: usize,
) -> Option<Result<(Step, u64, bool), String>> {
    let blocks = Blocks::read(data).ok()?;
    let (_, single) = check_on(&blocks, kind, true).ok()?;
    let mut why = String::new();
    for bits in [false, true]
        .into_iter()
        .filter(|&bits| bits || kind == Kind::Register)
    {
        let found = step.find(data, bits, buses_of(kind, bits), |step| {
            copies_work(data, kind, &single, (step, copies), bits, false)
                .ok_or(())
        });
        match found {
            Some(Ok((step, cases))) => return Some(Ok((step, cases, bits))),
            _ => {
                why = format!(
                    "{copies} copies don't store and read right as more {}",
                    if bits { "bits" } else { "cells" }
                );
            }
        }
    }
    Some(Err(why))
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
/// longer, as the bus lets go of the last. The shortest period that holds,
/// at some timing of its writes, on a short stream and then a long one;
/// `None` when none up to twice its slowest read, write, or data does. A
/// buffer, which has no reads, is written every period, a cell at random,
/// and its cell's bus must show what it was given, as its timing counts
/// from a write.
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
    let levered =
        Blocks::read(&crate::schem::standardize::edit(data, &levers)?)?;
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
    value: Value,
    write: Option<usize>,
    read: Option<(usize, usize)>,
}

/// A fixed stream of `count` accesses to `register`, from a simple
/// generator: each writes a cell or reads one, at random, as a register
/// file may not read one cell while it writes another; a buffer's each
/// write a cell and look at what it shows.
fn accesses(register: &Register, count: usize, kind: Kind) -> Vec<Access> {
    let mut rng = Rng::default();
    (0..count)
        .map(|_| {
            let value = Value::from(rng.draw()) & mask(register.width);
            let cell = rng.below(register.cells.len());
            if kind == Kind::Buffer {
                return Access {
                    value,
                    write: Some(cell),
                    read: Some((cell, 0)),
                };
            }
            if rng.draw() & 1 == 0 {
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
                    read: Some((cell, rng.below(reads))),
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
        let mut reads: Vec<(i32, usize, Value)> = Vec::new();
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
        let mut shown: Vec<Vec<Value>> = Vec::new();
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
