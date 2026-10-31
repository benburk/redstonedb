//! A build's details as people give them, held to the site's rules in one
//! place whichever way they arrive: the upload page, the edit page, or
//! `rsdb set`.

use sqlx::SqlitePool;

use crate::{
    category,
    import::Prepared,
    models::{
        catalogue::{self, Category, Measure},
        schematic::{
            self, Encoding, Logic, Orientation, Schematic, StackDirection,
        },
        simulation, tag,
    },
    mojang,
    simulate::{Report, Test},
    stack,
};

/// A build's details, typed. As given they may break a rule, such as naming
/// the category by its label, giving a capacity to a category not ranked
/// by one, or crediting a name no player has; [`check`] holds them to every
/// rule.
#[derive(Debug, Clone, Default)]
pub struct Details {
    /// A standard category by its name or label, in any capitals, or a name
    /// of the builder's own.
    pub category: String,
    pub width: Option<i64>,
    pub orientation: Orientation,
    pub encoding: Option<Encoding>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    /// A date such as `2024-02-02`.
    pub built_on: Option<String>,
    pub description: Option<String>,
    /// The build's own name, kept only for a category that names each
    /// build.
    pub given_name: Option<String>,
    /// `None` follows the category.
    pub logic: Option<Logic>,
    /// The fewest ticks from one input to the next.
    pub throughput: Option<i64>,
    /// How many bytes a memory holds.
    pub bytes: Option<i64>,
    /// A threshold's k.
    pub at_least: Option<i64>,
    /// A monostable's pulse, in ticks.
    pub pulse: Option<i64>,
    /// A design of the category; `None` for `other`.
    pub design: Option<String>,
    /// Flags it has; any not of its category are dropped.
    pub flags: Vec<String>,
    pub builders: Vec<String>,
    pub contributors: Vec<String>,
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
    /// Tag names, which may be written with spaces and capitals.
    pub tags: Vec<String>,
}

impl Details {
    /// Build `s`'s details as stored, to change some and check them again.
    pub fn of(s: &Schematic) -> Self {
        Details {
            category: s.category.clone(),
            width: s.width,
            orientation: s.orientation,
            encoding: s.encoding,
            latency: s.latency,
            synchronized: s.synchronized,
            built_on: s.built_on.clone(),
            description: s.description.clone(),
            given_name: s.given_name.clone(),
            logic: s.own_logic,
            throughput: s.throughput,
            bytes: s.bytes,
            at_least: s.at_least,
            pulse: s.pulse,
            design: Some(s.design.clone()),
            flags: s.flags.iter().map(|flag| flag.name.clone()).collect(),
            builders: s.builders.clone(),
            contributors: s.contributors.clone(),
            stacking: s.stacking.clone(),
            tags: s
                .tags
                .iter()
                .filter(|tag| !tag.is_derived())
                .map(|tag| tag.name.clone())
                .collect(),
        }
    }
}

/// The file the details describe, as far as the rules ask about it.
#[derive(Clone, Copy)]
pub struct Build<'a> {
    /// What the stacking must fit.
    pub data: &'a [u8],
    /// Whether it follows the conventions, which lets the simulation
    /// decide how a build of some categories stacks.
    pub conventional: bool,
    /// Whether it is a stored file whose test, as the category it is
    /// saved under, has passed already, so needs no running again.
    pub passes: bool,
}

impl<'a> Build<'a> {
    /// A file as `import::prepare` made it.
    pub fn of(prepared: &'a Prepared) -> Self {
        Build {
            data: &prepared.data,
            conventional: prepared.conventional,
            passes: false,
        }
    }
}

/// Details that keep every rule, ready to store.
pub struct Ready {
    /// The category, as the list has it, or one of the builder's own.
    pub category: Category,
    /// The category by its name, the fields it has no use for dropped,
    /// flags not of it dropped, the names respelled as their players spell
    /// them, and tags in the form [`tag::name`] gives.
    pub details: Details,
    /// Whether the simulation decides how it stacks, so the stacking given
    /// counts for nothing and whatever the simulation found stays.
    pub stacking_inferred: bool,
    /// Whether the simulation works out its every measured detail, as for
    /// a conventional adder, so an edit leaves them as it set them.
    pub evaluated: bool,
}

