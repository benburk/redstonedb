//! What an edit changed of a build, detail by detail, as the moderation
//! log keeps it, and which of those can be put back.

use super::Build;
use crate::stack::StackDirection;

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
fn details(s: &Build) -> Vec<(&'static str, Option<String>)> {
    let list =
        |items: Vec<String>| Some(items.join(", ")).filter(|l| !l.is_empty());
    let number = |n: Option<i64>| n.map(|n| n.to_string());
    vec![
        ("category", Some(s.category_label())),
        ("design", s.design_label()),
        ("description", s.description.clone()),
        ("date built", s.built_on.clone()),
        ("width", number(s.specs.width)),
        ("orientation", Some(s.specs.orientation.label().to_owned())),
        ("encoding", s.specs.encoding.map(|e| e.label().to_owned())),
        ("logic", s.specs.logic.map(|l| l.label().to_owned())),
        ("latency", number(s.specs.latency)),
        (
            "synchronized",
            s.specs
                .synchronized
                .map(|yes| if yes { "yes" } else { "no" }.to_owned()),
        ),
        ("throughput", number(s.specs.throughput)),
        ("at least", number(s.specs.at_least)),
        ("pulse", number(s.specs.pulse)),
        (
            "flags",
            list(s.flags.iter().map(|f| f.label.to_owned()).collect()),
        ),
        ("authors", list(s.credits.authors.clone())),
        ("contributors", list(s.credits.contributors.clone())),
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
            Some(if s.entry.public { "public" } else { "private" }.to_owned()),
        ),
    ]
}

/// What an edit changed of build `before`, which is `after` now: each
/// detail whose text differs, with its text before and after.
pub fn changed(before: &Build, after: &Build) -> Vec<Change> {
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

/// A detail the moderation log can put back as an edit left it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restorable {
    Description,
    Authors,
    Contributors,
}

impl Restorable {
    /// The detail the log names `name`, if it can be put back.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "description" => Some(Self::Description),
            "authors" => Some(Self::Authors),
            "contributors" => Some(Self::Contributors),
            _ => None,
        }
    }
}

/// Whether `change`, an edit of build `s`, can be put back: a description
/// or credits, still as the edit left them.
pub fn revertible(s: &Build, change: &Change) -> bool {
    Restorable::named(&change.detail).is_some()
        && details(s)
            .into_iter()
            .any(|(detail, now)| detail == change.detail && now == change.to)
}
