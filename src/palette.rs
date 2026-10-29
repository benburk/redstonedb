//! A viewer's palette: the blocks a recognized build is shown and
//! downloaded in. The stored file keeps the site's own, white concrete for
//! solid blocks, white stained glass for transparent ones, and its roles'
//! colours; a palette swaps those for the viewer's choices, each block of a
//! build that may be either kind becoming the kind the viewer prefers.
//!
//! Any full block may be the solid one, and any glass the transparent one:
//! an output's mark is a kind of block used for nothing else, so a build of
//! coloured wool or glass is still read right if it comes back.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    schem::{self, Blocks, Pos},
    standardize,
};

/// Minecraft's dyes, in its own order.
pub const DYES: [&str; 16] = [
    "white",
    "light_gray",
    "gray",
    "black",
    "brown",
    "red",
    "orange",
    "yellow",
    "lime",
    "green",
    "cyan",
    "light_blue",
    "blue",
    "purple",
    "magenta",
    "pink",
];

const WOODS: [&str; 11] = [
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove",
    "cherry", "bamboo", "crimson", "warped",
];

const STONES: [&str; 13] = [
    "stone",
    "smooth_stone",
    "smooth_stone_slab[type=double,waterlogged=false]",
    "stone_bricks",
    "cobblestone",
    "polished_andesite",
    "polished_diorite",
    "polished_granite",
    "deepslate_tiles",
    "polished_blackstone",
    "smooth_sandstone",
    "smooth_quartz",
    "quartz_block",
];

/// The solid blocks a palette may build with, by group: any full block
/// that carries power as concrete does.
pub fn solid() -> Vec<(&'static str, Vec<String>)> {
    let dyed =
        |kind: &str| DYES.iter().map(|d| format!("{d}_{kind}")).collect();
    vec![
        ("Concrete", dyed("concrete")),
        ("Wool", dyed("wool")),
        (
            "Terracotta",
            std::iter::once("terracotta".to_owned())
                .chain(DYES.iter().map(|d| format!("{d}_terracotta")))
                .collect(),
        ),
        (
            "Planks",
            WOODS.iter().map(|w| format!("{w}_planks")).collect(),
        ),
        ("Stone", STONES.iter().map(|&s| s.to_owned()).collect()),
        (
            "Logs",
            WOODS
                .iter()
                .filter(|&&w| w != "bamboo")
                .map(|w| match *w {
                    "crimson" | "warped" => {
                        format!("stripped_{w}_stem[axis=y]")
                    }
                    _ => format!("stripped_{w}_log[axis=y]"),
                })
                .collect(),
        ),
        ("Ice", vec!["packed_ice".to_owned(), "blue_ice".to_owned()]),
    ]
}

/// The slabs a palette may build with in the upper half of a block, as
/// they hold dust and torches on top and carry no power, as glass does.
const SLABS: [&str; 8] = [
    "quartz",
    "smooth_quartz",
    "smooth_stone",
    "stone",
    "stone_brick",
    "oak",
    "spruce",
    "bamboo_mosaic",
];

/// The transparent blocks a palette may build with, by group.
pub fn transparent() -> Vec<(&'static str, Vec<String>)> {
    vec![
        (
            "Glass",
            std::iter::once("glass".to_owned())
                .chain(DYES.iter().map(|d| format!("{d}_stained_glass")))
                .collect(),
        ),
        (
            "Top slabs",
            SLABS
                .iter()
                .map(|s| format!("{s}_slab[type=top,waterlogged=false]"))
                .collect(),
        ),
    ]
}

/// What to call `palette`: the preset it is, the default, or custom.
pub fn name(palette: &Palette) -> &'static str {
    if *palette == Palette::default() {
        return "Default";
    }
    presets()
        .into_iter()
        .find(|(.., preset)| preset == palette)
        .map_or("Custom", |(_, name, _)| name)
}

/// How a block reads to people, such as `top quartz slab` or `double
/// smooth stone slab`.
pub fn label(block: &str) -> String {
    let (name, state) = block.split_once('[').unwrap_or((block, ""));
    let name = name.replace('_', " ");
    if state.contains("type=top") {
        format!("top {name}")
    } else if state.contains("type=double") {
        format!("double {name}")
    } else {
        name
    }
}

/// Where the game's own block textures are fetched from by the viewer's
/// browser, as the 3D viewer fetches its resource pack: a public copy of
/// one release's, so the site serves none of them itself.
pub const TEXTURES: &str = "https://cdn.jsdelivr.net/gh/InventivetalentDev/minecraft-assets@1.21.4/assets/minecraft/textures/block";

