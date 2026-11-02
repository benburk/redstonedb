//! Recognizing a build by probing: the file made simulatable, inspected,
//! and probed as each blessed category in turn until one holds, each
//! category's probe named by its kind, the signs naming only a role it
//! found taken out; and, when none holds, the fault to show and the file
//! as the closest try coloured it.

use std::collections::BTreeMap;

use super::{
    Fault, adder, alu, colour, decoder, multiplier, one_operand,
    operands_probe, register, sequential, shifter,
};
use crate::{
    category::{Kind, Probe, Probing},
    recognize::inspect,
    schem::{self, Blocks, Pos},
    simulate::{self, Test},
};

/// What the check by function makes of an uploaded file: the file with its
/// roles coloured in, when it is recognized as a category; when not, as
/// the try that came closest coloured it, when that try ran and got cases
/// wrong, so its test shows which; else as it came; what was found; and
/// what only its builder can put right.
pub struct Recognition {
    pub file: Vec<u8>,
    pub found: Option<String>,
    pub problems: Vec<String>,
}

/// The file in `data` with what the simulator does not model but redstone
/// treats as a plain block in its place: a slab, stairs, or glowstone as
/// white stained glass, which redstone sees through as it does them, and a
/// furnace, cauldron, or cake as a barrel giving the same strength. Every
/// colour stays, so no input or output is lost, as standardizing would
/// whiten an output it does not know.
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

/// The file in `data` inspected and, if nothing is wrong with its levers
/// and outputs, probed as an adder, then as a register, an ALU, and a
/// decoder.
pub fn recognize(data: &[u8]) -> anyhow::Result<Recognition> {
    recognize_as(data, None)
}

/// As `recognize`, probing only as the category named `meant`, when
/// given. Probed as each and recognized as none, its fault is as
/// `fault_of` chooses.
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
    let hex_adder = meant.and_then(Kind::named) == Some(Kind::HexAdder);
    // Each meant as another, as a subtractor is the adder it is, its B
    // subtracted, is probed as that one; one never probed, as a hex adder,
    // or a category off the list, is probed as nothing.
    let meant: Option<Option<Kind>> = meant.map(|meant| {
        let kind = Kind::named(meant)?;
        match kind.facts().probe {
            Probing::As(other) => Some(other),
            Probing::Tried { .. } => Some(kind),
            Probing::Not => None,
        }
    });
    let simulated = simulatable(data)?;
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
    let unrecognized = |problems| Recognition {
        file: data.to_vec(),
        found: None,
        problems,
    };
    // Inputs taking a hex digit per block through dust are a hex adder's,
    // which probing doesn't read, whatever else the check finds.
    if !hex_adder
        && !crate::recognize::interface::dust_inputs(&blocks).is_empty()
    {
        return Ok(unrecognized(vec![
            crate::details::WITHOUT_LEVERS.to_owned(),
        ]));
    }
    if !problems.is_empty() {
        return Ok(unrecognized(problems));
    }
    // What a probe found, unless a data input it found has a block behind
    // its lever, which only a control line's may.
    let accept = |data_inputs: &[Pos], file: Vec<u8>, found: String| {
        let faults = report.data_faults(data_inputs);
        if faults.is_empty() {
            Recognition {
                file,
                found: Some(found),
                problems: Vec::new(),
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
        match probe_by(kind, how, test, data, &blocks, &report)? {
            Ok((data_inputs, file, found)) => {
                return Ok(accept(&data_inputs, file, found));
            }
            Err(fault) => faults.push((kind, fault)),
        }
    }
    // Meant as a kind never probed, nothing was, so there is no fault.
    let fault = fault_of(&blocks, &report, meant.flatten(), &faults);
    problems.push(fault.map_or_else(String::new, ToString::to_string));
    // The file as the try whose fault is shown coloured it, when it ran.
    let closest = fault.and_then(Fault::file).unwrap_or(data);
    Ok(Recognition {
        file: closest.to_vec(),
        ..unrecognized(problems)
    })
}

/// What a probe found: the build's data inputs, its file coloured by the
/// roles found, and what it is, in words.
pub type Found = (Vec<Pos>, Vec<u8>, String);

/// The file in `data`, with `blocks` and `report`, probed as `kind`, or as
/// the kind it is probed as, such as an adder for a subtractor: its data
/// inputs, the file coloured by the roles found, and what it was found to
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
    probe_by(kind, how, test, data, blocks, report)
}

