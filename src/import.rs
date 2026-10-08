//! Turning a `.schem` file into a stored schematic, shared by `rsdb add`
//! and the upload page.

use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    interface::{self, Interface, Port},
    lint::{self, Violation},
    models::schematic::{self, Credit, New, StackDirection},
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

/// Standardizes `data` if asked, and in any case takes out corner markers
/// and fits its box tightly; then measures, lints, and hashes it.
pub fn prepare(
    data: &[u8],
    standardize: bool,
    conventional: bool,
) -> Result<Prepared> {
    let data = if standardize {
        standardize::standardize(data).context("cannot standardize it")?
    } else {
        standardize::tidy(data)?
    };
    let blocks = Blocks::read(&data)?;
    let violations = if conventional {
        lint::check(&blocks)
    } else {
        Vec::new()
    };
    let size = measure(&blocks, conventional)?;
    let sha256 = sha256(&data);
    Ok(Prepared {
        data,
        blocks,
        violations,
        size,
        sha256,
    })
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

/// A build's size: its core, as [`interface::core`] finds it, for a build
/// that follows the conventions, and its whole box for one that does not.
pub fn measure(blocks: &Blocks, conventional: bool) -> Result<Size> {
    if !conventional {
        return Ok(blocks.full_size());
    }
    let (low, high) = interface::core(blocks);
    let [x, y, z] = [0, 1, 2]
        .map(|axis| u16::try_from(high[axis] - low[axis] + 1).unwrap_or(0));
    Ok(Size { x, y, z })
}

/// What else a stored schematic carries besides its row.
pub struct Extras<'a> {
    pub builders: &'a [String],
    pub contributors: &'a [String],
    pub tags: &'a [String],
    /// Make tags that do not exist yet, of the `other` kind, rather
    /// than refusing them.
    pub new_tags: bool,
    pub stacking: &'a [StackDirection],
}

/// Stores a schematic with its credits, tags, and stacking, and returns
/// its id.
pub async fn store(
    db: &SqlitePool,
    new: &New<'_>,
    extras: &Extras<'_>,
) -> Result<String> {
    if let Some(name) = extras
        .builders
        .iter()
        .chain(extras.contributors)
        .find(|n| !schematic::valid_player_name(n))
    {
        bail!(
            "{name} is not a Minecraft name: letters, digits, and underscores"
        );
    }
    if let Some(id) = schematic::id_by_sha256(db, new.sha256).await? {
        bail!("this file is already stored as {}", &id[..8]);
    }
    let design = new.design.unwrap_or("other");
    let category =
        match schematic::check_category(db, new.category, design).await? {
            Ok(category) => category,
            Err(message) => bail!("{message}"),
        };
    // Carry lines mean nothing for a category without them.
    let (carry_in, carry_out) = if category.carries {
        (new.carry_in, new.carry_out)
    } else {
        (None, None)
    };
    let new = &New {
        design: Some(design),
        carry_in,
        carry_out,
        // Only a category that names each build keeps a given name.
        given_name: new.given_name.filter(|_| category.named),
        ..*new
    };
    let mut tx = db.begin().await?;
    let id = schematic::insert(&mut *tx, new).await?;
    schematic::set_credits(&mut tx, &id, Credit::Builder, extras.builders)
        .await?;
    schematic::set_credits(
        &mut tx,
        &id,
        Credit::Contributor,
        extras.contributors,
    )
    .await?;
    for name in extras.tags {
        let tag = match schematic::tag_id(&mut *tx, name).await? {
            Some(tag) => tag,
            None if extras.new_tags => {
                schematic::define_tag(&mut *tx, name, None, "other")
                    .await
                    .with_context(|| {
                        format!("{name} cannot be a tag's name")
                    })?;
                schematic::tag_id(&mut *tx, name)
                    .await?
                    .context("the new tag vanished")?
            }
            None => bail!("there is no tag named {name}"),
        };
        schematic::attach(&mut *tx, &id, &tag).await?;
    }
    schematic::set_stacking(&mut tx, &id, extras.stacking).await?;
    tx.commit().await?;
    Ok(id)
}

/// Whether the build has a carry in and a carry out, from lines labelled
/// `carry in` or `cin`, and `carry out` or `cout`.
pub fn carries(interface: &Interface) -> (bool, bool) {
    let named = |ports: &[Port], names: [&str; 2]| {
        ports
            .iter()
            .any(|port| port.labelled && names.contains(&port.name.as_str()))
    };
    (
        named(&interface.inputs, ["carry in", "cin"]),
        named(&interface.outputs, ["carry out", "cout"]),
    )
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
