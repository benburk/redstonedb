//! Loading stored builds, each together with everything that hangs off
//! it: its tags, flags, stacking, wider builds, and credits. Also the
//! names credited, and the ids a prefix picks out.

use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

use super::{Encoding, Logic, Orientation, Schematic, StackDirection, Widened};
use crate::models::{
    catalogue::{self, Flag},
    tag::{self, Tag},
};

/// Every schematic, ordered by category, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    load(db, None).await
}

/// Every schematic `viewer` may see, ordered as [`all`] orders them.
pub async fn visible(
    db: &SqlitePool,
    viewer: Option<&crate::models::user::User>,
) -> Result<Vec<Schematic>, sqlx::Error> {
    let mut all = all(db).await?;
    all.retain(|s| s.visible_to(viewer));
    Ok(all)
}

/// The schematic `id`, if there is one and `viewer` may see it.
pub async fn get_visible(
    db: &SqlitePool,
    id: &str,
    viewer: Option<&crate::models::user::User>,
) -> Result<Option<Schematic>, sqlx::Error> {
    Ok(get(db, id).await?.filter(|s| s.visible_to(viewer)))
}

/// The schematic `id`, if there is one.
pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    Ok(load(db, Some(ids_json(&[id]))).await?.pop())
}

/// The schematics `ids` name, in that order, leaving out any id that names
/// none.
pub async fn get_many(
    db: &SqlitePool,
    ids: &[&str],
) -> Result<Vec<Schematic>, sqlx::Error> {
    let mut found: HashMap<String, Schematic> = load(db, Some(ids_json(ids)))
        .await?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    Ok(ids.iter().filter_map(|&id| found.remove(id)).collect())
}

/// `ids` as a JSON array, as the loader's queries take a list.
fn ids_json(ids: &[&str]) -> String {
    serde_json::to_string(ids).expect("a list of strings is JSON")
}

/// The schematics `only` names, as a JSON array of ids, or every one for
/// `None`, ordered by category, width, then insertion: each row with its
/// tags, flags, stacking, credits, and its category's measures and aliases.
async fn load(
    db: &SqlitePool,
    only: Option<String>,
) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT schematic.id, schematic.category, width, orientation
            as "orientation: Orientation", encoding as "encoding: Encoding", latency,
            synchronized as "synchronized: bool", throughput, at_least, pulse,
            bytes, torches, works_from, works_from_by, size_x, size_y, size_z,
            built_on, imported_by, file.sha256 as "sha256!", added_at,
            edited_at, downloads, keeps_paste as "keeps_paste: bool",
            public as "public: bool", unverified,
            schematic.logic as "own_logic: Logic",
            coalesce(schematic.logic, category.logic) as "logic?: Logic",
            conventional as "conventional: bool", description, given_name,
            design, design.abbreviation as "design_abbreviation?: String",
            design.label as "design_given_label?: String", category.label
                as "category_label?: String",
            user.username as "uploader?: String", coalesce(user.minecraft_name,
                user.username) as "uploader_name?: String",
            EXISTS (SELECT 1 FROM simulation WHERE simulation.failed
                AND simulation.schematic_id = schematic.id
                AND simulation.sha256 = file.sha256) as "fails_simulation!: bool"
         FROM schematic
         JOIN current_file AS file ON file.schematic_id = schematic.id
         LEFT JOIN category ON category.name = schematic.category
         LEFT JOIN design ON design.category = schematic.category
            AND design.name = schematic.design
         LEFT JOIN user ON user.id = schematic.uploaded_by
         WHERE ?1 IS NULL OR schematic.id IN (SELECT value FROM json_each(?1))
         ORDER BY schematic.category, width, added_at"#,
        only
    )
    .fetch_all(db)
    .await?;
    let mut tags = tags_by_schematic(db, only.as_deref()).await?;
    let mut flags = flags_by_schematic(db, only.as_deref()).await?;
    let mut stacking = stacking_by_schematic(db, only.as_deref()).await?;
    let mut widened = widened_by_schematic(db, only.as_deref()).await?;
    let mut credits = credits_by_schematic(db, only.as_deref()).await?;
    let extras = catalogue::Extras::load(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (builders, contributors) =
                credits.remove(&row.id).unwrap_or_default();
            Schematic {
                measures: extras.measures(&row.category),
                aliases: extras.aliases(&row.category),
                tags: tag::with_derived(
                    tags.remove(&row.id).unwrap_or_default(),
                    tag::unverified(
                        row.fails_simulation,
                        row.unverified.as_deref(),
                    ),
                    row.torches,
                    row.works_from.as_deref(),
                ),
                flags: flags.remove(&row.id).unwrap_or_default(),
                stacking: stacking.remove(&row.id).unwrap_or_default(),
                widened: widened.remove(&row.id).unwrap_or_default(),
                stacked_from: None,
                builders,
                contributors,
                id: row.id,
                category: row.category,
                category_label: row.category_label,
                width: row.width,
                orientation: row.orientation,
                encoding: row.encoding,
                latency: row.latency,
                synchronized: row.synchronized,
                logic: row.logic,
                own_logic: row.own_logic,
                throughput: row.throughput,
                at_least: row.at_least,
                pulse: row.pulse,
                torches: row.torches,
                works_from: row.works_from,
                works_from_by: row.works_from_by,
                bytes: row.bytes,
                size: (row.size_x, row.size_y, row.size_z),
                built_on: row.built_on,
                imported_by: row.imported_by,
                conventional: row.conventional,
                keeps_paste: row.keeps_paste,
                design: row.design,
                design_abbreviation: row.design_abbreviation,
                design_given_label: row.design_given_label,
                description: row.description,
                given_name: row.given_name,
                uploader: row.uploader,
                uploader_name: row.uploader_name,
                sha256: row.sha256,
                added_at: row.added_at,
                edited_at: row.edited_at,
                downloads: row.downloads,
                public: row.public,
                unverified: row.unverified,
            }
        })
        .collect())
}

