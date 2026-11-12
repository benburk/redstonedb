//! A new file taken as a build's next version: prepared, compared
//! with the current one, and refused where it cannot replace it; and
//! one build merged into another as its next version.

use crate::{
    build::{Build, Prepared},
    compare::blocks::{Comparison, compare, warnings},
    ingest::{
        prepare::{self, Purpose},
        publish,
    },
    interface::inspect,
    schem::{
        self, Blocks, Pos,
        turn::{self, Turn},
    },
    store::versions,
};
use sqlx::SqlitePool;

/// The number of build `s`'s version laid block for block as `blocks`,
/// how its levers stand and what is powered aside, if one is.
async fn laid_as(
    db: &SqlitePool,
    s: &Build,
    blocks: &Blocks,
) -> anyhow::Result<Option<i64>> {
    let placed = |blocks: &Blocks| -> (Pos, Vec<(Pos, String)>) {
        let states = blocks
            .iter()
            .map(|(pos, state)| (pos, schem::as_placed(state)))
            .collect();
        (blocks.size, states)
    };
    let new = placed(blocks);
    for listed in versions::list(db, &s.id).await? {
        let Some(data) = versions::data(db, &s.id, listed.version).await?
        else {
            continue;
        };
        if placed(&Blocks::read(&data)?) == new {
            return Ok(Some(listed.version));
        }
    }
    Ok(None)
}

/// A file prepared as the build's next version, how it compares with the
/// current one, why it cannot replace it, and what to warn of when it
/// may.
pub struct Checked {
    pub prepared: Prepared,
    pub comparison: Comparison,
    pub refusals: Vec<String>,
    pub warnings: Vec<String>,
}

/// Prepares `file`, standardized if asked, as build `s`'s next version and
/// checks it against the current one.
pub async fn check(
    db: &SqlitePool,
    s: &Build,
    file: &[u8],
    standardize: bool,
) -> anyhow::Result<Checked> {
    checked(db, s, file, standardize, None).await
}

/// [`check`], but for a file build `leaving` holds, which a merge moves
/// into `s` and so may be on the site already.
async fn checked(
    db: &SqlitePool,
    s: &Build,
    file: &[u8],
    standardize: bool,
    leaving: Option<&str>,
) -> anyhow::Result<Checked> {
    // An adder's or register's inputs are its levers and buttons, given
    // their roles by probing before standardizing, as an upload's are.
    let coloured = if standardize {
        publish::probed(file, &s.category).await?
    } else {
        file.to_vec()
    };
    let current = versions::current_file(db, &s.id).await?.unwrap_or_default();
    let current = Blocks::read(&current)?;
    // Saved again from the game, it may lie another way than the version
    // before: it is compared turned and mirrored as matches that best.
    let mut best: Option<(Prepared, Comparison)> = None;
    for mirrored in [false, true] {
        for quarters in 0..4 {
            let turned = turn::turned(&coloured, Turn { mirrored, quarters })?;
            let prepared =
                prepare::prepare_for(&turned, Purpose::of(s, standardize))?;
            let comparison = compare(&current, &prepared.blocks);
            if best
                .as_ref()
                .is_none_or(|(_, b)| comparison.changed < b.changed)
            {
                best = Some((prepared, comparison));
            }
        }
    }
    let (prepared, comparison) = best.expect("it was compared at least once");
    let warnings = warnings(comparison);
    let mut refusals = Vec::new();
    // Only problems the current version doesn't have count against it, so
    // a build from before a rule can still be fixed. They are the check by
    // function's, as an upload's, colour playing no part: inputs are its
    // levers, buttons, and comparators with air behind, and outputs its
    // repeaters and comparators facing air.
    if s.file.conventional {
        // Dust on top of an input is a fault only on a data input, which
        // probing a recognized build checks.
        let current = inspect::inspect(&current);
        let had: Vec<&str> =
            current.faults().map(|problem| problem.rule).collect();
        let found = inspect::inspect(&prepared.blocks);
        let broken: Vec<&str> = found
            .faults()
            .filter(|problem| !had.contains(&problem.rule))
            .map(|problem| problem.message.as_str())
            .collect();
        if let Some(first) = broken.first() {
            refusals.push(format!(
                "It breaks {} building conventions the current version \
                 keeps, such as: {first}",
                broken.len()
            ));
        }
    }
    let holder = versions::holder(db, &prepared.sha256).await?;
    match holder {
        Some((other, number)) if other == s.id => {
            refusals.push(format!("It is the same file as version {number}."));
        }
        Some((other, _)) if leaving == Some(other.as_str()) => {}
        Some(_) => refusals.push(
            "This exact file is already on the site as another build."
                .to_owned(),
        ),
        None => {
            // Laid the same as a version before, only how its levers stood
            // and its power differing, it is that version again, which
            // comes to rest with its inputs at logical 0 all the same.
            if let Some(number) = laid_as(db, s, &prepared.blocks).await? {
                refusals.push(format!(
                    "It is version {number} again, only its levers set \
                     differently."
                ));
            }
        }
    }
    Ok(Checked {
        prepared,
        comparison,
        refusals,
        warnings,
    })
}

/// Moves build `from`'s current file into build `into` as its next version,
/// added by `by`, with `note`, its colours kept, and deletes `from`, as a
/// build uploaded on its own is folded into the one it fixes: the file is
/// checked as any new version of `into` is, but for `from` holding it.
/// Returns the version's number, or why it can't be one.
pub async fn merge(
    db: &SqlitePool,
    from: &Build,
    into: &Build,
    by: Option<uuid::Uuid>,
    note: &str,
) -> anyhow::Result<Result<i64, Vec<String>>> {
    if from.id == into.id {
        return Ok(Err(vec![
            "A build can't be merged into itself.".to_owned(),
        ]));
    }
    let Some(number) = versions::list(db, &from.id)
        .await?
        .first()
        .map(|v| v.version)
    else {
        return Ok(Err(vec!["It has no file to merge.".to_owned()]));
    };
    let file = versions::file(db, &from.id, number)
        .await?
        .ok_or_else(|| anyhow::anyhow!("its version {number} has no file"))?;
    let checked = checked(db, into, &file, false, Some(&from.id)).await?;
    if !checked.refusals.is_empty() {
        return Ok(Err(checked.refusals));
    }
    let colours = versions::colours_of(db, &from.id, number).await?;
    // A file is held by one version at most, so `from` goes first, its file
    // and colours read already and every check passed.
    crate::store::builds::purge(db, &from.id).await?;
    let how = versions::Replacing::NextVersion {
        added_by: by,
        added_from: None,
        note: Some(note),
        restores: None,
    };
    let added = versions::replace(db, &into.id, &checked.prepared, how).await?;
    if let Some(colours) = colours {
        versions::set_colours(db, &into.id, added, &colours).await?;
    }
    Ok(Ok(added))
}
