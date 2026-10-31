//! Turning a `.schem` file into a stored schematic, shared by
//! `rsdb rebuild`, `rsdb reread`, the upload page, and new versions.

use std::fmt::Write as _;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    category::{self, Lines},
    colour,
    details::{self, Ready, Unrecognized},
    interface::{self, Interface, Port},
    models::catalogue::Category,
    models::schematic::{self, Encoding, New, Orientation},
    models::version,
    orient::{self, Turn},
    schem::{self, Blocks, Size},
    standardize,
    works_from::{self, WorksFrom},
};

/// A file made ready to store.
pub struct Prepared {
    pub data: Vec<u8>,
    pub blocks: Blocks,
    /// Whether the build follows the conventions, so is measured without
    /// its interface layers and has its status lines turned pink.
    pub conventional: bool,
    /// Whether the file keeps where its uploader's `//paste` put it.
    pub keeps_paste: bool,
    pub size: Size,
    pub sha256: String,
    /// What is read from it, such as its torches.
    pub facts: FileFacts,
    /// The cauldrons and cakes swapped for barrels, by kind, with how many.
    pub swapped: Vec<(String, usize)>,
    /// How it was turned and mirrored to lie the standard way.
    pub turn: Turn,
}

/// The one way a file becomes the file the site keeps, whether uploaded,
/// added, or read again: swaps cauldrons and cakes for barrels giving the
/// same signal strength; standardizes it if asked, and in any case takes out corner markers and fits its box
/// tightly; turns a conventional build's status lines pink, as older builds
/// marked them on yellow; and sets it to paste from one block south of its
/// south-east-bottom corner unless `keep_paste`, which keeps where its
/// uploader's `//paste` put it. With `orient`, a conventional build is first
/// turned and mirrored to lie the standard way, as `orient::orient` does; a
/// new version is not, as it is compared block by block with the one
/// before. Then measures it, reads its facts, and hashes it.
#[allow(clippy::fn_params_excessive_bools)]
pub fn prepare(
    data: &[u8],
    standardize: bool,
    conventional: bool,
    keep_paste: bool,
    orient: bool,
) -> Result<Prepared> {
    let swapped = standardize::swappable(&Blocks::read(data)?).stand_ins;
    let data = standardize::barrels(data, &[])?;
    let data = if standardize {
        standardize::standardize(&data).context("cannot standardize it")?
    } else {
        standardize::tidy(&data)?
    };
    let data = if conventional {
        standardize::pink_status(&data)?
    } else {
        data
    };
    let (data, turn) = if conventional && orient {
        orient::orient(&data)?
    } else {
        (data, Turn::default())
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
        turn,
    })
}

/// `file` as its builder made it, as an upload the site does not recognize
/// is kept: prepared as `prepare` does without standardizing or turning
/// it, measured whole, interface and all.
pub fn as_built(file: &[u8]) -> Result<Prepared> {
    prepare(file, false, false, false, false)
}

/// What a build's file says about itself: its inputs and outputs, named as
/// its category names them.
#[derive(Default)]
pub struct Analysis {
    pub interface: Interface,
}

impl Analysis {
    /// `blocks` read as a build of the category named `category`.
    pub fn of(blocks: &Blocks, category: &str) -> Self {
        let mut interface = interface::read(blocks);
        interface::name_lines(category, blocks, &mut interface);
        Analysis { interface }
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
/// uploaded by `uploaded_by` from IP address `uploaded_from`, and returns
/// its id; the `Err` says the file is already stored.
pub async fn store(
    db: &SqlitePool,
    prepared: &Prepared,
    ready: &Ready,
    uploaded_by: Option<uuid::Uuid>,
    uploaded_from: Option<&str>,
    imported_by: Option<&str>,
    id: Option<&str>,
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
        uploaded_from,
        imported_by,
        id,
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
        (named(&interface.inputs, &colour::CARRY_IN), "carry_in"),
        (named(&interface.outputs, &colour::CARRY_OUT), "carry_out"),
        (named(&interface.inputs, &colour::BORROW_IN), "borrow_in"),
        (named(&interface.outputs, &colour::BORROW_OUT), "borrow_out"),
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
            .filter(|port| port.block.starts_with(colour::CONTROL))
            .flat_map(|port| port.words.iter())
            .filter_map(|word| crate::simulate::alu_op(word))
            .filter(|op| !matches!(*op, "add" | "sub"))
            .map(str::to_owned),
    )
    .collect()
}

/// What a category that reads its builds' files, as
/// `category::Behaviour::reads` says, takes from them in place of
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
    match category::behaviour(category).reads {
        category::Reads::Output => read_from_output(interface),
        category::Reads::InputA => read_from_file(interface),
        category::Reads::Nothing => Derived::default(),
    }
}