/// A block's top and side textures, by name, its state left out.
pub fn textures(block: &str) -> (String, String) {
    let name = block.split('[').next().unwrap_or(block);
    if name.starts_with("stripped_")
        && (name.ends_with("_log") || name.ends_with("_stem"))
    {
        return (format!("{name}_top"), name.to_owned());
    }
    let (top, side) = match name {
        "smooth_sandstone" => ("sandstone_top", "sandstone_top"),
        "smooth_quartz" | "smooth_quartz_slab" => {
            ("quartz_block_bottom", "quartz_block_bottom")
        }
        "quartz_block" | "quartz_slab" => {
            ("quartz_block_top", "quartz_block_side")
        }
        "smooth_stone_slab" => ("smooth_stone", "smooth_stone_slab_side"),
        "stone_slab" => ("stone", "stone"),
        "stone_brick_slab" => ("stone_bricks", "stone_bricks"),
        "oak_slab" => ("oak_planks", "oak_planks"),
        "spruce_slab" => ("spruce_planks", "spruce_planks"),
        "bamboo_mosaic_slab" => ("bamboo_mosaic", "bamboo_mosaic"),
        _ => (name, name),
    };
    (top.to_owned(), side.to_owned())
}

/// A block's icon for picking it: a cube from the south-east and above in
/// its own textures, kept pixelated, its sides shaded as a thumbnail's
/// are, as an SVG.
pub fn icon(block: &str) -> String {
    if block == NONE {
        return concat!(
            r#"<svg viewBox="0 0 32 32" width="100%" height="100%" aria-hidden="true">"#,
            r#"<polygon points="16,3 29,10 29,24 16,31 3,24 3,10" fill="none" stroke="currentColor" stroke-opacity="0.5" stroke-dasharray="2 2"/>"#,
            "</svg>"
        )
        .to_owned();
    }
    let (top, side) = textures(block);
    // A top slab is the cube's upper half: its sides half as tall.
    let half = block.contains("type=top");
    let (height, bottom) = if half { ("0.4375", 24) } else { ("0.875", 31) };
    // Each face maps the 16-pixel texture onto its side of an isometric
    // cube in a 32-unit square.
    let face = |texture: &str, matrix: &str| {
        format!(
            r#"<image href="{TEXTURES}/{texture}.png" width="16" height="16" transform="matrix({matrix})" style="image-rendering:pixelated"/>"#
        )
    };
    format!(
        concat!(
            r#"<svg viewBox="0 0 32 32" width="100%" height="100%" aria-hidden="true">"#,
            "{top}{south}{east}",
            r#"<polygon points="3,10 16,17 16,{bottom} 3,{left}" fill="black" fill-opacity="0.2"/>"#,
            r#"<polygon points="16,17 29,10 29,{left} 16,{bottom}" fill="black" fill-opacity="0.36"/>"#,
            "</svg>"
        ),
        top = face(&top, "0.8125 -0.4375 0.8125 0.4375 3 10"),
        south = face(&side, &format!("0.8125 0.4375 0 {height} 3 10")),
        east = face(&side, &format!("0.8125 -0.4375 0 {height} 16 17")),
        bottom = bottom,
        left = bottom - 7,
    )
}

/// Every block a palette may name, of either kind.
pub fn every_block() -> impl Iterator<Item = String> {
    solid()
        .into_iter()
        .chain(transparent())
        .flat_map(|(_, blocks)| blocks)
}

/// The colours a role may be marked with: any dye but white and gray, which
/// mark plain blocks and a stack's first copy's logic.
pub fn colours() -> impl Iterator<Item = &'static str> {
    DYES.into_iter().filter(|d| !matches!(*d, "white" | "gray"))
}

/// What marks no role: its blocks are plain, the palette's own.
pub const NONE: &str = "none";

/// The blocks a role may be marked with, by group: any coloured concrete,
/// wool, or terracotta, solid as an input's block must be.
pub fn marks() -> Vec<(&'static str, Vec<String>)> {
    let dyed = |kind: &str| colours().map(|d| format!("{d}_{kind}")).collect();
    vec![
        ("Concrete", dyed("concrete")),
        ("Wool", dyed("wool")),
        ("Terracotta", dyed("terracotta")),
    ]
}

/// The dye a block is coloured with, such as `lime` for lime wool.
fn dye(block: &str) -> Option<&'static str> {
    DYES.iter()
        .copied()
        .filter(|d| block.starts_with(&format!("{d}_")))
        .max_by_key(|d| d.len())
}

