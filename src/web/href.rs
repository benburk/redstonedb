//! The addresses of the site's pages, for links and redirects.

/// The page of build `id`.
pub(crate) fn build(id: &str) -> String {
    format!("/schematics/{id}")
}

/// The page of the account `username`.
pub(crate) fn user(username: &str) -> String {
    format!("/users/{username}")
}

/// The page of the Minecraft player `name`.
pub(crate) fn player(name: &str) -> String {
    format!("/players/{name}")
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
