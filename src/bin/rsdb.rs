//! Command-line access to the schematic database named by `DATABASE_URL`.

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use app::{
    db::connect_to_database,
    lint,
    models::schematic::{self, Changes, New, Optional, StackDirection},
    schem::Blocks,
    search, stack,
};
use clap::{Args, Parser, Subcommand};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// A database of Minecraft redstone schematics.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import a Sponge .schem file, after checking it against the
    /// building conventions
    Add(Add),
    /// Check a .schem file, or a stored schematic by id, against the
    /// building conventions
    Lint { file_or_id: String },
    /// Change fields of a schematic; omitted fields are kept
    Set(Set),
    /// Delete a schematic
    Remove { id: String },
    /// List schematics matching every term, such as
    /// `register vertical alpha -torchless`
    Search {
        #[arg(required = true, allow_hyphen_values = true)]
        terms: Vec<String>,
    },
    /// List schematics by id prefix and filename
    List {
        #[arg(long)]
        function: Option<String>,
        /// Only schematics carrying this tag; repeatable
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Show every field of a schematic
    Show { id: String },
    /// Write a schematic to a directory under its generated filename
    Export {
        id: String,
        #[arg(long, default_value = ".")]
        out: PathBuf,
        /// Copies of a stackable build to join into one: a count, or one per
        /// direction such as `west=7,north=3`
        #[arg(long)]
        copies: Option<String>,
    },
    /// Attach tags to a schematic
    Tag { id: String, tags: Vec<String> },
    /// Detach tags from a schematic
    Untag { id: String, tags: Vec<String> },
    /// List known tags by category
    Tags,
    /// Create a new tag, named in full words
    DefineTag {
        name: String,
        #[arg(long, value_parser = ["restriction", "timing", "other"])]
        category: String,
        /// Abbreviation that filenames write instead of the name
        #[arg(long)]
        token: Option<String>,
    },
}

#[derive(Args)]
struct Set {
    id: String,
    #[arg(long)]
    function: Option<String>,
    #[arg(long, value_parser = ["horz", "vert", "diag"])]
    orientation: Option<String>,
    #[arg(long, value_parser = ["bin", "hex"])]
    encoding: Option<String>,
    #[command(flatten)]
    details: Details,
    /// Field to make unknown; repeatable
    #[arg(
        long,
        value_parser = [
            "width",
            "latency",
            "synchronized",
            "stacking",
            "author",
            "built-on",
            "imported-by",
            "description",
        ]
    )]
    unset: Vec<String>,
}

