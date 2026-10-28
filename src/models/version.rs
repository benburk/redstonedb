//! Each file a build has had: its versions, numbered from 1, the highest
//! being the one it has now. This is the only place files are kept.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use crate::import::Prepared;

/// One version, without its file.
pub struct Version {
    pub version: i64,
    pub added_at: String,
    /// The uploader as people know them: Minecraft name, else username.
    pub added_by: Option<String>,
    /// What it changed; `None` for the first.
    pub note: Option<String>,
}

/// Build `id`'s versions, newest first.
pub async fn list<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Vec<Version>, sqlx::Error> {
    sqlx::query_as!(
        Version,
        r#"SELECT version, schematic_version.added_at,
            coalesce(user.minecraft_name, user.username) as "added_by?: String",
            note
         FROM schematic_version LEFT JOIN user ON user.id = added_by
         WHERE schematic_id = ? ORDER BY version DESC"#,
        id
    )
    .fetch_all(db)
    .await
}

/// The file of build `id`'s version `version`.
pub async fn data<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    version: i64,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT data FROM schematic_version
         WHERE schematic_id = ? AND version = ?",
        id,
        version
    )
    .fetch_optional(db)
    .await
}

/// Build `id`'s current file: its highest version's.
pub async fn current_file<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT data as "data!" FROM current_file WHERE schematic_id = ?"#,
        id
    )
    .fetch_optional(db)
    .await
}

/// The build and version whose file has the hash `sha256`, if any is
/// stored: each file is stored once.
pub async fn holder<'e, E: SqliteExecutor<'e>>(
    db: E,
    sha256: &str,
) -> Result<Option<(String, i64)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT schematic_id, version FROM schematic_version WHERE sha256 = ?",
        sha256
    )
    .fetch_optional(db)
    .await?
    .map(|row| (row.schematic_id, row.version)))
}

/// Records a new build's file as its first version.
pub(super) async fn first(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    file: &Prepared,
    added_by: Option<Uuid>,
    added_from: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO schematic_version
            (schematic_id, version, data, sha256, added_by, added_from)
         VALUES (?, 1, ?, ?, ?, ?)",
        id,
        file.data,
        file.sha256,
        added_by,
        added_from
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// How a new file takes the place of a build's current one.
#[derive(Clone, Copy)]
pub enum Replacing<'a> {
    /// As its next version, uploaded by `added_by` from IP address
    /// `added_from` with a `note` saying what it changed. The build counts
    /// as edited.
    NextVersion {
        added_by: Option<Uuid>,
        added_from: Option<&'a str>,
        note: Option<&'a str>,
    },
    /// In its current version's place, as `rsdb reread` and `rsdb replace`
    /// fix a file: the same build, tidied, with no version added.
    InPlace,
}

/// Makes `file` build `id`'s current file, as `how` says, together with
/// what is measured and read from it, and returns the number of the
/// version that holds it.
pub async fn replace(
    db: &SqlitePool,
    id: &str,
    file: &Prepared,
    how: Replacing<'_>,
) -> Result<i64, sqlx::Error> {
    let mut tx = db.begin().await?;
    let current = sqlx::query_scalar!(
        r#"SELECT coalesce(max(version), 0) as "version!: i64"
         FROM schematic_version WHERE schematic_id = ?"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    let (version, edited) = match how {
        Replacing::NextVersion {
            added_by,
            added_from,
            note,
        } => {
            let version = current + 1;
            sqlx::query!(
                "INSERT INTO schematic_version
                    (schematic_id, version, data, sha256, added_by,
                    added_from, note)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                id,
                version,
                file.data,
                file.sha256,
                added_by,
                added_from,
                note
            )
            .execute(&mut *tx)
            .await?;
            (version, true)
        }
        Replacing::InPlace => {
            sqlx::query!(
                "UPDATE schematic_version SET data = ?, sha256 = ?
                 WHERE schematic_id = ? AND version = ?",
                file.data,
                file.sha256,
                id,
                current
            )
            .execute(&mut *tx)
            .await?;
            (current, false)
        }
    };
    let (size, facts) = (&file.size, &file.facts);
    let (x, y, z) = (size.x, size.y, size.z);
    sqlx::query!(
        "UPDATE schematic SET size_x = ?, size_y = ?, size_z = ?,
            conventional = ?, torches = ?, works_from = ?, works_from_by = ?,
            edited_at = iif(?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                edited_at)
         WHERE id = ?",
        x,
        y,
        z,
        file.conventional,
        facts.torches,
        facts.works_from.release,
        facts.works_from.because,
        edited,
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(version)
}