/// Palettes people like, each with a key, whose it is, and the palette.
pub fn presets() -> Vec<(&'static str, &'static str, Palette)> {
    let palette =
        |solid: &str, transparent: &str, prefer_solid: bool| Palette {
            prefer_solid,
            solid: solid.to_owned(),
            transparent: transparent.to_owned(),
            ..Palette::default()
        };
    let top = |slab: &str| format!("{slab}_slab[type=top,waterlogged=false]");
    vec![
        (
            "fearless",
            "Fearless",
            palette("gray_concrete", "lime_stained_glass", false),
        ),
        (
            "0ptimo",
            "0ptimo",
            palette(
                "smooth_stone_slab[type=double,waterlogged=false]",
                &top("quartz"),
                true,
            ),
        ),
        (
            "electronfalcon",
            "electronfalcon",
            palette("orange_wool", "glass", true),
        ),
        (
            "teddy",
            "Teddy",
            palette("gray_terracotta", &top("bamboo_mosaic"), true),
        ),
        (
            "unnecessarymb",
            "unnecessarymb",
            // Blue ice where power means something: wherever a block
            // passes it on, hard or soft, to dust or a component.
            Palette {
                hard: Some("blue_ice".to_owned()),
                soft: Some("blue_ice".to_owned()),
                ..palette(
                    "stripped_cherry_log[axis=y]",
                    "white_stained_glass",
                    true,
                )
            },
        ),
    ]
}

/// The roles a palette colours, each with the site's own colour for it.
pub const ROLES: [(&str, &str, &str); 5] = [
    ("a", "A", "lime"),
    ("b", "B", "light_blue"),
    ("control", "Control inputs, such as carry in", "magenta"),
    ("output", "Outputs", "yellow"),
    ("status", "Status outputs, such as carry out", "pink"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// Whether a block that may be either kind is solid rather than
    /// transparent.
    pub prefer_solid: bool,
    pub solid: String,
    pub transparent: String,
    /// The block each role is marked with, or `none`, by the role's key in
    /// `ROLES`. An older palette's plain colour stands for its concrete.
    pub colours: BTreeMap<String, String>,
    /// The solid block for blocks that carry hard power, as from a torch
    /// under them or a repeater into them; `None` for the solid one.
    #[serde(default)]
    pub hard: Option<String>,
    /// The solid block for blocks that carry soft power, as from dust on
    /// them; `None` for the solid one.
    #[serde(default)]
    pub soft: Option<String>,
}

impl Default for Palette {
    /// The site's own: what stored files hold.
    fn default() -> Self {
        Palette {
            prefer_solid: true,
            solid: "white_concrete".to_owned(),
            transparent: "white_stained_glass".to_owned(),
            colours: ROLES
                .iter()
                .map(|&(key, _, colour)| {
                    (key.to_owned(), format!("{colour}_concrete"))
                })
                .collect(),
            hard: None,
            soft: None,
        }
    }
}

impl Palette {
    /// Whether every choice is one a palette may make.
    pub fn valid(&self) -> bool {
        let among = |groups: Vec<(&str, Vec<String>)>, block: &str| {
            groups
                .iter()
                .any(|(_, blocks)| blocks.iter().any(|b| b == block))
        };
        among(solid(), &self.solid)
            && among(transparent(), &self.transparent)
            && [&self.hard, &self.soft]
                .iter()
                .all(|block| block.as_ref().is_none_or(|b| among(solid(), b)))
            && self.colours.iter().all(|(key, mark)| {
                ROLES.iter().any(|(k, ..)| k == key)
                    && (mark == NONE || among(marks(), mark))
            })
    }

    /// The palette with an older one's plain colours as their concrete.
    #[must_use]
    pub fn normalised(mut self) -> Self {
        for mark in self.colours.values_mut() {
            if DYES.contains(&mark.as_str()) {
                *mark = format!("{mark}_concrete");
            }
        }
        self
    }

    /// A short name for it, for a picture's address, so each palette's
    /// thumbnails are cached apart.
    pub fn key(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        crate::import::sha256(json.as_bytes())[..12].to_owned()
    }

    /// The block role `key` is marked with, `None` for none.
    pub fn mark(&self, key: &str) -> Option<String> {
        let site = || {
            ROLES
                .iter()
                .find(|(k, ..)| *k == key)
                .map(|(.., c)| format!("{c}_concrete"))
        };
        match self.colours.get(key).map(String::as_str) {
            Some(NONE) => None,
            Some(mark) if DYES.contains(&mark) => {
                Some(format!("{mark}_concrete"))
            }
            Some(mark) => Some(mark.to_owned()),
            None => site(),
        }
    }
}

/// What each building block of a recognized build's file must be, as kept
/// with its version: lists of positions under `solid`, `transparent`, and
/// `either`.
pub type Needs = BTreeMap<String, Vec<Pos>>;

/// The file in `data`, a recognized build's whose blocks need `needs`, in
/// `palette`: its plain building blocks the palette's, by what each must
/// be, and its roles' colours the palette's, each block keeping its kind.
pub fn apply(data: &[u8], needs: &Needs, palette: &Palette) -> Result<Vec<u8>> {
    colour(&blocks_in(data, needs, palette)?, palette)
}

/// The file in `data` with its plain building blocks `palette`'s, by what
/// `needs` says each must be. Its positions are its own, so a stacked copy
/// takes it before stacking.
pub fn blocks_in(
    data: &[u8],
    needs: &Needs,
    palette: &Palette,
) -> Result<Vec<u8>> {
    let site = Palette::default();
    if palette.prefer_solid == site.prefer_solid
        && palette.solid == site.solid
        && palette.transparent == site.transparent
        && palette.hard.is_none()
        && palette.soft.is_none()
    {
        return Ok(data.to_vec());
    }
    let blocks = Blocks::read(data)?;
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    let plain = |pos: Pos| {
        matches!(
            schem::short(blocks.get(pos)),
            "white_concrete" | "white_stained_glass"
        )
    };
    for (need, positions) in needs {
        let block = match need.as_str() {
            "solid" => &palette.solid,
            "either" if palette.prefer_solid => &palette.solid,
            "transparent" | "either" => &palette.transparent,
            _ => continue,
        };
        for &pos in positions.iter().filter(|&&pos| plain(pos)) {
            changes.insert(pos, format!("minecraft:{block}"));
        }
    }
    // Blocks carrying power take their own block, when the theme gives
    // one; they are solid already, so each takes the solid one's place.
    for (power, block) in
        [("soft_power", &palette.soft), ("hard_power", &palette.hard)]
    {
        let Some(block) = block else { continue };
        for &pos in needs
            .get(power)
            .into_iter()
            .flatten()
            .filter(|&&pos| plain(pos))
        {
            changes.insert(pos, format!("minecraft:{block}"));
        }
    }
    standardize::edit(data, &changes)
}

/// The file in `data` with its roles' colours `palette`'s, each block
/// keeping its kind. By block alone, so it takes a stacked file too, after
/// stacking, which finds buses by their colours.
pub fn colour(data: &[u8], palette: &Palette) -> Result<Vec<u8>> {
    if palette.colours == Palette::default().colours {
        return Ok(data.to_vec());
    }
    let blocks = Blocks::read(data)?;
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        for (key, _, site) in ROLES {
            let mark = palette.mark(key);
            // A role's glass stays glass: its colour's, or the palette's
            // plain glass for an undyed mark or none.
            let to = if name == format!("{site}_concrete") {
                mark.unwrap_or_else(|| palette.solid.clone())
            } else if name == format!("{site}_stained_glass") {
                match mark.as_deref().and_then(dye) {
                    Some(dye) => format!("{dye}_stained_glass"),
                    None => palette.transparent.clone(),
                }
            } else {
                continue;
            };
            if to != name {
                changes.insert(pos, format!("minecraft:{to}"));
            }
        }
    }
    standardize::edit(data, &changes)
}

