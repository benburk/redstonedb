//! Recognizing a build: the file made simulatable, inspected, and tried
//! as each standard category in turn until one holds, by matching where
//! matching reads the kind and by its probe where not, each judged by the
//! kind's own test, its signs taken out but those the test reads; and,
//! when none holds, the fault to show and the file carrying the closest
//! try's roles.

use std::collections::BTreeMap;

use super::{
    Fault, Order, Roles, alu, hex_adder, hex_to_binary, multiplier,
    one_operand, register, sequential, shifter,
};
use crate::{
    category::{Kind, Probe, Probing, Test},
    interface::inspect,
    recognize::{bus_drivable::not_bus_drivable, matching},
    schem::{
        self, Blocks, Pos,
        lines::{self, Role},
    },
    simulate,
    standard::orient,
};

/// What the check by function makes of an uploaded file: the file carrying
/// its roles, see [`crate::schem::lines`], when it is recognized as a
/// category; when not, the closest try's, when that try ran and got cases
/// wrong, so its test shows which; else none; what was found; and what
/// only its author can put right. Its blocks keep their colours.
pub struct Recognition {
    pub file: Vec<u8>,
    pub found: Option<String>,
    pub problems: Vec<String>,
    /// Its inputs and outputs as the check found them, in short; see
    /// [`inspect::Report::summary`].
    pub lines: String,
}

/// The file in `data` with what the simulator does not model but redstone
/// treats as a plain block in its place: a slab, stairs, or glowstone as
/// white stained glass, which redstone sees through as it does them, and a
/// furnace, cauldron, or cake as a barrel giving the same strength. Every
/// colour stays.
pub fn simulatable(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let data = crate::standard::standardize::barrels(data, &[])?;
    let blocks = Blocks::read(&data)?;
    let changes: BTreeMap<Pos, String> = blocks
        .iter()
        .filter(|&(_, state)| schem::block::like_glass(schem::short(state)))
        .map(|(pos, _)| (pos, "minecraft:white_stained_glass".to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data);
    }
    crate::schem::edit(&data, &changes)
}

/// The file in `data` inspected and, if nothing is wrong with its inputs
/// and outputs, probed as each kind probing tries in turn.
pub fn recognize(data: &[u8]) -> anyhow::Result<Recognition> {
    recognize_as(data, None)
}

/// As `recognize`, probing only as the category named `meant`, when
/// given. Probed as each and recognized as none, its fault is as
/// `fault_of` chooses. Its inputs and outputs are as
/// [`inspect::inspect`] finds them, colour playing no part.
pub fn recognize_as(
    data: &[u8],
    meant: Option<&str>,
) -> anyhow::Result<Recognition> {
    recognize_watched(data, meant, &mut |_, _| {})
}

/// The file in `data`, a stored build of the category named `category`,
/// carrying the roles probing it again finds, and whether it was found as
/// it: those of the reading found, or, found as none, of the nearest
/// reading whose test ran and got cases wrong, so its fault is told on the
/// lines it has, as an upload's is. `None` when no reading's test ran, its
/// roles best kept as they are.
pub fn reprobe(
    data: &[u8],
    category: &str,
) -> anyhow::Result<Option<(Vec<u8>, bool)>> {
    let recognition = recognize_as(data, Some(category))?;
    let found = recognition.found.is_some();
    // Found as none, its file is the nearest reading's, or else `data`.
    Ok(
        (found || recognition.file != data)
            .then_some((recognition.file, found)),
    )
}

/// As [`recognize_as`], telling `trying` each kind as it is probed, with
/// how many of the kinds probing tries came before it; see
/// [`Kind::probed`].
pub fn recognize_watched(
    data: &[u8],
    meant: Option<&str>,
    trying: &mut dyn FnMut(Kind, usize),
) -> anyhow::Result<Recognition> {
    recognize_laid(data, meant, trying, true)
}

