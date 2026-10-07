//! Command-line access to the schematic database named by `DATABASE_URL`.

use std::{fmt::Write as _, fs, io::Write as _, path::PathBuf};

use anyhow::{Context, Result, bail};
use app::{
    db::connect_to_database,
    models::schematic::{self, Changes, New},
    schem,
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
    /// Import a Sponge .schem file
    Add {
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
    },
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
    match cli.command {
        Command::Add {
            file,
            function,
            orientation,
            encoding,
            details,
            tags,
        } => {
            add(
                &db,
                &file,
                &function,
                &orientation,
                &encoding,
                &details,
                &tags,
            )
            .await
        }
        Command::Set {
            id,
            function,
            orientation,
            encoding,
            details,
        } => {
            let id = resolve(&db, &id).await?;
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
            schematic::update(&db, &id, &changes).await?;
            println!("{}", load(&db, &id).await?.filename());
            Ok(())
        }
        Command::List { function, tags } => {
            list(&db, function.as_deref(), &tags).await
        }
        Command::Show { id } => show(&db, &resolve(&db, &id).await?).await,
        Command::Export { id, out } => {
            let id = resolve(&db, &id).await?;
            let path = out.join(load(&db, &id).await?.filename());
            let data = schematic::data(&db, &id)
                .await?
                .context("schematic vanished")?;
            fs::File::create_new(&path)
                .with_context(|| format!("cannot create {}", path.display()))?
                .write_all(&data)?;
            println!("{}", path.display());
            Ok(())
        }
        Command::Tag { id, tags } => {
            let id = resolve(&db, &id).await?;
            for name in &tags {
                schematic::attach(&db, &id, &tag_id(&db, name).await?).await?;
            }
            Ok(())
        }
        Command::Untag { id, tags } => {
            let id = resolve(&db, &id).await?;
            for name in &tags {
                schematic::detach(&db, &id, &tag_id(&db, name).await?).await?;
            }
            Ok(())
        }
        Command::Tags => {
            for tag in schematic::all_tags(&db).await? {
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
            schematic::define_tag(&db, &name, token.as_deref(), &category)
                .await?;
            Ok(())
        }
    }
}

async fn add(
    db: &SqlitePool,
    file: &PathBuf,
    function: &str,
    orientation: &str,
    encoding: &str,
    details: &Details,
    tags: &[String],
) -> Result<()> {
    let data = fs::read(file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let size =
        schem::size(&data).with_context(|| file.display().to_string())?;
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
    let author = details.author.as_ref().map(|a| a.to_lowercase());

    let mut tx = db.begin().await?;
    let id = schematic::insert(
        &mut *tx,
        &New {
            function,
            width: details.width.map(i64::from),
            orientation,
            encoding,
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
    for name in tags {
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
    println!("size         {x}x{y}x{z}");
    println!("author       {}", unknown(s.author));
    println!("built on     {}", unknown(s.built_on));
    println!("tags         {}", tags.join(", "));
    println!("source name  {}", s.source_name);
    println!("added at     {}", s.added_at);
    Ok(())
}
