//! Command-line access to the schematic database named by `DATABASE_URL`.

use std::{
    collections::HashMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use app::{
    db::connect_to_database,
    import, interface, lint,
    models::schematic::{self, Changes, Credit, New, Optional, StackDirection},
    models::user::{Role, User},
    models::{simulation, version},
    mojang,
    schem::{self, Blocks},
    search,
    simulate::{Report, Signal, Stacking, Timing},
    stack, standardize,
};
use clap::{Args, Parser, Subcommand};
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
    /// building conventions, as an upload would: with any corner markers
    /// taken out and the box fitted first
    Lint { file_or_id: String },
    /// Simulate a stored adder through its inputs and check every sum;
    /// with no id, every stored adder. Up to 20 input bits, every
    /// combination is tried, and beyond that every case of each byte and
    /// pairings of test patterns
    Simulate { id: Option<String> },
    /// Replace a stored build's file with a new version that fixes it, as
    /// the site's New version page does: as wide and deep, with the same
    /// inputs and outputs, and keeping 95% of its blocks or changing at
    /// most 10
    NewVersion {
        id: String,
        file: PathBuf,
        /// What it fixes
        #[arg(long)]
        note: Option<String>,
        /// Standardize the file first, as `add --standardize` does
        #[arg(long)]
        standardize: bool,
    },
    /// Change fields of a schematic; omitted fields are kept
    Set(Set),
    /// Delete a schematic
    Remove { id: String },
    /// Read every stored file again: take out corner markers, fit its box,
    /// turn status lines pink, paste from its south-east-bottom corner
    /// unless it keeps its own, measure it, and count its torches
    Reread,
    /// Respell every stored Minecraft name as its player spells it, asking
    /// Mojang: credits, importers, and accounts, which also gain the UUID
    Respell,
    /// List the standard categories, with their designs and flags
    Categories,
    /// Add a standard category to the list
    DefineCategory(NewCategory),
    /// Add a yes-or-no flag to a category's builds, such as a shift
    /// register being bidirectional
    DefineFlag {
        category: String,
        /// Lowercase words joined by underscores, such as `bidirectional`
        name: String,
        /// How people read it; the name's words by default
        #[arg(long)]
        label: Option<String>,
    },
    /// Add a design to a category
    DefineDesign {
        category: String,
        name: String,
        /// The usual short name, such as CLA
        #[arg(long)]
        abbreviation: Option<String>,
        /// How people read it, such as XOR; the name's words by default
        #[arg(long)]
        label: Option<String>,
    },
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
        /// Put a glass block off each corner, to select the build in-game by
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
        #[arg(long, value_parser = ["restriction", "other"])]
        kind: String,
        /// Abbreviation that filenames write instead of the name
        #[arg(long)]
        token: Option<String>,
    },
}

#[derive(Args)]
struct Set {
    id: String,
    #[arg(long)]
    category: Option<String>,
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
            "builders",
            "contributors",
            "built-on",
            "imported-by",
            "description",
            "design",
            "flags",
            "name",
            "logic",
            "throughput",
            "bytes",
        ]
    )]
    unset: Vec<String>,
}

#[derive(Args)]
struct Add {
    file: PathBuf,
    #[arg(long)]
    category: String,
    #[arg(long, value_parser = ["horz", "vert", "diag"])]
    orientation: String,
    /// Left out for a category off the list, such as a piston door
    #[arg(long, value_parser = ["bin", "hex"])]
    encoding: Option<String>,
    #[command(flatten)]
    details: Details,
    /// Tag to attach; repeatable
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Import even if the file breaks the building conventions
    #[arg(long)]
    force: bool,
    /// Whiten every block but redstone components, inputs, and outputs,
    /// and turn slabs and glass into white stained glass, before linting
    #[arg(long)]
    standardize: bool,
    /// Keep where the file's //paste puts the build, rather than with the
    /// player one block south of its south-east-bottom corner
    #[arg(long)]
    keep_paste: bool,
}

