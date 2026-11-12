//! Accounts as they are kept: signing in, the Minecraft name a person
//! gives, roles, and what is known of an account's Schemati session.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use crate::{
    account::{REDSTONEDB, Role, SchematiLink, User},
    external::discord::DiscordProfile,
    recognize::palette::Palette,
};

struct Row {
    id: Uuid,
    discord_id: String,
    username: String,
    display_name: Option<String>,
    avatar: Option<String>,
    role: String,
    minecraft_name: Option<String>,
    minecraft_verified: bool,
    minecraft_uuid: Option<String>,
    created_at: String,
}

impl From<Row> for User {
    fn from(row: Row) -> Self {
        User {
            id: row.id,
            discord_id: row.discord_id,
            username: row.username,
            display_name: row.display_name,
            avatar: row.avatar,
            role: Role::parse(&row.role).unwrap_or(Role::User),
            minecraft_name: row.minecraft_name,
            minecraft_verified: row.minecraft_verified,
            minecraft_uuid: row.minecraft_uuid,
            created_at: row.created_at,
        }
    }
}

impl User {
    pub async fn get_by_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let row = sqlx::query_as!(
            Row,
            r#"SELECT id as "id: Uuid", discord_id, username, display_name,
                avatar, role, minecraft_name,
                minecraft_verified as "minecraft_verified: bool", minecraft_uuid,
                created_at
             FROM user WHERE id = ?"#,
            id
        )
        .fetch_optional(db)
        .await?;
        Ok(row.map(User::from))
    }

    pub async fn get_by_username<'e, E: SqliteExecutor<'e>>(
        db: E,
        username: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        let row = sqlx::query_as!(
            Row,
            r#"SELECT id as "id: Uuid", discord_id, username, display_name,
                avatar, role, minecraft_name,
                minecraft_verified as "minecraft_verified: bool", minecraft_uuid,
                created_at
             FROM user WHERE username = ?"#,
            username
        )
        .fetch_optional(db)
        .await?;
        Ok(row.map(User::from))
    }

    /// Every account, admins first, then mods, then by username.
    pub async fn all<'e, E: SqliteExecutor<'e>>(
        db: E,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let rows = sqlx::query_as!(
            Row,
            r#"SELECT id as "id: Uuid", discord_id, username, display_name,
                avatar, role, minecraft_name,
                minecraft_verified as "minecraft_verified: bool", minecraft_uuid,
                created_at
             FROM user WHERE id <> ?
             ORDER BY role = 'user', role = 'mod', username"#,
            REDSTONEDB
        )
        .fetch_all(db)
        .await?;
        Ok(rows.into_iter().map(User::from).collect())
    }

    /// The account for a Discord login, made on first login and refreshed
    /// on every later one. Discord usernames are unique but can change, so
    /// another account still holding this name gives it up.
    pub async fn sign_in(
        db: &SqlitePool,
        profile: &DiscordProfile,
    ) -> Result<Self, sqlx::Error> {
        let mut tx = db.begin().await?;
        sqlx::query!(
            "UPDATE user SET username = discord_id
             WHERE username = ? AND discord_id <> ?",
            profile.username,
            profile.id
        )
        .execute(&mut *tx)
        .await?;
        let id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO user
                (id, discord_id, username, display_name, avatar, created_at)
             VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             ON CONFLICT (discord_id) DO UPDATE SET
                username = excluded.username,
                display_name = excluded.display_name,
                avatar = excluded.avatar",
            id,
            profile.id,
            profile.username,
            profile.display_name,
            profile.avatar,
        )
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query_as!(
            Row,
            r#"SELECT id as "id: Uuid", discord_id, username, display_name,
                avatar, role, minecraft_name,
                minecraft_verified as "minecraft_verified: bool", minecraft_uuid,
                created_at
             FROM user WHERE discord_id = ?"#,
            profile.id
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
    }

    /// Sets or clears the account's Minecraft name. A different name needs
    /// confirming again; the same one in other capitals keeps its confirmation,
    /// and stored credits take the new spelling. The `Err` is a sentence for
    /// the person when the name is taken by another account.
    pub async fn set_minecraft_name(
        db: &SqlitePool,
        id: Uuid,
        name: Option<&str>,
        uuid: Option<&str>,
    ) -> Result<Result<(), String>, sqlx::Error> {
        let mut tx = db.begin().await?;
        let result = sqlx::query!(
            "UPDATE user SET minecraft_name = ?, minecraft_uuid = ?,
                minecraft_verified = coalesce(
                    minecraft_verified AND minecraft_name = ? COLLATE NOCASE,
                    0)
             WHERE id = ?",
            name,
            uuid,
            name,
            id
        )
        .execute(&mut *tx)
        .await;
        match result {
            // Still confirmed, the name may be newly spelled.
            Ok(_) => {
                respell(&mut tx, id).await?;
                tx.commit().await?;
                Ok(Ok(()))
            }
            Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                Ok(Err(format!(
                    "{} is already the Minecraft name of another account.",
                    name.unwrap_or_default()
                )))
            }
            Err(err) => Err(err),
        }
    }

    /// Records whether an admin has confirmed the account's Minecraft name.
    /// A confirmed name's spelling becomes the one stored credits and
    /// importers use.
    pub async fn set_minecraft_verified(
        db: &SqlitePool,
        id: Uuid,
        verified: bool,
    ) -> Result<(), sqlx::Error> {
        let mut tx = db.begin().await?;
        sqlx::query!(
            "UPDATE user SET minecraft_verified = ? WHERE id = ?",
            verified,
            id
        )
        .execute(&mut *tx)
        .await?;
        respell(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_role<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
        role: Role,
    ) -> Result<(), sqlx::Error> {
        let role = role.as_str();
        sqlx::query!("UPDATE user SET role = ? WHERE id = ?", role, id)
            .execute(db)
            .await?;
        Ok(())
    }

    /// The Schemati session the account last gave, if any.
    pub async fn schemati_session(
        db: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT schemati_session FROM user WHERE id = ?",
            id
        )
        .fetch_one(db)
        .await
    }

    /// How the account's Schemati session is known to stand, as kept from
    /// Schemati's last answer with it; `None` when none is given.
    pub async fn schemati_link(
        db: &SqlitePool,
        id: Uuid,
    ) -> Result<Option<SchematiLink>, sqlx::Error> {
        sqlx::query_as!(
            SchematiLink,
            r#"SELECT schemati_player AS player,
                unixepoch(schemati_checked_at) AS "checked: i64",
                schemati_expired_at IS NOT NULL AS "expired!: bool"
             FROM user WHERE id = ? AND schemati_session IS NOT NULL"#,
            id
        )
        .fetch_optional(db)
        .await
    }

    /// Keeps `session` as the account's Schemati session, just checked and
    /// found to open `player`'s folder, in place of any it gave before.
    pub async fn connect_schemati(
        db: &SqlitePool,
        id: Uuid,
        session: &str,
        player: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user SET schemati_session = ?, schemati_player = ?,
                schemati_checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                schemati_expired_at = NULL
             WHERE id = ?",
            session,
            player,
            id
        )
        .execute(db)
        .await?;
        Ok(())
    }

    /// Forgets the account's Schemati session and all that was known of
    /// it.
    pub async fn disconnect_schemati(
        db: &SqlitePool,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user SET schemati_session = NULL, schemati_player = NULL,
                schemati_checked_at = NULL, schemati_expired_at = NULL
             WHERE id = ?",
            id
        )
        .execute(db)
        .await?;
        Ok(())
    }

    /// Notes that Schemati just answered with `session`, so it still works,
    /// greeting `player` if the answer named one. Nothing changes if the
    /// account has since given another session, or none.
    pub async fn schemati_answered(
        db: &SqlitePool,
        id: Uuid,
        session: &str,
        player: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user SET schemati_player = COALESCE(?, schemati_player),
                schemati_checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                schemati_expired_at = NULL
             WHERE id = ? AND schemati_session = ?",
            player,
            id,
            session
        )
        .execute(db)
        .await?;
        Ok(())
    }

    /// Notes that Schemati no longer knows `session`, when it first says
    /// so. Nothing changes if the account has since given another session,
    /// or none.
    pub async fn schemati_forgot(
        db: &SqlitePool,
        id: Uuid,
        session: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user SET schemati_expired_at = COALESCE(
                    schemati_expired_at,
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             WHERE id = ? AND schemati_session = ?",
            id,
            session
        )
        .execute(db)
        .await?;
        Ok(())
    }
}

