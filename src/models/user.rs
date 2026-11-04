//! Accounts, made at a person's first Discord login, each with a role, and
//! the Minecraft name they give on their profile. One name belongs to one
//! account, whatever its capitals; once an admin confirms it, the account
//! may edit the builds crediting it as author.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use super::schematic::Schematic;

/// The account of `RedstoneDB` itself, the author of the versions the site
/// makes, as when it takes out a build's unused blocks. No one signs in as
/// it, and it is listed among no accounts.
pub const REDSTONEDB: Uuid = Uuid::from_u128(0xdb);

/// What an account may do. Each role can do everything the ones before it
/// can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// Uploads builds.
    User,
    /// Also edits any build's details and tags.
    Mod,
    /// Also deletes builds and sets roles.
    Admin,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::User, Role::Mod, Role::Admin];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Mod => "mod",
            Role::Admin => "admin",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Role::ALL.into_iter().find(|role| role.as_str() == text)
    }
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub discord_id: String,
    /// The Discord username, refreshed at each login.
    pub username: String,
    pub display_name: Option<String>,
    /// The Discord avatar's hash.
    pub avatar: Option<String>,
    pub role: Role,
    /// The in-game name the person gave, if any.
    pub minecraft_name: Option<String>,
    /// Whether an admin has confirmed the Minecraft name is the person's.
    pub minecraft_verified: bool,
    /// The player behind the Minecraft name, as Mojang knows them.
    pub minecraft_uuid: Option<String>,
    /// When the account joined, as far as is known.
    pub joined: Joined,
}

/// When an account joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Joined {
    /// At its first login, at this moment.
    At(String),
    /// On this day or before: the earliest thing it did, as it joined
    /// before join dates were kept.
    By(String),
    /// Before join dates were kept, having done nothing to date it by.
    Unknown,
}

/// What is known of an account's Schemati session, from Schemati's last
/// answer with it, so it can be shown without asking again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchematiLink {
    /// The player whose folder it opened, once Schemati has said.
    pub player: Option<String>,
    /// When Schemati last answered with it, in seconds since 1970; `None`
    /// for a session given before this was kept, until it next answers.
    pub checked: Option<i64>,
    /// Whether Schemati has said it no longer knows the session.
    pub expired: bool,
}

/// Who Discord says a person is, from its `/users/@me`.
pub struct DiscordProfile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

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
    joined_at: Option<String>,
    joined_estimated: bool,
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
            joined: match (row.joined_at, row.joined_estimated) {
                (Some(at), false) => Joined::At(at),
                (Some(at), true) => Joined::By(at),
                (None, _) => Joined::Unknown,
            },
        }
    }
}

impl User {
    /// Whether the account may edit any build's details and tags.
    pub fn can_edit(&self) -> bool {
        self.role >= Role::Mod
    }

    /// Whether the account may edit this build: a mod or admin may edit
    /// any, an author the ones crediting their confirmed Minecraft name,
    /// and an uploader the ones they uploaded.
    pub fn may_edit(&self, s: &Schematic) -> bool {
        self.can_edit() || self.built(s) || self.uploaded(s)
    }

    /// Whether the account may restore an earlier version of the build:
    /// its uploader, or a mod or admin.
    pub fn may_restore(&self, s: &Schematic) -> bool {
        self.uploaded(s) || self.can_edit()
    }

    /// Whether the account uploaded the build through the site.
    pub fn uploaded(&self, s: &Schematic) -> bool {
        s.uploader.as_deref() == Some(self.username.as_str())
    }

    /// Whether the build credits the account's confirmed Minecraft name as
    /// an author.
    pub fn built(&self, s: &Schematic) -> bool {
        self.minecraft_verified
            && self.minecraft_name.as_deref().is_some_and(|own| {
                s.authors.iter().any(|name| name.eq_ignore_ascii_case(own))
            })
    }

    /// Whether the account may delete any build: a mod's or admin's.
    pub fn can_delete(&self) -> bool {
        self.role >= Role::Mod
    }

    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// The name to show: the Minecraft name, as players know each other,
    /// else the Discord display name, else the username.
    pub fn name(&self) -> &str {
        self.minecraft_name
            .as_deref()
            .or(self.display_name.as_deref())
            .unwrap_or(&self.username)
    }

    /// The account's picture at `size` pixels square: its player's head,
    /// drawn pixelated, when Mojang knows the player, else the Discord
    /// avatar, round, if it has one.
    pub fn picture(&self, size: u32) -> Option<maud::Markup> {
        let (src, shape) = match (&self.minecraft_uuid, self.avatar_url()) {
            (Some(uuid), _) => (
                format!("/heads/{uuid}"),
                "rounded-[2px] [image-rendering:pixelated]",
            ),
            (None, Some(avatar)) => (avatar, "rounded-full"),
            (None, None) => return None,
        };
        Some(maud::html! {
            img src=(src) alt="" width=(size) height=(size)
                class=(shape) style={ "width:" (size) "px;height:" (size) "px" };
        })
    }

    /// The Discord avatar's image address, when the account has one.
    fn avatar_url(&self) -> Option<String> {
        self.avatar.as_ref().map(|hash| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{hash}.png?size=64",
                self.discord_id
            )
        })
    }

    pub async fn get_by_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let row = sqlx::query_as!(
            Row,
            r#"SELECT id as "id: Uuid", discord_id, username, display_name,
                avatar, role, minecraft_name,
                minecraft_verified as "minecraft_verified: bool", minecraft_uuid,
                joined_at, joined_estimated as "joined_estimated: bool"
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
                joined_at, joined_estimated as "joined_estimated: bool"
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
                joined_at, joined_estimated as "joined_estimated: bool"
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
                (id, discord_id, username, display_name, avatar, joined_at)
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
                joined_at, joined_estimated as "joined_estimated: bool"
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

impl User {
    /// Every Minecraft name stored: credited, importing, or an account's.
    pub async fn player_names(
        db: &SqlitePool,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT name as "name!: String" FROM credit
             UNION SELECT imported_by FROM schematic
                WHERE imported_by IS NOT NULL
             UNION SELECT minecraft_name FROM user
                WHERE minecraft_name IS NOT NULL"#
        )
        .fetch_all(db)
        .await
    }

    /// Spells `name` as given wherever it is stored in other capitals, and
    /// records `uuid` for an account with the name. Returns how many places
    /// changed.
    pub async fn respell_player(
        db: &SqlitePool,
        name: &str,
        uuid: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut tx = db.begin().await?;
        // The columns compare without capitals, so a change in spelling
        // alone shows only to a binary comparison.
        let credits = sqlx::query!(
            "UPDATE credit SET name = ?1
             WHERE name = ?1 AND name <> ?1 COLLATE BINARY",
            name
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let imports = sqlx::query!(
            "UPDATE schematic SET imported_by = ?1
             WHERE imported_by = ?1 COLLATE NOCASE
                AND imported_by <> ?1 COLLATE BINARY",
            name
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let accounts = sqlx::query!(
            "UPDATE user SET minecraft_name = ?1, minecraft_uuid = ?2
             WHERE minecraft_name = ?1 AND (minecraft_name <> ?1 COLLATE BINARY
                OR minecraft_uuid IS NOT ?2)",
            name,
            uuid
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(credits + imports + accounts)
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
        "UPDATE schematic
         SET imported_by = (SELECT minecraft_name FROM user WHERE id = ?1)
         WHERE imported_by = (SELECT minecraft_name FROM user
            WHERE id = ?1 AND minecraft_verified) COLLATE NOCASE",
        id
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}
