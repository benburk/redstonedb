//! Each file a build has had: its versions, numbered from 1, the highest
//! being the one it has now.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

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

/// Records a build's first version, as it is stored.
pub async fn first<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    data: &[u8],
    sha256: &str,
    added_by: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO schematic_version
            (schematic_id, version, data, sha256, added_by)
         VALUES (?, 1, ?, ?, ?)",
        id,
        data,
        sha256,
        added_by
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A new version's file, measured.
pub struct New<'a> {
    pub data: &'a [u8],
    pub sha256: &'a str,
    pub size: (i64, i64, i64),
    pub added_by: Option<Uuid>,
    pub note: Option<&'a str>,
}

/// Makes `new` build `id`'s file, as its next version, and returns that
/// version's number. The build counts as edited.
pub async fn publish(
    db: &SqlitePool,
    id: &str,
    new: &New<'_>,
) -> Result<i64, sqlx::Error> {
    let mut tx = db.begin().await?;
    let version = sqlx::query_scalar!(
        r#"SELECT coalesce(max(version), 0) + 1 as "version!: i64"
         FROM schematic_version WHERE schematic_id = ?"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO schematic_version
            (schematic_id, version, data, sha256, added_by, note)
         VALUES (?, ?, ?, ?, ?, ?)",
        id,
        version,
        new.data,
        new.sha256,
        new.added_by,
        new.note
    )
    .execute(&mut *tx)
    .await?;
    let (x, y, z) = new.size;
    sqlx::query!(
        "UPDATE schematic SET data = ?, sha256 = ?, size_x = ?, size_y = ?,
            size_z = ?, edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
        new.data,
        new.sha256,
        x,
        y,
        z,
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(version)
}

/// Rewrites build `id`'s current version's file, as `rsdb reread` does
/// when the rules for fitting a file change: the same build, tidied.
pub async fn rewrite_current<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    data: &[u8],
    sha256: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic_version SET data = ?, sha256 = ?
         WHERE schematic_id = ? AND version =
            (SELECT max(version) FROM schematic_version
             WHERE schematic_id = ?)",
        data,
        sha256,
        id,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}
