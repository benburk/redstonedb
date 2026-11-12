//! Running a register or buffer through its cases: finding its ports and
//! which read shows which cell, writing each value into every cell and
//! reading it back, and, when a case comes out wrong, the faults that
//! explain it.

use anyhow::{Result, bail};

use super::{Cell, Kind, Line, Read, Register, STRIDE, Shape};
use crate::{
    interface::{self, Interface},
    schem::Blocks,
    simulate::{
        Action, Broken, CellControls, Reading, Run, Signal, Value, Wrong,
        bench::{mask, patterns},
        broken,
        circuit::{Circuit, output_bits},
    },
};

/// The run, and the shape of the register it found.
pub(super) fn check_on(
    blocks: &Blocks,
    kind: Kind,
    quick: bool,
) -> Result<(Run, Shape)> {
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
        let stored: Vec<Value> = (0..cells as Value)
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
/// its levers off, as the writes and reads that show it: a cell that reads
/// wrong after a write into it alone, and a cell that reads differently
/// after another is written. The value written into another cell is
/// narrowed to a single bit set, or else clear, so the steps point at the
/// bit that disturbs it.
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
    let single: Vec<Value> = (0..width)
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
    others: &[Value],
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
    value: Value,
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
    held: Value,
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
fn shown(register: &Register, stored: &[Value]) -> Vec<Value> {
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
pub(super) fn values(width: usize) -> (Vec<Value>, bool) {
    if width <= 8 {
        return ((0..1 << width).collect(), true);
    }
    let mut values = patterns(width);
    for low in (0..width).step_by(8) {
        values.extend((0..256).map(|byte| byte << low & mask(width)));
    }
    values.sort_unstable();
    values.dedup();
    (values, false)
}

/// A few values that show whether each bit stores and reads: none and all
/// set, and alternate bits each way.
pub(super) fn telling(width: usize) -> Vec<Value> {
    let all = mask(width);
    let alternate = 0x5555_5555_5555_5555 & all;
    let mut values = vec![all, 0, alternate, !alternate & all];
    values.dedup();
    values
}

/// What `writes` and `reads`, a build's control lines, are, as its
/// faults say: so many buttons, so many levers, as `2 buttons, no levers`.
fn switches(writes: &[Line], reads: &[Line]) -> String {
    let lines = writes.len() + reads.len();
    let buttons = writes
        .iter()
        .chain(reads)
        .filter(|line| line.button)
        .count();
    let count = |n: usize, one: &str| match n {
        0 => format!("no {one}s"),
        1 => format!("1 {one}"),
        n => format!("{n} {one}s"),
    };
    format!(
        "{}, {}",
        count(buttons, "button"),
        count(lines - buttons, "lever")
    )
}

/// The register's ports, and its circuit settled with its levers off,
/// with its cells found.
pub(super) fn ports(
    blocks: &Blocks,
    interface: &Interface,
    kind: Kind,
) -> Result<(Circuit, Register)> {
    let what = kind.category().facts().noun;
    let operands: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| !interface.inputs[i].control())
        .collect();
    let &[data] = operands.as_slice() else {
        broken!(
            "A {what} takes one data bus, but this has {} inputs besides its \
             control lines.",
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
        .filter(|&i| interface.inputs[i].control())
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
    let found = switches(&writes, &reads);
    if writes.is_empty() {
        broken!(
            "Control lines: expected a button writing each cell, found {found}."
        );
    }
    match kind {
        Kind::Register if reads.is_empty() => broken!(
            "Control lines: expected a button writing each cell and a lever \
             reading it, found {found}."
        ),
        Kind::Buffer if !reads.is_empty() => broken!(
            "Control lines: expected only writes, as a buffer shows what it \
             holds, found {found}."
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
        let changed = |a: &[Value], b: &[Value]| -> Vec<Value> {
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

/// A buffer's cells, each write with the buses that show what it wrote, all
/// the time. With every cell holding 0 and then each alone holding all
/// ones, a bus that changes is the cell's, and shows it as it is: unlike a
/// register's reads, a buffer's outputs may not show their cells inverted,
/// so one that shows all ones as zeros gets its cases wrong. A write that
/// changes the same buses as one before is a second button on that cell's
/// line, as each copy of a stacked buffer keeps its own, and the first
/// stands for it. Each bus must show one cell.
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
                inverted: false,
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
) -> Result<(Vec<Value>, Vec<Vec<Value>>)> {
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
pub(super) fn write(
    circuit: &mut Circuit,
    register: &Register,
    line: Line,
    value: Value,
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
pub(super) fn set_reads(
    circuit: &mut Circuit,
    register: &Register,
    on: &[Line],
) {
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
pub(super) fn read(
    circuit: &mut Circuit,
    register: &Register,
    on: &[Line],
) -> Result<()> {
    set_reads(circuit, register, on);
    circuit.settle()?;
    Ok(())
}

/// What each output bus shows.
pub(super) fn buses(circuit: &Circuit, register: &Register) -> Vec<Value> {
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
        value: Value,
        expected: &[Value],
        got: Vec<Value>,
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
                *count += u64::from((want ^ showed) >> bit & 1 == 1);
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
        let inverted = kind == Kind::Register
            && register
                .cells
                .iter()
                .any(|cell| cell.reads.iter().any(|read| read.inverted));
        Run {
            pattern: {
                let read = match (kind, inverted) {
                    (Kind::Register, false) => "read",
                    (Kind::Register, true) => "read inverted",
                    (Kind::Buffer, _) => "shown",
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
            bit_counts: Vec::new(),
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
            reading: Some(Reading::Register { inverted }),
            strength: None,
            cells: Vec::new(),
            faults: Vec::new(),
            timing: None,
            stacking: None,
            design: None,
        }
    }
}
