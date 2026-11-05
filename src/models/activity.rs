//! The moderation log: accounts made, builds and versions uploaded,
//! details edited, and files downloaded, newest first. Each is read from
//! where the site keeps it already, but edits, which `edit` keeps for the
//! log alone.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use super::{
    schematic::{Schematic, StackDirection},
    user::REDSTONEDB,
};

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An account was made, at its first login.
    SignUp,
    /// A build was uploaded or imported, as its first version.
    Upload,
    /// A build was given a later version, or an earlier one restored.
    Version,
    /// A build's details were edited.
    Edit,
    /// A build's file, or part of it, was downloaded.
    Download,
}

impl Kind {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "signup" => Self::SignUp,
            "upload" => Self::Upload,
            "version" => Self::Version,
            "edit" => Self::Edit,
            "download" => Self::Download,
            _ => return None,
        })
    }
}

/// Which events the log shows, as its buttons choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    SignUps,
    /// New builds and new versions.
    Uploads,
    Edits,
    Downloads,
}

impl Filter {
    pub const ALL: [Filter; 5] = [
        Filter::All,
        Filter::SignUps,
        Filter::Uploads,
        Filter::Edits,
        Filter::Downloads,
    ];

    /// Its name in the address, such as `signups`; empty for all.
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "",
            Self::SignUps => "signups",
            Self::Uploads => "uploads",
            Self::Edits => "edits",
            Self::Downloads => "downloads",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::SignUps => "Sign-ups",
            Self::Uploads => "Uploads",
            Self::Edits => "Edits",
            Self::Downloads => "Downloads",
        }
    }

    /// The filter named `name`; all for any other name.
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == name)
            .unwrap_or(Self::All)
    }

    /// The kind its events are, for the query: `None` for every kind.
    fn kind(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::SignUps => Some("signup"),
            Self::Uploads => Some("upload"),
            Self::Edits => Some("edit"),
            Self::Downloads => Some("download"),
        }
    }
}

/// One thing that happened.
#[derive(Debug, Clone)]
pub struct Event {
    pub kind: Kind,
    /// Such as `2026-11-04T17:03:12Z`.
    pub at: String,
    /// Who did it: the account made, for a sign-up; `None` for a deleted
    /// account, or a build imported with rsdb.
    pub user: Option<Uuid>,
    /// The build it was done to; `None` for a sign-up.
    pub schematic: Option<String>,
    /// The version uploaded or downloaded.
    pub version: Option<i64>,
    /// For a version, its note; for a download, what it took, such as
    /// `part`.
    pub detail: Option<String>,
    /// For a version, the one it restores.
    pub restores: Option<i64>,
    /// For an edit, each detail it changed, before and after.
    pub changes: Vec<Change>,
}

/// A page of the log: its events, newest first, and the time to pass as
/// `before` for the next, older page, if there is one.
pub struct Page {
    pub events: Vec<Event>,
    pub older: Option<String>,
}

/// Up to `limit` events `filter` shows, newest first, from those at or
/// before `before`, or the newest for `None`. A page never splits the
/// events of one second, unless more than `limit` share it.
pub async fn page(
    db: &SqlitePool,
    filter: Filter,
    before: Option<&str>,
    limit: i64,
) -> Result<Page, sqlx::Error> {
    let kind = filter.kind();
    let fetch = limit + 1;
    let rows = sqlx::query!(
        r#"SELECT kind as "kind!: String", at as "at!: String",
            user_id as "user_id?: Uuid", schematic_id as "schematic_id?: String",
            version as "version?: i64", detail as "detail?: String",
            restores as "restores?: i64"
         FROM (
            SELECT 'signup' AS kind, created_at AS at, id AS user_id,
                NULL AS schematic_id, NULL AS version, NULL AS detail,
                NULL AS restores
            FROM user WHERE id <> ?1
            UNION ALL
            SELECT iif(version = 1, 'upload', 'version'), added_at, added_by,
                schematic_id, version, note, restores
            FROM schematic_version
            UNION ALL
            SELECT 'edit', at, user_id, schematic_id, NULL, changes, NULL
            FROM edit
            UNION ALL
            SELECT 'download', at, user_id, schematic_id, version, kind, NULL
            FROM download
         )
         WHERE (?2 IS NULL OR kind = ?2 OR (?2 = 'upload' AND kind = 'version'))
            AND (?3 IS NULL OR at <= ?3)
         ORDER BY at DESC
         LIMIT ?4"#,
        REDSTONEDB,
        kind,
        before,
        fetch
    )
    .fetch_all(db)
    .await?;
    let mut events: Vec<Event> = rows
        .into_iter()
        .filter_map(|row| {
            let kind = Kind::parse(&row.kind)?;
            let (detail, changes) = if kind == Kind::Edit {
                let changes = row
                    .detail
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok())
                    .unwrap_or_default();
                (None, changes)
            } else {
                (row.detail, Vec::new())
            };
            Some(Event {
                kind,
                at: row.at,
                user: row.user_id,
                schematic: row.schematic_id,
                version: row.version,
                detail,
                restores: row.restores,
                changes,
            })
        })
        .collect();
    let older = if i64::try_from(events.len()).unwrap_or(i64::MAX) > limit {
        // The next page starts at the second of the first event left off,
        // which this page leaves off whole.
        let next = events.pop().map(|e| e.at).unwrap_or_default();
        let whole = events.iter().filter(|e| e.at != next).count();
        if whole == 0 {
            // More than a page in one second: the rest of it is skipped.
            Some(older_than(&next))
        } else {
            events.truncate(whole);
            Some(next)
        }
    } else {
        None
    };
    Ok(Page { events, older })
}

