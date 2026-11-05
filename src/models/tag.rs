//! Tags: words a build carries beyond its details, such as `alpha`, each a
//! restriction or other. A few are worked out from the build itself and
//! never stored.

use sqlx::{SqliteExecutor, SqlitePool};

named_values! {
    /// What a tag says of a build, which colours its badge.
    Kind {
        /// A limit the build keeps, such as working from Alpha.
        Restriction = "restriction", "Restriction";
        Other = "other", "Other";
        /// That the build gives wrong outputs, never stored.
        #[value(skip)]
        Failing = "failing", "Failing";
        /// That the build's inputs, outputs, or parts don't fit its
        /// category's, so it couldn't be verified, never stored.
        #[value(skip)]
        Untested = "untested", "Untested";
        /// Its design, such as `carry_cancel`, when the site works it out,
        /// never stored as a tag.
        #[value(skip)]
        Design = "design", "Design";
        /// The way its bits run, such as `vertical`, never stored as a tag.
        #[value(skip)]
        Orientation = "orientation", "Orientation";
        /// One of its category's flags, such as `carry_in`, when the site
        /// reads it from the build, never stored as a tag.
        #[value(skip)]
        Feature = "feature", "Feature";
    }
}

/// A tag's name is written in full words joined by underscores, such as
/// `carry_lookahead`, and may hold dashes, as `double-density` does.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    /// A short form that commands and search accept.
    pub token: Option<String>,
    pub kind: Kind,
}

/// The name of the tag a build carries that gets some cases of its test
/// wrong. It is not stored, so no one can add or remove it.
pub const FAILING: &str = "failing";

/// The name of the tag a build carries whose inputs, outputs, or parts
/// don't fit its category's, so its test couldn't run, though it may
/// work. It is not stored either.
pub const UNTESTED: &str = "untested";

/// The tag a build carries whose copies stack.
pub const STACKABLE: &str = "stackable";

/// The tag a build carries whose copies stack a few blocks apart, `pitch`
/// such as `2-high`, a bus's spacing: `2-high_stackable`.
pub fn pitch_stackable(pitch: &str) -> String {
    format!("{pitch}_{STACKABLE}")
}

/// Whether the tag `name` says how far apart copies stack; see
/// [`pitch_stackable`].
pub fn is_pitch_stackable(name: &str) -> bool {
    name.strip_suffix(STACKABLE)
        .is_some_and(|pitch| pitch.ends_with('_'))
}

/// The tag a register carries whose copies across its bits add words on
/// its shared data bus `step` apart, such as `3-wide`, the step west, or
/// `4-high`, up: `3-wide_word_stackable`. It is not stored.
pub fn word_stackable(step: &str) -> String {
    format!("{step}_word_{STACKABLE}")
}

/// Whether the tag `name` says how far apart copies add words; see
/// [`word_stackable`].
pub fn is_word_stackable(name: &str) -> bool {
    name.strip_suffix(STACKABLE)
        .and_then(|step| step.strip_suffix("_word_"))
        .is_some_and(|step| !step.is_empty())
}

/// The tag a horizontal build carries whose operands' bits alternate
/// along one row, rather than lie split, one operand above the other; see
/// `Kept::interleaved`. It is not stored.
pub const INTERLEAVED_INPUTS: &str = "interleaved_inputs";

/// The tag a build carries whose outputs all settle on the same tick.
pub const SYNCED: &str = "synced";

/// The tag a build carries whose outputs settle on different ticks.
pub const UNSYNCED: &str = "unsynced";

/// The name of the tag a build whose file has no redstone torches carries.
/// It is not stored, so no one can add or remove it.
const TORCHLESS: &str = "torchless";

/// The name of the tag a build carries that works from Alpha, using no
/// redstone component or mechanic added since. It is not stored either.
const ALPHA: &str = "alpha";

/// The tags worked out from a build rather than given by a person.
pub const DERIVED: [&str; 4] = [FAILING, UNTESTED, TORCHLESS, ALPHA];

impl Tag {
    pub(super) fn derived(name: &str, kind: Kind) -> Self {
        Tag {
            name: name.to_owned(),
            token: None,
            kind,
        }
    }

    /// Whether the build's file or simulation sets it, rather than a
    /// person.
    pub fn is_derived(&self) -> bool {
        DERIVED.contains(&self.name.as_str())
    }

