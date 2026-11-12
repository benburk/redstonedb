//! The tags the site knows, and which builds carry them.

use sqlx::{SqliteExecutor, SqlitePool};

use crate::build::tag::{self, Kind, Tag};

/// Replaces a build's tags with the ones `names` name. A name no tag has
/// yet makes a new tag under `other`.
pub(crate) async fn set(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM build_tag WHERE build_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for name in names {
        if find(&mut *tx, name).await?.is_none() {
            define(&mut *tx, name, None, Kind::Other).await?;
        }
        let tag = find(&mut *tx, name)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        attach(&mut *tx, id, &tag).await?;
    }
    Ok(())
}

/// Refuses `names` that no tag has unless `may_create`, as only a mod or admin
/// may: the `Err` says which and why.
pub async fn check_new(
    db: &SqlitePool,
    names: &[String],
    may_create: bool,
) -> Result<Result<(), String>, sqlx::Error> {
    if may_create {
        return Ok(Ok(()));
    }
    let mut unknown = Vec::new();
    for name in names {
        if find(db, name).await?.is_none() {
            unknown.push(name.replace('_', " "));
        }
    }
    Ok(match unknown.as_slice() {
        [] => Ok(()),
        [one] => Err(format!(
            "There is no tag {one}, and only a mod or admin can make a new one."
        )),
        many => Err(format!(
            "There are no tags {}, and only a mod or admin can make new ones.",
            many.join(", ")
        )),
    })
}

/// The id of the tag named `name`, which may also be its short token.
pub async fn find<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
) -> Result<Option<String>, sqlx::Error> {
    let name = tag::name(name);
    sqlx::query_scalar!(
        "SELECT id FROM tag WHERE name = ? OR token = ?",
        name,
        name
    )
    .fetch_optional(db)
    .await
}

pub async fn attach<'e, E: SqliteExecutor<'e>>(
    db: E,
    build_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT OR IGNORE INTO build_tag (build_id, tag_id)
         VALUES (?, ?)",
        build_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn detach<'e, E: SqliteExecutor<'e>>(
    db: E,
    build_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM build_tag WHERE build_id = ? AND tag_id = ?",
        build_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Every tag, by kind then name.
pub async fn all<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        r#"SELECT name, token, kind AS "kind: Kind" FROM tag
         ORDER BY kind = 'other', kind, name"#
    )
    .fetch_all(db)
    .await
}

pub async fn define<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
    token: Option<&str>,
    kind: Kind,
) -> Result<(), sqlx::Error> {
    let name = tag::name(name);
    sqlx::query!(
        "INSERT INTO tag (name, token, kind) VALUES (?, ?, ?)",
        name,
        token,
        kind
    )
    .execute(db)
    .await?;
    Ok(())
}
