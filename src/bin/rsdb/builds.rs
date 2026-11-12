//! Finding, showing, changing, and removing stored builds, writing them out
//! as files, and drawing the guide page's examples.

use std::{collections::HashMap, fs, io::Write as _, path::Path};

use anyhow::{Context, Result, bail, ensure};
use app::{
    category,
    details::{self, Details},
    models::{
        schematic::{self, Encoding, Logic, Orientation, StackDirection},
        simulation, tag, version,
    },
    search, stack,
};
use clap::Args;
use sqlx::SqlitePool;

use super::{
    load,
    refresh::{Refreshing, refresh_one},
    resolve,
};

#[derive(Args)]
pub(super) struct Set {
    id: String,
    #[arg(long)]
    category: Option<String>,
    #[arg(long)]
    orientation: Option<Orientation>,
    #[command(flatten)]
    details: DetailArgs,
    /// A tag to attach, named in full or by its filename token; repeatable
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// A tag to detach, named in full or by its filename token; repeatable
    #[arg(long = "untag")]
    untags: Vec<String>,
    /// Field to make unknown; repeatable
    #[arg(
        long,
        value_parser = [
            "latency",
            "synchronized",
            "stacking",
            "authors",
            "contributors",
            "built-on",
            "imported-by",
            "description",
            "logic",
        ]
    )]
    unset: Vec<String>,
}

#[derive(Args)]
struct DetailArgs {
    /// Latency in ticks
    #[arg(long)]
    latency: Option<u32>,
    /// Stateless or stateful, where it differs from what its category
    /// usually is
    #[arg(long)]
    logic: Option<Logic>,
    /// Whether every output bit settles on the same tick: true or false
    #[arg(long)]
    synchronized: Option<bool>,
    /// A direction the build stacks along, such as `west=2,max=7`: blocks
    /// each copy sits west, up, or north of the one before, and the most
    /// copies that work; repeatable, and replaces any stored directions
    #[arg(long = "stack")]
    stacking: Vec<StackDirection>,
    /// An author's in-game name, as written; repeatable, in order.
    /// Given to `set`, the names replace the stored authors
    #[arg(long = "author", alias = "builder")]
    authors: Vec<String>,
    /// The in-game name of someone who helped without building it, such as
    /// with the design; repeatable, in order. Given to `set`, the names
    /// replace the stored contributors
    #[arg(long = "contributor")]
    contributors: Vec<String>,
    /// Build date as YYYY-MM-DD
    #[arg(long)]
    built_on: Option<String>,
    /// In-game name of whoever imported the build
    #[arg(long)]
    imported_by: Option<String>,
    /// Free text about the build, such as how to use it
    #[arg(long)]
    description: Option<String>,
    /// Whether the build follows the building conventions, so its file is
    /// read by them: true or false
    #[arg(long)]
    conventional: Option<bool>,
}

/// Prints the id and file name of each build matching `terms`, every build
/// with none. A term that is the start of an id, such as `72e896eb`, finds
/// the build whose id starts so, beside what the other terms find.
pub(super) async fn search_command(
    db: &SqlitePool,
    terms: &[String],
    ids: bool,
) -> Result<()> {
    let (prefixes, words): (Vec<&String>, Vec<&String>) =
        terms.iter().partition(|term| id_prefix(term));
    let words: Vec<&str> = words.iter().map(|word| word.as_str()).collect();
    let terms = search::parse(&words.join(" "));
    let builds = schematic::all(db).await?;
    let builds = if prefixes.is_empty() {
        builds
    } else {
        builds
            .into_iter()
            .filter(|s| prefixes.iter().any(|p| s.id.starts_with(p.as_str())))
            .collect()
    };
    for s in search::matching(&terms, builds) {
        if ids {
            println!("{}", &s.id[..8]);
        } else {
            println!("{}  {}", &s.id[..8], s.filename());
        }
    }
    Ok(())
}

/// Whether `term` reads as the start of a build's id: four or more hex
/// digits, with a letter among them, or eight or more, as a width such as
/// `1024` has neither.
fn id_prefix(term: &str) -> bool {
    let hex = term.len() >= 4 && term.chars().all(|c| c.is_ascii_hexdigit());
    let lettered = term.chars().any(|c| c.is_ascii_alphabetic());
    let digited = term.chars().any(|c| c.is_ascii_digit());
    hex && digited && (lettered || term.len() >= 8)
}