/// What a counter or shift register reads from a build with `interface`:
/// its width and the way its bits run from its widest output, the value
/// it holds.
fn read_from_output(interface: &Interface) -> Derived {
    let held = interface
        .outputs
        .iter()
        .filter(|port| !port.block.starts_with(colour::STATUS))
        .max_by_key(|port| port.bits.len());
    match held {
        Some(port) => Derived {
            width: i64::try_from(port.bits.len()).ok(),
            orientation: orientation(port),
            encoding: Some(Encoding::Binary),
        },
        None => Derived::default(),
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

/// A multiplier's flags, from its ports: `fma` for a third operand, as a
/// fused multiply-add's, and `upper_product` for outputs holding more bits
/// than its wider operand, the upper half of the product as well.
fn multiplier_flags(interface: &Interface) -> Vec<String> {
    let operands: Vec<usize> = interface
        .inputs
        .iter()
        .filter(|port| !port.block.starts_with(colour::CONTROL))
        .map(|port| port.bits.len())
        .collect();
    let product: usize = interface
        .outputs
        .iter()
        .filter(|port| !port.block.starts_with(colour::STATUS))
        .map(|port| port.bits.len())
        .sum();
    let mut flags = Vec::new();
    if product > operands.iter().copied().max().unwrap_or(0) {
        flags.push("upper_product".to_owned());
    }
    if operands.len() > 2 {
        flags.push("fma".to_owned());
    }
    flags
}

/// The flags a category takes from its builds' files rather than their
/// details, as `category::Behaviour::flags_from_file` says: one for a
/// magenta control line and one for a pink status line, named as its lone
/// lines are, such as an adder's `carry_in` and `carry_out` or a
/// subtractor's `borrow_in` and `borrow_out`; a multiplier's from its
/// operands and product. `None` for other categories.
pub fn derived_flags(
    category: &str,
    interface: &Interface,
) -> Option<Vec<String>> {
    let behaviour = category::behaviour(category);
    if !behaviour.flags_from_file {
        return None;
    }
    if category == "multiplier" {
        return Some(multiplier_flags(interface));
    }
    let Lines::Lone {
        control, status, ..
    } = behaviour.lines
    else {
        return None;
    };
    let has = |ports: &[Port], colour: &str| {
        ports.iter().any(|port| port.block.starts_with(colour))
    };
    let flags = [
        (has(&interface.inputs, colour::CONTROL), control),
        (has(&interface.outputs, colour::STATUS), status),
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

/// An uploaded file made ready both ways it may be kept.
pub struct Upload {
    /// As its builder made it, which a build the site does not recognize
    /// keeps; see `import::as_built`.
    pub original: Prepared,
    /// Standardized and turned to lie the standard way, as a recognized
    /// build is kept; or why it cannot be, as when it marks no inputs.
    pub standard: Result<Prepared, String>,
}

impl Upload {
    pub fn read(file: &[u8]) -> anyhow::Result<Self> {
        let original = as_built(file)?;
        let standard = prepare(&original.data, true, true, false, true)
            .map_err(|err| format!("{err:#}"));
        Ok(Upload { original, standard })
    }

    /// The file the site reads its inputs and outputs from: the standard
    /// one, when there is one.
    pub fn read_as(&self) -> &Prepared {
        self.standard.as_ref().unwrap_or(&self.original)
    }
}

/// Which of an upload's files a build of the category it is given keeps.
pub enum Kept {
    /// As built, the build not being one the site recognizes, with why
    /// not when its category is one the site works out.
    Original(Option<Unrecognized>),
    /// Standardized, as a build of the
    /// listed category named `category`, which an off-list name such as
    /// `8-bit adder` stands in for.
    Standard {
        file: Box<Prepared>,
        category: String,
    },
}

/// Which of `upload`'s files a build given as `category` keeps: the
/// standard one when it is recognized as the blessed category `category`
/// means, as `details::meant` reads it; else the original, off the list
/// when `category` names a listed one. The `Err` says it could not be
/// simulated.
pub async fn kept(
    categories: &[Category],
    category: &str,
    upload: &Upload,
) -> anyhow::Result<Result<Kept, String>> {
    let Some(meant) = details::meant(categories, category) else {
        return Ok(Ok(Kept::Original(None)));
    };
    // Only a blessed build goes on the list; any other listed category's
    // goes off it, named for what it is.
    if !category::BLESSED.contains(&meant.name.as_str()) {
        let width = upload.standard.as_ref().ok().and_then(|standard| {
            derived_width(&meant.name, &interface::read(&standard.blocks))
        });
        return Ok(Ok(Kept::Original(Some(Unrecognized {
            why: format!(
                "{} aren't recognized yet",
                meant.plural.to_lowercase()
            ),
            name: details::off_list_name(meant, width),
        }))));
    }
    let unrecognized = match &upload.standard {
        Ok(standard) => {
            details::unrecognized(meant, details::Build::of(standard)).await
        }
        // Read as built, it can only say it breaks the conventions.
        Err(_) => {
            details::unrecognized(
                meant,
                details::Build {
                    data: &upload.original.data,
                    conventional: false,
                    passes: false,
                },
            )
            .await
        }
    };
    Ok(match unrecognized {
        Err(message) => Err(message),
        Ok(Some(off)) => Ok(Kept::Original(Some(off))),
        Ok(None) => match &upload.standard {
            Ok(standard) => Ok(Kept::Standard {
                file: Box::new(rewrapped(standard)?),
                category: meant.name.clone(),
            }),
            Err(_) => Ok(Kept::Original(None)),
        },
    })
}

/// A recognized build's standardized file, `standard`, as it is kept:
/// prepared again as it stands, neither standardized nor turned.
pub fn rewrapped(standard: &Prepared) -> Result<Prepared> {
    prepare(
        &standard.data,
        false,
        standard.conventional,
        standard.keeps_paste,
        false,
    )
}

/// The file in `data` with its inputs and outputs coloured by probing,
/// as an upload's are, for a build meant as one of a blessed category,
/// see `category::BLESSED`: its inputs are its levers and buttons,
/// whatever colour they sit on. Any other build, or one probing doesn't
/// recognize, as it is.
pub async fn probed(data: &[u8], meant: &str) -> Result<Vec<u8>> {
    if !category::BLESSED.contains(&meant) {
        return Ok(data.to_vec());
    }
    let (data, meant) = (data.to_vec(), meant.to_owned());
    // Probing simulates, so off the async threads.
    let coloured = tokio::task::spawn_blocking(move || {
        match crate::probe::recognize_as(&data, Some(&meant)) {
            Ok(recognition) if recognition.found.is_some() => recognition.file,
            // One the simulator can't run, as with a block it doesn't
            // model, isn't recognized either.
            _ => data,
        }
    })
    .await?;
    Ok(coloured)
}

/// A build off the list, as `category` names it, moved up by its new
/// version `file`: when that is recognized as the listed category the
/// name means, as an 8-bit adder's as an adder, the file standardized and
/// turned to lie the standard way, as an upload's is,
/// with that category. `None` for a build on the list, or one its version
/// does not move up.
pub async fn moved_up<'a>(
    categories: &'a [Category],
    category: &str,
    file: &Prepared,
    keep_paste: bool,
) -> Result<Option<(Prepared, &'a Category)>> {
    if Category::find(categories, category).is_some() {
        return Ok(None);
    }
    let Some(meant) = details::meant(categories, category)
        .filter(|c| category::BLESSED.contains(&c.name.as_str()))
    else {
        return Ok(None);
    };
    let coloured = probed(&file.data, &meant.name).await?;
    let Ok(candidate) = prepare(&coloured, true, true, keep_paste, true) else {
        return Ok(None);
    };
    let recognized =
        details::unrecognized(meant, details::Build::of(&candidate))
            .await
            .is_ok_and(|off| off.is_none());
    if !recognized {
        return Ok(None);
    }
    Ok(Some((rewrapped(&candidate)?, meant)))
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
