//! Turning a `.schem` file into a stored schematic, shared by
//! `rsdb refresh`, `rsdb seed`, the upload page, and new versions.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use crate::{
    category::{self, Category, Kind, Lines},
    details::{self, Ready},
    ingest::standardize,
    interface::{self, Encoding, Interface, Orientation, Port},
    models::{
        schematic::{self, Build, New},
        version,
    },
    recognize::probe,
    schem::{self, Blocks, Size, turn::Turn},
    works_from::{self, WorksFrom},
};

/// A file made ready to store.
pub struct Prepared {
    /// The file as it is worked on, carrying its roles, if any; see
    /// [`Prepared::stored`] for the file as it is kept.
    pub data: Vec<u8>,
    pub blocks: Blocks,
    /// Whether the build follows the conventions, so is measured without
    /// its interface layers.
    pub conventional: bool,
    /// Whether the file keeps where its uploader's `//paste` put it.
    pub keeps_paste: bool,
    pub size: Size,
    /// The hash of the file as it is kept, without its roles.
    pub sha256: String,
    /// What is read from it, such as its torches.
    pub facts: FileFacts,
    /// The cauldrons and cakes swapped for barrels, by kind, with how many.
    pub swapped: Vec<(String, usize)>,
    /// How it was turned and mirrored to lie the standard way.
    pub turn: Turn,
}

/// What a file is prepared for, which decides what `prepare_for` does to it
/// beyond what it does to every file.
#[derive(Clone, Copy, Debug)]
pub enum Purpose {
    /// A recognized build's file as the site first keeps it: standardized,
    /// held to the conventions, and turned and mirrored to lie the standard
    /// way, as `orient::orient` does. With `keep_paste` it keeps where its
    /// uploader's `//paste` put it.
    Standard { keep_paste: bool },
    /// A file kept the way it lies, as a stored build's file read again, a
    /// new version, which is compared block by block with the one before, or
    /// a file read to evaluate it: standardized only if asked, held to the
    /// conventions if `conventional`, and keeping where it pastes if
    /// `keep_paste`.
    InPlace {
        standardize: bool,
        conventional: bool,
        keep_paste: bool,
    },
}

impl Purpose {
    /// A file as its author made it, as an upload the site does not
    /// recognize is kept: neither standardized nor turned, and measured
    /// whole, interface and all.
    pub const AS_BUILT: Purpose = Purpose::InPlace {
        standardize: false,
        conventional: false,
        keep_paste: false,
    };

    /// A stored build's file as `s` keeps it: held to the conventions and
    /// pasting from where it does, standardized only if `standardize`.
    pub fn of(s: &Build, standardize: bool) -> Purpose {
        Purpose::InPlace {
            standardize,
            conventional: s.file.conventional,
            keep_paste: s.file.keeps_paste,
        }
    }
}

