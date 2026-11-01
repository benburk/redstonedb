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
        /// That the build fails its simulation, never stored.
        #[value(skip)]
        Failing = "failing", "Simulation";
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

/// The name of the tag simulation puts on a build that gets some cases
/// wrong. It is not stored, so no one can add or remove it.
pub const FAILS_SIMULATION: &str = "fails_simulation";

/// The name of the tag a build whose file has no redstone torches carries.
/// It is not stored, so no one can add or remove it.
const TORCHLESS: &str = "torchless";

/// The name of the tag a build carries that works from Alpha, using no
/// redstone component or mechanic added since. It is not stored either.
const ALPHA: &str = "alpha";

/// The tags worked out from a build rather than given by a person.
pub const DERIVED: [&str; 3] = [FAILS_SIMULATION, TORCHLESS, ALPHA];

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

    /// The class that colours its badge: failing in the colour of danger,
    /// any other the site finds in its own, and one a person gives by its
    /// kind.
    pub fn class(&self) -> &'static str {
        match self.kind {
            Kind::Failing => "badge-failing",
            _ if self.is_found() => "badge-found",
            Kind::Restriction => "badge-restriction",
            _ => "badge-other",
        }
    }

    /// What its badge's tooltip says it is.
    pub fn about(&self) -> String {
        if self.is_found() && self.kind != Kind::Failing {
            format!("{}, found by the site", self.kind.label())
        } else {
            self.kind.label().to_owned()
        }
    }

    /// The name as words, such as `carry lookahead`.
    pub fn label(&self) -> String {
        self.name.replace('_', " ")
    }

    /// How a filename writes this tag.
    pub fn token(&self) -> &str {
        self.token.as_deref().unwrap_or(&self.name)
    }
}

/// The form of a tag name a person types, with spaces for underscores.
pub fn name(typed: &str) -> String {
    typed.trim().to_lowercase().replace(' ', "_")
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
        false,
        Some(facts.torches),
        Some(facts.works_from.release),
    )
}

/// A build's stored `tags`, led by `fails_simulation` when it fails, with
/// `torchless` among its restrictions when its file has no torches, and
/// `alpha` when it works from Alpha.
pub(super) fn with_derived(
    mut tags: Vec<Tag>,
    fails_simulation: bool,
    torches: Option<i64>,
    works_from: Option<&str>,
) -> Vec<Tag> {
    let alpha = works_from == Some(crate::works_from::RELEASES[0].name);
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
    if fails_simulation {
        tags.insert(0, Tag::derived(FAILS_SIMULATION, Kind::Failing));
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
            names(with_derived(stored.clone(), true, Some(0), None)),
            ["fails_simulation", "alpha", "torchless", "buffer"]
        );
        assert_eq!(
            names(with_derived(stored.clone(), false, Some(3), None)),
            ["alpha", "buffer"]
        );
        assert_eq!(
            names(with_derived(stored, false, None, None)),
            ["alpha", "buffer"],
            "an uncounted build is not torchless yet"
        );
        // Alpha is read from the file too, before torchless by name.
        let other = vec![tag("buffer", Kind::Other)];
        assert_eq!(
            names(with_derived(other, false, Some(0), Some("Alpha"))),
            ["alpha", "torchless", "buffer"]
        );
    }
}