/// Holds `details` of `build` to every rule, and returns them ready to
/// store, or a sentence for the person saying what to change. Only when
/// `may_create_tags`, as for an admin, may a tag not yet on the list be
/// named, to be made under `other`.
///
/// A category off the list, such as a piston door, has no width, encoding,
/// or throughput; only a category that names each build keeps a given name;
/// only one ranked by a measure, as a memory is by its capacity, keeps it.
pub async fn check(
    db: &SqlitePool,
    details: Details,
    build: Build<'_>,
    may_create_tags: bool,
) -> Result<Result<Ready, String>, sqlx::Error> {
    match ready(db, details, build, may_create_tags).await {
        Ok(ready) => Ok(Ok(ready)),
        Err(Stop::Refused(message)) => Ok(Err(message)),
        Err(Stop::Failed(err)) => Err(err),
    }
}

/// Why checking stopped short: a sentence for the person, or the database
/// failing.
enum Stop {
    Refused(String),
    Failed(sqlx::Error),
}

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop::Refused(message)
    }
}

impl From<sqlx::Error> for Stop {
    fn from(err: sqlx::Error) -> Self {
        Stop::Failed(err)
    }
}

/// [`check`], with `?` for both ways it stops.
async fn ready(
    db: &SqlitePool,
    mut details: Details,
    build: Build<'_>,
    may_create_tags: bool,
) -> Result<Ready, Stop> {
    for text in [
        &mut details.built_on,
        &mut details.description,
        &mut details.given_name,
        &mut details.design,
    ] {
        *text = text
            .take()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
    }
    details.tags = details.tags.iter().map(|name| tag::name(name)).collect();
    shaped(&details)?;
    tag::check_new(db, &details.tags, may_create_tags).await??;

    let categories = catalogue::categories(db).await?;
    let listed = Category::find(&categories, &details.category).is_some();
    details.category = Category::name_of(&categories, &details.category);
    if details.category.is_empty() {
        return Err("Choose a category, or type a name of your own."
            .to_owned()
            .into());
    }
    let design = details.design.take().unwrap_or_else(|| "other".to_owned());
    let designs = catalogue::designs(db).await?;
    let category = catalogue::check_category(
        &categories,
        &designs,
        &details.category,
        &design,
    )?;
    details.design = Some(design);
    // Only a blessed build, one probing recognizes, goes on the list.
    if listed && !category::BLESSED.contains(&category.name.as_str()) {
        let width = simulation::derived_width(&category.name, true, build.data)
            .ok()
            .flatten();
        return Err(format!(
            "Only adders, registers, ALUs, decoders, multipliers, counters, and shift registers go on the list so far: \
             give it a category of its own, such as {}.",
            off_list_name(&category, width).replace('_', " ")
        )
        .into());
    }

    let positive = |n: Option<i64>| n.filter(|n| *n > 0);
    details.width = positive(details.width).filter(|_| listed);
    details.throughput = positive(details.throughput).filter(|_| listed);
    details.encoding = details.encoding.filter(|_| listed);
    details.given_name = details.given_name.filter(|_| category.named);
    details.bytes =
        positive(details.bytes).filter(|_| category.has(Measure::Bytes));
    details.at_least =
        positive(details.at_least).filter(|_| category.has(Measure::AtLeast));
    details.pulse =
        positive(details.pulse).filter(|_| category.has(Measure::Pulse));
    details.flags =
        catalogue::keep_flags(db, &category.name, &details.flags).await?;

    evaluated(&category, &mut details, build).await?;
    let policy = category::behaviour(&category.name).stacking;
    if policy == category::Stacking::Never {
        details.stacking.clear();
    }
    let stacking_inferred =
        build.conventional && policy == category::Stacking::Inferred;
    if !stacking_inferred && policy != category::Stacking::Never {
        let buses = stack::Buses::of(&category.name, details.orientation);
        stack::check(build.data, &details.stacking, buses)?;
    }

    // Asked last, as it goes to Mojang, which is slow.
    details.builders = mojang::respell(&details.builders).await?;
    details.contributors = mojang::respell(&details.contributors).await?;
    let evaluated =
        build.conventional && category::behaviour(&category.name).evaluated;
    Ok(Ready {
        category,
        details,
        stacking_inferred,
        evaluated,
    })
}

