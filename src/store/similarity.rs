//! How alike builds are, as kept, and the files a comparison reads.

use sqlx::SqlitePool;

/// A build as comparing sees it: what it would be compared as now, its
/// category and its file's hash, and what it was last compared as.
pub struct Compared {
    pub id: String,
    pub category: String,
    pub now: String,
    pub compared_as: Option<String>,
}

/// Every build but deleted ones, as comparing sees it.
pub async fn compared(db: &SqlitePool) -> Result<Vec<Compared>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT build.id as "id!", category,
            category || '/' || file.sha256 as "now!: String", compared_as
         FROM build
         JOIN current_file AS file ON file.build_id = build.id
         WHERE deleted_at IS NULL"#
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Compared {
            id: row.id,
            category: row.category,
            now: row.now,
            compared_as: row.compared_as,
        })
        .collect())
}

/// The current file of every build of `categories` but deleted ones, each
/// with its id and category.
pub async fn files_of(
    db: &SqlitePool,
    categories: &[&str],
) -> Result<Vec<(String, String, Vec<u8>)>, sqlx::Error> {
    let categories =
        serde_json::to_string(categories).expect("a list of names is JSON");
    Ok(sqlx::query!(
        r#"SELECT build.id as "id!", category, file.data as "data!"
         FROM build
         JOIN current_file AS file ON file.build_id = build.id
         WHERE category IN (SELECT value FROM json_each(?))
            AND deleted_at IS NULL"#,
        categories
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| (row.id, row.category, row.data))
    .collect())
}

/// Replaces what is kept for the builds `stale` names, each with what it
/// is compared as now, by `pairs`, each two ids, the lesser first, and
/// how alike they are.
pub async fn replace(
    db: &SqlitePool,
    stale: &[(&str, &str)],
    pairs: &[(String, String, u32)],
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    for (id, _) in stale {
        sqlx::query!(
            "DELETE FROM similarity WHERE build_id = ? OR other_id = ?",
            id,
            id
        )
        .execute(&mut *tx)
        .await?;
    }
    for (a, b, permille) in pairs {
        sqlx::query!(
            "INSERT OR REPLACE INTO similarity (build_id, other_id, permille)
             VALUES (?, ?, ?)",
            a,
            b,
            permille
        )
        .execute(&mut *tx)
        .await?;
    }
    for (id, now) in stale {
        sqlx::query!("UPDATE build SET compared_as = ? WHERE id = ?", now, id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// The builds of its category build `id` is at least `at_least` alike,
/// with how alike, the most alike first.
pub async fn similar(
    db: &SqlitePool,
    id: &str,
    at_least: u32,
) -> Result<Vec<(String, u32)>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT iif(build_id = ?, other_id, build_id) as "other!: String",
            permille as "permille!: u32"
         FROM similarity
         WHERE (build_id = ? OR other_id = ?) AND permille >= ?
         ORDER BY permille DESC"#,
        id,
        id,
        id,
        at_least
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| (row.other, row.permille))
    .collect())
}

/// Every build's current file, with its id.
pub async fn current_files(
    db: &SqlitePool,
) -> Result<Vec<(String, Vec<u8>)>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT build_id as "id!", data as "data!" FROM current_file"#
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| (row.id, row.data))
    .collect())
}

/// The current file of every build but deleted ones whose size is within
/// half again of `volume` blocks, with its id.
pub async fn files_sized(
    db: &SqlitePool,
    volume: i64,
) -> Result<Vec<(String, Vec<u8>)>, sqlx::Error> {
    Ok(sqlx::query!(
        r#"SELECT id as "id!", file.data as "data!"
         FROM build
         JOIN current_file AS file ON file.build_id = build.id
         WHERE size_x * size_y * size_z * 3 >= ? * 2
            AND size_x * size_y * size_z * 2 <= ? * 3
            AND deleted_at IS NULL"#,
        volume,
        volume
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| (row.id, row.data))
    .collect())
}
