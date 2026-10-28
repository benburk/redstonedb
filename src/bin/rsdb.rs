//! Command-line access to the schematic database named by `DATABASE_URL`.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use app::{
    db::connect_to_database,
    details::{self, Details},
    import::{self, Analysis},
    interface,
    models::{
        catalogue::{self, Measure, Section},
        schematic::{self, Encoding, Logic, Orientation, StackDirection},
        similarity, simulation, tag,
        user::{Role, User},
        version::{self, Replacing},
    },
    mojang, search,
    simulate::{Action, Report, Signal, Stacking, Timing},
    stack,
    standardize::{self, Gray},
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
    /// Check a .schem file, or a stored build by id, or with neither every
    /// build on the list, the colour-free way: levers in, lamps out, what
    /// the site would fix and what only the builder can
    Inspect { file_or_id: Option<String> },
    /// Run a stored build's test and timing again, or with no id every
    /// build with a test, as is needed once the simulator changes
    Simulate { id: Option<String> },
    /// Write each build's simulation result and the details it sets, as
    /// JSON, to stdout, for `import-results` on another copy of the
    /// database
    ExportResults {
        /// Builds' ids, or the starts of them
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Take results `export-results` wrote, for each build whose file and
    /// details are as they were when they were worked out, and say which
    /// were not
    ImportResults { file: PathBuf },
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
        #[command(flatten)]
        standardizing: Standardizing,
    },
    /// Change fields of a schematic; omitted fields are kept
    Set(Set),
    /// Delete a schematic
    Remove { id: String },
    /// Read every stored file again: take out corner markers, fit its box,
    /// turn status lines pink, paste from its south-east-bottom corner
    /// unless it keeps its own, measure it, and read its facts, such as its
    /// torches and the oldest release it works in
    Reread,
    /// Put a file in place of a stored build's, as a fix to the file, such
    /// as whitening its gray blocks: no version is added and nothing is
    /// compared. Then read it as `reread` does
    Replace(Replace),
    /// Respell every stored Minecraft name as its player spells it, asking
    /// Mojang: credits, importers, and accounts, which also gain the UUID
    Respell,
    /// List the standard categories, with their measures, designs, flags,
    /// and aliases
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
    /// Give a category, of the list or off it, another word to be searched
    /// by, such as `connect4` for a four-in-a-row detector
    DefineAlias { category: String, alias: String },
    /// Add a design to a category: a way of working, as its every design
    /// does the same job
    DefineDesign {
        category: String,
        name: String,
        /// The usual short name, such as CLA
        #[arg(long)]
        abbreviation: Option<String>,
        /// How people read it, such as LFSR; the name's words by default
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
        #[arg(long)]
        kind: tag::Kind,
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
    #[arg(long)]
    orientation: Option<Orientation>,
    #[arg(long)]
    encoding: Option<Encoding>,
    #[command(flatten)]
    details: DetailArgs,
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
            "at-least",
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
    #[arg(long)]
    orientation: Orientation,
    /// Left out for a category off the list, such as a piston door
    #[arg(long)]
    encoding: Option<Encoding>,
    #[command(flatten)]
    details: DetailArgs,
    /// Tag to attach; repeatable
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Import even if the file breaks the building conventions
    #[arg(long)]
    force: bool,
    #[command(flatten)]
    standardizing: Standardizing,
    /// Keep where the file's //paste puts the build, rather than with the
    /// player one block south of its south-east-bottom corner
    #[arg(long)]
    keep_paste: bool,
}

#[derive(Args)]
struct Replace {
    id: String,
    file: PathBuf,
    #[command(flatten)]
    standardizing: Standardizing,
}

#[derive(Args)]
struct Standardizing {
    /// Whiten every block but redstone components, inputs, and outputs,
    /// and turn slabs and glass into white stained glass, before linting;
    /// adding also turns the build so A's levers face south, and mirrors it
    /// when that reads right as what it seems to be
    #[arg(long)]
    standardize: bool,
    /// Keep gray blocks when standardizing, marking logic only a stack's
    /// first copy keeps, rather than whitening them
    #[arg(long, requires = "standardize")]
    keep_gray: bool,
}