/// The one way a file becomes the file the site keeps, whether uploaded,
/// added, or read again: swaps cauldrons and cakes for barrels giving the
/// same signal strength; standardizes it if `purpose` says to, and in any
/// case takes out corner markers and fits its box tightly; turns it to lie
/// the standard way for `Purpose::Standard`; and sets it to paste from one
/// block south of its south-east-bottom corner unless it keeps where its
/// uploader's `//paste` put it. Then measures it, reads its facts, and
/// hashes it as it is kept, without the roles it carries. Its blocks keep
/// their colours throughout.
pub fn prepare_for(data: &[u8], purpose: Purpose) -> Result<Prepared> {
    let (standardize, conventional, keep_paste, orient) = match purpose {
        Purpose::Standard { keep_paste } => (true, true, keep_paste, true),
        Purpose::InPlace {
            standardize,
            conventional,
            keep_paste,
        } => (standardize, conventional, keep_paste, false),
    };
    // Read once, changed in memory a step at a time, and written once.
    let blocks = Blocks::read(data)?;
    let swapped = standardize::swappable(&blocks).stand_ins;
    let blocks = standardize::barrelled(&blocks, &[])?.unwrap_or(blocks);
    let blocks = standardize::trapdoors_out(&blocks)?.unwrap_or(blocks);
    let blocks = if standardize {
        standardize::standardized(&blocks).context("cannot standardize it")?
    } else {
        standardize::tidied(&blocks)?
    };
    // Dust as the game shapes it among its neighbours, as `//update` leaves
    // it, whatever way it was saved.
    let blocks = schem::dust::updated(&blocks)?.unwrap_or(blocks);
    let (blocks, turn) = if orient {
        let (laid, turn) = crate::ingest::orient::oriented(&blocks)?;
        (laid.unwrap_or(blocks), turn)
    } else {
        (blocks, Turn::default())
    };
    let blocks = if keep_paste {
        blocks
    } else {
        blocks.pasting_from_corner()?
    };
    let data = blocks.encode()?;
    let size = measure(&blocks, conventional)?;
    let sha256 = schem::sha256(&schem::lines::without(&data)?);
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

impl Prepared {
    /// The file as it is kept: without the roles it carries, which are
    /// kept beside it; see [`Prepared::roles`].
    pub fn stored(&self) -> Result<Vec<u8>> {
        schem::lines::without(&self.data)
    }

    /// The roles its file carries, as they are kept with its version, if
    /// any.
    pub fn roles(&self) -> Option<&schem::lines::Roles> {
        self.blocks.roles.as_ref()
    }
}

/// `file` as its author made it, as an upload the site does not recognize
/// is kept; see `Purpose::AS_BUILT`.
pub fn as_built(file: &[u8]) -> Result<Prepared> {
    prepare_for(file, Purpose::AS_BUILT)
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
        interface::name_lines(
            category::behaviour(category).lines,
            blocks,
            &mut interface,
        );
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
        .filter(|port| !port.status())
        .max_by_key(|port| port.bits.len());
    match held {
        Some(port) => Derived {
            width: i64::try_from(port.bits.len()).ok(),
            orientation: interface::orientation(port),
            encoding: Some(Encoding::Binary),
        },
        None => Derived::default(),
    }
}

/// What a category that reads its builds' files, such as adder, reads
/// from one with `interface`: its width, orientation, and encoding from
/// its A, the unsigned first operand; nothing without one.
pub fn read_from_file(interface: &Interface) -> Derived {
    let a = interface
        .inputs
        .iter()
        .find(|port| !port.labelled && port.a());
    match a {
        Some(a) => Derived {
            width: i64::try_from(
                a.bits.len() * if a.signal_strength { 4 } else { 1 },
            )
            .ok(),
            orientation: interface::orientation(a),
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
        .filter(|port| !port.control())
        .map(|port| port.bits.len())
        .collect();
    let product: usize = interface
        .outputs
        .iter()
        .filter(|port| !port.status())
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
/// control line and one for a status line, named as its lone lines are, such as an adder's `carry_in` and `carry_out` or a
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
    if Kind::named(category) == Some(Kind::Multiplier) {
        return Some(multiplier_flags(interface));
    }
    let Lines::Lone {
        control, status, ..
    } = behaviour.lines
    else {
        return None;
    };
    let flags = [
        (interface.inputs.iter().any(Port::control), control),
        (interface.outputs.iter().any(Port::status), status),
    ];
    Some(
        flags
            .into_iter()
            .filter(|(has, _)| *has)
            .map(|(_, line)| line.replace(' ', "_"))
            .collect(),
    )
}

/// An uploaded file made ready both ways it may be kept.
pub struct Upload {
    /// As its author made it, which a build the site does not recognize
    /// keeps; see `import::as_built`.
    pub original: Prepared,
    /// Standardized and turned to lie the standard way, as a recognized
    /// build is kept; or why it cannot be, as when it marks no inputs.
    pub standard: Result<Prepared, String>,
}

impl Upload {
    pub fn read(file: &[u8]) -> anyhow::Result<Self> {
        let original = as_built(file)?;
        let standard = prepare_for(
            &original.data,
            Purpose::Standard { keep_paste: false },
        )
        .map_err(|err| format!("{err:#}"));
        Ok(Upload { original, standard })
    }

    /// The file the site reads its inputs and outputs from: the standard
    /// one, when there is one.
    pub fn read_as(&self) -> &Prepared {
        self.standard.as_ref().unwrap_or(&self.original)
    }
}

/// Which of an upload's files a build of the category it is given keeps,
/// and how it is filed.
pub enum Kept {
    /// Standardized and turned to lie the standard way, as a build
    /// recognized as its category is kept.
    Standard(Box<Prepared>),
    /// As built, unverified as its category, one the site recognizes
    /// builds of, for this reason.
    Unverified(String),
    /// As built, in the category it was given, one of its author's own.
    Original,
}

/// Which of `upload`'s files a build given as `category` keeps: the
/// standard one when it is recognized as that category, a standard one;
/// else the original. The `Err` says it could not be simulated.
pub async fn kept(
    categories: &[Category],
    category: &str,
    upload: &Upload,
) -> anyhow::Result<Result<Kept, String>> {
    let Some(listed) = Category::find(categories, category) else {
        return Ok(Ok(Kept::Original));
    };
    let unrecognized = match &upload.standard {
        Ok(standard) => {
            details::unrecognized(listed, details::Candidate::of(standard))
                .await
        }
        // Read as built, it can only say it breaks the conventions.
        Err(_) => {
            details::unrecognized(
                listed,
                details::Candidate {
                    data: &upload.original.data,
                    conventional: false,
                    passes: false,
                },
            )
            .await
        }
    };
    Ok(match (unrecognized, &upload.standard) {
        (Err(message), _) => Err(message),
        (Ok(Some(why)), _) => Ok(Kept::Unverified(why)),
        (Ok(None), Ok(standard)) => {
            Ok(Kept::Standard(Box::new(rewrapped(standard)?)))
        }
        (Ok(None), Err(_)) => Ok(Kept::Original),
    })
}

/// A recognized build's standardized file, `standard`, as it is kept:
/// prepared again as it stands, neither standardized nor turned.
pub fn rewrapped(standard: &Prepared) -> Result<Prepared> {
    prepare_for(
        &standard.data,
        Purpose::InPlace {
            standardize: false,
            conventional: standard.conventional,
            keep_paste: standard.keeps_paste,
        },
    )
}

/// The file in `data`, a build's of `category`, carrying the roles of its
/// lines, when it carries none: the roles probing as its category finds,
/// for a category the site recognizes builds of, its lines read loosely
/// as a build kept as built may need, see [`probe::kept_roles`]; else, or
/// when no probe's test runs, those its shapes give it, see
/// [`interface::shaped_roles`]. Its colours never give a line its role,
/// but a file carrying no roles is a stored build's from before the site
/// kept them, or a port of one to the rule for outputs, so its colours are
/// the marking the site once read: of two operands its test passes either
/// way round, A is the one they mark A, see
/// [`crate::recognize::legacy::operand_marks`] and [`probe::Order`]. A file
/// carrying roles comes back as it is.
pub fn with_roles(data: &[u8], category: &str) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    if blocks.roles.is_some() {
        return Ok(data.to_vec());
    }
    let meant = Kind::named(category);
    let marked = crate::recognize::legacy::operand_marks(&blocks);
    let order = probe::Order {
        marked: Some(&marked),
    };
    let probed = match meant {
        Some(kind) => probe::kept_roles(data, kind, order)?,
        None => None,
    };
    let roles = probed.unwrap_or_else(|| interface::shaped_roles(&blocks));
    schem::lines::with(data, Some(&roles))
}

/// The file in `data` carrying the roles probing finds for its inputs and
/// outputs, as an upload's are, for a build meant as one of a standard
/// category: its inputs are its levers
/// and buttons, whatever colour they sit on. Any other build, or one
/// probing doesn't recognize, as it is.
pub async fn probed(data: &[u8], meant: &str) -> Result<Vec<u8>> {
    if !category::listed(meant) {
        return Ok(data.to_vec());
    }
    let (data, meant) = (data.to_vec(), meant.to_owned());
    // Probing simulates, so off the async threads.
    let coloured = tokio::task::spawn_blocking(move || {
        match crate::recognize::probe::recognize_as(&data, Some(&meant)) {
            Ok(recognition) if recognition.found.is_some() => recognition.file,
            // One the simulator can't run, as with a block it doesn't
            // model, isn't recognized either.
            _ => data,
        }
    })
    .await?;
    Ok(coloured)
}

/// A build unverified as `category`, verified by its new version `file`:
/// when that is recognized as the category, the file standardized and
/// turned to lie the standard way, as an upload's is. `None` for one its
/// version does not verify.
pub async fn verified_by(
    category: &Category,
    file: &Prepared,
    keep_paste: bool,
) -> Result<Option<Prepared>> {
    if !category::listed(&category.name) {
        return Ok(None);
    }
    let coloured = probed(&file.data, &category.name).await?;
    let Ok(candidate) =
        prepare_for(&coloured, Purpose::Standard { keep_paste })
    else {
        return Ok(None);
    };
    let recognized =
        details::unrecognized(category, details::Candidate::of(&candidate))
            .await
            .is_ok_and(|why| why.is_none());
    if !recognized {
        return Ok(None);
    }
    Ok(Some(rewrapped(&candidate)?))
}
