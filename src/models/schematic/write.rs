//! The writes that change stored builds: storing a new one, its details,
//! credits, tags, flags, and stacking, what its file and simulation decide,
//! whether it is public, its downloads, and deleting it; and the builds
//! people favourite.

use sqlx::{SqliteExecutor, SqlitePool};

use super::{
    Build, Encoding, Logic, Orientation, StackDirection, Widened, load::Credit,
};
use crate::{
    details::Ready,
    import::Prepared,
    models::{tag, version},
};

/// Makes build `id` public or private.
pub async fn set_public<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    public: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE schematic SET public = ? WHERE id = ?", public, id)
        .execute(db)
        .await?;
    Ok(())
}

/// Whether `user` has favourited build `id`.
pub async fn favorited(
    db: &SqlitePool,
    user: uuid::Uuid,
    id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM favorite
            WHERE user_id = ? AND schematic_id = ?) AS "found!: bool""#,
        user,
        id
    )
    .fetch_one(db)
    .await
}

/// How many people have favourited build `id`.
pub async fn favorite_count(
    db: &SqlitePool,
    id: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!: i64" FROM favorite WHERE schematic_id = ?"#,
        id
    )
    .fetch_one(db)
    .await
}

/// Favourites build `id` for `user`, or takes it back when `on` is false.
pub async fn set_favorite(
    db: &SqlitePool,
    user: uuid::Uuid,
    id: &str,
    on: bool,
) -> Result<(), sqlx::Error> {
    if on {
        sqlx::query!(
            "INSERT OR IGNORE INTO favorite (user_id, schematic_id) VALUES (?, ?)",
            user,
            id
        )
        .execute(db)
        .await?;
    } else {
        sqlx::query!(
            "DELETE FROM favorite WHERE user_id = ? AND schematic_id = ?",
            user,
            id
        )
        .execute(db)
        .await?;
    }
    Ok(())
}

/// The ids of the builds `user` has favourited, newest first, but deleted
/// ones.
pub async fn favorites(
    db: &SqlitePool,
    user: uuid::Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT schematic_id FROM favorite
         JOIN schematic ON schematic.id = favorite.schematic_id
         WHERE user_id = ? AND deleted_at IS NULL
         ORDER BY favorite.added_at DESC",
        user
    )
    .fetch_all(db)
    .await
}
/// A new build: its details as checked, and its file as prepared.
#[derive(Clone, Copy)]
pub struct New<'a> {
    pub ready: &'a Ready,
    pub file: &'a Prepared,
    pub uploaded_by: Option<uuid::Uuid>,
    /// The IP address it was uploaded from.
    pub uploaded_from: Option<&'a str>,
    /// The in-game name of whoever imported it with `rsdb`.
    pub imported_by: Option<&'a str>,
    /// The id to store it under, as a build rebuilt from another copy of
    /// the database keeps its own; `None` makes one.
    pub id: Option<&'a str>,
}

/// Stores a new build, with its file as its first version, and returns its
/// id, the one given or a generated one.
pub async fn insert(
    db: &SqlitePool,
    new: &New<'_>,
) -> Result<String, sqlx::Error> {
    let (details, file) = (&new.ready.details, new.file);
    let (size, facts) = (&file.size, &file.facts);
    let (x, y, z) = (size.x, size.y, size.z);
    let mut tx = db.begin().await?;
    let id = sqlx::query_scalar!(
        r#"INSERT INTO schematic (id, category, width, orientation, encoding,
            latency, synchronized, size_x, size_y, size_z, built_on,
            imported_by, conventional, design, description, given_name,
            logic, throughput, at_least, pulse, bytes, torches, works_from,
            works_from_by, keeps_paste, uploaded_by, unverified)
         VALUES (coalesce(?, lower(hex(randomblob(16)))), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            ?, ?, ?, ?)
         RETURNING id as "id!""#,
        new.id,
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        x,
        y,
        z,
        details.built_on,
        new.imported_by,
        file.conventional,
        details.design,
        details.description,
        details.given_name,
        details.logic,
        details.throughput,
        details.at_least,
        details.pulse,
        details.bytes,
        facts.torches,
        facts.works_from.release,
        facts.works_from.because,
        file.keeps_paste,
        new.uploaded_by,
        details.unverified,
    )
    .fetch_one(&mut *tx)
    .await?;
    version::first(&mut tx, &id, file, new.uploaded_by, new.uploaded_from)
        .await?;
    set_lists(&mut tx, &id, new.ready).await?;
    tx.commit().await?;
    Ok(id)
}

