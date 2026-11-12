//! What a build's levers and outputs are, found by how it behaves rather than
//! by colour. A stateless build driven by levers is read by matching, see
//! [`crate::recognize::matching`]: driven once, its lines read as each
//! kind's, and the kind's own test the judge of the reading. The kinds
//! matching does not read yet, those held in time, as a register's cells
//! or a counter, and those taking a digit a block, each have a probe
//! here, which lays their lines out and runs their test. The roles found
//! go with the file, see [`crate::schem::lines`], as the site's tests read
//! them; no block changes colour.
use crate::interface::problem::{Problem, Rule};
use std::fmt;

use crate::{
    category::Test,
    interface::inspect::Run,
    schem::{Blocks, Pos, lines},
    simulate,
};

pub(crate) use crate::schem::lines::{Role, Roles};

mod alu;
mod hex_adder;
mod hex_to_binary;
mod multiplier;
mod one_operand;
mod recognize;
mod register;
mod sequential;
mod shifter;

pub use alu::{AluProbed, alu};
pub use hex_adder::{HexAdderProbed, hex_adder};
pub use hex_to_binary::{HexToBinaryProbed, hex_to_binary};
pub use multiplier::{MultiplierProbed, multiplier};
pub use one_operand::{OneInputProbed, one_operand};
pub use recognize::{
    Found, Recognition, kept_roles, recognize, recognize_as, recognize_watched,
    reprobe, simulatable,
};
pub use register::{RegisterProbed, register};
pub use sequential::{SequentialProbed, sequential};
pub use shifter::shifter;

/// Why a probe found a build isn't the kind it tried. What it says is one
/// of the build's problems, word for word, as the site shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// Its lines don't fit the kind's roles, or its file couldn't be read
    /// or given its roles, in these words.
    Unfit(String),
    /// As `Unfit`, breaking a rule with a fix of its own.
    Breaks(Rule, String),
    /// The kind's test couldn't run on the roles tried, for this reason,
    /// which breaks this rule, when it names one.
    NotRun(String, Option<Rule>),
    /// The kind's test ran on the roles `file` carries, getting `wrong` of
    /// its `cases` wrong: as near as a build comes to holding. `said` is
    /// that, with the first wrong case as evidence; see `Run::wrong_cases`.
    Wrong {
        wrong: u64,
        cases: u64,
        said: String,
        file: Vec<u8>,
    },
}

impl Fault {
    /// The fault of `run`, which got cases wrong on the roles `file`
    /// carries.
    fn wrong(run: &simulate::Run, file: &[u8]) -> Fault {
        Fault::Wrong {
            wrong: run.wrong,
            cases: run.cases,
            said: run.wrong_cases(),
            file: file.to_vec(),
        }
    }

    /// Of this, one try's fault, and `later`, the next try's, the one
    /// nearest holding: one whose test ran over one whose test couldn't,
    /// so the closest real try is the one shown; of two that ran, the one
    /// with the smaller share of its cases wrong, the earlier on a tie;
    /// else the later.
    #[must_use]
    pub fn nearer(self, later: Fault) -> Fault {
        match (&self, &later) {
            (
                Fault::Wrong { wrong, cases, .. },
                Fault::Wrong {
                    wrong: later_wrong,
                    cases: later_cases,
                    ..
                },
            ) => {
                let share =
                    |wrong: u64, of: u64| u128::from(wrong) * u128::from(of);
                if share(*later_wrong, *cases) < share(*wrong, *later_cases) {
                    later
                } else {
                    self
                }
            }
            (Fault::Wrong { .. }, _) => self,
            _ => later,
        }
    }

    /// Whether the kind's test ran, getting cases wrong.
    pub fn ran(&self) -> bool {
        matches!(self, Fault::Wrong { .. })
    }

    /// The file carrying the roles of the try that got cases wrong, so its
    /// test shows which.
    pub fn file(&self) -> Option<&[u8]> {
        match self {
            Fault::Wrong { file, .. } => Some(file),
            Fault::Unfit(_) | Fault::Breaks(..) | Fault::NotRun(..) => None,
        }
    }
}

impl Fault {
    /// The fault of a test that couldn't run, for `err`.
    pub(crate) fn not_run(err: &anyhow::Error) -> Fault {
        Fault::NotRun(err.to_string(), simulate::rule_of(err))
    }

    /// It as one of the build's problems, as the site shows them.
    pub fn problem(&self) -> Problem {
        match self {
            Fault::Unfit(why) => Problem::said(Rule::Lines, why),
            Fault::Breaks(rule, why) => Problem::said(*rule, why),
            Fault::NotRun(reason, rule) => Problem::said(
                rule.unwrap_or(Rule::Lines),
                format!("Test couldn't run: {reason}"),
            ),
            Fault::Wrong { said, .. } => Problem::said(Rule::WrongCases, said),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.problem().message)
    }
}