impl Standardizing {
    /// Whether to standardize, and what to do with gray blocks if so.
    fn asked(&self) -> Option<Gray> {
        let gray = if self.keep_gray {
            Gray::Keep
        } else {
            Gray::Whiten
        };
        self.standardize.then_some(gray)
    }
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
    #[arg(long)]
    section: Option<Section>,
    /// A number its builds are named and ranked by, in order; repeatable,
    /// and width alone when left out. A memory takes `width` then `bytes`,
    /// a serial build `throughput`, a threshold `width` then `at_least`,
    /// and a monostable `pulse`
    #[arg(long = "measure")]
    measures: Vec<Measure>,
    /// Whether its builds are usually stateless or stateful, which a build
    /// follows unless it gives its own
    #[arg(long)]
    logic: Option<Logic>,
    /// Another word it is searched by; repeatable
    #[arg(long = "alias")]
    aliases: Vec<String>,
}

#[derive(Args)]
struct DetailArgs {
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
    /// Stateless or stateful, where it differs from what its category
    /// usually is
    #[arg(long)]
    logic: Option<Logic>,
    /// For a memory, how many bytes it holds, as in a 32-byte RAM
    #[arg(long)]
    bytes: Option<u32>,
    /// For a threshold, the fewest inputs on that turn it on, as in an
    /// at-least-3 threshold
    #[arg(long)]
    at_least: Option<u32>,
    /// For a monostable, the ticks its pulse lasts, as in a 3-tick
    /// monostable; a conventional one's comes from its simulation
    #[arg(long)]
    pulse: Option<u32>,
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
        Command::Add(args) => add(db, &args).await,
        Command::Lint { file_or_id } => lint_command(db, &file_or_id).await,
        Command::Inspect { file_or_id } => {
            inspect_command(db, file_or_id.as_deref()).await
        }
        Command::Simulate { id } => simulate(db, id.as_deref()).await,
        Command::ExportResults { ids } => export_results(db, &ids).await,
        Command::ImportResults { file } => import_results(db, &file).await,
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
            standardizing,
        } => new_version(db, &id, &file, note.as_deref(), &standardizing).await,
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
        Command::Replace(args) => replace(db, &args).await,
        Command::Respell => respell(db).await,
        Command::Categories => print_categories(db).await,
        Command::DefineCategory(args) => define_category(db, args).await,
        Command::DefineAlias { category, alias } => {
            Ok(catalogue::define_alias(db, &alias, &category).await?)
        }
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
        } => Ok(catalogue::define_design(
            db,
            &catalogue::Design {
                category,
                name,
                abbreviation,
                label,
            },
        )
        .await?),
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

/// Prints every account's role and username.
async fn list_users(db: &SqlitePool) -> Result<()> {
    for user in User::all(db).await? {
        println!("{:<6} {}", user.role.as_str(), user.username);
    }
    Ok(())
}

/// Deletes build `id`.
async fn remove(db: &SqlitePool, id: &str) -> Result<()> {
    let id = resolve(db, id).await?;
    let filename = load(db, &id).await?.filename();
    schematic::delete(db, &id).await?;
    println!("removed {}  {filename}", &id[..8]);
    Ok(())
}

/// Adds a tag, unless the build itself works it out, as it does torchless.
async fn define_tag(
    db: &SqlitePool,
    name: &str,
    kind: tag::Kind,
    token: Option<&str>,
) -> Result<()> {
    if tag::DERIVED.contains(&tag::name(name).as_str()) {
        bail!("{name} is worked out from the build, not tagged");
    }
    for given in std::iter::once(name).chain(token) {
        if !tag::valid_name(&tag::name(given)) {
            bail!(
                "{given} cannot name a tag: use letters, digits, spaces, and \
                 dashes, with no dash at either end"
            );
        }
    }
    tag::define(db, name, token, kind).await?;
    Ok(())
}

