//! Minecraft players looked up by name, and their heads, as kept from
//! Mojang so a page never waits for one asked about before.

use sqlx::SqlitePool;

use crate::mojang::Profile;

/// How long a player found, or a head, is kept before Mojang is asked
/// again, in the background.
const FRESH: &str = "-7 days";

/// How long a name no player has is kept before Mojang is asked again.
const FRESH_NONE: &str = "-1 day";

/// What is kept of the player named `name`: the player, or `None` for a
/// name no player has, and whether it is fresh. `None` when it was never
/// asked about.
pub async fn known(
    db: &SqlitePool,
    name: &str,
) -> Result<Option<(Option<Profile>, bool)>, sqlx::Error> {
    let name = name.to_lowercase();
    let row = sqlx::query!(
        r#"SELECT uuid, spelling,
            checked_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now',
                CASE WHEN uuid IS NULL THEN ? ELSE ? END) AS "fresh!: bool"
         FROM player WHERE name = ?"#,
        FRESH_NONE,
        FRESH,
        name
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| {
        let profile = row
            .uuid
            .zip(row.spelling)
            .map(|(uuid, name)| Profile { uuid, name });
        (profile, row.fresh)
    }))
}

/// Keeps what Mojang said of the player named `name`: `found`, or `None`
/// for a name no player has.
pub async fn remember(
    db: &SqlitePool,
    name: &str,
    found: Option<&Profile>,
) -> Result<(), sqlx::Error> {
    let name = name.to_lowercase();
    let (uuid, spelling) = (found.map(|p| &p.uuid), found.map(|p| &p.name));
    sqlx::query!(
        "INSERT INTO player (name, uuid, spelling) VALUES (?, ?, ?)
         ON CONFLICT (name) DO UPDATE SET uuid = excluded.uuid,
            spelling = excluded.spelling,
            checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        name,
        uuid,
        spelling
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The head kept for the player with `uuid`: its PNG, or `None` for one
/// with no skin to draw, and whether it is fresh. `None` when it was never
/// fetched.
pub async fn head(
    db: &SqlitePool,
    uuid: &str,
) -> Result<Option<(Option<Vec<u8>>, bool)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT png, fetched_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now',
                CASE WHEN png IS NULL THEN ? ELSE ? END) AS "fresh!: bool"
         FROM head WHERE uuid = ?"#,
        FRESH_NONE,
        FRESH,
        uuid
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| (row.png, row.fresh)))
}

/// Keeps the head of the player with `uuid`: `png`, or `None` for one with
/// no skin to draw.
pub async fn keep_head(
    db: &SqlitePool,
    uuid: &str,
    png: Option<&[u8]>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO head (uuid, png) VALUES (?, ?)
         ON CONFLICT (uuid) DO UPDATE SET png = excluded.png,
            fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        uuid,
        png
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The names of every player the site shows a head for, in lowercase:
/// each credited, importing, or signed up, and each a description names
/// with `@name`, never asked about yet.
pub async fn unasked(
    db: &SqlitePool,
    mentioned: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    let mut names: Vec<String> = sqlx::query_scalar!(
        r#"SELECT DISTINCT lower(name) AS "name!" FROM (
            SELECT name FROM credit
            UNION SELECT imported_by FROM schematic WHERE imported_by IS NOT NULL
            UNION SELECT minecraft_name FROM user WHERE minecraft_name IS NOT NULL)
         WHERE lower(name) NOT IN (SELECT name FROM player)"#
    )
    .fetch_all(db)
    .await?;
    for name in mentioned {
        let name = name.to_lowercase();
        if !names.contains(&name) && known(db, &name).await?.is_none() {
            names.push(name);
        }
    }
    Ok(names)
}

/// The UUIDs of every player found, or account confirmed, whose head was
/// never fetched.
pub async fn headless(db: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT DISTINCT uuid AS "uuid!" FROM (
            SELECT uuid FROM player WHERE uuid IS NOT NULL
            UNION SELECT minecraft_uuid FROM user
                WHERE minecraft_uuid IS NOT NULL)
         WHERE uuid NOT IN (SELECT uuid FROM head)"#
    )
    .fetch_all(db)
    .await
}
