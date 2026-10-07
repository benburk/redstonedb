//! Command-line access to the schematic database named by `DATABASE_URL`.

use std::{fmt::Write as _, fs, io::Write as _, path::PathBuf};

use anyhow::{Context, Result, bail};
use app::{
    db::connect_to_database,
    lint,
    models::schematic::{self, Changes, New, Optional},
    schem::Blocks,
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
    Set {
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
        #[arg(long, value_parser = ["width", "latency", "author", "built-on"])]
        unset: Vec<String>,
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
    /// In-game name; stored lowercase
    #[arg(long)]
    author: Option<String>,
    /// Build date as YYYY-MM-DD
    #[arg(long)]
    built_on: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = connect_to_database().await;
    let result = run(&db, cli.command).await;
    // Closing checkpoints SQLite's write-ahead log into redstone.db, which
    // is committed; the log is not.
    db.close().await;
    result
}

async fn run(db: &SqlitePool, command: Command) -> Result<()> {
    match command {
        Command::Add(args) => add(db, &args).await,
        Command::Lint { file_or_id } => lint_command(db, &file_or_id).await,
        Command::Set {
            id,
            function,
            orientation,
            encoding,
            details,
            unset,
        } => {
            let id = resolve(db, &id).await?;
            let author = details.author.map(|a| a.to_lowercase());
            let changes = Changes {
                function: function.as_deref(),
                width: details.width.map(i64::from),
                orientation: orientation.as_deref(),
                encoding: encoding.as_deref(),
                latency: details.latency.map(i64::from),
                author: author.as_deref(),
                built_on: details.built_on.as_deref(),
            };
            schematic::update(db, &id, &changes).await?;
            let fields: Vec<Optional> = unset
                .iter()
                .filter_map(|field| match field.as_str() {
                    "width" => Some(Optional::Width),
                    "latency" => Some(Optional::Latency),
                    "author" => Some(Optional::Author),
                    "built-on" => Some(Optional::BuiltOn),
                    _ => None,
                })
                .collect();
            schematic::forget(db, &id, &fields).await?;
            println!("{}", load(db, &id).await?.filename());
            Ok(())
        }
        Command::List { function, tags } => {
            list(db, function.as_deref(), &tags).await
        }
        Command::Show { id } => show(db, &resolve(db, &id).await?).await,
        Command::Export { id, out } => {
            let id = resolve(db, &id).await?;
            let path = out.join(load(db, &id).await?.filename());
            let data = schematic::data(db, &id)
                .await?
                .context("schematic vanished")?;
            fs::File::create_new(&path)
                .with_context(|| format!("cannot create {}", path.display()))?
                .write_all(&data)?;
            println!("{}", path.display());
            Ok(())
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

async fn add(db: &SqlitePool, args: &Add) -> Result<()> {
    let file = &args.file;
    let data = fs::read(file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let blocks =
        Blocks::read(&data).with_context(|| file.display().to_string())?;
    let violations = lint::check(&blocks);
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
    let size = blocks.inner_size()?;
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
    let source_name = file
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let details = &args.details;
    let author = details.author.as_ref().map(|a| a.to_lowercase());

    let mut tx = db.begin().await?;
    let id = schematic::insert(
        &mut *tx,
        &New {
            function: &args.function,
            width: details.width.map(i64::from),
            orientation: &args.orientation,
            encoding: &args.encoding,
            latency: details.latency.map(i64::from),
            size: (size.x.into(), size.y.into(), size.z.into()),
            author: author.as_deref(),
            built_on: details.built_on.as_deref(),
            source_name: &source_name,
            sha256: &sha256,
            data: &data,
        },
    )
    .await?;
    for name in &args.tags {
        let tag = tag_id(&mut *tx, name).await?;
        schematic::attach(&mut *tx, &id, &tag).await?;
    }
    tx.commit().await?;
    println!("{}  {}", &id[..8], load(db, &id).await?.filename());
    Ok(())
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
    println!(
        "width        {}",
        unknown(s.width.map(|w| format!("{w} bits")))
    );
    println!("orientation  {}", s.orientation_label());
    println!("encoding     {}", s.encoding_label());
    println!(
        "latency      {}",
        unknown(s.latency.map(|t| format!("{t} ticks")))
    );
    println!("size         {x}x{y}x{z}, {} blocks", s.volume());
    println!("author       {}", unknown(s.author));
    println!("built on     {}", unknown(s.built_on));
    println!("tags         {}", tags.join(", "));
    println!("source name  {}", s.source_name);
    println!("added at     {}", s.added_at);
    Ok(())
}