/// Removes build `id` for good, leaving nothing to restore.
pub(super) async fn remove(db: &SqlitePool, id: &str) -> Result<()> {
    let id = resolve(db, id).await?;
    let filename = load(db, &id).await?.filename();
    schematic::purge(db, &id).await?;
    println!("removed {}  {filename}", &id[..8]);
    Ok(())
}

/// Changes the fields `args` gives, its tags too, and forgets the ones it
/// unsets, holding the details to the rules the edit page does, then
/// refreshes the build, as `rsdb refresh` does.
pub(super) async fn set(db: &SqlitePool, args: Set) -> Result<()> {
    let Set {
        id,
        category,
        orientation,
        details: given,
        tags,
        untags,
        unset,
    } = args;
    let id = resolve(db, &id).await?;
    let s = load(db, &id).await?;
    let mut details = Details::of(&s);
    if let Some(category) = category {
        details.category = category;
    }
    if let Some(orientation) = orientation {
        details.specs.orientation = orientation;
    }
    overlay(&mut details, &given, &unset);
    for name in &tags {
        let name = tag_name(db, name).await?;
        if !details.tags.contains(&name) {
            details.tags.push(name);
        }
    }
    for name in &untags {
        let name = tag_name(db, name).await?;
        details.tags.retain(|kept| *kept != name);
    }
    let conventional = given.conventional.unwrap_or(s.file.conventional);
    // Under its category, read as it was, it stays as verified as it was;
    // another category, or its file read another way, is recognized
    // afresh, as the edit page does.
    if details.category != s.category || conventional != s.file.conventional {
        details.unverified = None;
    }
    let data = version::current_file(db, &id)
        .await?
        .context("schematic vanished")?;
    // Its stored file, kept under its category, passed its test already.
    let passes = details.category == s.category
        && conventional == s.file.conventional
        && simulation::current(db, &s)
            .await?
            .is_some_and(|report| report.passed());
    let build = details::Candidate {
        data: &data,
        conventional,
        passes,
    };
    // The command line acts as an admin, so may name a new tag.
    let ready = match details::check(db, details, build, true).await? {
        Ok(ready) => ready,
        Err(message) => bail!("{message}"),
    };
    schematic::set_details(db, &id, &ready).await?;
    let imported_by = if unset.iter().any(|f| f == "imported-by") {
        None
    } else {
        given.imported_by.or(s.entry.imported_by)
    };
    schematic::set_import_details(
        db,
        &id,
        imported_by.as_deref(),
        conventional,
    )
    .await?;
    // A new category or width may bring a test or drop one; recognized
    // now, it keeps what its building blocks must be, for themes, as an
    // upload does. It was judged as its category just now.
    let how = Refreshing {
        again: false,
        rejudge: false,
        prune: false,
        probe: false,
    };
    refresh_one(db, &id, how).await?;
    println!("{}", load(db, &id).await?.filename());
    Ok(())
}

/// The full name of the tag `given` names, in full or by its filename
/// token, which must be known; see `rsdb tags`.
async fn tag_name(db: &SqlitePool, given: &str) -> Result<String> {
    let name = tag::name(given);
    let found = tag::all(db)
        .await?
        .into_iter()
        .find(|t| t.name == name || t.token.as_deref() == Some(&name))
        .with_context(|| {
            format!(
                "unknown tag {given}; see `rsdb tags`, or `rsdb tags --define`"
            )
        })?;
    Ok(found.name)
}

/// Judges build `id` again as a build of its category, as `set` does when
/// its category or conventions change: its stored reason for being
/// unverified cleared, its details held to the rules again, which tests
/// it, and stored only when the verdict differs, so a build is not marked
/// edited by being judged the same. Details the rules refuse, as a name
/// Mojang no longer knows, leave it as it was, saying why.
pub(super) async fn rejudge(db: &SqlitePool, id: &str) -> Result<()> {
    let s = load(db, id).await?;
    let data = version::current_file(db, id)
        .await?
        .context("schematic vanished")?;
    let details = Details {
        unverified: None,
        ..Details::of(&s)
    };
    let build = details::Candidate {
        data: &data,
        conventional: s.file.conventional,
        passes: false,
    };
    match details::check(db, details, build, true).await? {
        Ok(ready) if ready.details.unverified != s.unverified => {
            schematic::set_details(db, id, &ready).await?;
            let now = ready.details.unverified.as_deref();
            println!(
                "{}  {}",
                &id[..8],
                now.map_or("verified".to_owned(), |why| format!(
                    "unverified: {why}"
                ))
            );
        }
        Ok(_) => {}
        Err(message) => {
            println!("{}  not judged again: {message}", &id[..8]);
        }
    }
    Ok(())
}

