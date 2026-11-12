//! Who a build is credited to.

/// The people a build is credited to, by their in-game names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credits {
    /// Who built it, in the order given; empty when unknown.
    pub authors: Vec<String>,
    /// Who helped without building it, in the order given.
    pub contributors: Vec<String>,
}

impl Credits {
    /// Everyone credited, authors first.
    pub fn everyone(&self) -> impl Iterator<Item = &String> {
        self.authors.iter().chain(&self.contributors)
    }
}

/// Whether a name can be credited: a Minecraft name of letters, digits,
/// and underscores.
pub fn valid_player_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The player `@name` names at the start of `text`, `prior` the character
/// before: an in-game name of 3 to 16 letters, digits, and underscores,
/// after no word, as an email address's `@` is not.
pub fn mention(text: &str, prior: Option<char>) -> Option<&str> {
    let rest = text.strip_prefix('@')?;
    if prior.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    (3..=16).contains(&name.len()).then_some(name)
}

/// Every player `text` names with `@name`, as written, each once whatever
/// its capitals.
pub fn mentions(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut prior = None;
        for (at, c) in line.char_indices() {
            if let Some(name) = mention(&line[at..], prior)
                && !found.iter().any(|n| n.eq_ignore_ascii_case(name))
            {
                found.push(name.to_owned());
            }
            prior = Some(c);
        }
    }
    found
}