/// A build being probed: its file, made simulatable, its blocks, what
/// inspecting them found, and, when levers alone drive it, what it did
/// driven once, which matching reads every kind from.
struct Subject {
    data: Vec<u8>,
    blocks: Blocks,
    report: inspect::Report,
    observed: Option<matching::Observation>,
}

impl Subject {
    /// The build in `data`, whatever roles it came with left out: its
    /// roles are what probing finds.
    fn of(data: &[u8]) -> anyhow::Result<Subject> {
        let data = simulatable(&lines::without(data)?)?;
        let blocks = Blocks::read(&data)?;
        let report = inspect::inspect(&blocks);
        let observed = matching::observe(&blocks, &report).ok();
        Ok(Subject {
            data,
            blocks,
            report,
            observed,
        })
    }

    /// What is wrong with its inputs and outputs, which keeps it from
    /// being probed at all.
    fn problems(&self) -> anyhow::Result<Vec<String>> {
        let mut problems: Vec<String> =
            self.report.faults().map(|p| p.message.clone()).collect();
        if problems.is_empty() {
            problems.extend(not_bus_drivable(&self.data)?.into_iter().map(
                |lever| {
                    format!(
                        "Lever at {}: not bus-drivable",
                        inspect::place(lever)
                    )
                },
            ));
        }
        Ok(problems)
    }

    /// It recognized as nothing, for `problems`, its file as it is.
    fn unrecognized(&self, problems: Vec<String>) -> Recognition {
        Recognition {
            file: self.data.clone(),
            found: None,
            problems,
            lines: self.report.summary(),
        }
    }

    /// It recognized as a `kind`, as `found` has it, unless a data input
    /// found has a block behind its lever, which only a control line's
    /// may.
    fn accept(&self, kind: Kind, found: Found) -> Recognition {
        let (data_inputs, file, found) = found;
        let file = carry_on_top(kind, &file).unwrap_or(file);
        let faults = self.report.data_faults(&data_inputs);
        if !faults.is_empty() {
            return self.unrecognized(faults);
        }
        Recognition {
            file,
            found: Some(found),
            problems: Vec::new(),
            lines: self.report.summary(),
        }
    }
}

/// The kind a build meant as the category named `meant` is probed as: its
/// own, or another's, as a subtractor is the adder it is, its B
/// subtracted; `None` for one never probed, a category off the list.
fn probed_kind(meant: &str) -> Option<Kind> {
    Kind::named(meant).map(Kind::probed_as)
}

/// As [`recognize_watched`]. With `reorient`, a build whose closest try
/// got cases wrong is probed as that kind again laid the standard way, as
/// [`orient::orient`] lays it for publishing and the guess and the
/// simulation read it: one whose lowest bits sit at its west end reads
/// its buses the wrong way round as it lies.
fn recognize_laid(
    data: &[u8],
    meant: Option<&str>,
    trying: &mut dyn FnMut(Kind, usize),
    reorient: bool,
) -> anyhow::Result<Recognition> {
    let meant: Option<Option<Kind>> = meant.map(probed_kind);
    let subject = Subject::of(data)?;
    let mut problems = subject.problems()?;
    if !problems.is_empty() {
        return Ok(subject.unrecognized(problems));
    }
    // Each kind probing tries in turn, or only the one meant, its fault
    // kept when it isn't one.
    let mut faults: Vec<(Kind, Fault)> = Vec::new();
    for (i, &(kind, how, test)) in Kind::probed().iter().enumerate() {
        if meant.is_some_and(|meant| meant != Some(kind)) {
            continue;
        }
        trying(kind, i);
        match subject.probe(kind, how, test, false, Order::default())? {
            Ok((_, file, _)) if leaves_digits(&subject.report, &file) => {
                faults.push((kind, Fault::Unfit(LEAVES_DIGITS.to_owned())));
            }
            Ok(found) => return Ok(subject.accept(kind, found)),
            Err(fault) => faults.push((kind, fault)),
        }
    }
    // Meant as a kind never probed, it is none read loosely either.
    let any = meant != Some(None);
    if reorient
        && any
        && let Some((kind, found)) =
            subject.loose(meant.flatten(), &mut faults)?
    {
        return Ok(subject.accept(kind, found));
    }
    // Meant as a kind never probed, nothing was, so there is no fault.
    let fault =
        fault_of(&subject.blocks, &subject.report, meant.flatten(), &faults);
    // Laid as the roles of the try that came nearest have it read.
    let nearest = faults
        .iter()
        .find(|(_, f)| fault.is_some_and(|fault| std::ptr::eq(f, fault)))
        .and_then(|(kind, fault)| Some((*kind, fault.file()?)));
    if reorient && let Some((kind, file)) = nearest {
        let (laid, turn) = orient::orient(file)?;
        if !turn.is_none() {
            let again =
                recognize_laid(&laid, Some(kind.name()), trying, false)?;
            if again.found.is_some() {
                return Ok(again);
            }
        }
    }
    if reorient
        && any
        && let Some((found, file)) = subject.unaligned(meant.flatten())?
    {
        let reading = (found.data_inputs(), file, found.found());
        return Ok(subject.accept(found.kind, reading));
    }
    problems.push(fault.map_or_else(String::new, ToString::to_string));
    // The file with the roles of the try whose fault is shown, when it ran.
    let closest = fault.and_then(Fault::file).unwrap_or(&subject.data);
    Ok(Recognition {
        file: closest.to_vec(),
        ..subject.unrecognized(problems)
    })
}

