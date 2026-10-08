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
    import, interface,
    models::schematic::{self, Changes, Credit, New, Optional, StackDirection},
    models::user::{Role, User},
    schem::Blocks,
    search, stack, standardize,
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
    /// Change fields of a schematic; omitted fields are kept
    Set(Set),
    /// Delete a schematic
    Remove { id: String },
    /// Read every stored file again: take out corner markers and fit its
    /// box, measure it, and fill in its carry lines where they are unknown
    Reread,
    /// List the standard categories, with their designs
    Categories,
    /// Add a standard category to the list
    DefineCategory {
        name: String,
        /// How people read it, such as RAM; words of the name by default
        #[arg(long)]
        label: Option<String>,
        /// How people read many of them, such as ALUs; the label with an s
        /// by default
        #[arg(long)]
        plural: Option<String>,
        /// Its builds have carry lines worth recording
        #[arg(long)]
        carries: bool,
        /// Each of its builds gives its own name, as bussing's do
        #[arg(long)]
        named: bool,
    },
    /// Add a design to a category
    DefineDesign {
        category: String,
        name: String,
        /// The usual short name, such as CLA
        #[arg(long)]
        abbreviation: Option<String>,
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
        #[arg(long, value_parser = ["restriction", "timing", "other"])]
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
            "carries",
            "name",
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
    /// Whether it has a carry in: true or false. `add` reads it from the
    /// file's `carry in` sign when left out
    #[arg(long)]
    carry_in: Option<bool>,
    /// Whether it has a carry out: true or false. `add` reads it from the
    /// file's `carry out` sign when left out
    #[arg(long)]
    carry_out: Option<bool>,
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
        Command::Categories => print_categories(db).await,
        Command::DefineCategory {
            name,
            label,
            plural,
            carries,
            named,
        } => define_category(db, name, label, plural, carries, named).await,
        Command::DefineDesign {
            category,
            name,
            abbreviation,
        } => Ok(schematic::define_design(
            db,
            &schematic::Design {
                category,
                name,
                abbreviation,
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
        carry_in: details.carry_in,
        carry_out: details.carry_out,
        description: details.description.as_deref(),
        given_name: details.given_name.as_deref(),
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
            "design" => Some(Optional::Design),
            "carries" => Some(Optional::Carries),
            _ => None,
        })
        .collect();
    schematic::forget(db, &id, &fields).await?;
    let mut conn = db.acquire().await?;
    if !details.stacking.is_empty() || unset.iter().any(|f| f == "stacking") {
        schematic::set_stacking(&mut conn, &id, &details.stacking).await?;
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
        schematic::set_credits(&mut conn, &id, credit, names).await?;
    }
    println!("{}", load(db, &id).await?.filename());
    Ok(())
}

async fn add(db: &SqlitePool, args: &Add) -> Result<()> {
    let file = &args.file;
    let data = fs::read(file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let details = &args.details;
    let conventional = details.conventional.unwrap_or(true);
    let prepared = import::prepare(&data, args.standardize, conventional)
        .with_context(|| file.display().to_string())?;
    if !prepared.violations.is_empty() {
        for violation in &prepared.violations {
            eprintln!("{violation}");
        }
        if !args.force {
            bail!(
                "{} breaks {} building conventions; fix them or pass --force",
                file.display(),
                prepared.violations.len()
            );
        }
    }
    let size = &prepared.size;
    let file_carries = import::carries(&interface::read(&prepared.blocks));
    let imported_by = details.imported_by.clone();
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
            carry_in: details.carry_in.or(Some(file_carries.0)),
            carry_out: details.carry_out.or(Some(file_carries.1)),
            description: details.description.as_deref(),
            given_name: details.given_name.as_deref(),
            uploaded_by: None,
            sha256: &prepared.sha256,
            data: &prepared.data,
        },
        &import::Extras {
            builders: &details.builders,
            contributors: &details.contributors,
            tags: &args.tags,
            new_tags: false,
            stacking: &details.stacking,
        },
    )
    .await?;
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

/// Adds a standard category, labelled with its name's words unless given
/// a label.
async fn define_category(
    db: &SqlitePool,
    name: String,
    label: Option<String>,
    plural: Option<String>,
    carries: bool,
    named: bool,
) -> Result<()> {
    let label = label.unwrap_or_else(|| schematic::words(&name));
    let plural = plural.unwrap_or_else(|| format!("{label}s"));
    let category = schematic::Category {
        name,
        label,
        plural,
        carries,
        named,
    };
    Ok(schematic::define_category(db, &category).await?)
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
    for category in schematic::categories(db).await? {
        let names: Vec<&str> = designs
            .iter()
            .filter(|d| d.category == category.name)
            .map(|d| d.name.as_str())
            .collect();
        println!(
            "{:<20} {:<20} {}",
            category.name,
            category.label,
            names.join(", ")
        );
    }
    Ok(())
}

/// Reads every stored file again and prints what changed: the file
/// itself, with any corner markers taken out and its box fitted; its size;
/// and its carry lines where they are unknown and its category has them.
async fn reread(db: &SqlitePool) -> Result<()> {
    let categories = schematic::categories(db).await?;
    for s in schematic::all(db).await? {
        let stored = schematic::data(db, &s.id)
            .await?
            .context("schematic vanished")?;
        let data = standardize::tidy(&stored)?;
        if data != stored {
            let sha256 = import::sha256(&data);
            schematic::set_file(db, &s.id, &data, &sha256).await?;
            let [w, h, l] = Blocks::read(&stored)?.size;
            let [nw, nh, nl] = Blocks::read(&data)?.size;
            println!("{}  file {w}x{h}x{l} -> {nw}x{nh}x{nl}", &s.id[..8]);
        }
        let blocks = Blocks::read(&data)?;
        let carries =
            categories.iter().any(|f| f.name == s.category && f.carries);
        if carries
            && s.conventional
            && (s.carry_in.is_none() || s.carry_out.is_none())
        {
            let (carry_in, carry_out) =
                import::carries(&interface::read(&blocks));
            let carry_in = s.carry_in.or(Some(carry_in));
            let carry_out = s.carry_out.or(Some(carry_out));
            schematic::set_carries(db, &s.id, carry_in, carry_out).await?;
            println!(
                "{}  carry in {}, carry out {}",
                &s.id[..8],
                carry_in == Some(true),
                carry_out == Some(true)
            );
        }
        let size = import::measure(&blocks, s.conventional)?;
        let size = (size.x.into(), size.y.into(), size.z.into());
        if size != s.size {
            schematic::set_size(db, &s.id, size).await?;
            let (x, y, z) = s.size;
            let (nx, ny, nz) = size;
            println!("{}  {x}x{y}x{z} -> {nx}x{ny}x{nz}", &s.id[..8]);
        }
    }
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
    let violations = import::prepare(&data, false, true)?.violations;
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
    println!("category     {}", s.category);
    println!("width        {}", unknown(s.width_label()));
    println!("orientation  {}", s.orientation_label());
    println!("encoding     {}", s.encoding_label().unwrap_or("none"));
    println!(
        "latency      {}",
        unknown(s.latency.map(|t| format!("{t} ticks")))
    );
    println!("synchronized {}", s.synchronized_label());
    println!("design       {}", s.design.replace('_', " "));
    let yes_no = |v: Option<bool>| match v {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    println!("carry in     {}", yes_no(s.carry_in));
    println!("carry out    {}", yes_no(s.carry_out));
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
    if let Some(description) = &s.description {
        println!("\n{description}");
    }
    Ok(())
}