#[derive(Args)]
struct Add {
    file: PathBuf,
    #[arg(long)]
    function: String,
    #[arg(long, value_parser = ["horz", "vert", "diag"])]
    orientation: String,
    #[arg(long, value_parser = ["bin", "hex"])]
    encoding: String,
    #[command(flatten)]
    details: Details,
    /// Tag to attach; repeatable
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Import even if the file breaks the building conventions
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct Details {
    /// Datapath width in bits
    #[arg(long)]
    width: Option<u32>,
    /// Latency in ticks
    #[arg(long)]
    latency: Option<u32>,
    /// Whether every output bit settles on the same tick: true or false
    #[arg(long)]
    synchronized: Option<bool>,
    /// A direction the build stacks along, such as `west=2,max=7`: blocks
    /// each copy sits west, up, or north of the one before, and the most
    /// copies that work; repeatable, and replaces any stored directions
    #[arg(long = "stack")]
    stacking: Vec<StackDirection>,
    /// In-game name; stored lowercase
    #[arg(long)]
    author: Option<String>,
    /// Build date as YYYY-MM-DD
    #[arg(long)]
    built_on: Option<String>,
    /// In-game name of whoever imported the build; stored lowercase
    #[arg(long)]
    imported_by: Option<String>,
    /// Free text about the build, such as how to use it
    #[arg(long)]
    description: Option<String>,
    /// Whether the build follows the building conventions: true or false.
    /// A build that does not is imported without the lint.
    #[arg(long)]
    conventional: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = connect_to_database().await;
    let result = run(&db, cli.command).await;
    // Move the write-ahead log's pages into redstone.db, so the file alone
    // holds every write when it is copied or backed up. Closing alone does
    // this only when no other process, such as a running server, has the
    // database open.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&db)
        .await?;
    db.close().await;
    result
}

async fn run(db: &SqlitePool, command: Command) -> Result<()> {
    match command {
        Command::Add(args) => add(db, &args).await,
        Command::Lint { file_or_id } => lint_command(db, &file_or_id).await,
        Command::Set(args) => set(db, args).await,
        Command::List { function, tags } => {
            list(db, function.as_deref(), &tags).await
        }
        Command::Search { terms } => {
            let terms = search::parse(&terms.join(" "));
            for s in search::matching(&terms, schematic::all(db).await?) {
                println!("{}  {}", &s.id[..8], s.filename());
            }
            Ok(())
        }
        Command::Remove { id } => {
            let id = resolve(db, &id).await?;
            let filename = load(db, &id).await?.filename();
            schematic::delete(db, &id).await?;
            println!("removed {}  {filename}", &id[..8]);
            Ok(())
        }
        Command::Show { id } => show(db, &resolve(db, &id).await?).await,
        Command::Export { id, out, copies } => {
            export(db, &id, &out, copies.as_deref()).await
        }
        Command::Tag { id, tags } => {
            let id = resolve(db, &id).await?;
            for name in &tags {
                schematic::attach(db, &id, &tag_id(db, name).await?).await?;
            }
            Ok(())
        }
        Command::Untag { id, tags } => {
            let id = resolve(db, &id).await?;
            for name in &tags {
                schematic::detach(db, &id, &tag_id(db, name).await?).await?;
            }
            Ok(())
        }
        Command::Tags => {
            for tag in schematic::all_tags(db).await? {
                let token = tag.token.as_deref().unwrap_or("");
                println!("{:<12} {:<8} {}", tag.category, token, tag.label());
            }
            Ok(())
        }
        Command::DefineTag {
            name,
            category,
            token,
        } => {
            schematic::define_tag(db, &name, token.as_deref(), &category)
                .await?;
            Ok(())
        }
    }
}

/// Changes the fields `args` gives and forgets the ones it unsets.
async fn set(db: &SqlitePool, args: Set) -> Result<()> {
    let Set {
        id,
        function,
        orientation,
        encoding,
        details,
        unset,
    } = args;
    let id = resolve(db, &id).await?;
    let author = details.author.map(|a| a.to_lowercase());
    let imported_by = details.imported_by.map(|a| a.to_lowercase());
    let changes = Changes {
        function: function.as_deref(),
        width: details.width.map(i64::from),
        orientation: orientation.as_deref(),
        encoding: encoding.as_deref(),
        latency: details.latency.map(i64::from),
        synchronized: details.synchronized,
        author: author.as_deref(),
        built_on: details.built_on.as_deref(),
        imported_by: imported_by.as_deref(),
        conventional: details.conventional,
        description: details.description.as_deref(),
    };
    schematic::update(db, &id, &changes).await?;
    let fields: Vec<Optional> = unset
        .iter()
        .filter_map(|field| match field.as_str() {
            "width" => Some(Optional::Width),
            "latency" => Some(Optional::Latency),
            "synchronized" => Some(Optional::Synchronized),
            "author" => Some(Optional::Author),
            "built-on" => Some(Optional::BuiltOn),
            "imported-by" => Some(Optional::ImportedBy),
            "description" => Some(Optional::Description),
            _ => None,
        })
        .collect();
    schematic::forget(db, &id, &fields).await?;
    if !details.stacking.is_empty() || unset.iter().any(|f| f == "stacking") {
        let mut conn = db.acquire().await?;
        schematic::set_stacking(&mut conn, &id, &details.stacking).await?;
    }
    println!("{}", load(db, &id).await?.filename());
    Ok(())
}

async fn add(db: &SqlitePool, args: &Add) -> Result<()> {
    let file = &args.file;
    let data = fs::read(file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let blocks =
        Blocks::read(&data).with_context(|| file.display().to_string())?;
    let conventional = args.details.conventional.unwrap_or(true);
    let violations = if conventional {
        lint::check(&blocks)
    } else {
        Vec::new()
    };
    if !violations.is_empty() {
        for violation in &violations {
            eprintln!("{violation}");
        }
        if !args.force {
            bail!(
                "{} breaks {} building conventions; fix them or pass --force",
                file.display(),
                violations.len()
            );
        }
    }
    // A build without corner markers measures its whole box.
    let size = if blocks.missing_markers().is_empty() {
        blocks.inner_size()?
    } else {
        blocks.full_size()?
    };
    let sha256 =
        Sha256::digest(&data)
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            });
    if let Some(id) = schematic::id_by_sha256(db, &sha256).await? {
        bail!("{} is already stored as {}", file.display(), &id[..8]);
    }
    let details = &args.details;
    let author = details.author.as_ref().map(|a| a.to_lowercase());
    let imported_by = details.imported_by.as_ref().map(|a| a.to_lowercase());

