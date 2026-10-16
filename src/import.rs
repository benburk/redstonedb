//! Turning a `.schem` file into a stored schematic, shared by `rsdb add`,
//! `rsdb reread`, the upload page, and new versions.

use std::fmt::Write as _;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    category::{self, Lines},
    details::Ready,
    interface::{self, Interface, Port},
    lint::{self, Violation},
    models::schematic::{self, Encoding, New, Orientation},
    models::version,
    schem::{self, Blocks, Size},
    standardize::{self, Gray},
    works_from::{self, WorksFrom},
};

/// A file made ready to store.
pub struct Prepared {
    pub data: Vec<u8>,
    pub blocks: Blocks,
    /// Whether the build follows the conventions, so is measured, linted,
    /// and has its status lines turned pink.
    pub conventional: bool,
    /// Whether the file keeps where its uploader's `//paste` put it.
    pub keeps_paste: bool,
    pub size: Size,
    pub sha256: String,
    /// What is read from it, such as its torches.
    pub facts: FileFacts,
    /// The cauldrons and cakes swapped for barrels, by kind, with how many.
    pub swapped: Vec<(String, usize)>,
}

/// The one way a file becomes the file the site keeps, whether uploaded,
/// added, or read again: swaps cauldrons and cakes for barrels giving the
/// same signal strength; standardizes it if asked, gray as `standardize`
/// says, and in any case takes out corner markers and fits its box
/// tightly; turns a conventional build's status lines pink, as older builds
/// marked them on yellow; and sets it to paste from one block south of its
/// south-east-bottom corner unless `keep_paste`, which keeps where its
/// uploader's `//paste` put it. Then measures it, reads its facts, and
/// hashes it.
pub fn prepare(
    data: &[u8],
    standardize: Option<Gray>,
    conventional: bool,
    keep_paste: bool,
) -> Result<Prepared> {
    let swapped = standardize::swappable(&Blocks::read(data)?).stand_ins;
    let data = standardize::barrels(data, &[])?;
    let data = if let Some(gray) = standardize {
        standardize::standardize(&data, gray)
            .context("cannot standardize it")?
    } else {
        standardize::tidy(&data)?
    };
    let data = if conventional {
        standardize::pink_status(&data)?
    } else {
        data
    };
    let data = if keep_paste {
        data
    } else {
        schem::paste_from_corner(&data)?
    };
    let blocks = Blocks::read(&data)?;
    let size = measure(&blocks, conventional)?;
    let sha256 = sha256(&data);
    Ok(Prepared {
        facts: FileFacts::of(&blocks),
        data,
        blocks,
        conventional,
        keeps_paste: keep_paste,
        size,
        sha256,
        swapped,
    })
}

/// What a build's file says about itself: its inputs and outputs, and the
/// building conventions it breaks.
#[derive(Default)]
pub struct Analysis {
    /// Its lines named as its category makes them.
    pub interface: Interface,
    /// None for a build that does not follow the conventions.
    pub violations: Vec<Violation>,
}

impl Analysis {
    /// `blocks` read as a build of the category named `category`. A lone
    /// unsigned control or status line breaks a convention only once a
    /// category is chosen that needs it signed, so an empty `category`
    /// lets it pass.
    pub fn of(blocks: &Blocks, category: &str, conventional: bool) -> Self {
        let mut interface = interface::read(blocks);
        interface::name_lines(category, blocks, &mut interface);
        let violations = match (conventional, category) {
            (false, _) => Vec::new(),
            (true, "") => lint::check(blocks),
            (true, category) => lint::check_for(blocks, category),
        };
        Analysis {
            interface,
            violations,
        }
    }
}

/// What is read from a build's file rather than typed: its torches and the
/// oldest release it works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub torches: i64,
    pub works_from: WorksFrom,
}

impl FileFacts {
    pub fn of(blocks: &Blocks) -> Self {
        FileFacts {
            torches: torches(blocks),
            works_from: works_from::of(blocks),
        }
    }
}