/// The stored tags of the builds `only` names, as `load` takes it, by id,
/// by kind then name.
async fn tags_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Tag>>, sqlx::Error> {
    let mut tags: HashMap<String, Vec<Tag>> = HashMap::new();
    for row in sqlx::query!(
        r#"SELECT schematic_tag.schematic_id, tag.name, tag.token,
            tag.kind AS "kind: tag::Kind"
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         WHERE ?1 IS NULL
            OR schematic_tag.schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY tag.kind = 'other', tag.kind, tag.name"#,
        only
    )
    .fetch_all(db)
    .await?
    {
        tags.entry(row.schematic_id).or_default().push(Tag {
            name: row.name,
            token: row.token,
            kind: row.kind,
        });
    }
    Ok(tags)
}

/// The flags of the builds `only` names, by id, in their category's order.
async fn flags_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Flag>>, sqlx::Error> {
    let mut flags: HashMap<String, Vec<Flag>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_flag.schematic_id, flag.category, flag.name,
            flag.label
         FROM schematic_flag
         JOIN schematic ON schematic.id = schematic_flag.schematic_id
         JOIN flag ON flag.category = schematic.category
            AND flag.name = schematic_flag.name
         WHERE ?1 IS NULL
            OR schematic_flag.schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY flag.position",
        only
    )
    .fetch_all(db)
    .await?
    {
        flags.entry(row.schematic_id).or_default().push(Flag {
            category: row.category,
            name: row.name,
            label: row.label,
        });
    }
    Ok(flags)
}

/// The stacking directions of the builds `only` names, by id, the most
/// westward first.
async fn stacking_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<StackDirection>>, sqlx::Error> {
    let mut stacking: HashMap<String, Vec<StackDirection>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, west, up, north, max_copies, faces
         FROM stack_direction
         WHERE ?1 IS NULL
            OR schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY west DESC, up DESC, north DESC",
        only
    )
    .fetch_all(db)
    .await?
    {
        stacking
            .entry(row.schematic_id)
            .or_default()
            .push(StackDirection {
                west: row.west,
                up: row.up,
                north: row.north,
                max_copies: row.max_copies,
                faces: row
                    .faces
                    .and_then(|faces| serde_json::from_str(&faces).ok()),
            });
    }
    Ok(stacking)
}

/// The wider builds the copies of the builds `only` names make, by id,
/// narrowest first.
async fn widened_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Widened>>, sqlx::Error> {
    let mut widened: HashMap<String, Vec<Widened>> = HashMap::new();
    for row in sqlx::query!(
        r#"SELECT schematic_id, width, copies, latency,
            synchronized as "synchronized: bool", throughput
         FROM stacked_width
         WHERE ?1 IS NULL
            OR schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY width"#,
        only
    )
    .fetch_all(db)
    .await?
    {
        widened.entry(row.schematic_id).or_default().push(Widened {
            width: row.width,
            copies: row.copies,
            latency: row.latency,
            synchronized: row.synchronized,
            throughput: row.throughput,
        });
    }
    Ok(widened)
}

/// The builders and contributors of the builds `only` names, by id, each
/// in the order given.
async fn credits_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, (Vec<String>, Vec<String>)>, sqlx::Error> {
    let mut credits: HashMap<String, (Vec<String>, Vec<String>)> =
        HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, role, name FROM credit
         WHERE ?1 IS NULL
            OR schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY schematic_id, position",
        only
    )
    .fetch_all(db)
    .await?
    {
        let (builders, contributors) =
            credits.entry(row.schematic_id).or_default();
        if row.role == Credit::Builder.as_str() {
            builders.push(row.name);
        } else {
            contributors.push(row.name);
        }
    }
    Ok(credits)
}

/// What a person did for a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Credit {
    Builder,
    /// Helped, such as with the design or testing, without building it.
    Contributor,
}

impl Credit {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Credit::Builder => "builder",
            Credit::Contributor => "contributor",
        }
    }
}

/// Whether a name can be credited: a Minecraft name of letters, digits,
/// and underscores.
pub fn valid_player_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Everyone credited on a stored build, by name.
pub async fn all_people<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT DISTINCT name FROM credit ORDER BY name")
        .fetch_all(db)
        .await
}

/// Ids starting with `prefix`; at most two, enough to detect ambiguity.
pub async fn ids_with_prefix<'e, E: SqliteExecutor<'e>>(
    db: E,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM schematic WHERE id GLOB ? || '*' LIMIT 2",
        prefix
    )
    .fetch_all(db)
    .await
}