/// Changes the fields `args` gives and forgets the ones it unsets, holding
/// the details to the rules the edit page does.
async fn set(db: &SqlitePool, args: Set) -> Result<()> {
    let Set {
        id,
        category,
        orientation,
        encoding,
        details: given,
        unset,
    } = args;
    let id = resolve(db, &id).await?;
    let s = load(db, &id).await?;
    let mut details = Details::of(&s);
    if let Some(category) = category {
        details.category = category;
    }
    if let Some(orientation) = orientation {
        details.orientation = orientation;
    }
    if encoding.is_some() {
        details.encoding = encoding;
    }
    overlay(&mut details, &given, &unset);
    let conventional = given.conventional.unwrap_or(s.conventional);
    let data = version::current_file(db, &id)
        .await?
        .context("schematic vanished")?;
    let build = details::Build {
        data: &data,
        conventional,
    };
    // The command line acts as an admin, so may name a new tag.
    let ready = match details::check(db, details, build, true).await? {
        Ok(ready) => ready,
        Err(message) => bail!("{message}"),
    };
    if let Some(name) = given
        .flags
        .iter()
        .find(|f| !ready.details.flags.contains(f))
    {
        bail!(
            "{name} is not a flag of {}; see rsdb categories",
            ready.category.name
        );
    }
    schematic::set_details(db, &id, &ready).await?;
    let imported_by = if unset.iter().any(|f| f == "imported-by") {
        None
    } else {
        given.imported_by.or(s.imported_by)
    };
    schematic::set_import_details(
        db,
        &id,
        imported_by.as_deref(),
        conventional,
    )
    .await?;
    // A new category or width may bring a test or drop one.
    simulation::refresh(db, &id, false).await?;
    simulation::refresh_timing(db, &id, false).await?;
    println!("{}", load(db, &id).await?.filename());
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
    put(&mut details.width, number(given.width), unsets("width"));
    put(
        &mut details.latency,
        number(given.latency),
        unsets("latency"),
    );
    put(
        &mut details.synchronized,
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
    put(
        &mut details.given_name,
        given.given_name.clone(),
        unsets("name"),
    );
    put(&mut details.logic, given.logic, unsets("logic"));
    put(
        &mut details.throughput,
        number(given.throughput),
        unsets("throughput"),
    );
    put(&mut details.bytes, number(given.bytes), unsets("bytes"));
    put(
        &mut details.at_least,
        number(given.at_least),
        unsets("at-least"),
    );
    put(&mut details.pulse, number(given.pulse), unsets("pulse"));
    put(&mut details.design, given.design.clone(), unsets("design"));
    // A list given replaces the stored one whole.
    let lists = [
        (&mut details.flags, &given.flags, "flags"),
        (&mut details.builders, &given.builders, "builders"),
        (
            &mut details.contributors,
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

/// Imports a file with its details, held to the rules an upload is, as an
/// admin: a tag not yet on the list is made under `other`.
async fn add(db: &SqlitePool, args: &Add) -> Result<()> {
    let file = &args.file;
    let data = fs::read(file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let conventional = args.details.conventional.unwrap_or(true);
    let standardize = args.standardizing.asked();
    let prepared = import::prepare(
        &data,
        standardize,
        conventional,
        args.keep_paste,
        standardize.is_some(),
    )
    .with_context(|| file.display().to_string())?;
    let mut details = Details {
        category: args.category.clone(),
        orientation: args.orientation,
        encoding: args.encoding,
        tags: args.tags.clone(),
        ..Details::default()
    };
    overlay(&mut details, &args.details, &[]);
    // The carry lines the signs show, beside the flags given.
    details
        .flags
        .extend(import::sign_flags(&interface::read(&prepared.blocks)));
    let ready =
        match details::check(db, details, details::Build::of(&prepared), true)
            .await?
        {
            Ok(ready) => ready,
            Err(message) => bail!("{message}"),
        };
    let violations =
        Analysis::of(&prepared.blocks, &ready.category.name, conventional)
            .violations;
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
    let imported_by = args.details.imported_by.as_deref();
    let id = match import::store(db, &prepared, &ready, None, None, imported_by)
        .await?
    {
        Ok(id) => id,
        Err(message) => bail!("{message}"),
    };
    // Simulated before its name is printed, as the width the name leads
    // with may be read from the file then.
    let tested = simulation::refresh(db, &id, false).await?.is_some();
    let name = format!("{}  {}", &id[..8], load(db, &id).await?.filename());
    println!("{name}");
    if tested
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
    let data = version::current_file(db, &id)
        .await?
        .context("schematic vanished")?;
    let (data, s) = match copies {
        None => (data, s),
        Some(copies) => {
            let wanted = copy_counts(&s.stacking, copies)?;
            let plan = stack::plan(&s.stacking, &wanted)?;
            let buses = stack::Buses::of(&s.category, s.orientation);
            let (data, size) = stack::stack(&data, &plan, buses)?;
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
    let label = args.label.unwrap_or_else(|| catalogue::words(&args.name));
    let category = catalogue::Category {
        plural: args.plural.unwrap_or_else(|| format!("{label}s")),
        name: args.name,
        label,
        named: args.named,
        section: args.section,
        logic: args.logic,
        measures: if args.measures.is_empty() {
            vec![Measure::Width]
        } else {
            args.measures
        },
        aliases: args.aliases,
    };
    Ok(catalogue::define_category(db, &category).await?)
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
    let flag = catalogue::Flag {
        category,
        name,
        label,
    };
    Ok(catalogue::define_flag(db, &flag).await?)
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
    let designs = catalogue::designs(db).await?;
    let flags = catalogue::flags(db).await?;
    for category in catalogue::categories(db).await? {
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
        let measures: Vec<&str> =
            category.measures.iter().map(|m| m.name()).collect();
        let mut parts = vec![
            format!("{:<20} {:<22}", category.name, category.label),
            format!("by {}", measures.join(", ")),
        ];
        if !designs.is_empty() {
            parts.push(format!("designs: {}", designs.join(", ")));
        }
        if !flags.is_empty() {
            parts.push(format!("flags: {}", flags.join(", ")));
        }
        if !category.aliases.is_empty() {
            parts.push(format!("aliases: {}", category.aliases.join(", ")));
        }
        println!("{}", parts.join(" ").trim_end());
    }
    Ok(())
}

/// Reads every stored file again as an upload's is prepared, and prints
/// what changed; see [`refile`].
async fn reread(db: &SqlitePool) -> Result<()> {
    for s in schematic::all(db).await? {
        let stored = version::current_file(db, &s.id)
            .await?
            .context("schematic vanished")?;
        refile(db, &s, &stored, None).await?;
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
    standardize: Option<Gray>,
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
    Ok(())
}

/// Puts `args.file` in place of build `args.id`'s current file, as a fix
/// to the file rather than a new version; see [`refile`].
async fn replace(db: &SqlitePool, args: &Replace) -> Result<()> {
    let id = resolve(db, &args.id).await?;
    let s = load(db, &id).await?;
    let file = fs::read(&args.file)?;
    refile(db, &s, &file, args.standardizing.asked()).await
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
            let name = tag::name(t);
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

/// Lints a file, or a stored schematic as a build of its category when no
/// such file exists. A file's category is unknown, so a lone unsigned
/// control or status line passes.
/// Prints what `inspect` finds of one file or build, or of every build on
/// the list, with a count of each problem and of the builds it stops.
async fn inspect_command(
    db: &SqlitePool,
    file_or_id: Option<&str>,
) -> Result<()> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    match file_or_id {
        Some(path) if fs::exists(path)? => {
            files.push((path.to_owned(), fs::read(path)?));
        }
        Some(prefix) => {
            let id = resolve(db, prefix).await?;
            let s = load(db, &id).await?;
            let data = version::current_file(db, &id)
                .await?
                .context("schematic vanished")?;
            files.push((format!("{}  {}", &id[..8], s.filename()), data));
        }
        None => {
            let listed: Vec<String> = catalogue::categories(db)
                .await?
                .into_iter()
                .map(|c| c.name)
                .collect();
            for s in schematic::all(db).await? {
                if !listed.contains(&s.category) {
                    continue;
                }
                let data = version::current_file(db, &s.id)
                    .await?
                    .context("schematic vanished")?;
                files.push((format!("{}  {}", &s.id[..8], s.filename()), data));
            }
        }
    }
    let one = files.len() == 1;
    let mut problems: BTreeMap<&str, usize> = BTreeMap::new();
    let mut stopped: BTreeMap<&str, usize> = BTreeMap::new();
    let mut clean = 0;
    for (name, data) in &files {
        let report = app::inspect::inspect_file(data)?;
        let buses = |runs: &[app::inspect::Run]| {
            runs.iter()
                .map(|run| run.bits.len().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "{name}: {} errors; inputs {}; outputs {}",
            report.errors.len(),
            buses(&report.inputs),
            buses(&report.outputs)
        );
        if one {
            for fix in &report.fixes {
                println!("  fix: {fix}");
            }
            for note in &report.notes {
                println!("  note: {note}");
            }
        }
        let mut rules = BTreeSet::new();
        for problem in &report.errors {
            println!("  {}", problem.message);
            *problems.entry(problem.rule).or_default() += 1;
            rules.insert(problem.rule);
        }
        for rule in &rules {
            *stopped.entry(rule).or_default() += 1;
        }
        clean += usize::from(report.errors.is_empty());
    }
    if !one {
        println!("\n{clean} of {} builds have no errors", files.len());
        for (rule, builds) in &stopped {
            println!("{builds:4} builds  {:5} times  {rule}", problems[rule]);
        }
    }
    Ok(())
}

async fn lint_command(db: &SqlitePool, file_or_id: &str) -> Result<()> {
    let (data, category) = if fs::exists(file_or_id)? {
        (fs::read(file_or_id)?, String::new())
    } else {
        let id = resolve(db, file_or_id).await?;
        let data = version::current_file(db, &id)
            .await?
            .context("schematic vanished")?;
        (data, load(db, &id).await?.category)
    };
    let prepared = import::prepare(&data, None, true, true, false)?;
    let violations = Analysis::of(&prepared.blocks, &category, true).violations;
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
    standardizing: &Standardizing,
) -> Result<()> {
    let id = resolve(db, id).await?;
    let s = load(db, &id).await?;
    let checked =
        app::version::check(db, &s, &fs::read(file)?, standardizing.asked())
            .await?;
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
    let number = version::replace(db, &id, &checked.prepared, how).await?;
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

/// Prints the results of the builds `ids` name, as JSON.
async fn export_results(db: &SqlitePool, ids: &[String]) -> Result<()> {
    let mut full = Vec::new();
    for id in ids {
        full.push(resolve(db, id).await?);
    }
    let results = app::models::results::export(db, &full).await?;
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}

/// Takes the results in `file` that still fit their builds, and says which
/// do not.
async fn import_results(db: &SqlitePool, file: &Path) -> Result<()> {
    use app::models::results::{self, Taken};
    let sent: Vec<results::Results> = serde_json::from_slice(&fs::read(file)?)?;
    let taken = results::import(db, &sent).await?;
    let mut stale = 0;
    for (id, outcome) in &taken {
        if let Taken::Stale(why) = outcome {
            println!("{}  not taken: {why}", &id[..8]);
            stale += 1;
        }
    }
    println!("took {} of {}", taken.len() - stale, taken.len());
    Ok(())
}

/// Evaluates build `id`, or every build of `category`, from its file, and
/// prints each field where that differs from what is stored, then how
/// many builds differ in each.
async fn evaluate(
    db: &SqlitePool,
    id: Option<&str>,
    category: &str,
) -> Result<()> {
    let builds: Vec<schematic::Schematic> = match id {
        Some(prefix) => vec![load(db, &resolve(db, prefix).await?).await?],
        None => schematic::all(db)
            .await?
            .into_iter()
            .filter(|s| s.category == category)
            .collect(),
    };
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &builds {
        let name = format!("{}  {}", &s.id[..8], s.filename());
        let file = version::current_file(db, &s.id)
            .await?
            .context("schematic vanished")?;
        // Its test and timing spread over every core already.
        let e = match app::evaluate::evaluate(&file) {
            Ok(e) => e,
            Err(err) => {
                println!("{name}: could not evaluate: {err}");
                *tally.entry("could not evaluate").or_default() += 1;
                continue;
            }
        };
        let show = |v: &dyn std::fmt::Debug| format!("{v:?}");
        let mut stored_flags: Vec<String> =
            s.flags.iter().map(|f| f.name.clone()).collect();
        stored_flags.sort();
        let mut flags = e.flags.clone();
        flags.sort();
        let design = e.design.as_ref().map_or("other", |d| d.name);
        let fields: [(&str, String, String); 12] = [
            (
                "category",
                s.category.clone(),
                e.category.unwrap_or("none").to_owned(),
            ),
            ("passes", "true".to_owned(), e.passes().to_string()),
            ("width", show(&s.width), show(&e.width)),
            (
                "orientation",
                show(&Some(s.orientation)),
                show(&e.orientation),
            ),
            ("encoding", show(&s.encoding), show(&e.encoding)),
            ("flags", show(&stored_flags), show(&flags)),
            ("stacking", show(&s.stacking), show(&e.stacking)),
            ("latency", show(&s.latency), show(&e.latency)),
            ("synchronized", show(&s.synchronized), show(&e.synchronized)),
            ("throughput", show(&s.throughput), show(&e.throughput)),
            ("design", s.design.clone(), design.to_owned()),
            ("logic", show(&s.logic), show(&e.logic)),
        ];
        let differ: Vec<String> = fields
            .iter()
            .filter(|(_, stored, derived)| stored != derived)
            .map(|(field, stored, derived)| {
                *tally.entry(field).or_default() += 1;
                format!("{field} {stored} -> {derived}")
            })
            .collect();
        if !differ.is_empty() {
            let signature = e.design.as_ref().map_or(String::new(), |d| {
                format!(" [{} {} terms]", d.signature, d.products)
            });
            println!("{name}{signature}: {}", differ.join("; "));
        }
    }
    println!("{} builds evaluated", builds.len());
    for (field, n) in tally {
        println!("  {n} differ in {field}");
    }
    Ok(())
}

/// Takes the unused blocks out of build `id`, or of every build with a
/// test, as `rsdb prune` says.
async fn prune(db: &SqlitePool, id: Option<&str>, dry_run: bool) -> Result<()> {
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
        let buses = app::stack::Buses::of(&s.category, s.orientation);
        let pruned = match prune_file(&data, &s.stacking, buses) {
            Ok(Some(pruned)) => pruned,
            Ok(None) => continue,
            Err(err) => {
                println!("skip    {name}: {err}");
                continue;
            }
        };
        let checked = app::version::check(db, &s, &pruned.file, None).await?;
        if let Some(refusal) = checked.refusals.first() {
            println!("skip    {name}: {refusal}");
            continue;
        }
        builds += 1;
        blocks += pruned.gone.len();
        let verb = if dry_run { "would" } else { "pruned" };
        let n = pruned.gone.len();
        println!("{verb} {name}: {n} blocks: {}", pruned.gone.join(" "));
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
    }
    let verb = if dry_run { "would take" } else { "took" };
    println!("{verb} {blocks} blocks out of {builds} builds");
    Ok(())
}

/// A build's file with its unused blocks taken out, and where they were.
struct Pruned {
    file: Vec<u8>,
    gone: Vec<String>,
}

/// The build in `data`, stacking each of `ways`, without its unused
/// blocks, once compiling it before and after shows every link as it was;
/// `None` when it has none.
fn prune_file(
    data: &[u8],
    ways: &[StackDirection],
    buses: app::stack::Buses,
) -> Result<Option<Pruned>> {
    let gone: BTreeMap<_, String> = app::roles::roles(data, ways, buses)?
        .into_iter()
        .filter(|(_, role)| role.need == app::roles::Need::Unused)
        .map(|(pos, _)| (pos, app::schem::AIR.to_owned()))
        .collect();
    if gone.is_empty() {
        return Ok(None);
    }
    let file = standardize::edit(data, &gone)?;
    let changed = app::roles::changed(data, &file, ways, buses)?;
    if let Some(first) = changed.first() {
        bail!(
            "{} parts of its circuit would change, such as {first}",
            changed.len()
        );
    }
    let gone = gone
        .keys()
        .map(|[x, y, z]| format!("{x},{y},{z}"))
        .collect();
    Ok(Some(Pruned { file, gone }))
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
            for (i, cell) in run.cells.iter().enumerate() {
                let at = |[x, y, z]: [i32; 3]| format!("({x}, {y}, {z})");
                let read = cell.read.map(|pos| format!(", read {}", at(pos)));
                println!(
                    "        cell {}: write {}{}",
                    i + 1,
                    at(cell.write),
                    read.unwrap_or_default()
                );
            }
            let width = run.inputs.first().map_or(1, |signal| signal.bits);
            for steps in &run.faults {
                let steps: Vec<String> = steps
                    .iter()
                    .map(|step| match step {
                        Action::Write { cell, value } => {
                            format!("w{} {value:0width$b}", cell + 1)
                        }
                        Action::Read {
                            cell,
                            expected,
                            got,
                            ..
                        } if got == expected => {
                            format!("r{} {got:0width$b}", cell + 1)
                        }
                        Action::Read {
                            cell,
                            expected,
                            got,
                            ..
                        } => format!(
                            "r{} got {got:0width$b} expected {expected:0width$b}",
                            cell + 1
                        ),
                    })
                    .collect();
                println!("        fault: {}", steps.join("; "));
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
                let op =
                    w.op.as_deref()
                        .map(|op| format!("{op}: "))
                        .unwrap_or_default();
                println!(
                    "        {op}{}: got {}, expected {}",
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
        // A build with only input A, as a monostable, has no control
        // lines for its copies to share.
        print_stacking(stacking, run.inputs.len() > 1);
    }
    if let Some(timing) = report.timing() {
        print_timing(timing);
    }
}

/// What stacking copies of a simulated build found, whose copies share
/// any `controls` lines.
fn print_stacking(stacking: &Stacking, controls: bool) {
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
        Stacking::Registers {
            step,
            copies,
            registers,
            cases,
            most,
        } => println!(
            "        stacks: {copies} copies {} apart make {registers} registers, all {cases} cases right{}",
            step.describe(),
            if most.is_some() {
                "; one more copy fails"
            } else {
                ""
            }
        ),
        Stacking::Wider {
            step,
            copies,
            bits,
            cases,
            most,
        } => println!(
            "        stacks: {copies} copies {} apart make {bits} bits{}, all {cases} cases right{}",
            step.describe(),
            if controls {
                " sharing their control lines"
            } else {
                ""
            },
            if most.is_some() {
                "; one more copy fails"
            } else {
                ""
            }
        ),
        Stacking::Separate { ways } => {
            for way in ways {
                println!(
                    "        stacks: {} copies {} apart each work on their own, all {} cases right{}",
                    way.copies,
                    way.step.describe(),
                    way.cases,
                    if way.most.is_some() {
                        "; one more copy fails"
                    } else {
                        ""
                    }
                );
            }
        }
        Stacking::Not { step, reason } => println!(
            "        doesn't stack{}: {reason}",
            step.map(|s| format!(" {} apart", s.describe()))
                .unwrap_or_default()
        ),
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
    if let Some(pulse) = timing.pulse {
        println!(
            "        pulse: {} ticks, {}, as {}",
            pulse.ticks,
            if pulse.inverted {
                "low, inverted"
            } else {
                "high"
            },
            pulse.edge.words()
        );
    }
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
    tag::find(db, name).await?.with_context(|| {
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
    let tags: Vec<String> = s.tags.iter().map(tag::Tag::label).collect();
    println!("id           {}", s.id);
    println!("title        {}", s.title());
    println!("filename     {}", s.filename());
    println!("category     {}", s.category);
    println!("width        {}", unknown(s.width_label()));
    println!("orientation  {}", s.orientation.label());
    println!(
        "encoding     {}",
        s.encoding.map_or("none", Encoding::label)
    );
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
    if let Some(k) = s.at_least {
        println!("at least     {k} on");
    }
    if let Some(p) = s.pulse {
        println!("pulse        {p} ticks");
    }
    println!("synchronized {}", s.synchronized_label());
    println!(
        "state        {}{}",
        s.logic.map_or("unknown", Logic::name),
        if s.pipelined() { ", pipelined" } else { "" }
    );
    let from = s.works_from.as_deref().unwrap_or("unknown");
    let by = s.works_from_by.as_deref().map(|by| format!(" ({by})"));
    println!("works from   {from}{}", by.unwrap_or_default());
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
    println!("size         {y}x{x}x{z} (HWD), {} blocks", s.box_volume());
    if let Some((x, y, z)) = s.per_copy() {
        println!("per copy     {y}x{x}x{z} (HWD), {} blocks", x * y * z);
    }
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