impl Subject {
    /// It read as a kind matching reads, its buses bent too, as operands
    /// split in two or outputs unevenly spaced, every case right: the
    /// first such kind, of those probing tries, or only the kind `meant`,
    /// and what was found, its I/O unaligned. A reading whose test ran and
    /// got cases wrong, a failing one, has its fault take the place among
    /// `faults` of a strict reading's that couldn't run, so it is shown
    /// as the kind it is.
    fn loose(
        &self,
        meant: Option<Kind>,
        faults: &mut Vec<(Kind, Fault)>,
    ) -> anyhow::Result<Option<(Kind, Found)>> {
        let mut wrong = None;
        for &(kind, how, test) in &Kind::probed() {
            if how != Probe::Matched || meant.is_some_and(|meant| meant != kind)
            {
                continue;
            }
            match self.probe(kind, how, test, true, Order::default())? {
                Ok(found) => return Ok(Some((kind, found))),
                Err(fault) if fault.ran() && wrong.is_none() => {
                    wrong = Some((kind, fault));
                }
                Err(_) => {}
            }
        }
        if let Some((kind, fault)) = wrong {
            match faults.iter().position(|(k, _)| *k == kind) {
                Some(i) if !faults[i].1.ran() => faults[i].1 = fault,
                Some(_) => {}
                None => faults.push((kind, fault)),
            }
        }
        Ok(None)
    }
}

/// `file`, a reading of a build of `kind`, with the top bit of an adder's
/// output marked its carry out where the output is one bit wider than A
/// and it has no carry out line: a sum is as wide as its operands, and the
/// bit beyond them the carry, as a 10-bit adder's eleventh output is.
fn carry_on_top(kind: Kind, file: &[u8]) -> anyhow::Result<Vec<u8>> {
    let adds = matches!(
        kind,
        Kind::Adder | Kind::HexAdder | Kind::Subtractor | Kind::InvertedAdder
    );
    let blocks = Blocks::read(file)?;
    let (true, Some(roles)) = (adds, blocks.roles.as_ref()) else {
        return Ok(file.to_vec());
    };
    let found = crate::interface::read(&blocks);
    let role = |port: &&crate::interface::Port, role| port.role == Some(role);
    let a = found.inputs.iter().find(|p| role(p, Role::Operand(0)));
    let sums: Vec<_> = found
        .outputs
        .iter()
        .filter(|p| role(p, Role::Output(0)))
        .collect();
    let status = found.outputs.iter().any(|p| role(&p, Role::Status));
    match (a, sums.as_slice()) {
        (Some(a), [sum])
            if !status
                && !a.signal_strength
                && sum.bits.len() == a.bits.len() + 1 =>
        {
            let mut roles = roles.clone();
            if let Some(top) = sum.bits.last() {
                roles.mark(Role::Status, [&top.pos]);
            }
            lines::with(file, Some(&roles))
        }
        _ => Ok(file.to_vec()),
    }
}

