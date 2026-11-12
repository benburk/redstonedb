//! Whether copies of a register stack into a bigger one, and how often it
//! takes a new access as a clock drives it: a stream of writes and reads a
//! period apart, each read showing what was last written into its cell.

use anyhow::Result;

use super::{
    Kind, Line, Register, STRIDE, Shape,
    check::{buses, check_on, ports, set_reads},
};
use crate::{
    interface,
    schem::{Blocks, Pos},
    simulate::{
        Value,
        bench::mask,
        circuit::Circuit,
        rng::Rng,
        stacking::{
            Apart, MOST_COPIES, Stacker, Stacking, Step, Way, bit_ways,
        },
        timing::Timing,
    },
    stack,
};

/// How copies of the register or buffer in `data`, a file whose own cases
/// came out right, stack into more bits sharing its control lines, driven
/// at the first copy's end: at each pitch class the way its bits run,
/// every cell storing and reading right, quickly and then in full, with
/// as many copies as do, as a shared line that runs too far fails. `None`
/// when it is not of its `kind`.
pub fn stacker(data: &[u8], kind: Kind) -> Option<Stacker<'_>> {
    let blocks = Blocks::read(data).ok()?;
    let (_, single) = check_on(&blocks, kind, true).ok()?;
    let interface = interface::read(&blocks);
    let works = move |step: Step, copies: usize, buses| {
        let work = |quick| {
            copies_work(data, kind, &single, (step, copies), buses, true, quick)
        };
        work(true).and_then(|_| work(false)).ok_or_else(|| {
            format!("{copies} copies don't store and read right as more bits.")
        })
    };
    let not = match kind {
        Kind::Register => {
            "No pitch west or up makes copies whose every cell stores and \
             reads right as more bits."
        }
        Kind::Buffer => {
            "No pitch west or up makes copies whose every cell stores and \
             shows right as more bits."
        }
    };
    let ways = (single.width, bit_ways(&interface, single.width));
    Some(Stacker::new(
        data,
        kind.category().copying(),
        ways,
        works,
        not,
    ))
}

/// Whether copies of the register or buffer in `data`, a file whose own
/// cases came out right, stack into a bigger one: as more bits, at each
/// pitch class as [`stacker`] tries them, and, for a register, as more
/// words on its shared data buses, driven and read at the first copy's
/// end, the shortest step across its bits, as far as it reaches, at which
/// copies all store and read right, whatever the spacing. Words stack the
/// way across its bits, west for a vertical register and up for a
/// horizontal one, and a 1-bit register's the way its bits do not. A
/// buffer's copies only add bits, as its cells show all the time and so
/// cannot share a bus. `None` when it is not of its `kind`.
pub fn stacking(data: &[u8], kind: Kind) -> Option<Stacking> {
    let bits = stacker(data, kind)?;
    let found = bits.search();
    let blocks = Blocks::read(data).ok()?;
    let (_, single) = check_on(&blocks, kind, true).ok()?;
    let taken: Vec<Way> = found
        .iter()
        .flatten()
        .filter_map(|apart| Way::of(apart.step))
        .collect();
    let across: Vec<Way> = bit_ways(&interface::read(&blocks), single.width)
        .into_iter()
        .map(Way::across)
        .filter(|way| !taken.contains(way))
        .collect();
    let words = (kind == Kind::Register)
        .then(|| words(data, &blocks, &single, &across))
        .flatten();
    Some(match (found, words) {
        (Ok(mut ways), words) => {
            let first = ways.remove(0);
            Stacking::Wider {
                step: first.step,
                copies: first.copies,
                bits: single.width * first.copies,
                cases: first.cases,
                most: first.most,
                also: ways,
                words,
                place: 1,
            }
        }
        (Err(_), Some(words)) => Stacking::Registers {
            step: words.step,
            copies: words.copies,
            registers: single.cells * words.copies,
            cases: words.cases,
            most: words.most,
        },
        (Err((step, reason)), None) => Stacking::Not {
            step,
            reason: match kind {
                Kind::Register => format!(
                    "{reason} No step across its bits, up to its width, \
                     makes copies whose every cell stores and reads right \
                     as more words."
                ),
                Kind::Buffer => reason,
            },
        },
    })
}

