//! Recognizing a build by probing: the file made simulatable, inspected,
//! and probed as each blessed category in turn until one holds, each
//! category's probe named by its kind, its signs taken out but those its
//! test reads; and, when none holds, the fault to show and the file
//! carrying the closest try's roles.

use std::collections::BTreeMap;

use super::{
    Fault, Order, Roles, adder_from, alu, decoder, hex_adder, hex_to_binary,
    multiplier, one_operand, operands_probe, register, sequential, shifter,
};
use crate::{
    category::{Kind, Probe, Probing},
    recognize::{inspect, matching},
    schem::{self, Blocks, Pos, lines, lines::Role, orient},
    simulate::{self, Test},
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
    let data = crate::schem::standardize::barrels(data, &[])?;
    let blocks = Blocks::read(&data)?;
    let changes: BTreeMap<Pos, String> = blocks
        .iter()
        .filter(|&(_, state)| schem::block::like_glass(schem::short(state)))
        .map(|(pos, _)| (pos, "minecraft:white_stained_glass".to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data);
    }
    crate::schem::standardize::edit(&data, &changes)
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
    // Each meant as another, as a subtractor is the adder it is, its B
    // subtracted, is probed as that one; one never probed, a category off
    // the list, is probed as nothing.
    let meant: Option<Option<Kind>> = meant.map(|meant| {
        let kind = Kind::named(meant)?;
        match kind.facts().probe {
            Probing::As(other) => Some(other),
            Probing::Tried { .. } => Some(kind),
            Probing::Not => None,
        }
    });
    // An upload's roles are what probing finds, never what it came with.
    let simulated = simulatable(&lines::without(data)?)?;
    let data = simulated.as_slice();
    let blocks = Blocks::read(data)?;
    let report = inspect::inspect(&blocks);
    let mut problems: Vec<String> =
        report.faults().map(|p| p.message.clone()).collect();
    if problems.is_empty() {
        problems.extend(inspect::not_bus_drivable(data)?.into_iter().map(
            |lever| {
                format!("Lever at {}: not bus-drivable", inspect::place(lever))
            },
        ));
    }
    let lines = report.summary();
    let unrecognized = |problems| Recognition {
        file: data.to_vec(),
        found: None,
        problems,
        lines: lines.clone(),
    };
    if !problems.is_empty() {
        return Ok(unrecognized(problems));
    }
    // What a probe found, unless a data input it found has a block behind
    // its lever, which only a control line's may.
    let accept = |kind: Kind, data_inputs: &[Pos], file: Vec<u8>, found| {
        let file = carry_on_top(kind, &file).unwrap_or(file);
        let faults = report.data_faults(data_inputs);
        if faults.is_empty() {
            Recognition {
                file,
                found: Some(found),
                problems: Vec::new(),
                lines: lines.clone(),
            }
        } else {
            unrecognized(faults)
        }
    };
    // Each kind probing tries in turn, or only the one meant, its fault
    // kept when it isn't one.
    let probed = Kind::probed();
    let mut faults: Vec<(Kind, Fault)> = Vec::new();
    for (i, &(kind, how, test)) in probed.iter().enumerate() {
        if meant.is_some_and(|meant| meant != Some(kind)) {
            continue;
        }
        trying(kind, i);
        let order = Order::default();
        match probe_by(kind, how, test, data, &blocks, &report, false, order)? {
            Ok((_, file, _)) if leaves_digits(&report, &file) => {
                faults.push((kind, Fault::Unfit(LEAVES_DIGITS.to_owned())));
            }
            Ok((data_inputs, file, found)) => {
                return Ok(accept(kind, &data_inputs, file, found));
            }
            Err(fault) => faults.push((kind, fault)),
        }
    }
    // Meant as a kind never probed, nothing was, so there is no fault.
    let fault = fault_of(&blocks, &report, meant.flatten(), &faults);
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
    // Meant as a kind never probed, it is no adder either.
    if reorient
        && meant != Some(None)
        && let Some((kind, (data_inputs, file, found))) =
            loose_adder(meant.flatten(), data, &blocks, &report)?
    {
        return Ok(accept(kind, &data_inputs, file, found));
    }
    // Lines that bend the conventions, as top bits out of line or outputs
    // unevenly spaced, may still read as the kind meant: matching finds
    // them, the build's I/O unaligned.
    // Meant as a kind never probed, it is none matching finds either.
    if reorient
        && meant != Some(None)
        && let Some((found, file)) =
            unaligned(data, &blocks, &report, meant.flatten())?
    {
        let kind = found.kind;
        return Ok(accept(kind, &found.data_inputs(), file, found.found()));
    }
    problems.push(fault.map_or_else(String::new, ToString::to_string));
    // The file with the roles of the try whose fault is shown, when it ran.
    let closest = fault.and_then(Fault::file).unwrap_or(data);
    Ok(Recognition {
        file: closest.to_vec(),
        ..unrecognized(problems)
    })
}

