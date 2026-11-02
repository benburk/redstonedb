//! Adding the builds of a category from another copy of the database: each
//! rebuilt the colour-free way, or kept off the list as it was built; and
//! finishing recognized builds by taking out their unused blocks.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use app::{
    category::Category,
    details::{self, Details},
    import,
    models::{schematic, simulation, user::User, version},
    recognize::palette,
};
use clap::Args;
use sqlx::SqlitePool;

use super::{load, resolve};

#[derive(Args)]
pub(super) struct Rebuild {
    /// The copy of the database to take the builds from
    from: PathBuf,
    /// The category to rebuild, probed as it: one of the blessed ones,
    /// adder, register, alu, decoder, multiplier, counter, or
    /// `shift_register`
    #[arg(long, default_value = "adder")]
    category: String,
    /// A folder of fixed files, each named for the start of its build's
    /// id, such as `44935d21.schem`, taken in place of the stored file
    #[arg(long)]
    fixed: Option<PathBuf>,
    /// A build to leave out, by the start of its id; repeatable
    #[arg(long = "skip")]
    skip: Vec<String>,
    /// Instead, put every build here not already in as built, off the list,
    /// whatever its category: its file as it was before any unused blocks
    /// were taken out, as only a blessed build loses them
    #[arg(long)]
    rest: bool,
    /// Put each build that can't be rebuilt, or is left out, in as built,
    /// off the list, with the details it was given; with them any build
    /// already off the list under a name with the category's as a word, such as
    /// `adder_accumulator`
    #[arg(long)]
    other: bool,
}

/// Runs `simulation::keep_needs` on build `id`, or on every build whose
/// current file has no needs kept, as a version made by hand may not, and
/// says which gained them.
pub(super) async fn needs(db: &SqlitePool, id: Option<&str>) -> Result<()> {
    let ids = match id {
        Some(prefix) => vec![resolve(db, prefix).await?],
        None => schematic::all(db)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect(),
    };
    let mut gained = 0;
    for id in &ids {
        let had = palette::needs(db, id).await?.is_some();
        if had && ids.len() > 1 {
            continue;
        }
        simulation::keep_needs(db, id).await?;
        if !had && palette::needs(db, id).await?.is_some() {
            let s = load(db, id).await?;
            println!("{}  {}: needs kept", &id[..8], s.filename());
            gained += 1;
        }
    }
    println!("{gained} builds gained their needs");
    Ok(())
}

/// Runs `simulation::finish` on build `id`, or on every build, and says
/// which gained a version.
pub(super) async fn finish(db: &SqlitePool, id: Option<&str>) -> Result<()> {
    let ids = match id {
        Some(prefix) => vec![resolve(db, prefix).await?],
        None => schematic::all(db)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect(),
    };
    for id in &ids {
        let before = load(db, id).await?.sha256;
        simulation::finish(db, id).await?;
        let s = load(db, id).await?;
        if s.sha256 != before {
            println!("{}  {}: unused blocks taken out", &id[..8], s.filename());
        }
    }
    Ok(())
}

