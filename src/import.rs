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
    models::version,
    schem::{self, Blocks, Size},
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
/// and fits its box tightly; sets it to paste from one block south of its
/// south-east-bottom corner unless `keep_paste`, which keeps where its
/// uploader's `//paste` put it; then measures, lints, and hashes it.
pub fn prepare(
    data: &[u8],
    standardize: bool,
    conventional: bool,
    keep_paste: bool,
) -> Result<Prepared> {
    let data = if standardize {
        standardize::standardize(data).context("cannot standardize it")?
    } else {
        standardize::tidy(data)?
    };
    let data = if keep_paste {
        data
    } else {
        schem::paste_from_corner(&data)?
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
    /// Flags it has; any not of its category are dropped.
    pub flags: &'a [String],
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
    let flags = schematic::keep_flags(db, &category.name, extras.flags).await?;
    let new = &New {
        design: Some(design),
        // Only a category that names each build keeps a given name.
        given_name: new.given_name.filter(|_| category.named),
        ..*new
    };
    let mut tx = db.begin().await?;
    let id = schematic::insert(&mut *tx, new).await?;
    version::first(&mut *tx, &id, new.data, new.sha256, new.uploaded_by)
        .await?;
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
    schematic::set_flags(&mut tx, &id, &flags).await?;
    tx.commit().await?;
    Ok(id)
}

/// The flags the file's signs show: `carry_in` for a line labelled
/// `carry in` or `cin`, `carry_out` for `carry out` or `cout`, and
/// `overflow` and `zero` for status lines named so. Those a category has no
/// flag of are dropped when stored.
pub fn sign_flags(interface: &Interface) -> Vec<String> {
    let named = |ports: &[Port], names: &[&str]| {
        ports
            .iter()
            .any(|port| port.labelled && names.contains(&port.name.as_str()))
    };
    [
        (named(&interface.inputs, &["carry in", "cin"]), "carry_in"),
        (
            named(&interface.outputs, &["carry out", "cout"]),
            "carry_out",
        ),
        (named(&interface.outputs, &["overflow"]), "overflow"),
        (named(&interface.outputs, &["zero"]), "zero"),
    ]
    .into_iter()
    .filter(|(found, _)| *found)
    .map(|(_, flag)| flag.to_owned())
    .collect()
}

/// The categories whose width is read from the file; see `derived_width`.
pub const DERIVES_WIDTH: [&str; 1] = ["adder"];

/// The width a category takes from its builds' files rather than their
/// details: an adder's is its A input's bit count, four for each block of a
/// signal-strength input. `None` for other categories, or a build without
/// an A.
pub fn derived_width(category: &str, interface: &Interface) -> Option<i64> {
    if !DERIVES_WIDTH.contains(&category) {
        return None;
    }
    let a = interface
        .inputs
        .iter()
        .find(|port| !port.labelled && port.block.starts_with("lime"))?;
    let bits = a.bits.len() * if a.signal_strength { 4 } else { 1 };
    i64::try_from(bits).ok()
}

/// The width and orientation the inputs suggest: the widest operand's bit
/// count, four for each block of a signal-strength input, and which way its
/// bits run. `None` where the inputs say nothing.
pub fn guess(interface: &Interface) -> (Option<usize>, Option<&'static str>) {
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