/// For a build of a category whose measured details all come from
/// `evaluate`, drops those typed, as the simulation sets them once it is
/// stored, and refuses the build unless it is recognized, as
/// [`unrecognized`] says.
async fn evaluated(
    category: &Category,
    details: &mut Details,
    build: Build<'_>,
) -> Result<(), Stop> {
    if !build.passes
        && let Some(off) = unrecognized(category, build).await?
    {
        return Err(Stop::Refused(format!(
            "{}, so it can't go up as one: give it a category of its own, \
             such as {}, or fix it first.",
            off.why, off.name
        )));
    }
    if !category::behaviour(&category.name).evaluated {
        return Ok(());
    }
    // Nothing measured is typed: what the file shows, its width, encoding,
    // and the way its bits run, is read from it, and the rest the
    // simulation sets once it is stored.
    details.latency = None;
    details.synchronized = None;
    details.throughput = None;
    details.logic = None;
    details.pulse = None;
    details.design = Some("other".to_owned());
    details.flags.clear();
    let read = crate::schem::Blocks::read(build.data)
        .map(|blocks| {
            crate::import::derived(
                &category.name,
                &crate::interface::read(&blocks),
            )
        })
        .unwrap_or_default();
    // A lone bit shows no way its bits run, so it is horizontal until its
    // copies, if they stack, show otherwise.
    details.orientation = read.orientation.unwrap_or(Orientation::Horizontal);
    details.encoding = read.encoding;
    details.width = simulation::derived_width(&category.name, true, build.data)
        .ok()
        .flatten();
    Ok(())
}

/// The listed category `name` means: one of `categories` as typed, or the
/// one an off-list name such as `8-bit adder` stands in for, as
/// `simulation::meant_by` reads it.
pub fn meant<'a>(
    categories: &'a [Category],
    name: &str,
) -> Option<&'a Category> {
    Category::find(categories, name).or_else(|| {
        let name = Category::name_of(categories, name);
        let meant = simulation::meant_by(&name)?;
        Category::find(categories, meant)
    })
}

/// Why a build is not recognized as one of `category`, and the category
/// off the list it goes up under instead.
pub struct Unrecognized {
    /// A sentence without its full stop, such as `It does not pass the
    /// adder test`.
    pub why: String,
    /// Named for what it seems meant to be, such as `8-bit adder`.
    pub name: String,
}

/// The category off the list a build not recognized as one of `category`
/// goes up under, named for what it seems meant to be, such as `8-bit
/// adder`: from the category's name rather than its label, which may hold
/// characters no name may, as `XOR/XNOR Gate` does, so
/// `simulation::meant_by` reads it back.
pub fn off_list_name(category: &Category, width: Option<i64>) -> String {
    let words = category.name.replace('_', " ");
    width.map_or_else(
        || format!("unrecognized {words}"),
        |w| format!("{w}-bit {words}"),
    )
}

