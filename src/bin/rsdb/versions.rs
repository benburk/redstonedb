//! Changing a stored build's file: as a new version, checked as the site
//! checks one or with its unused blocks taken out, or in place as a fix to
//! the file; and reading a stored file's facts again.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use app::{
    category::Category,
    details::{self, Details},
    import,
    models::{
        schematic, simulation,
        version::{self, Replacing},
    },
};
use clap::Args;
use sqlx::SqlitePool;

use super::{
    load,
    refresh::{Refreshing, print_report, refresh_one},
    resolve,
};

#[derive(Args)]
pub(super) struct Standardizing {
    /// Probe it for its roles and take out what is no part of it, its
    /// blocks keeping their colours, before linting; adding also turns the
    /// build so A's levers face south, and mirrors it when that reads right
    /// as what it seems to be
    #[arg(long)]
    standardize: bool,
}

impl Standardizing {
    /// Whether to standardize.
    fn asked(&self) -> bool {
        self.standardize
    }
}

/// Reads build `id`'s stored file again as an upload's is prepared, and
/// prints what changed; see [`refile`].
pub(super) async fn reread(db: &SqlitePool, id: &str) -> Result<()> {
    let s = load(db, id).await?;
    let stored = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    refile(db, &s, &stored, false).await
}

/// Prepares `file` as build `s`'s file, standardized if asked, as an
/// upload's is, keeping where it pastes if the build does: puts it in place
/// of the current version if it, its facts, or its size differ from what
/// is stored, saying what changed. No version is added and nothing is
/// compared.
async fn refile(
    db: &SqlitePool,
    s: &schematic::Build,
    file: &[u8],
    standardize: bool,
) -> Result<()> {
    let prepared =
        import::prepare_for(file, import::Purpose::of(s, standardize))?;
    let facts = &prepared.facts;
    let stored = (
        s.torches,
        s.works_from.as_deref(),
        s.works_from_by.as_deref(),
    );
    let read = (
        Some(facts.torches),
        Some(facts.works_from.release),
        facts.works_from.because.as_deref(),
    );
    let size = &prepared.size;
    let size = (size.x.into(), size.y.into(), size.z.into());
    let rewritten = prepared.sha256 != s.sha256;
    if rewritten || stored != read || size != s.size {
        version::replace(db, &s.id, &prepared, Replacing::InPlace).await?;
    }
    if rewritten {
        println!("{}  file rewritten", &s.id[..8]);
    }
    if size != s.size {
        let (x, y, z) = s.size;
        let (nx, ny, nz) = size;
        println!("{}  {y}x{x}x{z} -> {ny}x{nx}x{nz}", &s.id[..8]);
    }
    Ok(())
}

/// Replaces build `id`'s file with `file` as its next version, after the
/// checks the site makes, or with `in_place` puts it in place of the
/// current one as a fix to the file, no version added and nothing
/// compared; then refreshes the build, as `rsdb refresh` does, saying how
/// its test went.
pub(super) async fn new_version(
    db: &SqlitePool,
    id: &str,
    file: &Path,
    note: Option<&str>,
    in_place: bool,
    standardizing: &Standardizing,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let s = load(db, &id).await?;
    let raw = fs::read(file)?;
    let name = format!("{}  {}", &id[..8], s.filename());
    if in_place {
        refile(db, &s, &raw, standardizing.asked()).await?;
    } else {
        next_version(db, &s, &raw, file, note, standardizing).await?;
    }
    let how = Refreshing {
        again: false,
        rejudge: true,
        prune: false,
        probe: false,
    };
    if let Some(report) = refresh_one(db, &id, how).await? {
        print_report(&name, &report);
    }
    Ok(())
}

/// Merges build `id` into build `into`, as the site does: its current file
/// becomes `into`'s next version, added as the site's own, if it may be one,
/// and `id` is deleted; then `into` is refreshed.
pub(super) async fn merge(
    db: &SqlitePool,
    id: &str,
    into: &str,
    note: Option<&str>,
) -> Result<()> {
    let from = load(db, &resolve(db, id).await?).await?;
    let into = load(db, &resolve(db, into).await?).await?;
    let note = match note.map(str::trim) {
        Some(note) if !note.is_empty() => note.to_owned(),
        _ => format!("Merged from {}, {}", from.name(), &from.id[..8]),
    };
    let by = Some(app::models::user::REDSTONEDB);
    match app::version::merge(db, &from, &into, by, &note).await? {
        Ok(number) => println!(
            "merged {}  {} into {}  {}: version {number}",
            &from.id[..8],
            from.filename(),
            &into.id[..8],
            into.filename()
        ),
        Err(refusals) => {
            for refusal in &refusals {
                println!("{refusal}");
            }
            bail!("{} cannot be merged into {}", &from.id[..8], &into.id[..8]);
        }
    }
    let how = Refreshing {
        again: false,
        rejudge: true,
        prune: false,
        probe: false,
    };
    let name = format!("{}  {}", &into.id[..8], into.filename());
    if let Some(report) = refresh_one(db, &into.id, how).await? {
        print_report(&name, &report);
    }
    Ok(())
}