/// How copies of the register in `data`, `blocks`, one copy shaped
/// `single`, stack as more words on its shared data buses, each of `ways`
/// in turn: the shortest step, up to how far it reaches that way, whose
/// copies touch and all store and read right, quickly and then in full,
/// then the most copies that do, up to `MOST_COPIES`, as a shared bus that
/// runs too far fails.
fn words(
    data: &[u8],
    blocks: &Blocks,
    single: &Shape,
    ways: &[Way],
) -> Option<Apart> {
    let (low, high) = interface::measured(blocks);
    let kind = Kind::Register;
    let works = |step: Step, copies: usize, quick: bool| {
        let buses = word_buses(kind, step);
        copies_work(data, kind, single, (step, copies), buses, false, quick)
    };
    ways.iter().find_map(|&way| {
        let axis = usize::from(way == Way::Up);
        let reach = i64::from(high[axis] - low[axis] + 1);
        let step = (1..=reach).map(|n| way.step(n)).find_map(|step| {
            if !stack::touches(data, step.direction()).unwrap_or(false) {
                return None;
            }
            let found =
                step.find(data, false, word_buses(kind, step), |step| {
                    (works(step, 2, true).is_some()
                        && works(step, 2, false).is_some())
                    .then_some(())
                    .ok_or(())
                })?;
            found.ok().map(|(step, ())| step)
        })?;
        let mut most = None;
        let mut copies = 2;
        while copies < MOST_COPIES {
            if works(step, copies + 1, true).is_none() {
                most = Some(copies);
                break;
            }
            copies += 1;
        }
        // The most that pass quickly may yet get a case wrong.
        let cases = loop {
            if let Some(cases) = works(step, copies, false) {
                break cases;
            }
            copies -= 1;
            most = Some(copies);
        };
        Some(Apart {
            step,
            copies,
            cases,
            most,
        })
    })
}

/// How a register's copies stacked `step` as more words share their
/// lines: their data buses, which run across the way they stack.
fn word_buses(kind: Kind, step: Step) -> stack::Buses {
    let across = Way::of(step).map_or(Way::Up, Way::across);
    stack::Buses::of(kind.category().copies(), across.orientation())
}

/// The cases `copies` of the register in `data`, one copy shaped `single`,
/// run when stacked `step`, their buses as `buses` says, and every cell
/// stores and reads right, as more bits with `bits`, else as more cells;
/// quickly with `quick`.
fn copies_work(
    data: &[u8],
    kind: Kind,
    single: &Shape,
    (step, copies): (Step, usize),
    buses: stack::Buses,
    bits: bool,
    quick: bool,
) -> Option<u64> {
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
/// read right, as [`stacking`] tries each step: for a register as more
/// words, then as more bits, a buffer only as more bits, the box rule's
/// box first, then boxes leaving out more of the first copy. The step
/// with its box, the cases run, and whether copies add bits; or why none
/// works. Copies apart, as `stack::touches` has them, never stack.
/// `None` when it is not of its `kind`.
pub fn stacks_at(
    data: &[u8],
    kind: Kind,
    step: Step,
    copies: usize,
) -> Option<Result<(Step, u64, bool), String>> {
    let blocks = Blocks::read(data).ok()?;
    let (_, single) = check_on(&blocks, kind, true).ok()?;
    if !stack::touches(data, step.direction()).ok()? {
        return Some(Err(format!(
            "Copies {} apart leave air between them.",
            step.describe()
        )));
    }
    let mut why = String::new();
    for bits in [false, true]
        .into_iter()
        .filter(|&bits| bits || kind == Kind::Register)
    {
        let buses = if bits {
            let way = Way::of(step).unwrap_or(Way::West);
            stack::Buses::of(kind.category().copies(), way.orientation())
        } else {
            word_buses(kind, step)
        };
        let found = step.find(data, bits, buses, |step| {
            copies_work(data, kind, &single, (step, copies), buses, bits, false)
                .ok_or(())
        });
        match found {
            Some(Ok((step, cases))) => return Some(Ok((step, cases, bits))),
            _ => {
                why = format!(
                    "{copies} copies don't store and read right as more {}",
                    if bits { "bits" } else { "words" }
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
    let levered = Blocks::read(&crate::schem::edit(data, &levers)?)?;
    // Each bus's latency as it arrives at the repeaters or comparators
    // carrying it out, as its timing gives it, each bit read as many ticks
    // later as its own takes to give it out.
    let latency: Vec<i32> =
        timing.outputs.iter().map(|o| o.latency.max(0)).collect();
    if latency.len() != register.buses.len() {
        return Ok(None);
    }
    let carried = Circuit::new(&levered, &interface::read(&levered))?.carried();
    let carried: Vec<Vec<(i32, Value)>> = register
        .buses
        .iter()
        .map(|&port| carried.groups(port, register.width))
        .collect();
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
                    carried: &carried,
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
    let on: Vec<Line> = read.into_iter().chain(write).collect();
    set_reads(circuit, register, &on);
}

/// A register's stream: an access every `period` ticks, each write
/// opening `offset` ticks after its data, for `open` ticks.
#[derive(Clone, Copy)]
struct Stream<'a> {
    register: &'a Register,
    latency: &'a [i32],
    /// Each bus's bits by the ticks their repeaters or comparators take to
    /// give them out.
    carried: &'a [Vec<(i32, Value)>],
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
        let carried = self.carried.iter().flatten().map(|&(ticks, _)| ticks);
        let end = self.period * i32::try_from(steps.len())?
            + late
            + self.latency.iter().copied().max().unwrap_or(0)
            + carried.max().unwrap_or(0);
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
                    self.carried[bus].iter().all(|&(carried, bits)| {
                        usize::try_from(at + delay + carried)
                            .ok()
                            .and_then(|t| shown.get(t))
                            .is_some_and(|buses| {
                                (buses[bus] ^ shows) & bits == 0
                            })
                    })
                })
            })
            .collect())
    }
}
