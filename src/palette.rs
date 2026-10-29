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

const STONES: [&str; 12] = [
    "stone",
    "smooth_stone",
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
    ]
}

/// The transparent blocks a palette may build with, by group.
pub fn transparent() -> Vec<(&'static str, Vec<String>)> {
    vec![(
        "Glass",
        std::iter::once("glass".to_owned())
            .chain(DYES.iter().map(|d| format!("{d}_stained_glass")))
            .collect(),
    )]
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

/// Palettes people like, each with a key and a name.
pub fn presets() -> Vec<(&'static str, &'static str, Palette)> {
    vec![(
        "gray-lime",
        "Gray concrete and lime glass, glass by default",
        Palette {
            prefer_solid: false,
            solid: "gray_concrete".to_owned(),
            transparent: "lime_stained_glass".to_owned(),
            ..Palette::default()
        },
    )]
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
    /// Each role's colour, by the role's key in `ROLES`.
    pub colours: BTreeMap<String, String>,
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
                .map(|&(key, _, colour)| (key.to_owned(), colour.to_owned()))
                .collect(),
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
            && self.colours.iter().all(|(key, colour)| {
                ROLES.iter().any(|(k, ..)| k == key)
                    && colours().any(|c| c == colour)
            })
    }

    /// A short name for it, for a picture's address, so each palette's
    /// thumbnails are cached apart.
    pub fn key(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        crate::import::sha256(json.as_bytes())[..12].to_owned()
    }

    /// The colour role `key` is marked with.
    fn colour(&self, key: &str) -> &str {
        self.colours.get(key).map_or_else(
            || {
                ROLES
                    .iter()
                    .find(|(k, ..)| *k == key)
                    .map_or("", |(.., c)| c)
            },
            String::as_str,
        )
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
            "transparent" => &palette.transparent,
            _ if palette.prefer_solid => &palette.solid,
            _ => &palette.transparent,
        };
        for &pos in positions.iter().filter(|&&pos| plain(pos)) {
            changes.insert(pos, format!("minecraft:{block}"));
        }
    }
    standardize::edit(data, &changes)
}

/// The file in `data` with its roles' colours `palette`'s, each block
/// keeping its kind. By block alone, so it takes a stacked file too, after
/// stacking, which finds buses by their colours.
pub fn colour(data: &[u8], palette: &Palette) -> Result<Vec<u8>> {
    let site = Palette::default();
    if palette.colours == site.colours {
        return Ok(data.to_vec());
    }
    let blocks = Blocks::read(data)?;
    let mut changes: BTreeMap<Pos, String> = BTreeMap::new();
    for (pos, state) in blocks.iter() {
        let name = schem::short(state);
        for (key, ..) in ROLES {
            let (from, to) = (site.colour(key), palette.colour(key));
            for kind in ["_concrete", "_stained_glass"] {
                if name == format!("{from}{kind}") && from != to {
                    changes.insert(pos, format!("minecraft:{to}{kind}"));
                }
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
