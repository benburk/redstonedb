//! Loading stored builds, each together with everything that hangs off
//! it: its tags, flags, stacking, wider builds, and credits. Also the
//! names credited, and the ids a prefix picks out.

use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

use crate::{
    build::{Build, Credits, Entry, File, Operands, Specs, Status, Widened},
    category::{self, Flag, Logic},
    interface::{Encoding, Kept, Orientation},
    models::tag::{self, Tag},
    stack::StackDirection,
};

/// Every schematic but deleted ones, ordered by category, width, then
/// insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Build>, sqlx::Error> {
    load(db, None, false).await
}

/// Every schematic listed for `viewer`, ordered as [`all`] orders them: a
/// private one is left out but for its uploader and admins.
pub async fn visible(
    db: &SqlitePool,
    viewer: Option<&crate::models::user::User>,
) -> Result<Vec<Build>, sqlx::Error> {
    let mut all = all(db).await?;
    all.retain(|s| s.listed_to(viewer));
    Ok(all)
}

/// The schematic `id`, if there is one and it is not deleted. Anyone with
/// its link may see it, private or not.
pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Build>, sqlx::Error> {
    Ok(load(db, Some(ids_json(&[id])), false).await?.pop())
}

/// The schematics `ids` name, in that order, leaving out any id that names
/// none, or a deleted one.
pub async fn get_many(
    db: &SqlitePool,
    ids: &[&str],
) -> Result<Vec<Build>, sqlx::Error> {
    get_ids(db, ids, false).await
}

/// The schematics `ids` name, deleted ones too, as the moderation log
/// shows them; in that order, leaving out any id that names none.
pub async fn get_many_with_deleted(
    db: &SqlitePool,
    ids: &[&str],
) -> Result<Vec<Build>, sqlx::Error> {
    get_ids(db, ids, true).await
}

async fn get_ids(
    db: &SqlitePool,
    ids: &[&str],
    deleted: bool,
) -> Result<Vec<Build>, sqlx::Error> {
    let mut found: HashMap<String, Build> =
        load(db, Some(ids_json(ids)), deleted)
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
/// Deleted ones are left out unless `deleted`.
#[allow(clippy::too_many_lines, reason = "a field for each column it reads")]
async fn load(
    db: &SqlitePool,
    only: Option<String>,
    deleted: bool,
) -> Result<Vec<Build>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT build.id, build.category, width, orientation
            as "orientation: Orientation", encoding as "encoding: Encoding", latency,
            synchronized as "synchronized: bool", throughput, at_least, pulse,
            torches, works_from, works_from_by, size_x, size_y, size_z,
            built_on, imported_by, file.sha256 as "sha256!", added_at,
            edited_at, downloads, keeps_paste as "keeps_paste: bool",
            public as "public: bool", unverified,
            build.logic as "own_logic: Logic",
            conventional as "conventional: bool", description,
            design, user.username as "uploader?: String",
            coalesce(user.minecraft_name, user.username) as "uploader_name?: String",
            EXISTS (SELECT 1 FROM simulation WHERE simulation.failed
                AND simulation.build_id = build.id
                AND simulation.sha256 = file.sha256) as "fails_simulation!: bool",
            (SELECT json_extract(simulation.report, '$.outcome') FROM simulation
                WHERE simulation.build_id = build.id
                AND simulation.sha256 = file.sha256) as "outcome?: String",
            (SELECT count(*) FROM favorite WHERE favorite.build_id = build.id)
                as "favorites!: i64",
            build.deleted_at IS NOT NULL as "deleted!: bool"
         FROM build
         JOIN current_file AS file ON file.build_id = build.id
         LEFT JOIN user ON user.id = build.uploaded_by
         WHERE (?1 IS NULL OR build.id IN (SELECT value FROM json_each(?1)))
            AND (?2 OR build.deleted_at IS NULL)
         ORDER BY build.category, width, added_at"#,
        only,
        deleted
    )
    .fetch_all(db)
    .await?;
    let mut tags = tags_by_schematic(db, only.as_deref()).await?;
    let mut flags = flags_by_schematic(db, only.as_deref()).await?;
    let mut stacking = stacking_by_schematic(db, only.as_deref()).await?;
    let mut widened = widened_by_schematic(db, only.as_deref()).await?;
    let mut credits = credits_by_schematic(db, only.as_deref()).await?;
    let mut layouts = layouts_by_schematic(db, only.as_deref()).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (authors, contributors) =
                credits.remove(&row.id).unwrap_or_default();
            let (operands, unaligned) =
                layouts.remove(&row.id).unwrap_or_default();
            let status = Status::of(
                row.outcome.as_deref(),
                row.fails_simulation,
                row.unverified.as_deref(),
            );
            Build {
                tags: tag::with_derived(
                    tags.remove(&row.id).unwrap_or_default(),
                    status.unverified(),
                    row.torches,
                    row.works_from.as_deref(),
                ),
                flags: flags.remove(&row.id).unwrap_or_default(),
                stacking: stacking.remove(&row.id).unwrap_or_default(),
                widened: widened.remove(&row.id).unwrap_or_default(),
                stacked_from: None,
                credits: Credits {
                    authors,
                    contributors,
                },
                category: row.category,
                specs: Specs {
                    width: row.width,
                    orientation: row.orientation,
                    encoding: row.encoding,
                    latency: row.latency,
                    synchronized: row.synchronized,
                    throughput: row.throughput,
                    at_least: row.at_least,
                    pulse: row.pulse,
                    design: row.design,
                    logic: row.own_logic,
                },
                file: File {
                    sha256: row.sha256,
                    size: (row.size_x, row.size_y, row.size_z),
                    torches: row.torches,
                    works_from: row.works_from,
                    works_from_by: row.works_from_by,
                    conventional: row.conventional,
                    keeps_paste: row.keeps_paste,
                },
                entry: Entry {
                    uploader: row.uploader,
                    uploader_name: row.uploader_name,
                    imported_by: row.imported_by,
                    added_at: row.added_at,
                    edited_at: row.edited_at,
                    downloads: row.downloads,
                    favorites: row.favorites,
                    public: row.public,
                    deleted: row.deleted,
                },
                built_on: row.built_on,
                description: row.description,
                unverified: row.unverified,
                status,
                operands,
                unaligned,
                id: row.id,
            }
        })
        .collect())
}