/// Why a reading that leaves a digit input out is none; see
/// [`leaves_digits`].
const LEAVES_DIGITS: &str = "Its digit inputs play no part in it.";

/// Whether the roles `file` carries, a probe's reading, leave out a digit
/// input `report` found: one that uses none of them, as a one-bit
/// popcount reading a hex adder's carry lever as its input, is not what
/// the build is.
fn leaves_digits(report: &inspect::Report, file: &[u8]) -> bool {
    let Ok(blocks) = Blocks::read(file) else {
        return false;
    };
    let Some(roles) = blocks.roles.as_ref() else {
        return false;
    };
    report
        .digits
        .iter()
        .flat_map(|run| &run.bits)
        .any(|&pos| roles.get(pos).is_none())
}

impl Subject {
    /// It as matching reads it where no kind's own test judges that
    /// reading, as an ALU's, whose test reads its operations from signs:
    /// as the kind `meant`, if one is, or else any, its lines read as the
    /// conventions don't lay them, every case observed right; what it
    /// found, and the file carrying its roles, its signs taken out as
    /// probing's are. `None` when there is no such reading, or it keeps
    /// the conventions, which probing would have found, or it is of a
    /// kind matching is probed by, which [`Subject::loose`] has judged.
    fn unaligned(
        &self,
        meant: Option<Kind>,
    ) -> anyhow::Result<Option<(matching::Match, Vec<u8>)>> {
        let Some(found) = self.observed.as_ref().and_then(|observed| {
            let order = Order::default();
            matching::read_as(None, &self.blocks, observed, true, order)
        }) else {
            return Ok(None);
        };
        let kind = found.kind.probed_as();
        let judged = matches!(
            kind.facts().probe,
            Probing::Tried {
                how: Probe::Matched,
                ..
            }
        );
        let fits = meant.is_none_or(|meant| meant == kind);
        if found.wrong > 0 || found.conventional || !fits || judged {
            return Ok(None);
        }
        let file = lines::with(&self.data, Some(&found.roles()))?;
        let file = unlabelled(found.kind, &file)?;
        Ok(Some((found, file)))
    }
}

/// What a probe found: the build's data inputs, its file carrying the
/// roles found, and what it is, in words.
pub type Found = (Vec<Pos>, Vec<u8>, String);

/// The roles of the lines of the build in `data`, kept as built and meant
/// as a `kind`, as probing as that kind finds them: its inputs and outputs
/// read whatever faults the conventions find in them, and its lines laid
/// out loosely, as only its test says which is which; of a try that got
/// cases wrong, the closest. Of two operands its test passes either way
/// round, A is as `order` prefers. `None` when no try's test ran, as for a
/// kind never probed.
pub fn kept_roles(
    data: &[u8],
    kind: Kind,
    order: Order,
) -> anyhow::Result<Option<Roles>> {
    let kind = kind.probed_as();
    let probed = Kind::probed();
    let Some(&(kind, how, test)) = probed.iter().find(|(k, ..)| *k == kind)
    else {
        return Ok(None);
    };
    let probed = Subject::of(data)?.probe(kind, how, test, true, order)?;
    let file = match probed {
        Ok((_, file, _)) => file,
        Err(fault) => match fault.file() {
            Some(file) => file.to_vec(),
            None => return Ok(None),
        },
    };
    lines::of(&file)
}

/// What a probe found before it is put in words: the build's data inputs,
/// its file carrying the roles found, and its test's run.
type Probed = (Vec<Pos>, Vec<u8>, simulate::Run);

