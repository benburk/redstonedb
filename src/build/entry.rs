//! A build's entry on the site.

/// A build's entry on the site: who added it and when, how it has been
/// received, and who sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// The username of the account that uploaded it through the site.
    pub uploader: Option<String>,
    /// That account as people know it: its Minecraft name, else its
    /// username.
    pub uploader_name: Option<String>,
    /// The in-game name of whoever imported the build.
    pub imported_by: Option<String>,
    pub added_at: String,
    /// When a person last changed its details; `None` if never.
    pub edited_at: Option<String>,
    /// How many times it has been downloaded.
    pub downloads: i64,
    /// How many people have favourited it.
    pub favorites: i64,
    /// Whether it is listed for everyone; a private build is listed only
    /// for its uploader and admins, though anyone with its link sees it.
    pub public: bool,
    /// Whether it was deleted, kept for an admin to restore; only the
    /// moderation log loads one.
    pub deleted: bool,
}