/// How many redstone torches `blocks` has, standing or on a wall.
fn torches(blocks: &Blocks) -> i64 {
    let count = blocks
        .iter()
        .filter(|(_, state)| {
            matches!(
                schem::id(state),
                "minecraft:redstone_torch" | "minecraft:redstone_wall_torch"
            )
        })
        .count();
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// The file's SHA-256, in hex.
pub fn sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

/// A build's size: its box less its outer layers of interface and outputs,
/// as [`interface::measured`] finds it, for a build that follows the
/// conventions, and its whole box for one that does not.
pub fn measure(blocks: &Blocks, conventional: bool) -> Result<Size> {
    if !conventional {
        return Ok(blocks.full_size());
    }
    let (low, high) = interface::measured(blocks);
    let [x, y, z] = [0, 1, 2]
        .map(|axis| u16::try_from(high[axis] - low[axis] + 1).unwrap_or(0));
    Ok(Size { x, y, z })
}

/// Stores a new build, its file as prepared and its details as checked,
/// and returns its id; the `Err` says the file is already stored.
pub async fn store(
    db: &SqlitePool,
    prepared: &Prepared,
    ready: &Ready,
    uploaded_by: Option<uuid::Uuid>,
    imported_by: Option<&str>,
) -> Result<Result<String, String>, sqlx::Error> {
    if let Some((id, _)) = version::holder(db, &prepared.sha256).await? {
        return Ok(Err(format!(
            "This exact file is already on the site, as {}.",
            &id[..8]
        )));
    }
    let new = New {
        ready,
        file: prepared,
        uploaded_by,
        imported_by,
    };
    Ok(Ok(schematic::insert(db, &new).await?))
}

/// The flags the file's signs show: `carry_in` for a line labelled
/// `carry in` or `cin`, `carry_out` for `carry out` or `cout`, their
/// borrowing kin `borrow_in` and `borrow_out` likewise, and `overflow` and
/// `zero` for status lines named so. Those a category has no
/// flag of are dropped when stored.
pub fn sign_flags(interface: &Interface) -> Vec<String> {
    let named = |ports: &[Port], names: &[&str]| {
        ports
            .iter()
            .any(|port| port.labelled && names.contains(&port.name.as_str()))
    };
    [
        (named(&interface.inputs, &lint::CARRY_IN), "carry_in"),
        (named(&interface.outputs, &lint::CARRY_OUT), "carry_out"),
        (named(&interface.inputs, &lint::BORROW_IN), "borrow_in"),
        (named(&interface.outputs, &lint::BORROW_OUT), "borrow_out"),
        (named(&interface.outputs, &["overflow"]), "overflow"),
        (named(&interface.outputs, &["zero"]), "zero"),
    ]
    .into_iter()
    .filter(|(found, _)| *found)
    .map(|(_, flag)| flag.to_owned())
    // An ALU's operations beyond adding and subtracting, as its control
    // lines' signs name them, such as `and` or `rsh` for shr.
    .chain(
        interface
            .inputs
            .iter()
            .filter(|port| port.block.starts_with(lint::CONTROL))
            .flat_map(|port| port.words.iter())
            .filter_map(|word| crate::simulate::alu_op(word))
            .filter(|op| !matches!(*op, "add" | "sub"))
            .map(str::to_owned),
    )
    .collect()
}

/// What a category that reads its builds' files, as
/// `category::Behaviour::reads_file` says, takes from them in place of
/// typed details.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Derived {
    /// A's bit count, four for each block of a signal-strength input.
    pub width: Option<i64>,
    /// The way A's bits run.
    pub orientation: Option<Orientation>,
    /// Hex when A takes a signal strength, a digit per block, else binary.
    pub encoding: Option<Encoding>,
}

/// What `category` reads from a build with `interface`: nothing for a
/// category that reads nothing from its file; see [`read_from_file`].
pub fn derived(category: &str, interface: &Interface) -> Derived {
    if category::behaviour(category).reads_file {
        read_from_file(interface)
    } else {
        Derived::default()
    }
}

