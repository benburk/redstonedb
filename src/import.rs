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
    /// How many redstone torches it has.
    pub torches: i64,
    /// The cauldrons and cakes swapped for barrels, by kind, with how many.
    pub swapped: Vec<(String, usize)>,
}

/// Swaps cauldrons and cakes for barrels giving the same signal strength,
/// standardizes `data` if asked, and in any case takes out corner markers
/// and fits its box tightly; sets it to paste from one block south of its
/// south-east-bottom corner unless `keep_paste`, which keeps where its
/// uploader's `//paste` put it; then measures, lints, counts its torches,
/// and hashes it.
pub fn prepare(
    data: &[u8],
    standardize: bool,
    conventional: bool,
    keep_paste: bool,
) -> Result<Prepared> {
    let swapped = standardize::swappable(&Blocks::read(data)?).stand_ins;
    let data = standardize::barrels(data, &[])?;
    let data = if standardize {
        standardize::standardize(&data).context("cannot standardize it")?
    } else {
        standardize::tidy(&data)?
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
        torches: torches(&blocks),
        data,
        blocks,
        violations,
        size,
        sha256,
        swapped,
    })
}

/// How many redstone torches `blocks` has, standing or on a wall.
pub fn torches(blocks: &Blocks) -> i64 {
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

/// Counts the torches of every build not yet counted, as after the count
/// began to be kept.
pub async fn count_torches(db: &SqlitePool) -> Result<()> {
    for id in schematic::uncounted_torches(db).await? {
        let data = schematic::data(db, &id)
            .await?
            .context("schematic vanished")?;
        let blocks = Blocks::read(&data)?;
        schematic::set_torches(db, &id, torches(&blocks)).await?;
    }
    Ok(())
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
    if let Some(name) = extras
        .tags
        .iter()
        .find(|n| schematic::DERIVED.contains(&schematic::tag_name(n).as_str()))
    {
        bail!("{name} is worked out from the build, not tagged");
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
        (named(&interface.inputs, &lint::CARRY_IN), "carry_in"),
        (named(&interface.outputs, &lint::CARRY_OUT), "carry_out"),
        (named(&interface.outputs, &["overflow"]), "overflow"),
        (named(&interface.outputs, &["zero"]), "zero"),
    ]
    .into_iter()
    .filter(|(found, _)| *found)
    .map(|(_, flag)| flag.to_owned())
    .collect()
}

/// The categories whose width, orientation, and encoding are read from the
/// file, from input A; see `derived`. An adder's flags are read too; see
/// `derived_flags`.
pub const READ_FROM_FILE: [&str; 4] =
    ["adder", "incrementer", "decrementer", "inc_dec"];

/// What a category in `READ_FROM_FILE` takes from its builds' files in
/// place of typed details.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Derived {
    /// A's bit count, four for each block of a signal-strength input.
    pub width: Option<i64>,
    /// `horz`, `vert`, or `diag`, the way A's bits run.
    pub orientation: Option<&'static str>,
    /// `hex` when A takes a signal strength, a digit per block, else `bin`.
    pub encoding: Option<&'static str>,
}

/// What `category` reads from a build with `interface`: nothing for other
/// categories, or a build without an A.
pub fn derived(category: &str, interface: &Interface) -> Derived {
    let a = interface
        .inputs
        .iter()
        .find(|port| !port.labelled && port.block.starts_with("lime"));
    match a.filter(|_| READ_FROM_FILE.contains(&category)) {
        Some(a) => Derived {
            width: i64::try_from(
                a.bits.len() * if a.signal_strength { 4 } else { 1 },
            )
            .ok(),
            orientation: orientation(a),
            encoding: Some(if a.signal_strength { "hex" } else { "bin" }),
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
/// details: an adder's `carry_in` for a magenta control line and
/// `carry_out` for a pink status line. `None` for other categories.
pub fn derived_flags(
    category: &str,
    interface: &Interface,
) -> Option<Vec<String>> {
    if category != "adder" {
        return None;
    }
    let has = |ports: &[Port], colour: &str| {
        ports.iter().any(|port| port.block.starts_with(colour))
    };
    let flags = [
        (has(&interface.inputs, lint::CONTROL), "carry_in"),
        (has(&interface.outputs, lint::STATUS), "carry_out"),
    ];
    Some(
        flags
            .into_iter()
            .filter(|(has, _)| *has)
            .map(|(_, flag)| flag.to_owned())
            .collect(),
    )
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
