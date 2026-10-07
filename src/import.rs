//! Turning a `.schem` file into a stored schematic, shared by `rsdb add`
//! and the upload page.

use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    interface::{Interface, Port},
    lint::{self, Violation},
    models::schematic::{self, New, StackDirection},
    schem::{Blocks, Size},
    standardize,
};

/// A file made ready to store.
pub struct Prepared {
    pub data: Vec<u8>,
    pub blocks: Blocks,
    /// Empty for a build that does not follow the conventions.
    pub violations: Vec<Violation>,
    pub size: Size,
    pub sha256: String,
}

/// Standardizes `data` if asked, then measures, lints, and hashes it.
pub fn prepare(
    data: Vec<u8>,
    standardize: bool,
    conventional: bool,
) -> Result<Prepared> {
    let data = if standardize {
        standardize::standardize(&data).context("cannot standardize it")?
    } else {
        data
    };
    let blocks = Blocks::read(&data)?;
    let violations = if conventional {
        lint::check(&blocks)
    } else {
        Vec::new()
    };
    // A build without corner markers measures its whole box.
    let size = if blocks.missing_markers().is_empty() {
        blocks.inner_size()?
    } else {
        blocks.full_size()?
    };
    let sha256 =
        Sha256::digest(&data)
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            });
    Ok(Prepared {
        data,
        blocks,
        violations,
        size,
        sha256,
    })
}

/// Stores a schematic with its tags and stacking, and returns its id. The
/// tags must already exist.
pub async fn store(
    db: &SqlitePool,
    new: &New<'_>,
    tags: &[String],
    stacking: &[StackDirection],
) -> Result<String> {
    if let Some(id) = schematic::id_by_sha256(db, new.sha256).await? {
        bail!("this file is already stored as {}", &id[..8]);
    }
    let mut tx = db.begin().await?;
    let id = schematic::insert(&mut *tx, new).await?;
    for name in tags {
        let tag = schematic::tag_id(&mut *tx, name)
            .await?
            .with_context(|| format!("there is no tag named {name}"))?;
        schematic::attach(&mut *tx, &id, &tag).await?;
    }
    schematic::set_stacking(&mut tx, &id, stacking).await?;
    tx.commit().await?;
    Ok(id)
}

/// The width and orientation the inputs suggest: the widest operand's bit
/// count, and which way its bits run. `None` where the inputs say nothing.
pub fn guess(interface: &Interface) -> (Option<usize>, Option<&'static str>) {
    let widest = interface
        .inputs
        .iter()
        .filter(|port| !port.labelled)
        .max_by_key(|port| port.bits.len());
    let width = widest.map(|port| port.bits.len());
    let orientation = widest.and_then(orientation);
    (width, orientation)
}

/// `horz`, `vert`, or `diag`, from how far a bus runs across and up. A bus
/// that mostly runs one way, such as a staggered horizontal one, counts as
/// running that way.
fn orientation(port: &Port) -> Option<&'static str> {
    let first = port.bits.first()?.pos;
    let last = port.bits.last()?.pos;
    let across = (last[0] - first[0]).abs() + (last[2] - first[2]).abs();
    let up = (last[1] - first[1]).abs();
    if across == 0 && up == 0 {
        None
    } else if across >= 2 * up {
        Some("horz")
    } else if up >= 2 * across {
        Some("vert")
    } else {
        Some("diag")
    }
}
