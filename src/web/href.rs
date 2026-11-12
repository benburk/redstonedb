//! The addresses of the site's pages, for links and redirects.

use crate::account::User;

/// The page of build `id`.
pub(crate) fn build(id: &str) -> String {
    format!("/schematics/{id}")
}

/// The page of the account `username`.
pub(crate) fn user(username: &str) -> String {
    format!("/users/{username}")
}

/// The page of account `user`: the player's, once their Minecraft name is
/// confirmed, as an account and its player share a page.
pub(crate) fn account(user: &User) -> String {
    match &user.minecraft_name {
        Some(name) if user.minecraft_verified => player(name),
        _ => self::user(&user.username),
    }
}

/// The page of the Minecraft player `name`.
pub(crate) fn player(name: &str) -> String {
    format!("/players/{name}")
}

/// The picture of the Minecraft player `name`'s head.
pub(crate) fn head(name: &str) -> String {
    format!("/players/{name}/head.png")
}

/// The home page listing the builds of `category`.
pub(crate) fn category(category: &str) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("category", category)
        .finish();
    format!("/?{query}")
}

/// The home page searching for `term`, such as a tag or an author.
pub(crate) fn search(term: &str) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("q", term)
        .finish();
    format!("/?{query}")
}