/// `user`'s palette, if they chose one.
pub async fn of(db: &SqlitePool, user: Uuid) -> Result<Option<Palette>> {
    let json: Option<String> =
        sqlx::query_scalar("SELECT palette FROM user WHERE id = ?")
            .bind(user)
            .fetch_one(db)
            .await?;
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

/// The key of `viewer`'s palette, for thumbnails' addresses, if they chose
/// one.
pub async fn key_for(
    db: &SqlitePool,
    viewer: Option<&crate::models::user::User>,
) -> Result<Option<String>> {
    Ok(match viewer {
        Some(user) => of(db, user.id).await?.map(|palette| palette.key()),
        None => None,
    })
}

/// Keeps `palette` as `user`'s, or with `None` goes back to the site's.
pub async fn set(
    db: &SqlitePool,
    user: Uuid,
    palette: Option<&Palette>,
) -> Result<()> {
    let json = palette.map(serde_json::to_string).transpose()?;
    sqlx::query("UPDATE user SET palette = ? WHERE id = ?")
        .bind(json)
        .bind(user)
        .execute(db)
        .await?;
    Ok(())
}

/// The palette `needs` of build `id`'s current file, if it is a recognized
/// build's.
pub async fn needs(db: &SqlitePool, id: &str) -> Result<Option<Needs>> {
    let json: Option<String> = sqlx::query_scalar(
        "SELECT v.needs FROM schematic_version AS v
         JOIN current_file AS c USING (schematic_id, version)
         WHERE v.schematic_id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .flatten();
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}
