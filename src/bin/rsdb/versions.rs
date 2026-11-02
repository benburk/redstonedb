//! Changing a stored build's file: as a new version, checked as the site
//! checks one or with its unused blocks taken out, or in place as a fix to
//! the file; reading every file again; and keeping a builder's colours.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use app::{
    details::{self, Details},
    import,
    models::{
        catalogue, schematic, simulation,
        version::{self, Replacing},
    },
};
use clap::Args;
use sqlx::SqlitePool;

use super::{load, resolve, simulate::print_report};

#[derive(Args)]
pub(super) struct Replace {
    id: String,
    file: PathBuf,
    #[command(flatten)]
    standardizing: Standardizing,
}

#[derive(Args)]
pub(super) struct Standardizing {
    /// Whiten every block but redstone components, inputs, and outputs,
    /// and turn slabs and glass into white stained glass, before linting;
    /// adding also turns the build so A's levers face south, and mirrors it
    /// when that reads right as what it seems to be
    #[arg(long)]
    standardize: bool,
}

impl Standardizing {
    /// Whether to standardize.
    fn asked(&self) -> bool {
        self.standardize
    }
}

/// Reads every stored file again as an upload's is prepared, and prints
/// what changed; see [`refile`].
pub(super) async fn reread(db: &SqlitePool) -> Result<()> {
    for s in schematic::all(db).await? {
        let stored = version::current_file(db, &s.id)
            .await?
            .context("schematic vanished")?;
        refile(db, &s, &stored, false).await?;
    }
    Ok(())
}

/// Prepares `file` as build `s`'s file, standardized if asked, as an
/// upload's is, keeping where it pastes if the build does: puts it in place
/// of the current version if it, its facts, or its size differ from what
/// is stored, saying what changed, and simulates it. No version is added
/// and nothing is compared.
async fn refile(
    db: &SqlitePool,
    s: &schematic::Schematic,
    file: &[u8],
    standardize: bool,
) -> Result<()> {
    let prepared = import::prepare(
        file,
        standardize,
        s.conventional,
        s.keeps_paste,
        false,
    )?;
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
    simulation::refresh(db, &s.id, false).await?;
    simulation::refresh_timing(db, &s.id, false).await?;
    simulation::keep_needs(db, &s.id).await?;
    Ok(())
}

/// Puts `args.file` in place of build `args.id`'s current file, as a fix
/// to the file rather than a new version; see [`refile`].
pub(super) async fn replace(db: &SqlitePool, args: &Replace) -> Result<()> {
    let id = resolve(db, &args.id).await?;
    let s = load(db, &id).await?;
    let file = fs::read(&args.file)?;
    refile(db, &s, &file, args.standardizing.asked()).await
}

/// Keeps the colours the builder's copy at `file` gives build `id`, with
/// the newest version it matches, or says where it comes closest to the
/// current one.
pub(super) async fn colours(
    db: &SqlitePool,
    id: &str,
    file: &Path,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let original = fs::read(file)?;
    let versions = sqlx::query_scalar!(
        "SELECT version FROM schematic_version WHERE schematic_id = ?
         ORDER BY version DESC",
        id,
    )
    .fetch_all(db)
    .await?;
    let mut closest = None;
    for version in versions {
        let Some(data) = version::data(db, &id, version).await? else {
            continue;
        };
        match app::recognize::colouring::read(&original, &data)? {
            Ok(colours) => {
                version::set_colours(db, &id, version, &colours).await?;
                println!(
                    "{}: kept {} blocks' colours with version {version}",
                    &id[..8],
                    colours.len()
                );
                return Ok(());
            }
            Err(faults) => {
                closest.get_or_insert((version, faults));
            }
        }
    }
    let (version, faults) = closest.context("no stored versions")?;
    bail!(
        "{} doesn't match any version; closest to version {version}:\n  {}",
        &id[..8],
        faults.join("\n  ")
    )
}