/// How the lines of the builds `only` names lie, by id, as each one's
/// current file's kept roles place them, for those whose operands do not
/// lie split, see `Kept::interleaved`, or whose buses are unaligned, see
/// `Kept::unaligned`: how its operands lie, and whether it is unaligned.
async fn layouts_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, (Operands, bool)>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT build_id, roles FROM current_file
         WHERE roles IS NOT NULL AND (?1 IS NULL
            OR build_id IN (SELECT value FROM json_each(?1)))",
    )
    .bind(only)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, roles)| {
            let kept = serde_json::from_str::<Kept>(&roles).ok()?;
            let operands = if kept.interleaved() {
                Operands::Interleaved
            } else {
                Operands::Split
            };
            let unaligned = kept.unaligned();
            (operands != Operands::Split || unaligned)
                .then_some((id, (operands, unaligned)))
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
        r#"SELECT build_tag.build_id, tag.name, tag.token,
            tag.kind AS "kind: tag::Kind"
         FROM build_tag JOIN tag ON tag.id = build_tag.tag_id
         WHERE ?1 IS NULL
            OR build_tag.build_id IN (SELECT value FROM json_each(?1))
         ORDER BY tag.kind = 'other', tag.kind, tag.name"#,
        only
    )
    .fetch_all(db)
    .await?
    {
        tags.entry(row.build_id).or_default().push(Tag {
            name: row.name,
            token: row.token,
            kind: row.kind,
        });
    }
    Ok(tags)
}

/// The flags of the builds `only` names, by id, in their category's order;
/// a name no flag of its category has is none.
async fn flags_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Flag>>, sqlx::Error> {
    let mut names: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for row in sqlx::query!(
        "SELECT build_flag.build_id, build.category,
            build_flag.name
         FROM build_flag
         JOIN build ON build.id = build_flag.build_id
         WHERE ?1 IS NULL
            OR build_flag.build_id IN (SELECT value FROM json_each(?1))",
        only
    )
    .fetch_all(db)
    .await?
    {
        names
            .entry(row.build_id)
            .or_insert_with(|| (row.category, Vec::new()))
            .1
            .push(row.name);
    }
    Ok(names
        .into_iter()
        .map(|(id, (category, names))| {
            let flags = category::flags_among(&category, &names);
            (id, flags.into_iter().copied().collect())
        })
        .collect())
}

/// The stacking directions of the builds `only` names, by id, the most
/// westward first.
async fn stacking_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<StackDirection>>, sqlx::Error> {
    let mut stacking: HashMap<String, Vec<StackDirection>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT build_id, west, up, max_copies, faces, pitch
         FROM stack_direction
         WHERE ?1 IS NULL
            OR build_id IN (SELECT value FROM json_each(?1))
         ORDER BY west DESC, up DESC",
        only
    )
    .fetch_all(db)
    .await?
    {
        stacking
            .entry(row.build_id)
            .or_default()
            .push(StackDirection {
                west: row.west,
                up: row.up,
                max_copies: row.max_copies,
                faces: row
                    .faces
                    .and_then(|faces| serde_json::from_str(&faces).ok()),
                pitch: row.pitch,
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
        r#"SELECT build_id, width, copies, latency,
            synchronized as "synchronized: bool", throughput
         FROM stacked_width
         WHERE ?1 IS NULL
            OR build_id IN (SELECT value FROM json_each(?1))
         ORDER BY width"#,
        only
    )
    .fetch_all(db)
    .await?
    {
        widened.entry(row.build_id).or_default().push(Widened {
            width: row.width,
            copies: row.copies,
            latency: row.latency,
            synchronized: row.synchronized,
            throughput: row.throughput,
        });
    }
    Ok(widened)
}

/// The authors and contributors of the builds `only` names, by id, each
/// in the order given.
async fn credits_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, (Vec<String>, Vec<String>)>, sqlx::Error> {
    let mut credits: HashMap<String, (Vec<String>, Vec<String>)> =
        HashMap::new();
    for row in sqlx::query!(
        "SELECT build_id, role, name FROM credit
         WHERE ?1 IS NULL
            OR build_id IN (SELECT value FROM json_each(?1))
         ORDER BY build_id, position",
        only
    )
    .fetch_all(db)
    .await?
    {
        let (authors, contributors) = credits.entry(row.build_id).or_default();
        if row.role == Credit::Author.as_str() {
            authors.push(row.name);
        } else {
            contributors.push(row.name);
        }
    }
    Ok(credits)
}

/// What a person did for a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Credit {
    Author,
    /// Helped, such as with the design or testing, without building it.
    Contributor,
}

impl Credit {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Credit::Author => "author",
            Credit::Contributor => "contributor",
        }
    }
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
        "SELECT id FROM build
         WHERE id GLOB ? || '*' AND deleted_at IS NULL LIMIT 2",
        prefix
    )
    .fetch_all(db)
    .await
}