/// The build in `data`, with `blocks` and `report`, as an adder whose
/// lines bend the conventions, as operands split in two or outputs
/// unevenly spaced, laid out loosely, every case right: its I/O unaligned.
/// Only as the kind `meant`, when one is; `None` when no loose layout
/// adds.
fn loose_adder(
    meant: Option<Kind>,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> anyhow::Result<Option<(Kind, Found)>> {
    for &(kind, how, test) in &Kind::probed() {
        if how != Probe::Adder || meant.is_some_and(|meant| meant != kind) {
            continue;
        }
        let order = Order::default();
        if let Ok(found) =
            probe_by(kind, how, test, data, blocks, report, true, order)?
        {
            return Ok(Some((kind, found)));
        }
    }
    Ok(None)
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
    let found = crate::recognize::interface::read(&blocks);
    let role = |port: &&crate::recognize::interface::Port, role| {
        port.role == Some(role)
    };
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

/// The build in `data`, with `blocks` and `report`, as matching finds it,
/// as the kind `meant`, if one is, or else any kind, its lines read as the
/// conventions don't lay them, every case right: what it found, and the
/// file carrying the roles it found, its signs taken out as probing's are.
/// `None` when it finds no such reading, or one that keeps the
/// conventions, which probing would have found.
fn unaligned(
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    meant: Option<Kind>,
) -> anyhow::Result<Option<(matching::Match, Vec<u8>)>> {
    let probed_as = |kind: Kind| match kind.facts().probe {
        Probing::As(other) => other,
        _ => kind,
    };
    let Some(found) = matching::closest(blocks, report) else {
        return Ok(None);
    };
    let fits = meant.is_none_or(|meant| meant == probed_as(found.kind));
    if found.wrong > 0 || found.conventional || !fits {
        return Ok(None);
    }
    let file = lines::with(data, Some(&found.roles()))?;
    let file = unlabelled(found.kind, &file)?;
    Ok(Some((found, file)))
}

/// What a probe found: the build's data inputs, its file carrying the
/// roles found, and what it is, in words.
pub type Found = (Vec<Pos>, Vec<u8>, String);

/// The file in `data`, with `blocks` and `report`, probed as `kind`, or as
/// the kind it is probed as, such as an adder for a subtractor: its data
/// inputs, the file carrying the roles found, and what it was found to
/// be, such as `an adder: all 131072 cases right, A + B + carry in`; or why
/// it isn't one.
pub fn probe_as(
    kind: Kind,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> anyhow::Result<Result<Found, Fault>> {
    let kind = match kind.facts().probe {
        Probing::As(other) => other,
        _ => kind,
    };
    let probed = Kind::probed();
    let Some(&(kind, how, test)) = probed.iter().find(|(k, ..)| *k == kind)
    else {
        return Ok(Err(Fault::Unfit(format!("{} isn't probed", kind.a()))));
    };
    probe_by(
        kind,
        how,
        test,
        data,
        blocks,
        report,
        false,
        Order::default(),
    )
}

/// The roles of the lines of the build in `data`, kept as built and meant
/// as a `kind`, as probing as that kind finds them: its inputs and outputs
/// read whatever faults the conventions find in them, and its lines laid
/// out loosely, as [`super::adder_from`] lays an adder's loosely, as only its
/// test says which is which; of a try that got cases wrong, the closest.
/// Of two operands its test passes either way round, A is as `order`
/// prefers. `None` when no try's test ran, as for a kind never probed.
pub fn kept_roles(
    data: &[u8],
    kind: Kind,
    order: Order,
) -> anyhow::Result<Option<Roles>> {
    let kind = match kind.facts().probe {
        Probing::As(other) => other,
        Probing::Tried { .. } => kind,
        Probing::Not => return Ok(None),
    };
    let probed = Kind::probed();
    let Some(&(kind, how, test)) = probed.iter().find(|(k, ..)| *k == kind)
    else {
        return Ok(None);
    };
    let simulated = simulatable(&lines::without(data)?)?;
    let blocks = Blocks::read(&simulated)?;
    let report = inspect::inspect(&blocks);
    let probed =
        probe_by(kind, how, test, &simulated, &blocks, &report, true, order)?;
    let file = match probed {
        Ok((_, file, _)) => file,
        Err(fault) => match fault.file() {
            Some(file) => file.to_vec(),
            None => return Ok(None),
        },
    };
    lines::of(&file)
}

/// As [`probe_as`], `kind` probed `how` its row says, by its `test`, its
/// lines laid out loosely with `loose`, its operands in `order`; see
/// [`kept_roles`].
#[allow(clippy::too_many_arguments)]
fn probe_by(
    kind: Kind,
    how: Probe,
    test: Test,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    loose: bool,
    order: Order,
) -> anyhow::Result<Result<Found, Fault>> {
    let found = |run: &simulate::Run| {
        // An adder is one or a subtractor, and a hex adder adds or
        // subtracts digits.
        let kind = match kind {
            Kind::Adder => run.adder_category(false),
            Kind::HexAdder => run.adder_category(true),
            _ => kind,
        };
        format!(
            "{}: all {} cases right, {}",
            kind.a(),
            run.cases,
            run.pattern
        )
    };
    let probed =
        probed_as(how, test, data, blocks, report, found, loose, order)?;
    Ok(match probed {
        Ok((inputs, file, found)) => {
            Ok((inputs, unlabelled(kind, &file)?, found))
        }
        Err(why) => Err(why),
    })
}

/// The file in `data`, carrying the roles probing found, found to be a
/// `kind`, less its signs: the site names its lines itself. A sign on a
/// control or status line stays where its kind's test reads what it says,
/// as an ALU's names the operations a line chooses and which status line
/// is its carry out, overflow, shift out, or zero, a barrel shifter's the
/// amount a line shifts by, and a register's or buffer's whether a line
/// reads or writes.
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
    let changes: BTreeMap<Pos, String> = blocks
        .signs
        .keys()
        .filter(|&&pos| !(reads_signs && on_line(pos)))
        .map(|&pos| (pos, schem::AIR.to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data.to_vec());
    }
    crate::schem::standardize::edit(data, &changes)
}

/// As `probe_as`, the file carrying the roles probing found, its signs as
/// they were; laid out loosely with `loose`, as [`kept_roles`] reads a
/// build kept as built; its operands in `order`.
#[allow(clippy::too_many_arguments)]
fn probed_as(
    how: Probe,
    test: Test,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    found: impl Fn(&simulate::Run) -> String,
    loose: bool,
    order: Order,
) -> anyhow::Result<Result<Found, Fault>> {
    Ok(match how {
        Probe::Adder => match adder_from(data, blocks, report, loose, order) {
            Ok(probed) => Ok((
                [probed.layout.a.clone(), probed.layout.b.clone()].concat(),
                super::with_layout(data, &probed.layout)?,
                found(&probed.run),
            )),
            Err(why) => Err(why),
        },
        Probe::HexAdder => {
            hex_adder(data, blocks, report, test, order).map(|probed| {
                let found = found(&probed.run);
                (probed.digits, probed.file, found)
            })
        }
        Probe::HexToBinary => {
            hex_to_binary(data, blocks, report, test).map(|probed| {
                let found = found(&probed.run);
                (probed.digits, probed.file, found)
            })
        }
        Probe::Register => register(data, report, test).map(|probed| {
            let found = found(&probed.run);
            (probed.data_in, probed.file, found)
        }),
        Probe::Shifter => shifter(data, report).map(|probed| {
            let found = found(&probed.run);
            (probed.a, probed.file, found)
        }),
        Probe::Alu => alu(data, report, order).map(|probed| {
            let found = found(&probed.run);
            ([probed.a, probed.b].concat(), probed.file, found)
        }),
        Probe::Decoder => decoder(data, report).map(|probed| {
            let found = found(&probed.run);
            (probed.a, probed.file, found)
        }),
        Probe::Multiplier => multiplier(data, report, order).map(|probed| {
            let found = found(&probed.run);
            ([probed.a, probed.b, probed.c].concat(), probed.file, found)
        }),
        Probe::Operands => operands_probe(data, report, test, loose, order)
            .map(|probed| {
                let found = found(&probed.run);
                (probed.inputs, probed.file, found)
            }),
        Probe::OneOperand(line) => {
            one_operand(data, report, test, line).map(|probed| {
                let found = found(&probed.run);
                (probed.a, probed.file, found)
            })
        }
        Probe::Sequential => sequential(data, report, test).map(|probed| {
            let found = found(&probed.run);
            (probed.data, probed.file, found)
        }),
    })
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
        let width = crate::models::simulation::derived_width(
            "multiplier",
            true,
            &found.file,
        );
        assert_eq!(width.unwrap(), Some(8));
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
        let interface = crate::recognize::interface::read(&blocks);
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
}
