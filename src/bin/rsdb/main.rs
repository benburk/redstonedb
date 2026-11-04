//! Command-line access to the schematic database named by `DATABASE_URL`.

mod accounts;
mod builds;
mod categories;
mod convert;
mod inspect;
mod outputs;
mod rebuild;
mod simulate;
mod stack_check;
mod versions;

use std::{path::PathBuf, str::FromStr as _};

use anyhow::{Context, Result, bail};
use app::{
    db::connect_to_database,
    models::{schematic, similarity, tag},
};
use clap::{Parser, Subcommand};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

use accounts::{list_users, respell, set_role};
use builds::{
    Set, export, guide_images, list, remove, search_command, set, show,
};
use categories::{define_tag, print_categories};
use convert::{Convert, convert_command};
use inspect::{Inspect, evaluate, inspect_command};
use outputs::{Outputs, outputs_command};
use rebuild::{Rebuild, finish, needs, rebuild};
use simulate::simulate;
use stack_check::{StackCheck, stack_check_command};
use versions::{
    Replace, Standardizing, colours, new_version, prune, replace, reread,
};

/// A database of Minecraft redstone schematics.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a .schem file, or a stored build by id, or with neither every
    /// build on the list, by function: levers, buttons, and comparators with
    /// air behind in, repeaters and comparators facing air out, what the
    /// site would fix and what only the author can
    Inspect(Inspect),
    /// Add the builds of a category from another copy of the database,
    /// such as a pull of the live one, the colour-free way: each probed,
    /// its roles kept with it, standardized, and stored under its own id
    /// with its details, credits, and tags; what can't be is listed with why
    Rebuild(Rebuild),
    /// For a recognized build that passes its test, or with no id every
    /// one: take out its unused blocks, as a new version, and keep what
    /// each building block must be, for palettes
    Finish { id: Option<String> },
    /// For a recognized build that passes its test, or with no id every one
    /// that has none: keep what each building block must be, for block
    /// themes, taking nothing out, as after a version fixed by hand
    Needs { id: Option<String> },
    /// Draw the How builds are recognized page's example builds into `dir`, one
    /// WebP each, named for the example
    GuideImages { dir: PathBuf },
    /// Run builds' tests and timing again, as after a change to the
    /// simulator: those `ids` name, such as `rsdb search --ids` lists, or
    /// with `--all` every build with a test. Nothing reruns on its own
    Simulate {
        /// Builds' ids, or the starts of them
        ids: Vec<String>,
        /// Every build with a test
        #[arg(long, conflicts_with = "ids")]
        all: bool,
    },
    /// Work out every build of `category`, or one build, from its file
    /// alone, and list where what is worked out differs from what is
    /// stored
    Evaluate {
        /// A build's id, or the start of one
        id: Option<String>,
        #[arg(long, default_value = "adder")]
        category: String,
    },
    /// Take out the blocks no part of a build's circuit uses, found by the
    /// rules of redstone, from a stored build, or with no id every build
    /// whose category the site tests, each as a new version, only where
    /// the site's test passes and its timing comes out the same
    Prune {
        id: Option<String>,
        /// List what would be taken out, changing nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Replace a stored build's file with a new version that fixes it, as
    /// the site's New version page does: keeping 90% of its blocks or
    /// changing at most 10, its inputs and outputs free to move
    NewVersion {
        id: String,
        file: PathBuf,
        /// What it fixes
        #[arg(long)]
        note: Option<String>,
        /// For the migration to the rule for outputs alone, its port of a
        /// build: let it change more than 10% of the blocks, the only check
        /// it waives, as a port's taps may on a small build
        #[arg(long)]
        outputs_migration: bool,
        #[command(flatten)]
        standardizing: Standardizing,
    },
    /// List a file's or stored build's inputs and its outputs by the rule
    /// for them, a repeater or comparator powering nothing, in buses by how
    /// they line up, beside the outputs the old colour rule read, and its
    /// signal-strength inputs, comparators with air behind, beside the old
    /// rule's, and probe it with them, or test one with signal-strength
    /// inputs through the lines their shapes give. Reads the database only
    Outputs(Outputs),
    /// Port a build saved under the old rule, which read outputs and
    /// signal-strength inputs by their colours, to the rule the site reads
    /// them by, as the owner does by hand: each output block whitened, its
    /// dust left, and beside it, out of the build, a repeater on a support
    /// reading the dust and giving out into air; and each signal-strength
    /// input fed by a comparator beside its dust with air behind; then
    /// check the port tests, computes, and stacks as the original does.
    /// With --all, every stored build, writing the ports that pass. Reads
    /// the database only
    Convert(Convert),
    /// Stack copies of a build as the site's stacking test does, the first
    /// whole and the rest its box, each way asked, settle them, read them
    /// by the rule for outputs, and test them as the site tests its kind's
    /// copies. Reads the database only
    StackCheck(StackCheck),
    /// Change fields of a schematic; omitted fields are kept
    Set(Set),
    /// Delete a schematic
    Remove { id: String },
    /// Read every stored file again: take out corner markers, fit its box,
    /// paste from its south-east-bottom corner unless it keeps its own,
    /// measure it, and read its facts, such as its torches and the oldest
    /// release it works in
    Reread,
    /// Put a file in place of a stored build's, as a fix to the file, such
    /// as recolouring its blocks: no version is added and nothing is
    /// compared. Then read it as `reread` does
    Replace(Replace),
    /// Keep the colours an author's own copy of a stored build gives it,
    /// shown as its original colours: the copy must hold the same kind of
    /// block in every place under some turn or mirror, any solid block for
    /// a solid one, any see-through one for glass, and a furnace for a
    /// barrel. Kept with the newest version it matches
    Colours { id: String, file: PathBuf },
    /// Respell every stored Minecraft name as its player spells it, asking
    /// Mojang: credits, importers, and accounts, which also gain the UUID
    Respell,
    /// List the standard categories, with their measures, designs, flags,
    /// and aliases, as the code lists them; a category is added there
    Categories,
    /// List accounts with their roles
    Users,
    /// Set an account's role: user, mod, or admin
    Role {
        /// The account's Discord username
        username: String,
        #[arg(value_parser = ["user", "mod", "admin"])]
        role: String,
    },
    /// List schematics matching every term, such as
    /// `register vertical alpha -torchless`
    Search {
        #[arg(required = true, allow_hyphen_values = true)]
        terms: Vec<String>,
        /// Only each build's id, one a line, to pass to `simulate`; give
        /// it before the terms, which may start with a hyphen
        #[arg(long)]
        ids: bool,
    },
    /// List schematics by id prefix and filename
    List {
        #[arg(long)]
        category: Option<String>,
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
        /// Put glass at two opposite corners of the box `//rstack` stacks,
        /// to select it in game by; with no copies
        #[arg(long)]
        markers: bool,
    },
    /// Attach tags to a schematic
    Tag { id: String, tags: Vec<String> },
    /// Detach tags from a schematic
    Untag { id: String, tags: Vec<String> },
    /// List known tags by kind
    Tags,
    /// Create a new tag, named in full words
    DefineTag {
        name: String,
        #[arg(long)]
        kind: tag::Kind,
        /// Abbreviation that filenames write instead of the name
        #[arg(long)]
        token: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Command::Outputs(_) | Command::Convert(_) | Command::StackCheck(_) =
        cli.command
    {
        // Measuring only, so nothing is written, not even a checkpoint.
        let db = connect_read_only().await?;
        let result = run(&db, cli.command).await;
        db.close().await;
        return result;
    }
    let db = connect_to_database().await;
    let result = run(&db, cli.command).await;
    // Builds whose file or category the command changed are compared again.
    similarity::refresh(&db).await?;
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
        Command::Inspect(args) => inspect_command(db, &args).await,
        Command::Rebuild(args) => rebuild(db, &args).await,
        Command::Finish { id } => finish(db, id.as_deref()).await,
        Command::Needs { id } => needs(db, id.as_deref()).await,
        Command::GuideImages { dir } => guide_images(&dir).await,
        Command::Simulate { ids, all } => simulate(db, &ids, all).await,
        Command::Evaluate { id, category } => {
            evaluate(db, id.as_deref(), &category).await
        }
        Command::Prune { id, dry_run } => {
            prune(db, id.as_deref(), dry_run).await
        }
        Command::NewVersion {
            id,
            file,
            note,
            outputs_migration,
            standardizing,
        } => {
            let note = note.as_deref();
            let migration = outputs_migration;
            new_version(db, &id, &file, note, migration, &standardizing).await
        }
        Command::Outputs(args) => outputs_command(db, &args).await,
        Command::Convert(args) => convert_command(db, &args).await,
        Command::StackCheck(args) => stack_check_command(db, &args).await,
        Command::Set(args) => set(db, args).await,
        Command::List { category, tags } => {
            list(db, category.as_deref(), &tags).await
        }
        Command::Search { terms, ids } => search_command(db, &terms, ids).await,
        Command::Reread => reread(db).await,
        Command::Replace(args) => replace(db, &args).await,
        Command::Colours { id, file } => colours(db, &id, &file).await,
        Command::Respell => respell(db).await,
        Command::Categories => {
            print_categories();
            Ok(())
        }
        Command::Users => list_users(db).await,
        Command::Role { username, role } => {
            set_role(db, &username, &role).await
        }
        Command::Remove { id } => remove(db, &id).await,
        Command::Show { id } => show(db, &resolve(db, &id).await?).await,
        Command::Export {
            id,
            out,
            copies,
            markers,
        } => export(db, &id, &out, copies.as_deref(), markers).await,
        Command::Tag { id, tags } => {
            let id = resolve(db, &id).await?;
            for name in &tags {
                tag::attach(db, &id, &tag_id(db, name).await?).await?;
            }
            Ok(())
        }
        Command::Untag { id, tags } => {
            let id = resolve(db, &id).await?;
            for name in &tags {
                tag::detach(db, &id, &tag_id(db, name).await?).await?;
            }
            Ok(())
        }
        Command::Tags => {
            for tag in tag::all(db).await? {
                let (kind, token) =
                    (tag.kind.name(), tag.token.as_deref().unwrap_or(""));
                println!("{kind:<12} {token:<8} {}", tag.label());
            }
            Ok(())
        }
        Command::DefineTag { name, kind, token } => {
            define_tag(db, &name, kind, token.as_deref()).await
        }
    }
}

/// Opens `DATABASE_URL` read-only, applying no migrations, for commands
/// that only measure.
async fn connect_read_only() -> Result<SqlitePool> {
    let url = dotenvy::var("DATABASE_URL").context("DATABASE_URL not set")?;
    let options = SqliteConnectOptions::from_str(&url)?.read_only(true);
    Ok(SqlitePool::connect_with(options).await?)
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
    tag::find(db, name).await?.with_context(|| {
        format!("unknown tag {name}; see `rsdb tags` or `rsdb define-tag`")
    })
}

async fn load(db: &SqlitePool, id: &str) -> Result<schematic::Schematic> {
    schematic::get(db, id).await?.context("schematic vanished")
}
