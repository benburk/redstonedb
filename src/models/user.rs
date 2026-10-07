use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

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
        }
    }
}

impl User {
    /// Whether the account may edit any build's details and tags.
    pub fn can_edit(&self) -> bool {
        self.role >= Role::Mod
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
                avatar, role
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
                avatar, role
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
                avatar, role
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
                avatar, role
             FROM user WHERE discord_id = ?"#,
            profile.id
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
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