#[derive(Args)]
struct NewCategory {
    name: String,
    /// How people read it, such as RAM; words of the name by default
    #[arg(long)]
    label: Option<String>,
    /// How people read many of them, such as ALUs; the label with an s by
    /// default
    #[arg(long)]
    plural: Option<String>,
    /// Each of its builds gives its own name, as bussing's do
    #[arg(long)]
    named: bool,
    /// The sidebar section it sits in; left out, it sits under Other
    #[arg(long, value_parser = schematic::SECTIONS.map(|(name, _)| name))]
    section: Option<String>,
    /// Its builds are serial, taking their operands a bit at a time, so
    /// their throughput leads their names and splits their leaderboards
    #[arg(long)]
    serial: bool,
    /// Its builds' usual logic, which a build follows unless it gives its
    /// own
    #[arg(long, value_parser = ["comb", "seq"])]
    logic: Option<String>,
    /// Its builds are memories, each holding a number of bytes
    #[arg(long)]
    holds_bytes: bool,
}

#[derive(Args)]
struct Details {
    /// Datapath width in bits
    #[arg(long)]
    width: Option<u32>,
    /// Latency in ticks
    #[arg(long)]
    latency: Option<u32>,
    /// The fewest ticks from one input to the next that still gives right
    /// outputs; for a serial build, from one bit to the next, as in a
    /// 2-tick serial adder
    #[arg(long)]
    throughput: Option<u32>,
    /// Combinational or sequential, where it differs from what its category
    /// usually is
    #[arg(long, value_parser = ["comb", "seq"])]
    logic: Option<String>,
    /// For a memory, how many bytes it holds, as in a 32-byte RAM
    #[arg(long)]
    bytes: Option<u32>,
    /// Whether every output bit settles on the same tick: true or false
    #[arg(long)]
    synchronized: Option<bool>,
    /// A direction the build stacks along, such as `west=2,max=7`: blocks
    /// each copy sits west, up, or north of the one before, and the most
    /// copies that work; repeatable, and replaces any stored directions
    #[arg(long = "stack")]
    stacking: Vec<StackDirection>,
    /// A builder's in-game name, as written; repeatable, in order.
    /// Given to `set`, the names replace the stored builders
    #[arg(long = "builder", alias = "author")]
    builders: Vec<String>,
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
    /// The build's own name, for a category that takes one, such as
    /// `Bus Crossing` under bussing
    #[arg(long = "name")]
    given_name: Option<String>,
    /// The design within its category, such as `carry_lookahead`; see
    /// `rsdb categories`. Left out, `add` stores `other`
    #[arg(long)]
    design: Option<String>,
    /// A flag of its category it has, such as `carry_in`; repeatable. `add`
    /// also sets the carry flags the file's signs show. Given to `set`, the
    /// flags replace the stored ones; see `rsdb categories`
    #[arg(long = "flag")]
    flags: Vec<String>,
    /// Whether the build follows the building conventions: true or false.
    /// A build that does not is imported without the lint.
    #[arg(long)]
    conventional: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = connect_to_database().await;
    import::count_torches(&db).await?;
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
        Command::Simulate { id } => simulate(db, id.as_deref()).await,
        Command::NewVersion {
            id,
            file,
            note,
            standardize,
        } => new_version(db, &id, &file, note.as_deref(), standardize).await,
        Command::Set(args) => set(db, args).await,
        Command::List { category, tags } => {
            list(db, category.as_deref(), &tags).await
        }
        Command::Search { terms } => {
            let terms = search::parse(&terms.join(" "));
            for s in search::matching(&terms, schematic::all(db).await?) {
                println!("{}  {}", &s.id[..8], s.filename());
            }
            Ok(())
        }
        Command::Reread => reread(db).await,
        Command::Respell => respell(db).await,
        Command::Categories => print_categories(db).await,
        Command::DefineCategory(args) => define_category(db, args).await,
        Command::DefineFlag {
            category,
            name,
            label,
        } => define_flag(db, category, name, label).await,
        Command::DefineDesign {
            category,
            name,
            abbreviation,
            label,
        } => Ok(schematic::define_design(
            db,
            &schematic::Design {
                category,
                name,
                abbreviation,
                label,
            },
        )
        .await?),
        Command::Users => {
            for user in User::all(db).await? {
                println!("{:<6} {}", user.role.as_str(), user.username);
            }
            Ok(())
        }
        Command::Role { username, role } => {
            set_role(db, &username, &role).await
        }
        Command::Remove { id } => {
            let id = resolve(db, &id).await?;
            let filename = load(db, &id).await?.filename();
            schematic::delete(db, &id).await?;
            println!("removed {}  {filename}", &id[..8]);
            Ok(())
        }
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
                println!("{:<12} {:<8} {}", tag.kind, token, tag.label());
            }
            Ok(())
        }
        Command::DefineTag { name, kind, token } => {
            if schematic::DERIVED.contains(&schematic::tag_name(&name).as_str())
            {
                bail!("{name} is worked out from the build, not tagged");
            }
            schematic::define_tag(db, &name, token.as_deref(), &kind).await?;
            Ok(())
        }
    }
}