    let mut tx = db.begin().await?;
    let id = schematic::insert(
        &mut *tx,
        &New {
            function: &args.function,
            width: details.width.map(i64::from),
            orientation: &args.orientation,
            encoding: &args.encoding,
            latency: details.latency.map(i64::from),
            synchronized: details.synchronized,
            size: (size.x.into(), size.y.into(), size.z.into()),
            author: author.as_deref(),
            built_on: details.built_on.as_deref(),
            imported_by: imported_by.as_deref(),
            conventional,
            description: details.description.as_deref(),
            sha256: &sha256,
            data: &data,
        },
    )
    .await?;
    for name in &args.tags {
        let tag = tag_id(&mut *tx, name).await?;
        schematic::attach(&mut *tx, &id, &tag).await?;
    }
    schematic::set_stacking(&mut tx, &id, &details.stacking).await?;
    tx.commit().await?;
    println!("{}  {}", &id[..8], load(db, &id).await?.filename());
    Ok(())
}

/// Writes a schematic to `out` under its generated filename, stacked as
/// `copies` says: a count for a build that stacks one way, or one count per
/// direction such as `west=7,north=3`.
async fn export(
    db: &SqlitePool,
    id: &str,
    out: &Path,
    copies: Option<&str>,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let s = load(db, &id).await?;
    let data = schematic::data(db, &id)
        .await?
        .context("schematic vanished")?;
    let (data, s) = match copies {
        None => (data, s),
        Some(copies) => {
            let wanted = copy_counts(&s.stacking, copies)?;
            let plan = stack::plan(&s.stacking, &wanted)?;
            let (data, size) = stack::stack(&data, &plan)?;
            (data, s.stacked(&plan, size))
        }
    };
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

/// Prints the schematics with `function`, if given, and every one of `tags`,
/// each named in full or by its filename token.
async fn list(
    db: &SqlitePool,
    function: Option<&str>,
    tags: &[String],
) -> Result<()> {
    for s in schematic::all(db).await? {
        let has_tags = tags.iter().all(|t| {
            let name = schematic::tag_name(t);
            s.tags
                .iter()
                .any(|tag| tag.name == name || tag.token() == t)
        });
        if function.is_none_or(|f| f == s.function) && has_tags {
            println!("{}  {}", &s.id[..8], s.filename());
        }
    }
    Ok(())
}

/// Lints a file, or a stored schematic when no such file exists.
async fn lint_command(db: &SqlitePool, file_or_id: &str) -> Result<()> {
    let data = if fs::exists(file_or_id)? {
        fs::read(file_or_id)?
    } else {
        let id = resolve(db, file_or_id).await?;
        schematic::data(db, &id)
            .await?
            .context("schematic vanished")?
    };
    let violations = lint::check(&Blocks::read(&data)?);
    for violation in &violations {
        println!("{violation}");
    }
    if !violations.is_empty() {
        bail!("{} building conventions broken", violations.len());
    }
    Ok(())
}

/// Resolves a unique prefix of a schematic id to the full id.
async fn resolve(db: &SqlitePool, prefix: &str) -> Result<String> {
    match schematic::ids_with_prefix(db, prefix).await?.as_slice() {
        [id] => Ok(id.clone()),
        [] => bail!("no schematic with id {prefix}"),
        _ => bail!("id prefix {prefix} is ambiguous"),
    }
}

async fn tag_id<'e, E: sqlx::SqliteExecutor<'e>>(
    db: E,
    name: &str,
) -> Result<String> {
    schematic::tag_id(db, name).await?.with_context(|| {
        format!("unknown tag {name}; see `rsdb tags` or `rsdb define-tag`")
    })
}

async fn load(db: &SqlitePool, id: &str) -> Result<schematic::Schematic> {
    schematic::get(db, id).await?.context("schematic vanished")
}

async fn show(db: &SqlitePool, id: &str) -> Result<()> {
    let s = load(db, id).await?;
    let unknown = |v: Option<String>| v.unwrap_or_else(|| "unknown".into());
    let (x, y, z) = s.size;
    let tags: Vec<String> = s.tags.iter().map(schematic::Tag::label).collect();
    println!("id           {}", s.id);
    println!("title        {}", s.title());
    println!("filename     {}", s.filename());
    println!("function     {}", s.function);
    println!("width        {}", unknown(s.width_label()));
    println!("orientation  {}", s.orientation_label());
    println!("encoding     {}", s.encoding_label());
    println!(
        "latency      {}",
        unknown(s.latency.map(|t| format!("{t} ticks")))
    );
    println!("synchronized {}", s.synchronized_label());
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
    println!("size         {x}x{y}x{z}, {} blocks", s.volume());
    println!("author       {}", unknown(s.author));
    println!("built on     {}", unknown(s.built_on));
    println!("tags         {}", tags.join(", "));
    println!("imported by  {}", unknown(s.imported_by));
    println!(
        "conventions  {}",
        if s.conventional {
            "followed"
        } else {
            "not followed"
        }
    );
    println!("added at     {}", s.added_at);
    if let Some(description) = &s.description {
        println!("\n{description}");
    }
    Ok(())
}