    /// Whether the site works it out from the build, so its badge has the
    /// colour of a found tag: a derived tag, or a design, orientation, or
    /// flag the site finds.
    pub fn is_found(&self) -> bool {
        self.is_derived()
            || matches!(
                self.kind,
                Kind::Design | Kind::Orientation | Kind::Feature
            )
    }

    /// The class that colours its badge: failing and untested in the
    /// colour of danger, as either keeps a build off the list, any other the site finds in its own, and
    /// one a person gives by its kind.
    pub fn class(&self) -> &'static str {
        match self.kind {
            Kind::Failing | Kind::Untested => "badge-failing",
            _ if self.is_found() => "badge-found",
            Kind::Restriction => "badge-restriction",
            _ => "badge-other",
        }
    }

    /// What its badge's tooltip says it is.
    pub fn about(&self) -> String {
        if self.is_found() && !self.is_unverified() {
            format!("{}, found by the site", self.kind.label())
        } else {
            self.kind.label().to_owned()
        }
    }

    /// Whether it says the build is not verified, as failing or untested.
    pub fn is_unverified(&self) -> bool {
        matches!(self.kind, Kind::Failing | Kind::Untested)
    }

    /// The name as words, such as `carry lookahead`.
    pub fn label(&self) -> String {
        // A token in capitals is the tag's abbreviation, shown in its
        // place, as CCA for carry cancel.
        match &self.token {
            Some(token) if token.chars().all(|c| c.is_ascii_uppercase()) => {
                token.clone()
            }
            _ => self.name.replace('_', " "),
        }
    }

    /// Whether `wanted`, a tag name as [`name`] gives it, names this tag,
    /// by its name or its token in any capitals.
    pub fn is_named(&self, wanted: &str) -> bool {
        self.name == wanted
            || self
                .token
                .as_deref()
                .is_some_and(|token| token.eq_ignore_ascii_case(wanted))
    }

    /// How a filename writes this tag.
    pub fn token(&self) -> &str {
        self.token.as_deref().unwrap_or(&self.name)
    }
}

/// The form of a tag name a person types, with spaces for underscores.
pub fn name(typed: &str) -> String {
    let name = typed.trim().to_lowercase().replace(' ', "_");
    // The names failing and untested builds' tags had, in links made
    // before.
    match name.as_str() {
        "fails_verification" => FAILING.to_owned(),
        "non-standard" => UNTESTED.to_owned(),
        _ => name,
    }
}

/// Whether a typed name, once in the form [`name`] gives, can name a tag:
/// lowercase letters, digits, underscores, and dashes, but no dash at
/// either end, as one before it would read as leaving the tag out of a
/// search.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
        })
}

/// The tags a build's file gives it, `facts` read from it: `alpha` when
/// it works from Alpha and `torchless` when it has no torches.
pub fn from_file(facts: &crate::import::FileFacts) -> Vec<Tag> {
    with_derived(
        Vec::new(),
        None,
        Some(facts.torches),
        Some(facts.works_from.release),
    )
}

/// Whether a build isn't verified, and why: it gives wrong outputs, or
/// its inputs, outputs, or parts don't fit its category's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Unverified {
    Failing,
    Untested,
}

/// Whether a build that `fails_simulation`, or is unverified for `reason`,
/// isn't verified, and why: wrong outputs, as a failing test or the reason
/// says, leave it failing, and any other reason untested.
pub fn unverified(
    fails_simulation: bool,
    reason: Option<&str>,
) -> Option<Unverified> {
    match reason {
        _ if fails_simulation => Some(Unverified::Failing),
        Some(reason) if crate::recognize::fault::wrong_outputs(reason) => {
            Some(Unverified::Failing)
        }
        Some(_) => Some(Unverified::Untested),
        None => None,
    }
}

/// A build's stored `tags`, led by `failing` or `untested`
/// when it is `unverified`, with `torchless` among its restrictions when
/// its file has no torches, and `alpha` when it works from Alpha.
pub(super) fn with_derived(
    mut tags: Vec<Tag>,
    unverified: Option<Unverified>,
    torches: Option<i64>,
    works_from: Option<&str>,
) -> Vec<Tag> {
    let alpha = works_from == Some(crate::schem::works_from::RELEASES[0].name);
    for (has, name) in [(alpha, ALPHA), (torches == Some(0), TORCHLESS)] {
        if !has {
            continue;
        }
        // Restrictions come first, by name.
        let at = tags
            .iter()
            .position(|tag| tag.kind != Kind::Restriction || *tag.name > *name)
            .unwrap_or(tags.len());
        tags.insert(at, Tag::derived(name, Kind::Restriction));
    }
    match unverified {
        Some(Unverified::Failing) => {
            tags.insert(0, Tag::derived(FAILING, Kind::Failing));
        }
        Some(Unverified::Untested) => {
            tags.insert(0, Tag::derived(UNTESTED, Kind::Untested));
        }
        None => {}
    }
    tags
}

