use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use super::schematic::Schematic;

/// What an account may do. Each role can do everything the ones before it
/// can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// Uploads builds.
    User,
    /// Also edits any build's details and tags.
    Mod,
    /// Also sets roles.
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
        }
    }
}

impl User {
    /// Whether the account may edit any build's details and tags.
    pub fn can_edit(&self) -> bool {
        self.role >= Role::Mod
    }

    /// Whether the account may edit this build: a mod or admin may edit
    /// any, and a builder the ones crediting their confirmed Minecraft name.
    pub fn may_edit(&self, s: &Schematic) -> bool {
        self.can_edit() || self.built(s)
    }

    /// Whether the build credits the account's confirmed Minecraft name as
    /// a builder.
    pub fn built(&self, s: &Schematic) -> bool {
        self.minecraft_verified
            && self.minecraft_name.as_deref().is_some_and(|own| {
                s.builders.iter().any(|name| name.eq_ignore_ascii_case(own))
            })
    }

    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// The name to show: the Discord display name, else the username.
    pub fn name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }

    /// The Discord avatar's image address, when the account has one.
    pub fn avatar_url(&self) -> Option<String> {
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
                minecraft_verified as "minecraft_verified: bool"
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
                minecraft_verified as "minecraft_verified: bool"
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
                minecraft_verified as "minecraft_verified: bool"
             FROM user
             ORDER BY role = 'user', role = 'mod', username"#
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
            "INSERT INTO user (id, discord_id, username, display_name, avatar)
             VALUES (?, ?, ?, ?, ?)
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
                minecraft_verified as "minecraft_verified: bool"
             FROM user WHERE discord_id = ?"#,
            profile.id
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
    }

    /// Sets or clears the account's Minecraft name. A different name needs
    /// confirming again; the same one in other capitals keeps its
    /// confirmation. The `Err` is a sentence for the person when the name
    /// is taken by another account.
    pub async fn set_minecraft_name<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
        name: Option<&str>,
    ) -> Result<Result<(), String>, sqlx::Error> {
        let result = sqlx::query!(
            "UPDATE user SET minecraft_name = ?,
                minecraft_verified = coalesce(
                    minecraft_verified AND minecraft_name = ? COLLATE NOCASE,
                    0)
             WHERE id = ?",
            name,
            name,
            id
        )
        .execute(db)
        .await;
        match result {
            Ok(_) => Ok(Ok(())),
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
    pub async fn set_minecraft_verified<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
        verified: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user SET minecraft_verified = ? WHERE id = ?",
            verified,
            id
        )
        .execute(db)
        .await?;
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
}