/// Changes the fields `args` gives and forgets the ones it unsets.
async fn set(db: &SqlitePool, args: Set) -> Result<()> {
    let Set {
        id,
        category,
        orientation,
        encoding,
        details,
        unset,
    } = args;
    let id = resolve(db, &id).await?;
    let imported_by = details.imported_by;
    let changes = Changes {
        category: category.as_deref(),
        width: details.width.map(i64::from),
        orientation: orientation.as_deref(),
        encoding: encoding.as_deref(),
        latency: details.latency.map(i64::from),
        synchronized: details.synchronized,
        built_on: details.built_on.as_deref(),
        imported_by: imported_by.as_deref(),
        conventional: details.conventional,
        design: details.design.as_deref(),
        description: details.description.as_deref(),
        given_name: details.given_name.as_deref(),
        logic: details.logic.as_deref(),
        throughput: details.throughput.map(i64::from),
        bytes: details.bytes.map(i64::from),
    };
    let current = load(db, &id).await?;
    let category = changes.category.unwrap_or(&current.category);
    let design = changes.design.unwrap_or(&current.design);
    if let Err(message) =
        schematic::check_category(db, category, design).await?
    {
        bail!("{message}");
    }
    schematic::update(db, &id, &changes).await?;
    let fields: Vec<Optional> = unset
        .iter()
        .filter_map(|field| match field.as_str() {
            "width" => Some(Optional::Width),
            "latency" => Some(Optional::Latency),
            "synchronized" => Some(Optional::Synchronized),
            "built-on" => Some(Optional::BuiltOn),
            "imported-by" => Some(Optional::ImportedBy),
            "description" => Some(Optional::Description),
            "name" => Some(Optional::GivenName),
            "logic" => Some(Optional::Logic),
            "throughput" => Some(Optional::Throughput),
            "bytes" => Some(Optional::Bytes),
            "design" => Some(Optional::Design),
            _ => None,
        })
        .collect();
    schematic::forget(db, &id, &fields).await?;
    let mut conn = db.acquire().await?;
    if !details.stacking.is_empty() || unset.iter().any(|f| f == "stacking") {
        schematic::set_stacking(&mut conn, &id, &details.stacking).await?;
    }
    if !details.flags.is_empty() || unset.iter().any(|f| f == "flags") {
        let kept =
            schematic::keep_flags(&mut *conn, category, &details.flags).await?;
        if let Some(name) = details.flags.iter().find(|f| !kept.contains(f)) {
            bail!("{name} is not a flag of {category}; see rsdb categories");
        }
        schematic::set_flags(&mut conn, &id, &kept).await?;
    }
    let credits = [
        (Credit::Builder, &details.builders, "builders"),
        (Credit::Contributor, &details.contributors, "contributors"),
    ];
    for (credit, names, field) in credits {
        if names.is_empty() && !unset.iter().any(|f| f == field) {
            continue;
        }
        if let Some(name) =
            names.iter().find(|n| !schematic::valid_player_name(n))
        {
            bail!("{name} is not a Minecraft name");
        }
        let names = mojang::respell(names).await.map_err(anyhow::Error::msg)?;
        schematic::set_credits(&mut conn, &id, credit, &names).await?;
    }
    // A new category or width may bring a test or drop one.
    simulation::refresh(db, &id, false).await?;
    simulation::refresh_timing(db, &id, false).await?;
    println!("{}", load(db, &id).await?.filename());
    Ok(())
}