/// `roles` given to `data` and run through `test`, after a few telling
/// cases when `quick_first`, so a try far from holding is not run in full:
/// the file and its run when every case comes out right, else why not. A
/// file that can't be read fails the outer result, ending the probe.
fn try_roles(
    data: &[u8],
    roles: &Roles,
    test: Test,
    quick_first: bool,
) -> Result<Result<(Vec<u8>, simulate::Run), Fault>, Fault> {
    let unfit = |err: anyhow::Error| Fault::Unfit(err.to_string());
    let file = given(data, roles).map_err(unfit)?;
    let coloured = Blocks::read(&file).map_err(unfit)?;
    if quick_first {
        match test.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => return Ok(Err(Fault::wrong(&run, &file))),
            Err(err) => return Ok(Err(Fault::not_run(&err))),
        }
    }
    Ok(match test.check(&coloured, false) {
        Ok(run) if run.wrong == 0 => Ok((file, run)),
        Ok(run) => Err(Fault::wrong(&run, &file)),
        Err(err) => Err(Fault::not_run(&err)),
    })
}

/// Which of two operands probing finds interchangeable, as an adder's or a
/// gate's, whose test passes either way round, is A: the one the build's
/// own marking names A, `marked`, as
/// [`crate::recognize::legacy::operand_marks`] reads a stored build's old
/// colours while its roles are first found; else, or where the marking
/// names neither or both, the one holding the least place, west first, then
/// down, then north, as [`Pos`] orders them. An operand the test passes
/// only one way round is that way whatever its marking.
#[derive(Clone, Copy, Default)]
pub struct Order<'r> {
    pub marked: Option<&'r Roles>,
}

impl Order<'_> {
    /// Whether `a` and `b` are A and B in that order rather than the other
    /// way round.
    pub fn keeps(&self, a: &[Pos], b: &[Pos]) -> bool {
        let named = |bits: &[Pos], n: u8| {
            self.marked.map_or(0, |marked| {
                bits.iter()
                    .filter(|&&pos| marked.get(pos) == Some(Role::Operand(n)))
                    .count()
            })
        };
        let kept = named(a, 0) + named(b, 1);
        let swapped = named(a, 1) + named(b, 0);
        if kept != swapped {
            return kept > swapped;
        }
        a.iter().min() <= b.iter().min()
    }

    /// `a` and `b` as A and B, in the order [`Order::keeps`] prefers.
    pub fn of<T: AsRef<[Pos]>>(&self, a: T, b: T) -> (T, T) {
        if self.keeps(a.as_ref(), b.as_ref()) {
            (a, b)
        } else {
            (b, a)
        }
    }
}

fn split(runs: &[Run]) -> (Vec<&Run>, Vec<Pos>) {
    let buses = runs.iter().filter(|run| run.bits.len() > 1).collect();
    let lones = runs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .map(|run| run.bits[0])
        .collect();
    (buses, lones)
}

/// `runs`' lengths, such as `7, 7, 1, 1`.
fn lengths(runs: &[Run]) -> String {
    let each: Vec<String> =
        runs.iter().map(|run| run.bits.len().to_string()).collect();
    each.join(", ")
}

/// The file in `data` carrying `roles`, in place of any it carried, its
/// blocks as they are.
fn given(data: &[u8], roles: &Roles) -> anyhow::Result<Vec<u8>> {
    lines::with(data, Some(roles))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::schem;

    /// The fixture `name` with its input colours, lime, light blue, and
    /// magenta, turned white, as a build without colour conventions is.
    pub(super) fn uncoloured(name: &str) -> Vec<u8> {
        let data =
            std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let changes: BTreeMap<Pos, String> = blocks
            .iter()
            .filter(|&(_, state)| {
                [
                    crate::interface::colour::INPUT_COLOURS[0],
                    crate::interface::colour::INPUT_COLOURS[1],
                    crate::interface::colour::CONTROL,
                ]
                .iter()
                .any(|c| schem::short(state) == format!("{c}_concrete"))
            })
            .map(|(pos, _)| (pos, "minecraft:white_concrete".to_owned()))
            .collect();
        crate::schem::edit(&data, &changes).unwrap()
    }

    /// A subtractor whose output would read inverted one way round reads,
    /// with A and B swapped, as one whose borrow in is inverted, a control
    /// line an author may well invert, where an output may not be.
    #[test]
    fn a_subtractor_reads_its_borrow_in_inverted_not_its_output() {
        let data =
            std::fs::read("tests/fixtures/sub-16-torchless.schem").unwrap();
        let found = recognize(&data).unwrap().found.unwrap_or_default();
        assert!(found.contains("A − B − not borrow in"), "{found}");
    }
}
