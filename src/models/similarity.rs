//! How alike each build is to the others of its category, as
//! `crate::similarity` measures it, kept so a build's page can list the
//! ones it is similar to. A build is compared again whenever its file or
//! category changes.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use crate::{
    schem::Blocks,
    similarity::{self, Skeleton},
};

/// One comparison at a time, so two never write the same pairs at once.
static COMPARING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Compares every build whose file or category changed since it was last
/// compared, or that never was, with each other build of its category,
/// replacing what was kept for it.
pub async fn refresh(db: &SqlitePool) -> Result<()> {
    let _alone = COMPARING.lock().await;
    let builds = sqlx::query!(
        r#"SELECT id as "id!", category, category || '/' || sha256 as "now!: String",
            compared_as FROM schematic"#
    )
    .fetch_all(db)
    .await?;
    let stale: HashSet<&str> = builds
        .iter()
        .filter(|b| b.compared_as.as_deref() != Some(&b.now))
        .map(|b| b.id.as_str())
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    // Every build of a category with a stale one, read once.
    let categories: HashSet<&str> = builds
        .iter()
        .filter(|b| stale.contains(b.id.as_str()))
        .map(|b| b.category.as_str())
        .collect();
    let mut files = Vec::new();
    for b in builds
        .iter()
        .filter(|b| categories.contains(b.category.as_str()))
    {
        let data = crate::models::schematic::data(db, &b.id)
            .await?
            .context("schematic vanished")?;
        files.push((b.id.clone(), b.category.clone(), data));
    }
    let stale_ids: HashSet<String> =
        stale.iter().map(|id| (*id).to_owned()).collect();
    let pairs =
        tokio::task::spawn_blocking(move || compare_stale(&files, &stale_ids))
            .await??;
    let mut tx = db.begin().await?;
    for id in &stale {
        sqlx::query!(
            "DELETE FROM similarity WHERE schematic_id = ? OR other_id = ?",
            id,
            id
        )
        .execute(&mut *tx)
        .await?;
    }
    for (a, b, permille) in &pairs {
        sqlx::query!(
            "INSERT OR REPLACE INTO similarity (schematic_id, other_id, permille)
             VALUES (?, ?, ?)",
            a,
            b,
            permille
        )
        .execute(&mut *tx)
        .await?;
    }
    for b in builds.iter().filter(|b| stale.contains(b.id.as_str())) {
        sqlx::query!(
            "UPDATE schematic SET compared_as = ? WHERE id = ?",
            b.now,
            b.id
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Each pair of `files`, `(id, category, data)`, of one category with a
/// `stale` build in it, scored once, the lesser id first.
fn compare_stale(
    files: &[(String, String, Vec<u8>)],
    stale: &HashSet<String>,
) -> Result<Vec<(String, String, u32)>> {
    let skeletons: HashMap<&str, Skeleton> = files
        .iter()
        .map(|(id, _, data)| {
            Ok((id.as_str(), Skeleton::of(&Blocks::read(data)?)))
        })
        .collect::<Result<_>>()?;
    let mut pairs = Vec::new();
    for (i, (a, category, _)) in files.iter().enumerate() {
        for (b, other, _) in &files[i + 1..] {
            if category != other || !(stale.contains(a) || stale.contains(b)) {
                continue;
            }
            let (low, high) = if a < b { (a, b) } else { (b, a) };
            let permille = similarity::permille(
                &skeletons[a.as_str()],
                &skeletons[b.as_str()],
            );
            pairs.push((low.clone(), high.clone(), permille));
        }
    }
    Ok(pairs)
}

/// Compares in the background, as the site does once a build is published
/// or edited.
pub fn refresh_soon(db: &SqlitePool) {
    let db = db.clone();
    tokio::spawn(async move {
        if let Err(err) = refresh(&db).await {
            tracing::error!("comparing builds: {err:#}");
        }
    });
}

/// The builds of its category build `id` is similar to, with how alike, the
/// most alike first.
pub async fn similar(db: &SqlitePool, id: &str) -> Result<Vec<(String, u32)>> {
    let at_least = similarity::SIMILAR;
    Ok(sqlx::query!(
        r#"SELECT iif(schematic_id = ?, other_id, schematic_id) as "other!: String",
            permille as "permille!: u32"
         FROM similarity
         WHERE (schematic_id = ? OR other_id = ?) AND permille >= ?
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

/// The stored builds most like `blocks`, any category, with how alike, the
/// most alike first: at most `limit`, and none nothing alike. Every build
/// is compared, so this takes a moment for a large file.
pub async fn ranked(
    db: &SqlitePool,
    blocks: &Blocks,
    limit: usize,
) -> Result<Vec<(String, u32)>> {
    let candidates = sqlx::query!(r#"SELECT id as "id!", data FROM schematic"#)
        .fetch_all(db)
        .await?;
    let mine = Skeleton::of(blocks);
    let found = tokio::task::spawn_blocking(move || {
        // From the most a build could be alike down, so the comparing stops
        // once none left could make the list.
        let mut theirs: Vec<(String, Skeleton, u32)> = candidates
            .into_iter()
            .filter_map(|row| {
                let skeleton = Skeleton::of(&Blocks::read(&row.data).ok()?);
                let bound = mine.bound(&skeleton);
                Some((row.id, skeleton, bound))
            })
            .collect();
        theirs.sort_by_key(|(_, _, bound)| std::cmp::Reverse(*bound));
        let mut found: Vec<(String, u32)> = Vec::new();
        for (id, skeleton, bound) in theirs {
            let last = found.get(limit.saturating_sub(1)).map(|(_, p)| *p);
            if bound == 0 || (found.len() >= limit && last >= Some(bound)) {
                break;
            }
            let permille = similarity::permille(&mine, &skeleton);
            if permille > 0 {
                let at = found.partition_point(|(_, p)| *p >= permille);
                found.insert(at, (id, permille));
                found.truncate(limit);
            }
        }
        found
    })
    .await?;
    Ok(found)
}

/// The stored build most like `blocks`, with how alike, if any is similar:
/// as an upload is checked before its category is known, any build of a
/// size within half again of `size` is tried.
pub async fn closest(
    db: &SqlitePool,
    blocks: &Blocks,
    size: (i64, i64, i64),
) -> Result<Option<(String, u32)>> {
    let volume = size.0 * size.1 * size.2;
    let candidates = sqlx::query!(
        r#"SELECT id as "id!", data FROM schematic
         WHERE size_x * size_y * size_z * 3 >= ? * 2
            AND size_x * size_y * size_z * 2 <= ? * 3"#,
        volume,
        volume
    )
    .fetch_all(db)
    .await?;
    let mine = Skeleton::of(blocks);
    let found = tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .filter_map(|row| {
                let theirs = Skeleton::of(&Blocks::read(&row.data).ok()?);
                Some((row.id, similarity::permille(&mine, &theirs)))
            })
            .max_by_key(|(_, permille)| *permille)
    })
    .await?;
    Ok(found.filter(|(_, permille)| *permille >= similarity::SIMILAR))
}
