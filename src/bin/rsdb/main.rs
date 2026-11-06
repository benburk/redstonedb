//! Command-line access to the schematic database named by `DATABASE_URL`.

mod accounts;
mod builds;
mod categories;
mod inspect;
mod outputs;
mod refresh;
mod seed;
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

use accounts::{list_users, set_role};
use builds::{Set, export, guide_images, remove, search_command, set, show};
use categories::{define_tag, print_categories, print_tags};
use inspect::{Inspect, inspect_command};
use refresh::refresh;
use seed::seed;
use versions::{Standardizing, new_version};

/// A database of Minecraft redstone schematics.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show every field of a schematic
    Show { id: String },
    /// List schematics matching every term, such as
    /// `register vertical alpha -torchless`, or the start of an id, such as
    /// `72e896eb`; with no terms, every schematic
    Search {
        #[arg(allow_hyphen_values = true)]
        terms: Vec<String>,
        /// Only each build's id, one a line, to pass to `refresh`; give it
        /// before the terms, which may start with a hyphen
        #[arg(long)]
        ids: bool,
    },
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
    /// Change fields and tags of a schematic, omitted ones kept, then
    /// refresh it, as `refresh` does
    Set(Set),
    /// Replace a stored build's file with a new version that fixes it, as
    /// the site's New version page does: keeping 90% of its blocks or
    /// changing at most 10, its inputs and outputs free to move; then
    /// refresh it, as `refresh` does
    NewVersion {
        id: String,
        file: PathBuf,
        /// What it fixes
        #[arg(long)]
        note: Option<String>,
        /// Put the file in place of the current one as a fix to the file,
        /// such as recolouring its blocks: no version added and nothing
        /// compared
        #[arg(long)]
        in_place: bool,
        #[command(flatten)]
        standardizing: Standardizing,
    },
    /// Delete a schematic
    Remove { id: String },
    /// Bring what is worked out from builds' files up to date, as after a
    /// change to the simulator or the rules: those `ids` name, such as
    /// `rsdb search --ids` lists, or with `--all` every build, each read
    /// again, judged again as its category, its test and timing run again,
    /// and what its building blocks must be kept, for themes. Nothing
    /// reruns on its own
    Refresh {
        /// Builds' ids, or the starts of them
        ids: Vec<String>,
        /// Every build
        #[arg(long, conflicts_with = "ids")]
        all: bool,
        /// Also take out the blocks no part of a build's circuit uses, found
        /// by the rules of redstone, as a new version, only where its test
        /// passes and its timing comes out the same
        #[arg(long)]
        prune: bool,
        /// Change nothing: work each build out from its file alone and list
        /// where that differs from what is stored, and with `--prune` what
        /// would be taken out; with `--all`, the builds of each category
        /// whose every measured detail is worked out so
        #[arg(long)]
        dry_run: bool,
    },
    /// Check a .schem file, or a stored build by id, or with neither every
    /// build on the list, by function: levers, buttons, and comparators with
    /// air behind in, repeaters and comparators facing air out, what the
    /// site would fix and what only the author can; for one, its inputs and
    /// outputs in buses and what probing finds with them, or with `--stack`
    /// how its copies stack. Reads the database only
    Inspect(Inspect),
    /// List known tags by kind, or with `--define` create a new one, named
    /// in full words
    Tags {
        /// A new tag's name, in full words
        #[arg(long)]
        define: Option<String>,
        /// The new tag's kind
        #[arg(long, requires = "define")]
        kind: Option<tag::Kind>,
        /// Abbreviation that filenames write instead of the new tag's name
        #[arg(long, requires = "define")]
        token: Option<String>,
    },
    /// List the standard categories, with their measures, designs, flags,
    /// and aliases, as the code lists them; a category is added there
    Categories,
    /// List accounts with their roles, or with `--role` set one's: user,
    /// mod, or admin
    Users {
        /// An account's Discord username and its new role
        #[arg(long, num_args = 2, value_names = ["USERNAME", "ROLE"])]
        role: Option<Vec<String>>,
    },
    /// Fill a new database for development: the admin account `debug`, as
    /// `/dev/login` makes, and every `.schem` file in `dir` published as its
    /// upload with the details its preview suggests, simulated and stored
    /// whole as it goes in
    Seed {
        #[arg(default_value = "tests/fixtures")]
        dir: PathBuf,
    },
    /// Draw the How builds are recognized page's example builds into `dir`, one
    /// WebP each, named for the example
    GuideImages { dir: PathBuf },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Command::Inspect(_) = cli.command {
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
    // Move the write-ahead log's pages into the database file, so it alone
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
        Command::Show { id } => show(db, &resolve(db, &id).await?).await,
        Command::Search { terms, ids } => search_command(db, &terms, ids).await,
        Command::Export {
            id,
            out,
            copies,
            markers,
        } => export(db, &id, &out, copies.as_deref(), markers).await,
        Command::Set(args) => set(db, args).await,
        Command::NewVersion {
            id,
            file,
            note,
            in_place,
            standardizing,
        } => {
            let note = note.as_deref();
            new_version(db, &id, &file, note, in_place, &standardizing).await
        }
        Command::Remove { id } => remove(db, &id).await,
        Command::Refresh {
            ids,
            all,
            prune,
            dry_run,
        } => refresh(db, &ids, all, prune, dry_run).await,
        Command::Inspect(args) => inspect_command(db, &args).await,
        Command::Tags {
            define,
            kind,
            token,
        } => match (define, kind) {
            (Some(name), Some(kind)) => {
                define_tag(db, &name, kind, token.as_deref()).await
            }
            (Some(_), None) => bail!("a new tag needs its --kind"),
            (None, _) => print_tags(db).await,
        },
        Command::Categories => {
            print_categories();
            Ok(())
        }
        Command::Users { role } => match role.as_deref() {
            Some([username, role]) => set_role(db, username, role).await,
            _ => list_users(db).await,
        },
        Command::Seed { dir } => seed(db, &dir).await,
        Command::GuideImages { dir } => guide_images(&dir).await,
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

async fn load(db: &SqlitePool, id: &str) -> Result<schematic::Schematic> {
    schematic::get(db, id).await?.context("schematic vanished")
}