/// Adds `raw`, read from `file`, as build `s`'s next version, after the
/// checks the site makes, keeping the colours its author gave it, and
/// moving the build up to the category it stands in for when the version
/// is recognized as one, as on the site.
async fn next_version(
    db: &SqlitePool,
    s: &schematic::Build,
    raw: &[u8],
    file: &Path,
    note: Option<&str>,
    standardizing: &Standardizing,
) -> Result<()> {
    let id = &s.id;
    let checked =
        app::version::check(db, s, raw, standardizing.asked()).await?;
    let comparison = checked.comparison;
    println!(
        "changed {} of {} blocks",
        comparison.changed,
        comparison.changed + comparison.same
    );
    for warning in &checked.warnings {
        println!("warning: {warning}");
    }
    if !checked.refusals.is_empty() {
        for refusal in &checked.refusals {
            println!("{refusal}");
        }
        bail!("{} cannot replace {}", file.display(), &id[..8]);
    }
    // Any new version from here is the site's own, as its pruning's is, so
    // its log names RedstoneDB rather than no account.
    let how = Replacing::NextVersion {
        added_by: Some(app::models::user::REDSTONEDB),
        added_from: None,
        note: note.map(str::trim).filter(|note| !note.is_empty()),
        restores: None,
    };
    // Unverified or off the list, a version recognized as the category it
    // stands in for moves it up, as on the site.
    let categories = Category::all();
    let moved = match simulation::stands_in(s) {
        Some(meant) => {
            import::moved_up(
                &categories,
                meant,
                &checked.prepared,
                s.keeps_paste,
            )
            .await?
        }
        None => None,
    };
    let kept = moved.as_ref().map_or(&checked.prepared, |(file, _)| file);
    let number = version::replace(db, id, kept, how).await?;
    let name = format!("{}  {}", &id[..8], s.filename());
    println!("{name}: version {number}");
    // The colours its author gave it, read off the file given.
    match app::recognize::colouring::read(raw, &kept.data)? {
        Ok(colours) if !colours.is_empty() => {
            version::set_colours(db, id, number, &colours).await?;
            println!("{name}: kept {} blocks' colours", colours.len());
        }
        Ok(_) => {}
        Err(faults) => println!(
            "{name}: no colours kept, as the file differs from the version: {}",
            faults.join("; ")
        ),
    }
    if let Some((file, to)) = &moved {
        let given = Details {
            category: to.name.clone(),
            unverified: None,
            ..Details::of(s)
        };
        match details::check(db, given, details::Candidate::of(file), true)
            .await?
        {
            Ok(ready) => {
                schematic::set_details(db, id, &ready).await?;
                println!("{name}: moved up as {}", to.name);
            }
            Err(why) => println!("{name}: not moved up: {why}"),
        }
    }
    Ok(())
}

/// Takes the blocks no part of build `id`'s circuit uses out of it, as a
/// new version, only where the site's test passes and its timing comes out
/// the same, then simulates it again; or with `dry_run` says what would be
/// taken out, changing nothing. A build with no test is left as it is.
pub(super) async fn prune(
    db: &SqlitePool,
    id: &str,
    dry_run: bool,
) -> Result<()> {
    let s = load(db, id).await?;
    let name = format!("{}  {}", &id[..8], s.filename());
    if app::simulate::Test::of(&s.category, s.width).is_none() {
        return Ok(());
    }
    let data = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    let buses = app::schem::stack::Buses::of(s.kind(), s.orientation);
    let pruned = match app::recognize::roles::prune(&data, &s.stacking, buses) {
        Ok(Some(pruned)) => pruned,
        Ok(None) => return Ok(()),
        Err(err) => {
            println!("skip    {name}: {err}");
            return Ok(());
        }
    };
    let checked = app::version::check(db, &s, &pruned.file, false).await?;
    if let Some(refusal) = checked.refusals.first() {
        println!("skip    {name}: {refusal}");
        return Ok(());
    }
    let verb = if dry_run { "would" } else { "pruned" };
    let n = pruned.gone.len();
    let gone: Vec<String> = pruned
        .gone
        .iter()
        .map(|[x, y, z]| format!("{x},{y},{z}"))
        .collect();
    println!("{verb} {name}: {n} blocks: {}", gone.join(" "));
    if dry_run {
        return Ok(());
    }
    let how = Replacing::NextVersion {
        added_by: Some(app::models::user::REDSTONEDB),
        added_from: None,
        note: Some("Unused blocks taken out"),
        restores: None,
    };
    version::replace(db, id, &checked.prepared, how).await?;
    if simulation::refresh(db, id, false).await?.is_some() {
        simulation::refresh_timing(db, id, false).await?;
    }
    simulation::keep_needs(db, id).await?;
    Ok(())
}