/// Puts the fields `given` has in place of `details`' own, after making the
/// ones `unset` names, as `--unset` takes them, unknown.
fn overlay(details: &mut Details, given: &DetailArgs, unset: &[String]) {
    fn put<T>(field: &mut Option<T>, given: Option<T>, unset: bool) {
        if unset {
            *field = None;
        }
        if given.is_some() {
            *field = given;
        }
    }
    let unsets = |name: &str| unset.iter().any(|field| field == name);
    let number = |n: Option<u32>| n.map(i64::from);
    put(
        &mut details.specs.latency,
        number(given.latency),
        unsets("latency"),
    );
    put(
        &mut details.specs.synchronized,
        given.synchronized,
        unsets("synchronized"),
    );
    put(
        &mut details.built_on,
        given.built_on.clone(),
        unsets("built-on"),
    );
    put(
        &mut details.description,
        given.description.clone(),
        unsets("description"),
    );
    put(&mut details.specs.logic, given.logic, unsets("logic"));
    // A list given replaces the stored one whole.
    let lists = [
        (&mut details.credits.authors, &given.authors, "authors"),
        (
            &mut details.credits.contributors,
            &given.contributors,
            "contributors",
        ),
    ];
    for (field, given, name) in lists {
        if !given.is_empty() || unsets(name) {
            field.clone_from(given);
        }
    }
    if !given.stacking.is_empty() || unsets("stacking") {
        details.stacking.clone_from(&given.stacking);
    }
}

/// Writes a schematic to `out` under its generated filename, stacked as
/// `copies` says: a count for a build that stacks one way, or one count per
/// direction such as `west=7,north=3`.
pub(super) async fn export(
    db: &SqlitePool,
    id: &str,
    out: &Path,
    copies: Option<&str>,
    markers: bool,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let s = load(db, &id).await?;
    let data = version::current_file(db, &id)
        .await?
        .context("schematic vanished")?;
    let (data, s) = match copies {
        None => (data, s),
        Some(copies) => {
            let wanted = copy_counts(&s.stacking, copies)?;
            let plan = stack::plan(&s.stacking, &wanted)?;
            let buses = stack::Buses::of(s.kind(), s.specs.orientation);
            let (data, size) = stack::stack(&data, &plan, buses)?;
            (data, s.stacked(&plan, size))
        }
    };
    let data = if markers {
        ensure!(copies.is_none(), "markers go on a single copy");
        let direction = s.stacking.first().context("it doesn't stack")?;
        stack::with_markers(&data, *direction)?
            .context("every pair of the box's opposite corners holds a block")?
    } else {
        data
    };
    // As it is stored, in its author's colours, without its roles.
    let data = app::schem::lines::without(&data)?;
    let path = out.join(s.filename());
    fs::File::create_new(&path)
        .with_context(|| format!("cannot create {}", path.display()))?
        .write_all(&data)?;
    println!("{}", path.display());
    Ok(())
}

/// Reads `8` for a build that stacks one way, or `west=7,north=3`.
fn copy_counts(
    stacking: &[StackDirection],
    text: &str,
) -> Result<HashMap<String, usize>> {
    if let Ok(count) = text.parse() {
        let [direction] = stacking else {
            bail!(
                "this build stacks {} ways; give a count for each, such as west=7",
                stacking.len()
            );
        };
        return Ok(HashMap::from([(direction.name(), count)]));
    }
    text.split(',')
        .map(|pair| {
            let (name, count) = pair
                .split_once('=')
                .context("give copies as a count or as direction=count")?;
            Ok((name.trim().to_owned(), count.trim().parse()?))
        })
        .collect()
}

