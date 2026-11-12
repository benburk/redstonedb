//! Accounts, made at a person's first Discord login, each with a role, and
//! the Minecraft name they give on their profile. One name belongs to one
//! account, whatever its capitals; once an admin confirms it, the account
//! may edit the builds crediting it as author.

use uuid::Uuid;

use crate::build::Build;

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
    /// Also edits, merges, and deletes any build, and makes new tags.
    Mod,
    /// Also sets roles, lists private builds, and keeps the moderation
    /// log, where deleted builds are restored.
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
    /// When the account joined: its first login, or a best guess for one
    /// from before join dates were kept.
    pub created_at: String,
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

impl User {
    /// Whether the account may edit any build's details and tags.
    pub fn can_edit(&self) -> bool {
        self.role >= Role::Mod
    }

    /// Whether the account may edit this build: a mod or admin may edit
    /// any, an author the ones crediting their confirmed Minecraft name,
    /// and an uploader the ones they uploaded.
    pub fn may_edit(&self, s: &Build) -> bool {
        self.can_edit() || self.built(s) || self.uploaded(s)
    }

    /// Whether the account may restore an earlier version of the build:
    /// its uploader, or a mod or admin.
    pub fn may_restore(&self, s: &Build) -> bool {
        self.uploaded(s) || self.can_edit()
    }

    /// Whether the account uploaded the build through the site.
    pub fn uploaded(&self, s: &Build) -> bool {
        s.entry.uploader.as_deref() == Some(self.username.as_str())
    }

    /// Whether the build credits the account's confirmed Minecraft name as
    /// an author.
    pub fn built(&self, s: &Build) -> bool {
        self.minecraft_verified
            && self.minecraft_name.as_deref().is_some_and(|own| {
                s.credits
                    .authors
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(own))
            })
    }

    /// Whether the account may delete any build: a mod's or admin's.
    pub fn can_delete(&self) -> bool {
        self.role >= Role::Mod
    }

    /// Whether the account may make a tag not yet on the list: a mod's or
    /// admin's.
    pub fn can_create_tags(&self) -> bool {
        self.role >= Role::Mod
    }

    /// Whether the account may delete this build: its uploader, or a mod
    /// or admin.
    pub fn may_delete(&self, s: &Build) -> bool {
        self.uploaded(s) || self.can_delete()
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

    /// The Discord avatar's image address, when the account has one.
    pub fn avatar_url(&self) -> Option<String> {
        self.avatar.as_ref().map(|hash| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{hash}.png?size=64",
                self.discord_id
            )
        })
    }
}

/// Whether listings show build `s` to `viewer`: a public build to anyone,
/// and a private one to its uploader and admins. Anyone with its link may
/// open it either way.
pub fn lists(viewer: Option<&User>, s: &Build) -> bool {
    s.entry.public
        || viewer.is_some_and(|user| user.is_admin() || user.uploaded(s))
}