/// Respells the credits and importers that name account `id`'s confirmed
/// Minecraft name, whatever their capitals, as the account spells it. An
/// unconfirmed name changes nothing.
async fn respell(
    tx: &mut sqlx::SqliteConnection,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE credit SET name = (SELECT minecraft_name FROM user WHERE id = ?1)
         WHERE name = (SELECT minecraft_name FROM user
            WHERE id = ?1 AND minecraft_verified) COLLATE NOCASE",
        id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE build
         SET imported_by = (SELECT minecraft_name FROM user WHERE id = ?1)
         WHERE imported_by = (SELECT minecraft_name FROM user
            WHERE id = ?1 AND minecraft_verified) COLLATE NOCASE",
        id
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// `user`'s block theme, if they chose one.
pub async fn palette(
    db: &SqlitePool,
    user: Uuid,
) -> Result<Option<Palette>, sqlx::Error> {
    let json: Option<String> =
        sqlx::query_scalar("SELECT palette FROM user WHERE id = ?")
            .bind(user)
            .fetch_one(db)
            .await?;
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

/// The key of `viewer`'s block theme, for thumbnails' addresses, if they
/// chose one.
pub async fn palette_key(
    db: &SqlitePool,
    viewer: Option<&User>,
) -> Result<Option<String>, sqlx::Error> {
    Ok(match viewer {
        Some(user) => palette(db, user.id).await?.map(|palette| palette.key()),
        None => None,
    })
}

/// Keeps `palette` as `user`'s block theme, or with `None` goes back to
/// the site's.
pub async fn set_palette(
    db: &SqlitePool,
    user: Uuid,
    palette: Option<&Palette>,
) -> Result<(), sqlx::Error> {
    let json = palette.map(|palette| {
        serde_json::to_string(palette).expect("a theme is JSON")
    });
    sqlx::query("UPDATE user SET palette = ? WHERE id = ?")
        .bind(json)
        .bind(user)
        .execute(db)
        .await?;
    Ok(())
}