/// Adds each build of `args.category` from the database at `args.from`
/// that isn't here yet, as `rebuild_one` does, and says how each went;
/// those filed off the list as one, such as `8-bit adder`, too, back
/// under the category if they pass now. With `args.other`, what can't be
/// rebuilt goes in off the list as `keep_off_list` does.
pub(super) async fn rebuild(db: &SqlitePool, args: &Rebuild) -> Result<()> {
    if args.rest {
        return rebuild_rest(db, &args.from).await;
    }
    if !app::category::blessed(&args.category) {
        bail!(
            "only the blessed categories are probed: {}",
            app::category::Kind::all()
                .filter(|kind| kind.blessed())
                .map(app::category::Kind::name)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let from =
        SqlitePool::connect(&format!("sqlite:{}?mode=ro", args.from.display()))
            .await?;
    let listed = Category::all();
    let (mut built, mut refused, mut other) = (0, 0, 0);
    for mut s in schematic::all(&from).await? {
        let meant = app::models::simulation::meant_by(&s.category);
        let off_list = Category::find(&listed, &s.category).is_none();
        // A word of its name, as `adder` of `adder_accumulator`, not
        // letters within one, as `alu` of `absolute_value`.
        let akin = off_list
            && s.category
                .split(['-', '_'])
                .any(|word| word == args.category);
        if s.category != args.category
            && meant != Some(args.category.as_str())
            && !(args.other && akin)
        {
            continue;
        }
        let name = format!("{}  {}", &s.id[..8], s.filename());
        if schematic::get(db, &s.id).await?.is_some() {
            println!("{name}: here already");
            continue;
        }
        // Why it goes in as built, off the list, when it does.
        let off =
            if args.skip.iter().any(|skip| s.id.starts_with(skip.as_str())) {
                if !args.other {
                    println!("{name}: left out");
                    continue;
                }
                "left out".to_owned()
            } else if meant != Some(args.category.as_str())
                && s.category != args.category
            {
                "off the list already".to_owned()
            } else {
                let kept = s.clone();
                args.category.clone_into(&mut s.category);
                let fixed = args
                    .fixed
                    .as_ref()
                    .map(|dir| dir.join(format!("{}.schem", &s.id[..8])))
                    .filter(|path| path.exists());
                let data = match &fixed {
                    Some(path) => fs::read(path)?,
                    None => version::current_file(&from, &s.id)
                        .await?
                        .context("schematic vanished")?,
                };
                let from_file =
                    if fixed.is_some() { " (fixed file)" } else { "" };
                match rebuild_one(db, &from, &s, &data).await? {
                    Ok(()) => {
                        built += 1;
                        println!("{name}{from_file}: rebuilt");
                        continue;
                    }
                    Err(why) if args.other => {
                        s = kept;
                        format!("{why}{from_file}")
                    }
                    Err(why) => {
                        refused += 1;
                        println!("{name}{from_file}: {why}");
                        continue;
                    }
                }
            };
        let data = version::current_file(&from, &s.id)
            .await?
            .context("schematic vanished")?;
        match keep_off_list(db, &from, &s, &data, &listed).await? {
            Ok(category) => {
                other += 1;
                println!("{name}: {off} → off the list, as {category}");
            }
            Err(why) => {
                refused += 1;
                println!("{name}: {off} → refused: {why}");
            }
        }
    }
    println!("\n{built} rebuilt, {other} off the list, {refused} refused");
    Ok(())
}

/// Puts every build of the database at `from` not here in as built, off
/// the list, as `keep_off_list` does, from its latest file before unused
/// blocks were taken out, as only a blessed build loses them.
async fn rebuild_rest(db: &SqlitePool, from: &Path) -> Result<()> {
    let from =
        SqlitePool::connect(&format!("sqlite:{}?mode=ro", from.display()))
            .await?;
    let listed = Category::all();
    let (mut other, mut refused) = (0, 0);
    for s in schematic::all(&from).await? {
        if schematic::get(db, &s.id).await?.is_some() {
            continue;
        }
        let name = format!("{}  {}", &s.id[..8], s.filename());
        let data = sqlx::query_scalar!(
            "SELECT data FROM schematic_version
             WHERE schematic_id = ?
               AND coalesce(note, '') <> 'Unused blocks taken out'
             ORDER BY version DESC LIMIT 1",
            s.id,
        )
        .fetch_one(&from)
        .await?;
        match keep_off_list(db, &from, &s, &data, &listed).await? {
            Ok(category) => {
                other += 1;
                println!("{name}: off the list, as {category}");
            }
            Err(why) => {
                refused += 1;
                println!("{name}: refused: {why}");
            }
        }
    }
    println!("\n{other} off the list, {refused} refused");
    Ok(())
}

/// Stores build `s` of database `from`, its file `data`, as it was built
/// and given, unchanged but tidied, off the list: under its own
/// off-list category, or one named for what it is, such as `8-bit adder`,
/// when it was listed among `listed`; then simulated, to say what keeps
/// it off. Says where it went, or the `Err` why it couldn't go.
async fn keep_off_list(
    db: &SqlitePool,
    from: &SqlitePool,
    s: &schematic::Schematic,
    data: &[u8],
    listed: &[Category],
) -> Result<Result<String, String>> {
    let prepared = import::prepare_for(
        data,
        import::Purpose::InPlace {
            standardize: false,
            conventional: s.conventional,
            keep_paste: true,
        },
    )?;
    let mut given = Details::of(s);
    // A category off the list has no designs of its own.
    if let Some(category) = Category::find(listed, &s.category) {
        given.category = details::off_list_name(category, s.width);
        given.design = None;
    }
    let category = given.category.clone();
    let ready =
        match details::check(db, given, details::Build::of(&prepared), true)
            .await?
        {
            Ok(ready) => ready,
            Err(why) => return Ok(Err(why)),
        };
    if let Err(why) = store_as_before(db, from, s, &prepared, &ready).await? {
        return Ok(Err(why));
    }
    simulation::refresh(db, &s.id, false).await?;
    Ok(Ok(category))
}

/// Stores build `s` of database `from`, `prepared` and `ready`, under its
/// own id, with its uploader, when their account is here too, its date
/// added, and its downloads. The `Err` says why it can't be.
async fn store_as_before(
    db: &SqlitePool,
    from: &SqlitePool,
    s: &schematic::Schematic,
    prepared: &import::Prepared,
    ready: &details::Ready,
) -> Result<Result<(), String>> {
    let uploader = sqlx::query_scalar!(
        r#"SELECT uploaded_by AS "uploaded_by: uuid::Uuid"
           FROM schematic WHERE id = ?"#,
        s.id,
    )
    .fetch_one(from)
    .await?;
    let uploader = match uploader {
        Some(id) if User::get_by_id(db, id).await?.is_some() => Some(id),
        _ => None,
    };
    let stored = import::store(
        db,
        prepared,
        ready,
        uploader,
        None,
        s.imported_by.as_deref(),
        Some(&s.id),
    )
    .await?;
    if let Err(why) = stored {
        return Ok(Err(why));
    }
    sqlx::query!(
        "UPDATE schematic SET added_at = ?, downloads = ? WHERE id = ?",
        s.added_at,
        s.downloads,
        s.id,
    )
    .execute(db)
    .await?;
    Ok(Ok(()))
}

/// Stores build `s` of database `from`, its file `data`: inspected and
/// probed as its category, one of the blessed ones, its roles coloured in,
/// standardized, and checked as an upload is, under its own id, with its
/// details, credits, tags, uploader, date added, and downloads; then
/// simulated. The `Err` says why
/// it can't be.
async fn rebuild_one(
    db: &SqlitePool,
    from: &SqlitePool,
    s: &schematic::Schematic,
    data: &[u8],
) -> Result<Result<(), String>> {
    let data = app::recognize::probe::simulatable(data)?;
    let data = data.as_slice();
    let blocks = app::schem::Blocks::read(data)?;
    let report = app::recognize::inspect::inspect_marked(
        &blocks,
        app::recognize::inspect::SITE,
    );
    if let Some(problem) = report.faults().next() {
        return Ok(Err(problem.message.clone()));
    }
    // Dust on top of an input is a fault only on a data input.
    let Some(kind) = s.kind() else {
        return Ok(Err(format!("{} is no standard category", s.category)));
    };
    let (data_inputs, coloured, _) =
        match app::recognize::probe::probe_as(kind, data, &blocks, &report)? {
            Ok(found) => found,
            Err(why) => return Ok(Err(why.to_string())),
        };
    if let Some(fault) = report.data_faults(&data_inputs).into_iter().next() {
        return Ok(Err(fault));
    }
    let prepared = import::prepare_for(
        &coloured,
        import::Purpose::Standard { keep_paste: false },
    )?;
    let ready = match details::check(
        db,
        Details::of(s),
        details::Build::of(&prepared),
        true,
    )
    .await?
    {
        Ok(ready) => ready,
        Err(why) => return Ok(Err(why)),
    };
    if let Err(why) = store_as_before(db, from, s, &prepared, &ready).await? {
        return Ok(Err(why));
    }
    simulation::refresh(db, &s.id, false).await?;
    simulation::finish(db, &s.id).await?;
    Ok(Ok(()))
}