/// Why `build` is not recognized as a build of `category`, a blessed one,
/// see `category::BLESSED`: it must be standardized, take no
/// signal-strength input, be of a width its test reads, and pass it.
/// `None` when it is, or when the category is not blessed. The `Err` says
/// it could not be simulated at all.
pub async fn unrecognized(
    category: &Category,
    build: Build<'_>,
) -> Result<Option<Unrecognized>, String> {
    if !category::BLESSED.contains(&category.name.as_str()) {
        return Ok(None);
    }
    let width = simulation::derived_width(&category.name, true, build.data)
        .ok()
        .flatten();
    let label = category.label.to_lowercase();
    let off = |why: String| {
        let name = off_list_name(category, width);
        Ok(Some(Unrecognized { why, name }))
    };
    if !build.conventional {
        return off(format!(
            "The {label} test can't read its inputs and outputs"
        ));
    }
    let Ok(blocks) = crate::schem::Blocks::read(build.data) else {
        return off(format!("The {label} test can't read its file"));
    };
    // Until the simulator has a good way to drive one, a strength is no
    // input a recognized build may take.
    if !crate::interface::dust_inputs(&blocks).is_empty() {
        return off(format!(
            "The {label} test takes no signal-strength inputs"
        ));
    }
    let Some(test) = Test::of(&category.name, width) else {
        return off(match width {
            None => format!(
                "The {label} test can't read its width from its inputs and \
                 outputs"
            ),
            Some(w) => format!("The {label} test does not run at {w} bits"),
        });
    };
    let report = simulation::run(test, build.data.to_vec())
        .await
        .map_err(|err| format!("It could not be simulated: {err}."))?;
    match report {
        _ if report.passed() => Ok(None),
        // A block the simulator does not model, as a piston.
        Report::Skipped { reason } => off(format!(
            "{} The {label} test can't run on it",
            reason.trim()
        )),
        _ => off(format!("It does not pass the {label} test")),
    }
}

/// Refuses details whose fields are not of the shape they must be, each
/// with a sentence saying which.
fn shaped(details: &Details) -> Result<(), String> {
    if let Some(name) = details
        .builders
        .iter()
        .chain(&details.contributors)
        .find(|n| !schematic::valid_player_name(n))
    {
        return Err(format!(
            "{name} is not a Minecraft name: letters, digits, and \
             underscores, up to 16."
        ));
    }
    if details
        .given_name
        .as_deref()
        .is_some_and(|name| name.chars().count() > 60)
    {
        return Err("A name is at most 60 characters.".to_owned());
    }
    if let Some(date) = details.built_on.as_deref() {
        if !is_date(date) {
            return Err(
                "Built on must be a date such as 2024-02-02.".to_owned()
            );
        }
        if *date > *latest_build_date().to_string() {
            return Err("Built on is a date still to come.".to_owned());
        }
    }
    if let Some(name) = details
        .tags
        .iter()
        .find(|n| tag::DERIVED.contains(&n.as_str()))
    {
        return Err(format!(
            "The {} tag is worked out from the build; it cannot be added by \
             hand.",
            name.replace('_', " ")
        ));
    }
    if let Some(name) = details.tags.iter().find(|n| !tag::valid_name(n)) {
        return Err(format!(
            "{} cannot be a tag: use letters, digits, spaces, and dashes, \
             with no dash at either end.",
            name.replace('_', " ")
        ));
    }
    Ok(())
}

/// The latest date a build can have been built on: tomorrow, as the server
/// counts days, since it is already tomorrow in some time zones.
pub fn latest_build_date() -> time::Date {
    let today = time::OffsetDateTime::now_utc().date();
    today.next_day().unwrap_or(today)
}

/// Whether `d` is a real day written as `yyyy-mm-dd`, such as `2024-02-29`.
fn is_date(d: &str) -> bool {
    let shaped = d.len() == 10
        && d.bytes().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    // Being all ASCII, it slices anywhere.
    shaped
        && match (d[..4].parse(), d[5..7].parse::<u8>(), d[8..].parse()) {
            (Ok(year), Ok(month), Ok(day)) => time::Month::try_from(month)
                .and_then(|month| {
                    time::Date::from_calendar_date(year, month, day)
                })
                .is_ok(),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_a_real_day_as_yyyy_mm_dd() {
        assert!(is_date("2024-02-29"));
        assert!(!is_date("2023-02-29"));
        assert!(!is_date("2024-13-01"));
        assert!(!is_date("02/29/2024"));
        assert!(!is_date("2024-2-29"));
        assert!(!is_date("2024-02-2é"));
    }
}