/// Replaces a build's tags with the ones `names` name. A name no tag has
/// yet makes a new tag under `other`.
pub(super) async fn set(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic_tag WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for name in names {
        if find(&mut *tx, name).await?.is_none() {
            define(&mut *tx, name, None, Kind::Other).await?;
        }
        let tag = find(&mut *tx, name)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        attach(&mut *tx, id, &tag).await?;
    }
    Ok(())
}

/// Refuses `names` that no tag has unless `may_create`, as only an admin
/// may: the `Err` says which and why.
pub async fn check_new(
    db: &SqlitePool,
    names: &[String],
    may_create: bool,
) -> Result<Result<(), String>, sqlx::Error> {
    if may_create {
        return Ok(Ok(()));
    }
    let mut unknown = Vec::new();
    for name in names {
        if find(db, name).await?.is_none() {
            unknown.push(name.replace('_', " "));
        }
    }
    Ok(match unknown.as_slice() {
        [] => Ok(()),
        [one] => Err(format!(
            "There is no tag {one}, and only an admin can make a new one."
        )),
        many => Err(format!(
            "There are no tags {}, and only an admin can make new ones.",
            many.join(", ")
        )),
    })
}

/// The id of the tag named `name`, which may also be its short token.
pub async fn find<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
) -> Result<Option<String>, sqlx::Error> {
    let name = self::name(name);
    sqlx::query_scalar!(
        "SELECT id FROM tag WHERE name = ? OR token = ?",
        name,
        name
    )
    .fetch_optional(db)
    .await
}

pub async fn attach<'e, E: SqliteExecutor<'e>>(
    db: E,
    schematic_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT OR IGNORE INTO schematic_tag (schematic_id, tag_id)
         VALUES (?, ?)",
        schematic_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn detach<'e, E: SqliteExecutor<'e>>(
    db: E,
    schematic_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM schematic_tag WHERE schematic_id = ? AND tag_id = ?",
        schematic_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Every tag, by kind then name.
pub async fn all<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        r#"SELECT name, token, kind AS "kind: Kind" FROM tag
         ORDER BY kind = 'other', kind, name"#
    )
    .fetch_all(db)
    .await
}

pub async fn define<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
    token: Option<&str>,
    kind: Kind,
) -> Result<(), sqlx::Error> {
    let name = self::name(name);
    sqlx::query!(
        "INSERT INTO tag (name, token, kind) VALUES (?, ?, ?)",
        name,
        token,
        kind
    )
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_may_hold_a_dash_but_not_at_either_end() {
        assert!(valid_name("double-density"));
        assert!(valid_name(&name("Double-Density CCA")));
        assert!(!valid_name("-density"));
        assert!(!valid_name("double-"));
        assert!(!valid_name("double.density"));
        assert!(!valid_name(""));
    }

    fn tag(name: &str, kind: Kind) -> Tag {
        Tag {
            name: name.to_owned(),
            token: None,
            kind,
        }
    }

    #[test]
    fn a_build_without_torches_is_torchless_among_its_restrictions() {
        let stored =
            vec![tag("alpha", Kind::Restriction), tag("buffer", Kind::Other)];
        let names = |tags: Vec<Tag>| -> Vec<String> {
            tags.into_iter().map(|tag| tag.name).collect()
        };
        assert_eq!(
            names(with_derived(
                stored.clone(),
                Some(Unverified::Failing),
                Some(0),
                None
            )),
            ["failing", "alpha", "torchless", "buffer"]
        );
        assert_eq!(
            names(with_derived(stored.clone(), None, Some(3), None)),
            ["alpha", "buffer"]
        );
        assert_eq!(
            names(with_derived(stored, None, None, None)),
            ["alpha", "buffer"],
            "an uncounted build is not torchless yet"
        );
        // Alpha is read from the file too, before torchless by name.
        let other = vec![tag("buffer", Kind::Other)];
        assert_eq!(
            names(with_derived(other, None, Some(0), Some("Alpha"))),
            ["alpha", "torchless", "buffer"]
        );
    }
}