/// Replaces build `id`'s file with `file` as its next version, after the
/// checks the site makes, then simulates it again.
pub(super) async fn new_version(
    db: &SqlitePool,
    id: &str,
    file: &Path,
    note: Option<&str>,
    standardizing: &Standardizing,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let s = load(db, &id).await?;
    let raw = fs::read(file)?;
    let checked =
        app::version::check(db, &s, &raw, standardizing.asked()).await?;
    let comparison = checked.comparison;
    println!(
        "changed {} of {} blocks",
        comparison.changed,
        comparison.changed + comparison.same
    );
    if !checked.refusals.is_empty() {
        for refusal in &checked.refusals {
            println!("{refusal}");
        }
        bail!("{} cannot replace {}", file.display(), &id[..8]);
    }
    let how = Replacing::NextVersion {
        added_by: None,
        added_from: None,
        note: note.map(str::trim).filter(|note| !note.is_empty()),
    };
    // Unverified or off the list, a version recognized as the category it
    // stands in for moves it up, as on the site.
    let categories = catalogue::categories(db).await?;
    let moved = match simulation::stands_in(&s) {
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
    let number = version::replace(db, &id, kept, how).await?;
    let name = format!("{}  {}", &id[..8], s.filename());
    println!("{name}: version {number}");
    // The colours its builder gave it, read off the file given.
    match app::recognize::colouring::read(&raw, &kept.data)? {
        Ok(colours) if !colours.is_empty() => {
            version::set_colours(db, &id, number, &colours).await?;
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
            ..Details::of(&s)
        };
        match details::check(db, given, details::Build::of(file), true).await? {
            Ok(ready) => {
                schematic::set_details(db, &id, &ready).await?;
                println!("{name}: moved up as {}", to.name);
            }
            Err(why) => println!("{name}: not moved up: {why}"),
        }
    }
    if simulation::refresh(db, &id, false).await?.is_some()
        && let Some(report) = simulation::refresh_timing(db, &id, false).await?
    {
        print_report(&name, &report);
    }
    // As the site does once a version is in, without taking anything out,
    // so a fix by hand is shown in each theme.
    simulation::keep_needs(db, &id).await?;
    Ok(())
}

/// Takes the unused blocks out of build `id`, or of every build with a
/// test, as `rsdb prune` says.
pub(super) async fn prune(
    db: &SqlitePool,
    id: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let ids = match id {
        Some(prefix) => vec![resolve(db, prefix).await?],
        None => schematic::all(db)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect(),
    };
    let (mut builds, mut blocks) = (0, 0);
    for id in &ids {
        let s = load(db, id).await?;
        let name = format!("{}  {}", &id[..8], s.filename());
        if app::simulate::Test::of(&s.category, s.width).is_none() {
            if ids.len() == 1 {
                println!("{name}: no test for its category and width");
            }
            continue;
        }
        let data = version::current_file(db, id)
            .await?
            .context("schematic vanished")?;
        let buses = app::schem::stack::Buses::of(&s.category, s.orientation);
        let pruned =
            match app::recognize::roles::prune(&data, &s.stacking, buses) {
                Ok(Some(pruned)) => pruned,
                Ok(None) => continue,
                Err(err) => {
                    println!("skip    {name}: {err}");
                    continue;
                }
            };
        let checked = app::version::check(db, &s, &pruned.file, false).await?;
        if let Some(refusal) = checked.refusals.first() {
            println!("skip    {name}: {refusal}");
            continue;
        }
        builds += 1;
        blocks += pruned.gone.len();
        let verb = if dry_run { "would" } else { "pruned" };
        let n = pruned.gone.len();
        let gone: Vec<String> = pruned
            .gone
            .iter()
            .map(|[x, y, z]| format!("{x},{y},{z}"))
            .collect();
        println!("{verb} {name}: {n} blocks: {}", gone.join(" "));
        if dry_run {
            continue;
        }
        let how = Replacing::NextVersion {
            added_by: None,
            added_from: None,
            note: Some("Unused blocks taken out"),
        };
        version::replace(db, id, &checked.prepared, how).await?;
        if simulation::refresh(db, id, false).await?.is_some() {
            simulation::refresh_timing(db, id, false).await?;
        }
        simulation::keep_needs(db, id).await?;
    }
    let verb = if dry_run { "would take" } else { "took" };
    println!("{verb} {blocks} blocks out of {builds} builds");
    Ok(())
}
