//! Recognizing a build by probing: the file made simulatable, inspected,
//! and probed as each blessed category in turn until one holds, each
//! category's probe named by its kind, the signs naming only a role it
//! found taken out; and, when none holds, the fault to show and the file
//! as the closest try coloured it.

use std::{cell::RefCell, collections::BTreeMap};

use super::{
    Line, adder, alu, colour, decoder, multiplier, one_operand, operands_probe,
    register, sequential, shifter,
};
use crate::{
    recognize::{category, inspect},
    schem::{self, Blocks, Pos},
    simulate::{self, Gate, Test},
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

thread_local! {
    /// What the last try that ran but got cases wrong said, with the file
    /// coloured as it had it; see [`doesnt_hold`].
    static MISSED: RefCell<Option<(String, Vec<u8>)>> = const { RefCell::new(None) };
}

/// What a try whose roles are coloured into `file` says when its `run`
/// got cases wrong, kept with `file`, so a build no try holds can show
/// what the closest got wrong.
pub(super) fn doesnt_hold(run: &simulate::Run, file: &[u8]) -> String {
    let why =
        format!("Doesn't hold: {} of {} cases wrong", run.wrong, run.cases);
    MISSED.with(|missed| {
        *missed.borrow_mut() = Some((why.clone(), file.to_vec()));
    });
    why
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
        .filter(|&(_, state)| {
            let name = schem::short(state);
            name.ends_with("_slab")
                || name.ends_with("_stairs")
                || name == "glowstone"
        })
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

/// As `recognize`, probing only as `meant`, one of [`KINDS`], when given.
/// Probed as each and recognized as none, its fault is as `fault_of`
/// chooses. Its outputs are an upload's, see [`inspect::UPLOAD`].
pub fn recognize_as(
    data: &[u8],
    meant: Option<&str>,
) -> anyhow::Result<Recognition> {
    recognize_watched(data, meant, inspect::UPLOAD, &mut |_, _| {})
}

/// As [`recognize_as`], its outputs marked as `marks` says, telling
/// `trying` each kind as it is probed, with how many of [`KINDS`] came
/// before it.
pub fn recognize_watched(
    data: &[u8],
    meant: Option<&str>,
    marks: inspect::Marks,
    trying: &mut dyn FnMut(&'static str, usize),
) -> anyhow::Result<Recognition> {
    // A subtractor is probed as the adder it is, its B subtracted.
    let meant = meant.map(|m| if m == "subtractor" { "adder" } else { m });
    let simulated = simulatable(data)?;
    let data = simulated.as_slice();
    let blocks = Blocks::read(data)?;
    let report = inspect::inspect_marked(&blocks, marks);
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
    if meant != Some("hex_adder")
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
    // Each blessed category in turn, or only the one meant, its fault kept
    // when it isn't one.
    let mut faults: Vec<Option<String>> = vec![None; KINDS.len()];
    // The file each category's closest try coloured, when it said so.
    let mut missed: Vec<Option<Vec<u8>>> = vec![None; KINDS.len()];
    for (i, kind) in KINDS.into_iter().enumerate() {
        if meant.is_some_and(|meant| meant != kind) {
            continue;
        }
        trying(kind, i);
        MISSED.with(|missed| missed.borrow_mut().take());
        match probe_as(kind, data, &blocks, &report)? {
            Ok((data_inputs, file, found)) => {
                return Ok(accept(&data_inputs, file, found));
            }
            Err(why) => {
                let last = MISSED.with(|missed| missed.borrow_mut().take());
                missed[i] =
                    last.filter(|(said, _)| *said == why).map(|(_, file)| file);
                faults[i] = Some(why);
            }
        }
    }
    let fault = fault_of(&blocks, &report, meant, &faults);
    let closest = faults
        .iter()
        .zip(missed)
        .find_map(|(why, file)| file.filter(|_| why.as_ref() == Some(&fault)));
    problems.push(fault);
    Ok(Recognition {
        file: closest.unwrap_or_else(|| data.to_vec()),
        ..unrecognized(problems)
    })
}

/// The blessed categories, in the order probing tries them: gates first,
/// as a 1-bit XOR is a sum without a carry, an inverter's bit, and a 1-bit
/// AND a product of one bit each, a 1-bit XNOR an equality, and a 1-bit
/// NAND+XNOR an adder's inverted, which no wider build of those is; an
/// adder before an incrementer, as a 1-bit adder is a 1-bit incrementer.
pub const KINDS: [&str; 31] = [
    "xor_gate",
    "xnor_gate",
    "and_gate",
    "nand_gate",
    "or_gate",
    "nor_gate",
    "nand_xnor_gate",
    "equality",
    "greater_than",
    "carry_out",
    "inverter",
    "absolute_value",
    "monostable",
    "dual_edge_monostable",
    "adder",
    "incrementer",
    "decrementer",
    "inc_dec",
    "popcount",
    "binary_to_hex",
    "threshold",
    "parity",
    "register",
    "buffer",
    "alu",
    "decoder",
    "barrel_shifter",
    "multiplier",
    "counter",
    "shift_register",
    "t_flip_flop",
];

/// What a probe found: the build's data inputs, its file coloured by the
/// roles found, and what it is, in words.
pub type Found = (Vec<Pos>, Vec<u8>, String);

/// The file in `data`, with `blocks` and `report`, probed as a `kind` of
/// [`KINDS`]: its data inputs, the file coloured by the roles found, and
/// what it was found to be, such as `an adder: all 131072 cases right, A +
/// B + carry in`; or why it isn't one.
pub fn probe_as(
    kind: &str,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> anyhow::Result<Result<Found, String>> {
    let found = |run: &simulate::Run| {
        // An adder whose B is subtracted is a subtractor.
        let kind = if kind == "adder"
            && simulate::AdderVariant::of(&run.pattern).subtract
        {
            "subtractor"
        } else {
            kind
        };
        format!(
            "{}: all {} cases right, {}",
            named(kind),
            run.cases,
            run.pattern
        )
    };
    let probed = probed_as(kind, data, blocks, report, found)?;
    Ok(match probed {
        Ok((inputs, file, found)) => {
            Ok((inputs, unlabelled(kind, &file)?, found))
        }
        Err(why) => Err(why),
    })
}

/// A build of `kind`, one of [`KINDS`], in words, such as `an adder`.
fn named(kind: &str) -> String {
    let name = match kind {
        "alu" => "ALU",
        "inc_dec" => "inc/dec",
        "xor_gate" => "XOR gate",
        "xnor_gate" => "XNOR gate",
        "and_gate" => "AND gate",
        "nand_gate" => "NAND gate",
        "or_gate" => "OR gate",
        "nor_gate" => "NOR gate",
        "nand_xnor_gate" => "NAND+XNOR gate",
        "equality" => "equality comparator",
        "greater_than" => "greater-than comparator",
        "dual_edge_monostable" => "dual-edge monostable",
        "t_flip_flop" => "T flip-flop",
        "parity" => "parity circuit",
        _ => &kind.replace('_', " "),
    };
    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'O', 'X'])
    {
        "an"
    } else {
        "a"
    };
    format!("{article} {name}")
}

/// The words a sign may name a role by that probing finds anyway, for a
/// `kind` of [`KINDS`]; an ALU's, a decoder's, and a shift register's
/// signs are kept, as they tell what their lines do.
fn role_words(kind: &str) -> &'static [&'static str] {
    match kind {
        "adder" => &[
            "carry in",
            "carry-in",
            "cin",
            "c in",
            "carry out",
            "carry-out",
            "cout",
            "c out",
            "borrow in",
            "borrow out",
            "bin",
            "bout",
            "subtract",
        ],
        "register" => &["write", "read", "write enable", "data in", "data out"],
        "buffer" => &["write", "write enable", "data in", "data out"],
        "multiplier" => &["calculate", "calc", "reset"],
        "counter" => &["clock", "clk", "write", "write enable", "load"],
        "t_flip_flop" => &["toggle", "t", "clock", "clk", "q"],
        "incrementer" => &["increment", "inc", "carry out", "cout"],
        "decrementer" => &["decrement", "dec", "borrow out", "bout"],
        "inverter" => &["invert", "inv"],
        "carry_out" => &["carry in", "cin", "carry out", "cout"],
        _ => &[],
    }
}

/// The file in `data`, found to be a `kind` of [`KINDS`], less each sign
/// that only names a role probing found, such as `carry in`; a sign saying
/// anything more, such as a strength, stays.
fn unlabelled(kind: &str, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let words = role_words(kind);
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
    kind: &str,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    found: impl Fn(&simulate::Run) -> String,
) -> anyhow::Result<Result<Found, String>> {
    Ok(match kind {
        "adder" => match adder(blocks, report) {
            Ok(probed) => Ok((
                [probed.layout.a.clone(), probed.layout.b.clone()].concat(),
                colour(data, &probed.layout)?,
                found(&probed.run),
            )),
            Err(why) => Err(why),
        },
        "register" | "buffer" => {
            let test = if kind == "register" {
                Test::Register
            } else {
                Test::Buffer
            };
            register(data, report, test).map(|probed| {
                let found = found(&probed.run);
                (probed.data_in, probed.file, found)
            })
        }
        "barrel_shifter" => shifter(data, report).map(|probed| {
            let found = found(&probed.run);
            (probed.a, probed.file, found)
        }),
        "alu" => alu(data, report).map(|probed| {
            let found = found(&probed.run);
            ([probed.a, probed.b].concat(), probed.file, found)
        }),
        "decoder" => decoder(data, report).map(|probed| {
            let found = found(&probed.run);
            (probed.a, probed.file, found)
        }),
        "multiplier" => multiplier(data, report).map(|probed| {
            let found = found(&probed.run);
            ([probed.a, probed.b, probed.c].concat(), probed.file, found)
        }),
        "xor_gate" | "xnor_gate" | "and_gate" | "nand_gate" | "or_gate"
        | "nor_gate" => {
            let gate = category::behaviour(kind)
                .test
                .and_then(Gate::of)
                .expect("a gate's category has a gate's test");
            operands_probe(data, report, gate.test()).map(|probed| {
                let found = found(&probed.run);
                (probed.inputs, probed.file, found)
            })
        }
        "equality" | "greater_than" | "carry_out" | "nand_xnor_gate" => {
            let test = category::behaviour(kind)
                .test
                .expect("a comparison's category has a test");
            operands_probe(data, report, test).map(|probed| {
                let found = found(&probed.run);
                (probed.inputs, probed.file, found)
            })
        }
        "inverter"
        | "absolute_value"
        | "incrementer"
        | "decrementer"
        | "inc_dec"
        | "monostable"
        | "dual_edge_monostable"
        | "popcount"
        | "binary_to_hex"
        | "threshold"
        | "parity" => {
            let test = category::behaviour(kind)
                .test
                .expect("a blessed category has a test");
            let line = match kind {
                "inverter" => Line::Always,
                "incrementer" | "decrementer" | "inc_dec" => Line::Maybe,
                _ => Line::Never,
            };
            one_operand(data, report, test, line).map(|probed| {
                let found = found(&probed.run);
                (probed.a, probed.file, found)
            })
        }
        "counter" | "shift_register" | "t_flip_flop" => {
            let test = category::behaviour(kind)
                .test
                .expect("a blessed category has a test");
            sequential(data, report, test).map(|probed| {
                let found = found(&probed.run);
                (probed.data, probed.file, found)
            })
        }
        _ => unreachable!("{kind} is not one of KINDS"),
    })
}

/// Of the faults probing found as each of [`KINDS`], the one to show: the
/// one it is meant as, if any; else one whose test ran and got cases
/// wrong, as nearest to holding, over those that could not run. Of either
/// kind, the decoder's when it has one input line; the register's when it
/// has a button, which only a register's writes and a sequential
/// multiplier's lines are; the ALU's when it has more than one lone lever,
/// its control lines; else the adder's.
fn fault_of(
    blocks: &Blocks,
    report: &inspect::Report,
    meant: Option<&str>,
    faults: &[Option<String>],
) -> String {
    let of = |kind: &str| {
        KINDS
            .iter()
            .position(|&k| k == kind)
            .and_then(|i| faults[i].clone())
    };
    if let Some(kind) = meant {
        return of(kind).unwrap_or_default();
    }
    let buttons = blocks.switches().iter().any(|switch| switch.button);
    let lone = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .count();
    let order: &[&str] = match () {
        () if report.inputs.len() == 1 => &["decoder", "adder", "register"],
        () if buttons => &["register", "multiplier", "adder", "alu"],
        () if lone > 1 => &["alu", "adder", "register"],
        () => &["adder", "register", "alu"],
    };
    let candidates: Vec<String> = order.iter().filter_map(|k| of(k)).collect();
    let ran = |why: &&String| why.starts_with("Doesn't hold");
    candidates
        .iter()
        .find(ran)
        .or(candidates.first())
        .cloned()
        .unwrap_or_default()
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
        assert_eq!(found.problems, ["Doesn't hold: 29 of 121 cases wrong"]);
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
