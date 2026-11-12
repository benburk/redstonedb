//! Making a `.schem` file ready to store: laid the standard way and
//! standardized when it is to be recognized, kept as built when not.

use crate::{
    build::{Build, FileFacts, Prepared},
    interface::{self},
    schem::{self, Blocks, Size, turn::Turn},
    standard::standardize,
};
use anyhow::{Context, Result};

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
        let (laid, turn) = crate::standard::orient::oriented(&blocks)?;
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

/// `file` as its author made it, as an upload the site does not recognize
/// is kept; see `Purpose::AS_BUILT`.
pub fn as_built(file: &[u8]) -> Result<Prepared> {
    prepare_for(file, Purpose::AS_BUILT)
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

/// An uploaded file made ready both ways it may be kept.
pub struct Upload {
    /// As its author made it, which a build the site does not recognize
    /// keeps; see [`as_built`].
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