impl Subject {
    /// It probed as a `kind`, `how` its row says, by its `test`: what was
    /// found, its signs taken out but those its test reads, or why it is
    /// not one. With `loose`, its lines may bend the conventions; of two
    /// operands its test passes either way round, A is as `order`
    /// prefers.
    fn probe(
        &self,
        kind: Kind,
        how: Probe,
        test: Test,
        loose: bool,
        order: Order,
    ) -> anyhow::Result<Result<Found, Fault>> {
        let probed = match how {
            Probe::Matched => self.matched(kind, test, loose, order),
            _ => self.probed(how, test, order),
        };
        let (inputs, file, run) = match probed {
            Ok(probed) => probed,
            Err(fault) => return Ok(Err(fault)),
        };
        // An adder is one or a subtractor, and a hex adder adds or
        // subtracts digits.
        let named = match kind {
            Kind::Adder => run.adder_category(false),
            Kind::HexAdder => run.adder_category(true),
            _ => kind,
        };
        let found = format!(
            "{}: all {} cases right, {}",
            named.a(),
            run.cases,
            run.pattern
        );
        Ok(Ok((inputs, unlabelled(kind, &file)?, found)))
    }
}

/// The file in `data`, carrying the roles probing found, found to be a
/// `kind`, less its signs: the site names its lines itself. A sign on a
/// control or status line stays where its kind's test reads what it says,
/// as an ALU's names the operations a line chooses and which status line
/// is its carry out, overflow, shift out, or zero, a barrel shifter's the
/// amount a line shifts by, and a register's or buffer's whether a line
/// reads or writes. A sign marking a bus's least or most significant bit
/// stays too, as its buses are read in the order it gives.
fn unlabelled(kind: Kind, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let reads_signs = matches!(
        kind,
        Kind::Alu | Kind::BarrelShifter | Kind::Register | Kind::Buffer
    );
    let blocks = Blocks::read(data)?;
    // A status line's sign hangs on the block holding its output up, which
    // carries its role, as a control line's does on its lever's block.
    let on_line = |pos: Pos| {
        blocks.sign_block(pos).is_some_and(|block| {
            matches!(
                blocks.roles.as_ref().and_then(|roles| roles.get(block)),
                Some(Role::Control | Role::Status)
            )
        })
    };
    let orders = |lines: &[String]| {
        lines.iter().any(|line| {
            matches!(line.trim().to_lowercase().as_str(), "lsb" | "msb")
        })
    };
    let changes: BTreeMap<Pos, String> = blocks
        .signs
        .iter()
        .filter(|&(&pos, lines)| {
            let stays = (reads_signs && on_line(pos)) || orders(lines);
            !stays
        })
        .map(|(&pos, _)| (pos, schem::AIR.to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data.to_vec());
    }
    crate::schem::edit(data, &changes)
}

impl Subject {
    /// It read as a `kind` the way matching reads it from what it was
    /// observed doing, bent buses too with `loose`, and judged by the
    /// kind's `test`. Where no reading's test runs, it is read plainly,
    /// see [`plain_roles`], so its test says what is wrong with its lines
    /// in its own words.
    fn matched(
        &self,
        kind: Kind,
        test: Test,
        loose: bool,
        order: Order,
    ) -> Result<Probed, Fault> {
        let found = self.observed.as_ref().and_then(|observed| {
            matching::read_as(Some(kind), &self.blocks, observed, loose, order)
        });
        let read = match &found {
            Some(found) => self
                .judged(test, found)?
                .map(|(file, run)| (found.data_inputs(), file, run)),
            None => Err(Fault::Unfit(String::new())),
        };
        match read {
            Ok(found) => Ok(found),
            Err(fault) if fault.ran() => Err(fault),
            Err(fault) => {
                let (roles, operands) = plain_roles(kind, &self.report, order);
                match super::try_roles(&self.data, &roles, test, false)? {
                    Ok((file, run)) => Ok((operands, file, run)),
                    Err(plain) => Err(fault.nearer(plain)),
                }
            }
        }
    }

