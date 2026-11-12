//! How alike each build is to the others of its category, as
//! `crate::compare::similarity` measures it, kept so a build's page can
//! list the ones it is similar to. A build is compared again whenever its
//! file or category changes.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use sqlx::SqlitePool;

use crate::{
    compare::similarity::{self, Skeleton},
    schem::Blocks,
    store::similarity as store,
};

/// One comparison at a time, so two never write the same pairs at once.
static COMPARING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Compares every build whose file or category changed since it was last
/// compared, or that never was, with each other build of its category,
/// replacing what was kept for it.
pub async fn refresh(db: &SqlitePool) -> Result<()> {
    let _alone = COMPARING.lock().await;
    let builds = store::compared(db).await?;
    let stale: Vec<(&str, &str)> = builds
        .iter()
        .filter(|b| b.compared_as.as_deref() != Some(&b.now))
        .map(|b| (b.id.as_str(), b.now.as_str()))
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    let stale_ids: HashSet<String> =
        stale.iter().map(|(id, _)| (*id).to_owned()).collect();
    // Every build of a category with a stale one, read at once.
    let mut categories: Vec<&str> = builds
        .iter()
        .filter(|b| stale_ids.contains(&b.id))
        .map(|b| b.category.as_str())
        .collect();
    categories.sort_unstable();
    categories.dedup();
    let files = store::files_of(db, &categories).await?;
    let pairs =
        tokio::task::spawn_blocking(move || compare_stale(&files, &stale_ids))
            .await??;
    store::replace(db, &stale, &pairs).await?;
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
    Ok(store::similar(db, id, similarity::SIMILAR).await?)
}

/// The stored builds most like `blocks`, any category, with how alike, the
/// most alike first: at most `limit`, and none nothing alike. Every build
/// is compared, so this takes a moment for a large file.
pub async fn ranked(
    db: &SqlitePool,
    blocks: &Blocks,
    limit: usize,
) -> Result<Vec<(String, u32)>> {
    let candidates = store::current_files(db).await?;
    let mine = Skeleton::of(blocks);
    let found = tokio::task::spawn_blocking(move || {
        // From the most a build could be alike down, so the comparing stops
        // once none left could make the list.
        let mut theirs: Vec<(String, Skeleton, u32)> = candidates
            .into_iter()
            .filter_map(|(id, data)| {
                let skeleton = Skeleton::of(&Blocks::read(&data).ok()?);
                let bound = mine.bound(&skeleton);
                Some((id, skeleton, bound))
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
    let candidates = store::files_sized(db, volume).await?;
    let mine = Skeleton::of(blocks);
    let found = tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .filter_map(|(id, data)| {
                let theirs = Skeleton::of(&Blocks::read(&data).ok()?);
                Some((id, similarity::permille(&mine, &theirs)))
            })
            .max_by_key(|(_, permille)| *permille)
    })
    .await?;
    Ok(found.filter(|(_, permille)| *permille >= similarity::SIMILAR))
}