/// As [`probe_as`], `kind` probed `how` its row says, by its `test`.
fn probe_by(
    kind: Kind,
    how: Probe,
    test: Test,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> anyhow::Result<Result<Found, Fault>> {
    let found = |run: &simulate::Run| {
        // Probing reads no hex digits, so an adder is one or a subtractor.
        let kind = if kind == Kind::Adder {
            run.adder_category(false)
        } else {
            kind
        };
        format!(
            "{}: all {} cases right, {}",
            kind.a(),
            run.cases,
            run.pattern
        )
    };
    let probed = probed_as(how, test, data, blocks, report, found)?;
    Ok(match probed {
        Ok((inputs, file, found)) => {
            Ok((inputs, unlabelled(kind, &file)?, found))
        }
        Err(why) => Err(why),
    })
}

/// The file in `data`, found to be a `kind`, less each sign that only
/// names a role probing found, such as `carry in`; a sign saying anything
/// more, such as a strength, stays.
fn unlabelled(kind: Kind, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let words = kind.facts().role_words;
    let blocks = Blocks::read(data)?;
    let changes: BTreeMap<Pos, String> = blocks
        .signs
        .iter()
        .filter(|(_, lines)| {
            let said: Vec<String> = lines
                .iter()
                .map(|line| line.trim().to_lowercase())
                .filter(|line| !line.is_empty())
                .collect();
            !said.is_empty()
                && said.iter().all(|line| words.contains(&line.as_str()))
        })
        .map(|(&pos, _)| (pos, schem::AIR.to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data.to_vec());
    }
    crate::schem::standardize::edit(data, &changes)
}

/// As `probe_as`, the file as probing coloured it, its signs as they were.
fn probed_as(
    how: Probe,
    test: Test,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    found: impl Fn(&simulate::Run) -> String,
) -> anyhow::Result<Result<Found, Fault>> {
    Ok(match how {
        Probe::Adder => match adder(blocks, report) {
            Ok(probed) => Ok((
                [probed.layout.a.clone(), probed.layout.b.clone()].concat(),
                colour(data, &probed.layout)?,
                found(&probed.run),
            )),
            Err(why) => Err(why),
        },
        Probe::Register => register(data, report, test).map(|probed| {
            let found = found(&probed.run);
            (probed.data_in, probed.file, found)
        }),
        Probe::Shifter => shifter(data, report).map(|probed| {
            let found = found(&probed.run);
            (probed.a, probed.file, found)
        }),
        Probe::Alu => alu(data, report).map(|probed| {
            let found = found(&probed.run);
            ([probed.a, probed.b].concat(), probed.file, found)
        }),
        Probe::Decoder => decoder(data, report).map(|probed| {
            let found = found(&probed.run);
            (probed.a, probed.file, found)
        }),
        Probe::Multiplier => multiplier(data, report).map(|probed| {
            let found = found(&probed.run);
            ([probed.a, probed.b, probed.c].concat(), probed.file, found)
        }),
        Probe::Operands => operands_probe(data, report, test).map(|probed| {
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
/// kind, the decoder's when it has one input line; the register's when it
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
    /// calculate a wooden button: the register's try can't run, as it has
    /// a button, but the multiplier's runs, so its fault is the one shown,
    /// with the file coloured as the multiplier's try had it.
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

    /// A sign that only names a role probing finds, such as `carry in`, is
    /// taken out; one that says more, such as a carry out's strength,
    /// stays.
    #[test]
    fn signs_naming_only_a_role_are_taken_out() {
        let found = recognize(&uncoloured("diag-3t-cca")).unwrap();
        assert!(found.found.is_some(), "{:?}", found.problems);
        let blocks = Blocks::read(&found.file).unwrap();
        let mut said: Vec<String> = blocks
            .signs
            .values()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        said.sort();
        assert_eq!(said, ["carry out 1ss"]);
    }
}