    /// What `test` makes of it read as `found` reads it, each bus in the
    /// order matching found it in: the file carrying its roles and the
    /// run, every case right; else the test's fault. A file that can't be
    /// given its roles fails the outer result.
    fn judged(
        &self,
        test: Test,
        found: &matching::Match,
    ) -> Result<Result<(Vec<u8>, simulate::Run), Fault>, Fault> {
        let file = lines::with(&self.data, Some(&found.roles()))
            .map_err(|err| Fault::Unfit(err.to_string()))?;
        let interface = found.interface(&self.blocks);
        Ok(match test.check_with(&self.blocks, interface, false) {
            Ok(run) if run.wrong == 0 => Ok((file, run)),
            Ok(run) => Err(Fault::wrong(&run, &file)),
            Err(err) => Err(Fault::NotRun(err.to_string())),
        })
    }

    /// It probed `how` a kind matching does not read is probed, by its
    /// `test`, its operands in `order`.
    fn probed(
        &self,
        how: Probe,
        test: Test,
        order: Order,
    ) -> Result<Probed, Fault> {
        let (data, blocks, report) = (&self.data, &self.blocks, &self.report);
        match how {
            Probe::Matched => unreachable!("see `Subject::matched`"),
            Probe::HexAdder => hex_adder(data, blocks, report, test, order)
                .map(|probed| (probed.digits, probed.file, probed.run)),
            Probe::HexToBinary => hex_to_binary(data, blocks, report, test)
                .map(|probed| (probed.digits, probed.file, probed.run)),
            Probe::Register => register(data, report, test)
                .map(|probed| (probed.data_in, probed.file, probed.run)),
            Probe::Shifter => shifter(data, report)
                .map(|probed| (probed.a, probed.file, probed.run)),
            Probe::Alu => alu(data, report, order).map(|probed| {
                ([probed.a, probed.b].concat(), probed.file, probed.run)
            }),
            Probe::Multiplier => {
                multiplier(data, report, order).map(|probed| {
                    let inputs = [probed.a, probed.b, probed.c].concat();
                    (inputs, probed.file, probed.run)
                })
            }
            Probe::OneOperand(line) => one_operand(data, report, test, line)
                .map(|probed| (probed.a, probed.file, probed.run)),
            Probe::Sequential => sequential(data, report, test)
                .map(|probed| (probed.data, probed.file, probed.run)),
        }
    }
}

/// The roles of `report`'s lines read plainly as a `kind`, with its data
/// inputs: as many of its widest runs of levers as the kind takes operands
/// its operands, A as `order` prefers, and every other lever a control
/// line; its widest run of outputs its output, and every other a status
/// line.
fn plain_roles(
    kind: Kind,
    report: &inspect::Report,
    order: Order,
) -> (Roles, Vec<Pos>) {
    let widest = |runs: &[inspect::Run]| {
        let mut runs: Vec<inspect::Run> = runs.to_vec();
        runs.sort_by_key(|run| std::cmp::Reverse(run.bits.len()));
        runs
    };
    let inputs = widest(&report.inputs);
    let takes = matching::operands(kind).min(inputs.len());
    let (operands, controls) = inputs.split_at(takes);
    let mut roles = Roles::default();
    match operands {
        [a, b] => {
            let (a, b) = order.of(&a.bits, &b.bits);
            roles.mark(Role::Operand(0), a);
            roles.mark(Role::Operand(1), b);
        }
        _ => {
            for (n, run) in (0..).zip(operands) {
                roles.mark(Role::Operand(n), &run.bits);
            }
        }
    }
    for run in controls {
        roles.mark(Role::Control, &run.bits);
    }
    let outputs = widest(&report.outputs);
    if let Some((output, status)) = outputs.split_first() {
        roles.mark(Role::Output(0), &output.bits);
        for run in status {
            roles.mark(Role::Status, &run.bits);
        }
    }
    let data = operands.iter().flat_map(|run| run.bits.clone()).collect();
    (roles, data)
}