/// Draws each of `app::recognize::guide`'s examples into `dir`, as the page
/// shows them, fetching the game's files as thumbnails do.
pub(super) async fn guide_images(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let assets = app::thumbnail::Assets::new(reqwest::Client::new());
    let dir = dir.to_owned();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let renderer = app::thumbnail::Renderer::new(assets);
        let view = blockrender::View {
            width: 480,
            height: 300,
            padding: 12.0,
        };
        for example in app::recognize::guide::examples() {
            let webp = app::thumbnail::render_view(
                &example.schematic(),
                &renderer,
                view,
            )?;
            fs::write(dir.join(format!("{}.webp", example.name)), webp)?;
            println!("{}", example.name);
        }
        Ok(())
    })
    .await?
}

pub(super) async fn show(db: &SqlitePool, id: &str) -> Result<()> {
    let s = load(db, id).await?;
    let unknown = |v: Option<String>| v.unwrap_or_else(|| "unknown".into());
    let (x, y, z) = s.file.size;
    let tags: Vec<String> = s.tags.iter().map(tag::Tag::label).collect();
    println!("id           {}", s.id);
    println!("title        {}", s.title());
    println!("filename     {}", s.filename());
    println!("category     {}", s.category);
    println!("width        {}", unknown(s.width_label()));
    println!("orientation  {}", s.specs.orientation.label());
    println!(
        "encoding     {}",
        s.specs.encoding.map_or("none", Encoding::label)
    );
    println!(
        "latency      {}",
        unknown(s.specs.latency.map(|t| format!("{t} ticks")))
    );
    println!(
        "throughput   {}",
        unknown(s.specs.throughput.map(|t| format!("{t} ticks")))
    );
    if let Some(bytes) = s.specs.bytes {
        println!("capacity     {bytes} bytes");
    }
    if let Some(k) = s.specs.at_least {
        println!("at least     {k} on");
    }
    if let Some(p) = s.specs.pulse {
        println!("pulse        {p} ticks");
    }
    println!("synchronized {}", s.synchronized_label());
    println!(
        "state        {}{}",
        s.logic().map_or("unknown", Logic::name),
        if s.pipelined() { ", pipelined" } else { "" }
    );
    let from = s.file.works_from.as_deref().unwrap_or("unknown");
    let by = s.file.works_from_by.as_deref().map(|by| format!(" ({by})"));
    println!("works from   {from}{}", by.unwrap_or_default());
    let design = s
        .specs
        .design
        .as_deref()
        .map_or("none".to_owned(), category::words);
    println!("design       {design}");
    let flags: Vec<&str> = s.flags.iter().map(|f| f.label).collect();
    println!("flags        {}", flags.join(", "));
    let stacking: Vec<String> =
        s.stacking.iter().map(StackDirection::describe).collect();
    println!(
        "stacking     {}",
        if stacking.is_empty() {
            "no".to_owned()
        } else {
            stacking.join("; ")
        }
    );
    println!("size         {y}x{x}x{z} (HWD), {} blocks", s.box_volume());
    if let Some((x, y, z)) = s.per_copy() {
        println!("per copy     {y}x{x}x{z} (HWD), {} blocks", x * y * z);
    }
    println!(
        "built by     {}",
        if s.credits.authors.is_empty() {
            "unknown".to_owned()
        } else {
            s.credits.authors.join(", ")
        }
    );
    if !s.credits.contributors.is_empty() {
        println!("contributors {}", s.credits.contributors.join(", "));
    }
    println!("built on     {}", unknown(s.built_on));
    println!("tags         {}", tags.join(", "));
    println!("imported by  {}", unknown(s.entry.imported_by));
    println!(
        "conventions  {}",
        if s.file.conventional {
            "followed"
        } else {
            "not followed"
        }
    );
    println!("added at     {}", s.entry.added_at);
    if let Some(edited) = &s.entry.edited_at {
        println!("edited at    {edited}");
    }
    println!("downloads    {}", s.entry.downloads);
    print_versions(db, &s.id).await?;
    if let Some(description) = &s.description {
        println!("\n{description}");
    }
    Ok(())
}

/// Build `id`'s versions, a line each, when it has more than one.
async fn print_versions(db: &SqlitePool, id: &str) -> Result<()> {
    let versions = version::list(db, id).await?;
    if versions.len() > 1 {
        for v in &versions {
            println!(
                "version {:<4} {}  {}  {}",
                v.version,
                &v.added_at[..10.min(v.added_at.len())],
                v.added_by.as_deref().unwrap_or("rsdb"),
                v.note.as_deref().unwrap_or("")
            );
        }
    }
    Ok(())
}