/// What a category that reads its builds' files, such as adder, reads
/// from one with `interface`: its width, orientation, and encoding from
/// its A, the unsigned lime input; nothing without one.
pub fn read_from_file(interface: &Interface) -> Derived {
    let a = interface
        .inputs
        .iter()
        .find(|port| !port.labelled && port.block.starts_with("lime"));
    match a {
        Some(a) => Derived {
            width: i64::try_from(
                a.bits.len() * if a.signal_strength { 4 } else { 1 },
            )
            .ok(),
            orientation: orientation(a),
            encoding: Some(if a.signal_strength {
                Encoding::Hex
            } else {
                Encoding::Binary
            }),
        },
        None => Derived::default(),
    }
}

/// The width `category` reads from a build with `interface`; see
/// `derived`.
pub fn derived_width(category: &str, interface: &Interface) -> Option<i64> {
    derived(category, interface).width
}

/// The flags a category takes from its builds' files rather than their
/// details, as `category::Behaviour::flags_from_file` says: one for a
/// magenta control line and one for a pink status line, named as its lone
/// lines are, such as an adder's `carry_in` and `carry_out` or a
/// subtractor's `borrow_in` and `borrow_out`. `None` for other categories.
pub fn derived_flags(
    category: &str,
    interface: &Interface,
) -> Option<Vec<String>> {
    let behaviour = category::behaviour(category);
    let Lines::Lone {
        control, status, ..
    } = behaviour.lines
    else {
        return None;
    };
    if !behaviour.flags_from_file {
        return None;
    }
    let has = |ports: &[Port], colour: &str| {
        ports.iter().any(|port| port.block.starts_with(colour))
    };
    let flags = [
        (has(&interface.inputs, lint::CONTROL), control),
        (has(&interface.outputs, lint::STATUS), status),
    ];
    Some(
        flags
            .into_iter()
            .filter(|(has, _)| *has)
            .map(|(_, line)| line.replace(' ', "_"))
            .collect(),
    )
}

/// The width and orientation the inputs suggest: the widest operand's bit
/// count, four for each block of a signal-strength input, and which way its
/// bits run. `None` where the inputs say nothing.
pub fn guess(interface: &Interface) -> (Option<usize>, Option<Orientation>) {
    let bits = |port: &Port| {
        port.bits.len() * if port.signal_strength { 4 } else { 1 }
    };
    let widest = interface
        .inputs
        .iter()
        .filter(|port| !port.labelled)
        .max_by_key(|port| bits(port));
    let width = widest.map(bits);
    let orientation = widest.and_then(orientation);
    (width, orientation)
}

/// The way a bus runs, from how far it runs across and up. A bus that
/// mostly runs one way, such as a staggered horizontal one, counts as
/// running that way.
fn orientation(port: &Port) -> Option<Orientation> {
    let first = port.bits.first()?.pos;
    let last = port.bits.last()?.pos;
    let across = (last[0] - first[0]).abs() + (last[2] - first[2]).abs();
    let up = (last[1] - first[1]).abs();
    if across == 0 && up == 0 {
        None
    } else if across >= 2 * up {
        Some(Orientation::Horizontal)
    } else if up >= 2 * across {
        Some(Orientation::Vertical)
    } else {
        Some(Orientation::Diagonal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str) -> Port {
        Port {
            name: name.to_owned(),
            block: "pink concrete".to_owned(),
            labelled: true,
            bits: Vec::new(),
            strength: 1,
            words: Vec::new(),
            signal_strength: false,
        }
    }

    #[test]
    fn status_lines_tick_their_flags() {
        let interface = Interface {
            inputs: vec![line("cin")],
            outputs: vec![
                line("cout"),
                line("overflow"),
                line("zero"),
                line("negative"),
            ],
        };
        assert_eq!(
            sign_flags(&interface),
            ["carry_in", "carry_out", "overflow", "zero"]
        );
    }
}