/// The last time that sorts before second `at`, for `at <=` to leave it
/// out.
fn older_than(at: &str) -> String {
    // `Z` sorts after every digit, so a time ending `Y` sorts before the
    // same second and after every earlier one.
    match at.strip_suffix('Z') {
        Some(time) => format!("{time}Y"),
        None => at.to_owned(),
    }
}

/// One detail an edit changed, as text, such as the authors before and
/// after; `None` for a detail that was empty.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Change {
    /// As the log names it, such as `description`.
    pub detail: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// Build `s`'s details an edit can change, each as the log names it and
/// as text; a new file is a version of its own, which this leaves out.
fn details(s: &Schematic) -> Vec<(&'static str, Option<String>)> {
    let list =
        |items: Vec<String>| Some(items.join(", ")).filter(|l| !l.is_empty());
    let number = |n: Option<i64>| n.map(|n| n.to_string());
    vec![
        ("category", Some(s.category_label())),
        ("name", s.given_name.clone()),
        ("design", s.design_label()),
        ("description", s.description.clone()),
        ("date built", s.built_on.clone()),
        ("width", number(s.width)),
        ("orientation", Some(s.orientation.label().to_owned())),
        ("encoding", s.encoding.map(|e| e.label().to_owned())),
        ("logic", s.own_logic.map(|l| l.label().to_owned())),
        ("latency", number(s.latency)),
        (
            "synchronized",
            s.synchronized
                .map(|yes| if yes { "yes" } else { "no" }.to_owned()),
        ),
        ("throughput", number(s.throughput)),
        ("at least", number(s.at_least)),
        ("pulse", number(s.pulse)),
        ("bytes", number(s.bytes)),
        (
            "flags",
            list(s.flags.iter().map(|f| f.label.to_owned()).collect()),
        ),
        ("authors", list(s.authors.clone())),
        ("contributors", list(s.contributors.clone())),
        (
            "tags",
            list(
                s.tags
                    .iter()
                    .filter(|t| !t.is_derived())
                    .map(|t| t.name.clone())
                    .collect(),
            ),
        ),
        (
            "stacking",
            list(s.stacking.iter().map(StackDirection::describe).collect()),
        ),
        (
            "visibility",
            Some(if s.public { "public" } else { "private" }.to_owned()),
        ),
    ]
}

/// What an edit changed of build `before`, which is `after` now: each
/// detail whose text differs, with its text before and after.
pub fn changed(before: &Schematic, after: &Schematic) -> Vec<Change> {
    details(before)
        .into_iter()
        .zip(details(after))
        .filter(|((_, from), (_, to))| from != to)
        .map(|((detail, from), (_, to))| Change {
            detail: detail.to_owned(),
            from,
            to,
        })
        .collect()
}

/// Keeps `user`'s edit of build `id`, which made `changes`; an edit that
/// changed nothing is not kept.
pub async fn log_edit<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    user: Uuid,
    changes: &[Change],
) -> Result<(), sqlx::Error> {
    if changes.is_empty() {
        return Ok(());
    }
    let changes =
        serde_json::to_string(changes).expect("a list of changes is JSON");
    sqlx::query!(
        "INSERT INTO edit (schematic_id, user_id, changes) VALUES (?, ?, ?)",
        id,
        user,
        changes
    )
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_than_sorts_between_seconds() {
        let at = "2026-11-04T17:03:12Z";
        let older = older_than(at);
        assert!(older.as_str() < at);
        assert!(older.as_str() > "2026-11-04T17:03:11Z");
    }

    #[test]
    fn a_filter_is_found_by_its_name() {
        for filter in Filter::ALL {
            assert_eq!(Filter::named(filter.name()), filter);
        }
        assert_eq!(Filter::named("nonsense"), Filter::All);
    }
}