/// Writes checked details over build `id`'s, with its credits, tags, and
/// flags, and its stacking unless the simulation decides it. A build whose
/// every measured detail the simulation works out keeps those as it set
/// them, as its result on the same file stands and is not run again. The
/// build counts as edited.
pub async fn set_details(
    db: &SqlitePool,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    let mut tx = db.begin().await?;
    if ready.evaluated {
        sqlx::query!(
            "UPDATE schematic SET category = ?, built_on = ?,
                description = ?, given_name = ?, at_least = ?, bytes = ?,
                unverified = NULL,
                edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?",
            details.category,
            details.built_on,
            details.description,
            details.given_name,
            details.at_least,
            details.bytes,
            id,
        )
        .execute(&mut *tx)
        .await?;
        set_people(&mut tx, id, ready).await?;
        return tx.commit().await;
    }
    sqlx::query!(
        "UPDATE schematic SET category = ?, width = ?, orientation = ?,
            encoding = ?, latency = ?, synchronized = ?, design = ?,
            built_on = ?, description = ?, given_name = ?, logic = ?,
            throughput = ?, at_least = ?, pulse = ?, bytes = ?, unverified = ?,
            edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        details.design,
        details.built_on,
        details.description,
        details.given_name,
        details.logic,
        details.throughput,
        details.at_least,
        details.pulse,
        details.bytes,
        details.unverified,
        id,
    )
    .execute(&mut *tx)
    .await?;
    set_lists(&mut tx, id, ready).await?;
    tx.commit().await
}

/// Replaces build `id`'s credits, tags, and flags with `ready`'s, and its
/// stacking unless the simulation decides it.
async fn set_lists(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    if !ready.stacking_inferred {
        set_stacking(tx, id, &details.stacking).await?;
    }
    set_flags(tx, id, &details.flags).await?;
    set_people(tx, id, ready).await
}

/// Replaces build `id`'s credits and tags with `ready`'s.
async fn set_people(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    let credits = [
        (Credit::Author, &details.authors),
        (Credit::Contributor, &details.contributors),
    ];
    for (credit, names) in credits {
        set_credits(tx, id, credit, names).await?;
    }
    tag::set(tx, id, &details.tags).await
}

/// A detail the moderation log can put back as an edit left it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restorable {
    Description,
    Authors,
    Contributors,
}

impl Restorable {
    /// The detail the log names `name`, if it can be put back.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "description" => Some(Self::Description),
            "authors" => Some(Self::Authors),
            "contributors" => Some(Self::Contributors),
            _ => None,
        }
    }
}

