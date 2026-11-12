//! Storing a build: which of an upload's files it keeps, the roles its
//! lines carry, and the build itself.

use crate::{
    build::{Prepared, Ready},
    category::{self, Category, Kind},
    ingest::{
        details,
        prepare::{Purpose, Upload, prepare_for, rewrapped},
    },
    interface::{self},
    recognize::probe,
    schem::{self, Blocks},
    store::{
        builds::{self, New},
        versions,
    },
};
use anyhow::Result;
use sqlx::SqlitePool;

/// Stores a new build, its file as prepared and its details as checked,
/// uploaded by `uploaded_by` from IP address `uploaded_from`, and returns
/// its id; the `Err` says the file is already stored.
pub async fn store(
    db: &SqlitePool,
    prepared: &Prepared,
    ready: &Ready,
    uploaded_by: Option<uuid::Uuid>,
    uploaded_from: Option<&str>,
    imported_by: Option<&str>,
    id: Option<&str>,
) -> Result<Result<String, String>, sqlx::Error> {
    if let Some((id, _)) = versions::holder(db, &prepared.sha256).await? {
        return Ok(Err(format!(
            "This exact file is already on the site, as {}.",
            &id[..8]
        )));
    }
    let new = New {
        ready,
        file: prepared,
        uploaded_by,
        uploaded_from,
        imported_by,
        id,
    };
    Ok(Ok(builds::insert(db, &new).await?))
}

/// Which of an upload's files a build of the category it is given keeps,
/// and how it is filed.
pub enum Kept {
    /// Standardized and turned to lie the standard way, as a build
    /// recognized as its category is kept.
    Standard(Box<Prepared>),
    /// As built, unverified as its category, one the site recognizes
    /// builds of, for this reason.
    Unverified(String),
    /// As built, in the category it was given, one of its author's own.
    Original,
}

/// Which of `upload`'s files a build given as `category` keeps: the
/// standard one when it is recognized as that category, a standard one;
/// else the original. The `Err` says it could not be simulated.
pub async fn kept(
    categories: &[Category],
    category: &str,
    upload: &Upload,
) -> anyhow::Result<Result<Kept, String>> {
    let Some(listed) = Category::find(categories, category) else {
        return Ok(Ok(Kept::Original));
    };
    let unrecognized = match &upload.standard {
        Ok(standard) => {
            details::unrecognized(listed, details::Candidate::of(standard))
                .await
        }
        // Read as built, it can only say it breaks the conventions.
        Err(_) => {
            details::unrecognized(
                listed,
                details::Candidate {
                    data: &upload.original.data,
                    conventional: false,
                    passes: false,
                },
            )
            .await
        }
    };
    Ok(match (unrecognized, &upload.standard) {
        (Err(message), _) => Err(message),
        (Ok(Some(why)), _) => Ok(Kept::Unverified(why)),
        (Ok(None), Ok(standard)) => {
            Ok(Kept::Standard(Box::new(rewrapped(standard)?)))
        }
        (Ok(None), Err(_)) => Ok(Kept::Original),
    })
}

/// The file in `data`, a build's of `category`, carrying the roles of its
/// lines, when it carries none: the roles probing as its category finds,
/// for a category the site recognizes builds of, its lines read loosely
/// as a build kept as built may need, see [`probe::kept_roles`]; else, or
/// when no probe's test runs, those its shapes give it, see
/// [`interface::shaped_roles`]. Its colours never give a line its role,
/// but a file carrying no roles is a stored build's from before the site
/// kept them, or a port of one to the rule for outputs, so its colours are
/// the marking the site once read: of two operands its test passes either
/// way round, A is the one they mark A, see
/// [`crate::recognize::legacy::operand_marks`] and [`probe::Order`]. A file
/// carrying roles comes back as it is.
pub fn with_roles(data: &[u8], category: &str) -> Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    if blocks.roles.is_some() {
        return Ok(data.to_vec());
    }
    let meant = Kind::named(category);
    let marked = crate::recognize::legacy::operand_marks(&blocks);
    let order = probe::Order {
        marked: Some(&marked),
    };
    let probed = match meant {
        Some(kind) => probe::kept_roles(data, kind, order)?,
        None => None,
    };
    let roles = probed.unwrap_or_else(|| interface::shaped_roles(&blocks));
    schem::lines::with(data, Some(&roles))
}

/// The file in `data` carrying the roles probing finds for its inputs and
/// outputs, as an upload's are, for a build meant as one of a standard
/// category: its inputs are its levers
/// and buttons, whatever colour they sit on. Any other build, or one
/// probing doesn't recognize, as it is.
pub async fn probed(data: &[u8], meant: &str) -> Result<Vec<u8>> {
    if !category::listed(meant) {
        return Ok(data.to_vec());
    }
    let (data, meant) = (data.to_vec(), meant.to_owned());
    // Probing simulates, so off the async threads.
    let coloured = tokio::task::spawn_blocking(move || {
        match crate::recognize::probe::recognize_as(&data, Some(&meant)) {
            Ok(recognition) if recognition.found.is_some() => recognition.file,
            // One the simulator can't run, as with a block it doesn't
            // model, isn't recognized either.
            _ => data,
        }
    })
    .await?;
    Ok(coloured)
}

/// A build unverified as `category`, verified by its new version `file`:
/// when that is recognized as the category, the file standardized and
/// turned to lie the standard way, as an upload's is. `None` for one its
/// version does not verify.
pub async fn verified_by(
    category: &Category,
    file: &Prepared,
    keep_paste: bool,
) -> Result<Option<Prepared>> {
    if !category::listed(&category.name) {
        return Ok(None);
    }
    let coloured = probed(&file.data, &category.name).await?;
    let Ok(candidate) =
        prepare_for(&coloured, Purpose::Standard { keep_paste })
    else {
        return Ok(None);
    };
    let recognized =
        details::unrecognized(category, details::Candidate::of(&candidate))
            .await
            .is_ok_and(|why| why.is_none());
    if !recognized {
        return Ok(None);
    }
    Ok(Some(rewrapped(&candidate)?))
}