/// Of the `faults` probing found as each kind it tried, the one to show:
/// the one it is meant as, if any; else one whose test ran and got cases
/// wrong, as nearest to holding, over those that could not run. Of either
/// kind, the hex adder's or else the hex-to-binary converter's when it has
/// signal-strength inputs; the decoder's
/// when it has one input line; the register's when it
/// has a button, which only a register's writes and a sequential
/// multiplier's lines are; the ALU's when it has more than one lone lever,
/// its control lines; else the adder's.
fn fault_of<'f>(
    blocks: &Blocks,
    report: &inspect::Report,
    meant: Option<Kind>,
    faults: &'f [(Kind, Fault)],
) -> Option<&'f Fault> {
    let of = |kind: Kind| {
        faults
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, fault)| fault)
    };
    if let Some(kind) = meant {
        return of(kind);
    }
    let buttons = blocks.switches().iter().any(|switch| switch.button);
    let lone = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .count();
    let order: &[Kind] = match () {
        () if !report.digits.is_empty() => {
            &[Kind::HexAdder, Kind::HexToBinary, Kind::Adder]
        }
        () if report.inputs.len() == 1 => {
            &[Kind::Decoder, Kind::Adder, Kind::Register]
        }
        () if buttons => {
            &[Kind::Register, Kind::Multiplier, Kind::Adder, Kind::Alu]
        }
        () if lone > 1 => &[Kind::Alu, Kind::Adder, Kind::Register],
        () => &[Kind::Adder, Kind::Register, Kind::Alu],
    };
    let candidates: Vec<&Fault> = order.iter().filter_map(|&k| of(k)).collect();
    candidates
        .iter()
        .find(|fault| fault.ran())
        .or(candidates.first())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recognize::probe::tests::uncoloured;

    /// A multiplier of the low byte whose A's top bit is wired wrong, its
    /// calculate a wooden button: the register's try can't run, as it has a
    /// button, but the multiplier's runs, so its fault is the one shown,
    /// with the file carrying the roles the multiplier's try gave it.
    #[test]
    fn a_try_that_ran_is_shown_over_one_that_could_not() {
        let data =
            std::fs::read("tests/fixtures/mult-8-low-byte-top-bit-wrong.schem")
                .unwrap();
        let found = recognize(&data).unwrap();
        assert_eq!(found.found, None);
        assert_eq!(
            found.problems,
            ["29 of 121 cases wrong: A=128 B=0 → output=0b11111111, \
              expected 0b00000000"]
        );
        let width = crate::ingest::results::derived_width(
            "multiplier",
            true,
            &found.file,
        );
        assert_eq!(width.unwrap(), Some(8));
    }

    /// A 1-bit XNOR of wool, one input carried up through dust on its block,
    /// and a lone block holding up a repeater that powers a block: the dust
    /// is wiring, as on any input, and the block under the repeater no
    /// output, so its one output is read and it is found an XNOR.
    #[test]
    fn dust_on_an_input_and_a_repeater_powering_a_block_are_wiring() {
        let data = std::fs::read("tests/fixtures/xnor-1-wool.schem").unwrap();
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.contains("A xnor B")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A 1-bit XOR gate, its two levers side by side read as one run, is
    /// found as a gate, not as a 1-bit adder without a carry.
    #[test]
    fn an_xor_gate_is_found_as_a_gate() {
        // Raw, with blocks the simulator doesn't model until they are
        // standardized.
        let raw = std::fs::read("tests/fixtures/xor-1.schem").unwrap();
        let standard = crate::standard::standardize::standardize(&raw).unwrap();
        let found = recognize(&standard).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("an XOR gate")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A recognized build's signs are all taken out, the site naming its
    /// lines itself: one naming a role, as `carry in` does, and one saying
    /// more, as a carry out's strength.
    #[test]
    fn a_recognized_builds_signs_are_taken_out() {
        let found = recognize(&uncoloured("diag-3t-cca")).unwrap();
        assert!(found.found.is_some(), "{:?}", found.problems);
        let blocks = Blocks::read(&found.file).unwrap();
        assert!(blocks.signs.is_empty(), "{:?}", blocks.signs);
    }

    /// A sign marking a bus's least significant bit stays when the rest
    /// go, as an adder whose low bits sit at the top is read by it.
    #[test]
    fn a_sign_ordering_a_bus_stays() {
        let raw =
            std::fs::read("tests/fixtures/adder-8-lsb-top.schem").unwrap();
        let before = Blocks::read(&raw).unwrap().signs.len();
        let file = unlabelled(Kind::Adder, &raw).unwrap();
        let signs = Blocks::read(&file).unwrap().signs;
        assert!(!signs.is_empty() && signs.len() < before, "{signs:?}");
        for lines in signs.values() {
            assert!(lines.iter().any(|line| line.trim() == "lsb"), "{lines:?}");
        }
    }

    /// A trapdoor, hung on a build to show an output flipping, plays no
    /// part in it: the build is recognized with it, and preparing the file
    /// takes it out, recognized or not.
    #[test]
    fn a_trapdoor_is_taken_out() {
        let data = uncoloured("diag-3t-cca");
        let blocks = Blocks::read(&data).unwrap();
        let [x, y, z] = blocks.size;
        let spot = (0..x)
            .flat_map(|x| {
                (0..y).flat_map(move |y| (0..z).map(move |z| [x, y, z]))
            })
            .find(|&pos| blocks.get(pos) == schem::AIR)
            .expect("a build has some air");
        let changes = BTreeMap::from([(
            spot,
            "minecraft:iron_trapdoor[facing=east,half=top,open=false,powered=false,waterlogged=false]"
                .to_owned(),
        )]);
        let with = crate::schem::edit(&data, &changes).unwrap();
        let found = recognize(&with).unwrap();
        assert!(found.found.is_some(), "{:?}", found.problems);
        let kept = crate::ingest::prepare::prepare_for(
            &with,
            crate::ingest::prepare::Purpose::AS_BUILT,
        )
        .unwrap()
        .blocks;
        assert!(
            !kept.iter().any(|(_, state)| state.contains("trapdoor")),
            "its trapdoor is taken out"
        );
    }

    /// An ALU keeps the signs on its control lines, which name the
    /// operations each chooses, and loses the rest.
    #[test]
    fn an_alu_keeps_the_signs_on_its_control_lines() {
        let found = recognize(&uncoloured("alu-8-and-xor")).unwrap();
        assert!(found.found.is_some(), "{:?}", found.problems);
        let blocks = Blocks::read(&found.file).unwrap();
        assert!(!blocks.signs.is_empty(), "its operations stay named");
        let roles = blocks.roles.as_ref().unwrap();
        for &pos in blocks.signs.keys() {
            let role = blocks.sign_block(pos).and_then(|b| roles.get(b));
            assert!(
                matches!(role, Some(Role::Control | Role::Status)),
                "a sign off a control or status line stays at {pos:?}"
            );
        }
        // Its status lines keep their names, which its test finds them by.
        let interface = crate::interface::read(&blocks);
        let statuses: Vec<&str> = interface
            .outputs
            .iter()
            .filter(|port| port.role == Some(Role::Status))
            .map(|port| port.name.as_str())
            .collect();
        assert!(
            statuses.iter().all(|name| !name.starts_with("unnamed")),
            "{statuses:?}"
        );
    }

    /// The owner's 8-bit carry-cancel adder reworked to give every output
    /// out through a repeater is recognized as the adder it is with those
    /// outputs alone.
    #[test]
    #[ignore = "probes a whole build"]
    fn a_carry_cancel_adder_with_repeater_outputs_probes_as_an_adder() {
        let data =
            std::fs::read("tests/fixtures/cca-8-repeater-out.schem").unwrap();
        let recognition = recognize_as(&data, Some("adder")).unwrap();
        assert!(recognition.found.is_some(), "{:?}", recognition.problems);
    }
}