/// Sets build `id`'s `detail` to `value`, as the log keeps it: a
/// description whole, or names joined by `, `; `None` for none. The build
/// counts as edited.
pub async fn restore(
    db: &SqlitePool,
    id: &str,
    detail: Restorable,
    value: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    let names = || -> Vec<String> {
        value
            .map(|v| v.split(", ").map(str::to_owned).collect())
            .unwrap_or_default()
    };
    match detail {
        Restorable::Description => {
            sqlx::query!(
                "UPDATE schematic SET description = ? WHERE id = ?",
                value,
                id
            )
            .execute(&mut *tx)
            .await?;
        }
        Restorable::Authors => {
            set_credits(&mut tx, id, Credit::Author, &names()).await?;
        }
        Restorable::Contributors => {
            set_credits(&mut tx, id, Credit::Contributor, &names()).await?;
        }
    }
    sqlx::query!(
        "UPDATE schematic
         SET edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Replaces the people a build credits in one role with `names`, in that
/// order. A name given twice, in any case, is credited once.
async fn set_credits(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    credit: Credit,
    names: &[String],
) -> Result<(), sqlx::Error> {
    let role = credit.as_str();
    sqlx::query!(
        "DELETE FROM credit WHERE schematic_id = ? AND role = ?",
        id,
        role
    )
    .execute(&mut *tx)
    .await?;
    for (position, name) in (0_i64..).zip(names) {
        // A name an admin has confirmed for an account is stored as that
        // account spells it.
        sqlx::query!(
            "INSERT OR IGNORE INTO credit (schematic_id, role, name, position)
             VALUES (?, ?, coalesce(
                (SELECT minecraft_name FROM user WHERE minecraft_verified
                    AND minecraft_name = ? COLLATE NOCASE),
                ?), ?)",
            id,
            role,
            name,
            name,
            position
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the directions a build stacks along; none makes it unstackable.
pub async fn set_stacking(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    directions: &[StackDirection],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM stack_direction WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for d in directions {
        let faces = d
            .faces
            .map(|faces| serde_json::to_string(&faces).unwrap_or_default());
        sqlx::query!(
            "INSERT INTO stack_direction
                (schematic_id, west, up, max_copies, faces, pitch)
             VALUES (?, ?, ?, ?, ?, ?)",
            id,
            d.west,
            d.up,
            d.max_copies,
            faces,
            d.pitch,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the wider builds a build's copies make with `widened`.
pub async fn set_widened(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    widened: &[Widened],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM stacked_width WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for w in widened {
        sqlx::query!(
            "INSERT INTO stacked_width
                (schematic_id, width, copies, latency, synchronized,
                 throughput)
             VALUES (?, ?, ?, ?, ?, ?)",
            id,
            w.width,
            w.copies,
            w.latency,
            w.synchronized,
            w.throughput,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the flags a build has with those of `names` its category has,
/// as a subtractor has no subtract flag, its category saying so.
pub async fn set_flags(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    names: &[String],
) -> Result<(), sqlx::Error> {
    let category =
        sqlx::query_scalar!("SELECT category FROM schematic WHERE id = ?", id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query!("DELETE FROM schematic_flag WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for flag in crate::category::flags_among(&category, names) {
        sqlx::query!(
            "INSERT INTO schematic_flag (schematic_id, name) VALUES (?, ?)",
            id,
            flag.name
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// What a build's file and its simulation decide of its details, rather
/// than a person: its width, orientation, and encoding as its file shows
/// them; its latency, whether it is synchronized, a monostable's pulse, and
/// a threshold's count as the simulation measured them; and, for a category
/// whose details are all evaluated, its throughput, design, and whether it
/// holds state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub width: Option<i64>,
    pub orientation: Orientation,
    pub encoding: Option<Encoding>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    /// A monostable's pulse, in ticks.
    pub pulse: Option<i64>,
    /// A threshold's count of A's bits on that turns it on.
    pub at_least: Option<i64>,
    pub throughput: Option<i64>,
    /// One of its category's designs; `None` for none.
    pub design: Option<String>,
    /// Whether it holds state, where set on the build itself; `None`
    /// follows its category.
    pub logic: Option<Logic>,
}

impl Measured {
    /// Build `s`'s, as stored.
    pub fn of(s: &Build) -> Self {
        Measured {
            width: s.width,
            orientation: s.orientation,
            encoding: s.encoding,
            latency: s.latency,
            synchronized: s.synchronized,
            pulse: s.pulse,
            at_least: s.at_least,
            throughput: s.throughput,
            design: s.design.clone(),
            logic: s.own_logic,
        }
    }
}

/// Sets what build `id`'s file and simulation decide, `measured`, none of
/// it a person's edit.
pub async fn set_measured<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    measured: &Measured,
) -> Result<(), sqlx::Error> {
    let logic = measured.logic.map(Logic::name);
    sqlx::query!(
        "UPDATE schematic SET width = ?, orientation = ?, encoding = ?,
            latency = ?, synchronized = ?, pulse = ?, at_least = ?,
            throughput = ?, design = ?, logic = ?
         WHERE id = ?",
        measured.width,
        measured.orientation,
        measured.encoding,
        measured.latency,
        measured.synchronized,
        measured.pulse,
        measured.at_least,
        measured.throughput,
        measured.design,
        logic,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets the fields only `rsdb set` changes once a build is stored: who
/// imported it, and whether it follows the building conventions.
pub async fn set_import_details<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    imported_by: Option<&str>,
    conventional: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET imported_by = ?, conventional = ? WHERE id = ?",
        imported_by,
        conventional,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Counts one more download of a schematic.
pub async fn count_download<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET downloads = downloads + 1 WHERE id = ?",
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// What a download took of a build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadKind {
    /// Its current file, whole or stacked.
    File,
    /// A stacking part of its current file.
    Part,
    /// An earlier version's file.
    Version,
    /// Its current file, sent to the account's Schemati.
    Schemati,
    /// One side of a diff between two versions.
    Diff,
}

impl DownloadKind {
    const fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Part => "part",
            Self::Version => "version",
            Self::Schemati => "schemati",
            Self::Diff => "diff",
        }
    }
}

/// Keeps a record of `user` taking schematic `id` as `kind`: version
/// `version`, or the current one when `None`.
pub async fn log_download<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    user: uuid::Uuid,
    version: Option<i64>,
    kind: DownloadKind,
) -> Result<(), sqlx::Error> {
    let kind = kind.name();
    sqlx::query!(
        "INSERT INTO download (schematic_id, user_id, version, kind)
         VALUES (?1, ?2, coalesce(?3, (SELECT version FROM current_file
             WHERE schematic_id = ?1)), ?4)",
        id,
        user,
        version,
        kind
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Deletes build `id` for `by`: it is hidden from everyone, but kept, with
/// the delete logged, for an admin to restore. False when it was deleted
/// already, or there is no such build.
pub async fn delete(
    db: &SqlitePool,
    id: &str,
    by: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    hide(db, id, by, true).await
}

/// Restores deleted build `id` for `by`, logging that. False when it was
/// not deleted, or there is no such build.
pub async fn undelete(
    db: &SqlitePool,
    id: &str,
    by: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    hide(db, id, by, false).await
}

/// Deletes build `id`, or restores it when `hidden` is false, logging it
/// for `by` when that changed anything.
async fn hide(
    db: &SqlitePool,
    id: &str,
    by: uuid::Uuid,
    hidden: bool,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let changed = sqlx::query!(
        "UPDATE schematic
         SET deleted_at = iif(?2, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), NULL)
         WHERE id = ?1 AND (deleted_at IS NULL) = ?2",
        id,
        hidden
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    if changed {
        let restored = !hidden;
        sqlx::query!(
            "INSERT INTO deletion (schematic_id, user_id, restored)
             VALUES (?, ?, ?)",
            id,
            by,
            restored
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(changed)
}

/// Removes a schematic for good, with its versions and everything else of
/// it, deleted or not, leaving nothing to restore.
pub async fn purge<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic WHERE id = ?", id)
        .execute(db)
        .await?;
    Ok(())
}