async fn add(db: &SqlitePool, args: &Add) -> Result<()> {
    let file = &args.file;
    let data = fs::read(file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let details = &args.details;
    let conventional = details.conventional.unwrap_or(true);
    let prepared =
        import::prepare(&data, args.standardize, conventional, args.keep_paste)
            .with_context(|| file.display().to_string())?;
    // As the category has it: only an adder may leave its lone control and
    // status lines unsigned.
    let violations: Vec<String> = if conventional {
        lint::check_for(&prepared.blocks, &args.category)
            .iter()
            .map(ToString::to_string)
            .collect()
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
    let size = &prepared.size;
    // The flags given, and the carry lines the signs show.
    let mut flags = details.flags.clone();
    flags.extend(import::sign_flags(&interface::read(&prepared.blocks)));
    let imported_by = details.imported_by.clone();
    // Credited as their players spell them.
    let builders = mojang::respell(&details.builders)
        .await
        .map_err(anyhow::Error::msg)?;
    let contributors = mojang::respell(&details.contributors)
        .await
        .map_err(anyhow::Error::msg)?;
    let id = import::store(
        db,
        &New {
            category: &args.category,
            width: details.width.map(i64::from),
            orientation: &args.orientation,
            encoding: args.encoding.as_deref(),
            latency: details.latency.map(i64::from),
            synchronized: details.synchronized,
            size: (size.x.into(), size.y.into(), size.z.into()),
            built_on: details.built_on.as_deref(),
            imported_by: imported_by.as_deref(),
            conventional,
            design: details.design.as_deref(),
            description: details.description.as_deref(),
            given_name: details.given_name.as_deref(),
            logic: details.logic.as_deref(),
            throughput: details.throughput.map(i64::from),
            bytes: details.bytes.map(i64::from),
            torches: prepared.torches,
            keeps_paste: args.keep_paste,
            uploaded_by: None,
            sha256: &prepared.sha256,
            data: &prepared.data,
        },
        &import::Extras {
            builders: &builders,
            contributors: &contributors,
            tags: &args.tags,
            new_tags: false,
            stacking: &details.stacking,
            flags: &flags,
        },
    )
    .await?;
    let name = format!("{}  {}", &id[..8], load(db, &id).await?.filename());
    println!("{name}");
    if simulation::refresh(db, &id, false).await?.is_some()
        && let Some(report) = simulation::refresh_timing(db, &id, false).await?
    {
        print_report(&name, &report);
    }
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
    markers: bool,
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
    let data = if markers {
        standardize::with_corner_markers(&data)?
    } else {
        data
    };
    let path = out.join(s.filename());
    fs::File::create_new(&path)
        .with_context(|| format!("cannot create {}", path.display()))?
        .write_all(&data)?;
    println!("{}", path.display());
    Ok(())
}

/// Adds a standard category, its label being its name's words and its
/// plural the label with an s unless given.
async fn define_category(db: &SqlitePool, args: NewCategory) -> Result<()> {
    let label = args.label.unwrap_or_else(|| schematic::words(&args.name));
    let category = schematic::Category {
        plural: args.plural.unwrap_or_else(|| format!("{label}s")),
        name: args.name,
        label,
        named: args.named,
        designs_interchangeable: true,
        section: args.section,
        serial: args.serial,
        logic: args.logic,
        holds_bytes: args.holds_bytes,
        named_by_design: false,
    };
    Ok(schematic::define_category(db, &category).await?)
}

/// Adds a flag to a category, labelled with its name's words, the first
/// capitalized, unless given a label.
async fn define_flag(
    db: &SqlitePool,
    category: String,
    name: String,
    label: Option<String>,
) -> Result<()> {
    let label = label.unwrap_or_else(|| {
        let words = name.replace('_', " ");
        let mut chars = words.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    });
    let flag = schematic::Flag {
        category,
        name,
        label,
    };
    Ok(schematic::define_flag(db, &flag).await?)
}

/// Sets the role of the account with a Discord username.
async fn set_role(db: &SqlitePool, username: &str, role: &str) -> Result<()> {
    let user = User::get_by_username(db, username).await?.with_context(
        || {
            format!(
                "no account named {username}; they log in with Discord first"
            )
        },
    )?;
    let role = Role::parse(role).context("unknown role")?;
    User::set_role(db, user.id, role).await?;
    println!("{} is now {}", user.username, role.as_str());
    Ok(())
}

/// Prints each category with its designs.
async fn print_categories(db: &SqlitePool) -> Result<()> {
    let designs = schematic::designs(db).await?;
    let flags = schematic::flags(db).await?;
    for category in schematic::categories(db).await? {
        let designs: Vec<&str> = designs
            .iter()
            .filter(|d| d.category == category.name)
            .map(|d| d.name.as_str())
            .collect();
        let flags: Vec<&str> = flags
            .iter()
            .filter(|f| f.category == category.name)
            .map(|f| f.name.as_str())
            .collect();
        let mut parts =
            vec![format!("{:<20} {:<22}", category.name, category.label)];
        if !designs.is_empty() {
            parts.push(format!("designs: {}", designs.join(", ")));
        }
        if !flags.is_empty() {
            parts.push(format!("flags: {}", flags.join(", ")));
        }
        println!("{}", parts.join(" ").trim_end());
    }
    Ok(())
}

/// Reads every stored file again and prints what changed: the file
/// itself, with any corner markers taken out, its box fitted, its status
/// lines pink, and pasting from one block south of its south-east-bottom
/// corner unless it keeps where its uploader's pasted, and its size.
async fn reread(db: &SqlitePool) -> Result<()> {
    let keeping = schematic::keeping_paste(db).await?;
    for s in schematic::all(db).await? {
        let stored = schematic::data(db, &s.id)
            .await?
            .context("schematic vanished")?;
        let mut data = standardize::tidy(&stored)?;
        if !keeping.contains(&s.id) {
            data = schem::paste_from_corner(&data)?;
        }
        // Older builds marked status lines yellow; they are pink now.
        if s.conventional {
            data = standardize::pink_status(&data)?;
        }
        if data != stored {
            let sha256 = import::sha256(&data);
            schematic::set_file(db, &s.id, &data, &sha256).await?;
            version::rewrite_current(db, &s.id, &data, &sha256).await?;
            let [w, h, l] = Blocks::read(&stored)?.size;
            let [nw, nh, nl] = Blocks::read(&data)?.size;
            if [w, h, l] == [nw, nh, nl] {
                println!("{}  file rewritten", &s.id[..8]);
            } else {
                println!("{}  file {h}x{w}x{l} -> {nh}x{nw}x{nl}", &s.id[..8]);
            }
        }
        let blocks = Blocks::read(&data)?;
        let torches = import::torches(&blocks);
        if s.torches != Some(torches) {
            schematic::set_torches(db, &s.id, torches).await?;
        }
        let size = import::measure(&blocks, s.conventional)?;
        let size = (size.x.into(), size.y.into(), size.z.into());
        if size != s.size {
            schematic::set_size(db, &s.id, size).await?;
            let (x, y, z) = s.size;
            let (nx, ny, nz) = size;
            println!("{}  {y}x{x}x{z} -> {ny}x{nx}x{nz}", &s.id[..8]);
        }
        simulation::refresh(db, &s.id, false).await?;
        simulation::refresh_timing(db, &s.id, false).await?;
    }
    Ok(())
}

/// Asks Mojang how each stored Minecraft name is spelled and respells it
/// wherever it differs. A name Mojang does not know is left as it is.
async fn respell(db: &SqlitePool) -> Result<()> {
    let names = User::player_names(db).await?;
    let found = mojang::profiles(&names).await.map_err(anyhow::Error::msg)?;
    for player in &found {
        let changed =
            User::respell_player(db, &player.name, &player.uuid).await?;
        if changed > 0 {
            println!("{}  {changed} respelled", player.name);
        }
    }
    println!("{} of {} names known to Mojang", found.len(), names.len());
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

/// Prints the schematics with `category`, if given, and every one of `tags`,
/// each named in full or by its filename token.
async fn list(
    db: &SqlitePool,
    category: Option<&str>,
    tags: &[String],
) -> Result<()> {
    for s in schematic::all(db).await? {
        let has_tags = tags.iter().all(|t| {
            let name = schematic::tag_name(t);
            s.tags
                .iter()
                .any(|tag| tag.name == name || tag.token() == t)
        });
        if category.is_none_or(|f| f == s.category) && has_tags {
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
    let violations = import::prepare(&data, false, true, true)?.violations;
    for violation in &violations {
        println!("{violation}");
    }
    if !violations.is_empty() {
        bail!("{} building conventions broken", violations.len());
    }
    Ok(())
}

/// Replaces build `id`'s file with `file` as its next version, after the
/// checks the site makes, then simulates it again.
async fn new_version(
    db: &SqlitePool,
    id: &str,
    file: &Path,
    note: Option<&str>,
    standardize: bool,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let s = load(db, &id).await?;
    let checked =
        app::version::check(db, &s, &fs::read(file)?, standardize).await?;
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
    let size = &checked.prepared.size;
    let number = version::publish(
        db,
        &id,
        &version::New {
            data: &checked.prepared.data,
            sha256: &checked.prepared.sha256,
            size: (size.x.into(), size.y.into(), size.z.into()),
            torches: checked.prepared.torches,
            added_by: None,
            note: note.map(str::trim).filter(|note| !note.is_empty()),
        },
    )
    .await?;
    let name = format!("{}  {}", &id[..8], s.filename());
    println!("{name}: version {number}");
    if simulation::refresh(db, &id, false).await?.is_some()
        && let Some(report) = simulation::refresh_timing(db, &id, false).await?
    {
        print_report(&name, &report);
    }
    Ok(())
}

/// Runs the test of build `id`, or of every build with one, again, and
/// stores what it finds.
async fn simulate(db: &SqlitePool, id: Option<&str>) -> Result<()> {
    let ids = match id {
        Some(prefix) => vec![resolve(db, prefix).await?],
        None => schematic::all(db)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect(),
    };
    let one = id.is_some();
    let mut failed = 0;
    for id in &ids {
        let start = std::time::Instant::now();
        let report = match simulation::refresh(db, id, true).await? {
            Some(_) => simulation::refresh_timing(db, id, true).await?,
            None => None,
        };
        let name = format!("{}  {}", &id[..8], load(db, id).await?.filename());
        match report {
            Some(report) => {
                print_report(&name, &report);
                println!("        in {:.1?}", start.elapsed());
                failed += usize::from(report.failed());
            }
            None if one => {
                println!("{name}: no test for its category and width");
            }
            // Every build is tried; only those with a test say so.
            None => {}
        }
    }
    if failed > 0 {
        bail!("{failed} builds fail their tests");
    }
    Ok(())
}

/// One build's simulation result, on a line or a few.
fn print_report(name: &str, report: &Report) {
    match report {
        Report::Ran(run) if run.wrong == 0 => println!(
            "ok      {name}: {} {} {}, all right",
            match (run.exhaustive, run.by_digit) {
                (true, _) => "every",
                (false, true) => "each digit's and the patterns'",
                (false, false) => "each byte's and the patterns'",
            },
            run.cases,
            run.pattern
        ),
        Report::Ran(run) => {
            println!(
                "WRONG   {name}: {} of {} {} wrong",
                run.wrong, run.cases, run.pattern
            );
            if let Some(hint) = &run.hint {
                println!("        {hint}");
            }
            let bits: Vec<String> = run
                .wrong_bits
                .iter()
                .map(|(bit, count)| format!("{bit} ({count})"))
                .collect();
            println!("        wrong bits: {}", bits.join(", "));
            let list = |signals: &[Signal], values: &[u64]| {
                signals
                    .iter()
                    .zip(values)
                    .map(|(signal, value)| format!("{} {value}", signal.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            for w in &run.examples {
                println!(
                    "        {}: got {}, expected {}",
                    list(&run.inputs, &w.inputs),
                    list(&run.outputs, &w.got),
                    list(&run.outputs, &w.expected)
                );
            }
        }
        Report::Skipped { reason } => println!("skipped {name}: {reason}"),
        Report::Broken { reason } => println!("BROKEN  {name}: {reason}"),
    }
    if let Report::Ran(run) = report
        && let Some(stacking) = &run.stacking
    {
        match stacking {
            Stacking::Stacks {
                step,
                copies,
                bits,
                cases,
            } => println!(
                "        stacks: {copies} copies {} apart make {bits} bits, all {cases} cases right",
                step.describe()
            ),
            Stacking::Not { step, reason } => println!(
                "        doesn't stack{}: {reason}",
                step.map(|s| format!(" {} apart", s.describe()))
                    .unwrap_or_default()
            ),
        }
    }
    if let Some(timing) = report.timing() {
        print_timing(timing);
    }
}

/// When a build's inputs may change and its outputs settle, from A's
/// change, and each bit's timing for an output that is not synced.
fn print_timing(timing: &Timing) {
    let tick = |tick: i32| match tick {
        0 => "0".to_owned(),
        t => format!("{t:+}"),
    };
    let inputs: Vec<String> = timing
        .inputs
        .iter()
        .map(|input| {
            let at = input.tick.map_or_else(|| "unused".to_owned(), tick);
            format!("{} at {at}", input.name)
        })
        .collect();
    let outputs: Vec<String> = timing
        .outputs
        .iter()
        .map(|output| match output.carried {
            0 => format!("{} by {}t", output.name, output.latency),
            carried => format!(
                "{} by {}t, carried out {carried}t later",
                output.name, output.latency
            ),
        })
        .collect();
    println!(
        "        timing: {}; {}; {}",
        inputs.join(", "),
        outputs.join(", "),
        if timing.synced {
            "synced"
        } else {
            "not synced"
        }
    );
    println!(
        "        half settle by {}t, 99% by {}t, slowest {:?} by {}t, over {} changes",
        timing.median,
        timing.p99,
        timing.slowest_case,
        timing.slowest,
        timing.changes
    );
    for output in timing
        .outputs
        .iter()
        .filter(|o| o.glitches > 0 || o.staggered > 0)
    {
        println!(
            "        {} glitches in {} changes and staggers in {}; by bit, first change, settled, glitches:",
            output.name, output.glitches, output.staggered
        );
        if let Some(glitch) = &output.glitch {
            println!(
                "          e.g. from all off to {:?}: from {} to {:?}",
                glitch.case, glitch.before, glitch.steps
            );
        }
        for (bit, b) in output.bits.iter().enumerate() {
            println!(
                "          {bit}: {}, {}, {}",
                tick(b.first_change),
                tick(b.latency),
                b.glitches
            );
        }
    }
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
    println!("category     {}", s.category);
    println!("width        {}", unknown(s.width_label()));
    println!("orientation  {}", s.orientation_label());
    println!("encoding     {}", s.encoding_label().unwrap_or("none"));
    println!(
        "latency      {}",
        unknown(s.latency.map(|t| format!("{t} ticks")))
    );
    println!(
        "throughput   {}",
        unknown(s.throughput.map(|t| format!("{t} ticks")))
    );
    if let Some(bytes) = s.bytes {
        println!("capacity     {bytes} bytes");
    }
    println!("synchronized {}", s.synchronized_label());
    println!(
        "logic        {}{}",
        s.logic_label().unwrap_or("unknown").to_lowercase(),
        if s.pipelined() { ", pipelined" } else { "" }
    );
    println!("design       {}", s.design.replace('_', " "));
    let flags: Vec<&str> = s.flags.iter().map(|f| f.label.as_str()).collect();
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
    println!("size         {y}x{x}x{z} (HWD), {} blocks", s.volume());
    println!(
        "built by     {}",
        if s.builders.is_empty() {
            "unknown".to_owned()
        } else {
            s.builders.join(", ")
        }
    );
    if !s.contributors.is_empty() {
        println!("contributors {}", s.contributors.join(", "));
    }
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
    if let Some(edited) = &s.edited_at {
        println!("edited at    {edited}");
    }
    println!("downloads    {}", s.downloads);
    let versions = version::list(db, &s.id).await?;
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
    if let Some(description) = &s.description {
        println!("\n{description}");
    }
    Ok(())
}
