//! A build's details as people give them, held to the site's rules in one
//! place whichever way they arrive: the upload page, the edit page, or
//! `rsdb set`.

use sqlx::SqlitePool;

use crate::{
    category::{self, Category, Kind, Measure},
    external::mojang,
    import::Prepared,
    models::{
        player,
        schematic::{self, Build, Credits, Orientation, Specs, StackDirection},
        simulation, tag,
    },
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
    /// of the author's own.
    pub category: String,
    /// Its specs as given; its design one of the category's, or `None`
    /// for `other`.
    pub specs: Specs,
    /// A date such as `2024-02-02`.
    pub built_on: Option<String>,
    pub description: Option<String>,
    /// The build's own name, kept only for a category that names each
    /// build.
    pub given_name: Option<String>,
    /// Flags it has; any not of its category are dropped.
    pub flags: Vec<String>,
    /// The names credited, each respelled as its player spells it.
    pub credits: Credits,
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
    /// Tag names, which may be written with spaces and capitals.
    pub tags: Vec<String>,
    /// Why it is not recognized as its category, one the site recognizes
    /// builds of, so it goes up unverified with the details given; see
    /// [`unrecognized`].
    pub unverified: Option<String>,
}

impl Details {
    /// Build `s`'s details as stored, to change some and check them again.
    pub fn of(s: &Build) -> Self {
        Details {
            category: s.category.clone(),
            specs: s.specs.clone(),
            built_on: s.built_on.clone(),
            description: s.description.clone(),
            given_name: s.given_name.clone(),
            flags: s.flags.iter().map(|flag| flag.name.to_owned()).collect(),
            credits: s.credits.clone(),
            stacking: s.stacking.clone(),
            tags: s
                .tags
                .iter()
                .filter(|tag| !tag.is_derived())
                .map(|tag| tag.name.clone())
                .collect(),
            unverified: s.unverified.clone(),
        }
    }
}

/// The file the details describe, as far as the rules ask about it.
#[derive(Clone, Copy)]
pub struct Candidate<'a> {
    /// What the stacking must fit.
    pub data: &'a [u8],
    /// Whether it follows the conventions, which lets the simulation
    /// decide how a build of some categories stacks.
    pub conventional: bool,
    /// Whether it is a stored file whose test, as the category it is
    /// saved under, has passed already, so needs no running again.
    pub passes: bool,
}

impl<'a> Candidate<'a> {
    /// A file as `import::prepare_for` made it.
    pub fn of(prepared: &'a Prepared) -> Self {
        Candidate {
            data: &prepared.data,
            conventional: prepared.conventional,
            passes: false,
        }
    }
}

/// Details that keep every rule, ready to store.
pub struct Ready {
    /// The category, as the list has it, or one of the author's own.
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
/// `may_create_tags`, as for a mod or admin, may a tag not yet on the list be
/// named, to be made under `other`.
///
/// A category off the list, such as a piston door, has no width, encoding,
/// or throughput; only a category that names each build keeps a given name;
/// only one ranked by a measure, as a memory is by its capacity, keeps it.
pub async fn check(
    db: &SqlitePool,
    details: Details,
    build: Candidate<'_>,
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
/// Drops from `details` what a build of `category` has no use for: a
/// width, throughput, or encoding where it isn't `listed`, a given name
/// where it names no build, a measure it isn't ranked by, and flags not
/// of it. A number kept is above zero.
fn keep_what_applies(details: &mut Details, category: &Category, listed: bool) {
    let positive = |n: Option<i64>| n.filter(|n| *n > 0);
    let specs = &mut details.specs;
    specs.width = positive(specs.width).filter(|_| listed);
    specs.throughput = positive(specs.throughput).filter(|_| listed);
    specs.encoding = specs.encoding.filter(|_| listed);
    specs.bytes =
        positive(specs.bytes).filter(|_| category.has(Measure::Bytes));
    specs.at_least =
        positive(specs.at_least).filter(|_| category.has(Measure::AtLeast));
    specs.pulse =
        positive(specs.pulse).filter(|_| category.has(Measure::Pulse));
    details.given_name = details.given_name.take().filter(|_| category.named);
    details.flags = category::flags_among(&category.name, &details.flags)
        .into_iter()
        .map(|flag| flag.name.to_owned())
        .collect();
}

async fn ready(
    db: &SqlitePool,
    mut details: Details,
    build: Candidate<'_>,
    may_create_tags: bool,
) -> Result<Ready, Stop> {
    for text in [
        &mut details.built_on,
        &mut details.description,
        &mut details.given_name,
        &mut details.specs.design,
    ] {
        *text = text
            .take()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
    }
    details.tags = details.tags.iter().map(|name| tag::name(name)).collect();
    shaped(&details)?;
    tag::check_new(db, &details.tags, may_create_tags).await??;

    let categories = Category::all();
    let listed = Category::find(&categories, &details.category).is_some();
    details.category = Category::name_of(&categories, &details.category);
    if details.category.is_empty() {
        return Err("Choose a category, or type a name of your own."
            .to_owned()
            .into());
    }
    let category = category::check_category(
        &details.category,
        details.specs.design.as_deref(),
    )?;
    // Only a blessed build, one probing recognizes, goes on the list.
    if listed && !category::blessed(&category.name) {
        let width = simulation::derived_width(&category.name, true, build.data)
            .ok()
            .flatten();
        return Err(format!(
            "Only {} go on the list so far: give it a category of its own, \
             such as {}.",
            Kind::listed(),
            off_list_name(&category, width).replace('_', " ")
        )
        .into());
    }

    keep_what_applies(&mut details, &category, listed);

    // Only a build of a category the site recognizes builds of is
    // unverified as one.
    if !category::blessed(&category.name) {
        details.unverified = None;
    }
    evaluated(&category, &mut details, build).await?;
    let policy = category::behaviour(&category.name).stacking;
    if policy == category::Stacking::Never {
        details.stacking.clear();
    }
    let verified = details.unverified.is_none();
    let stacking_inferred = build.conventional
        && verified
        && policy == category::Stacking::Inferred;
    if !stacking_inferred && policy != category::Stacking::Never {
        let buses = stack::Buses::of(
            Kind::named(&category.name),
            details.specs.orientation,
        );
        stack::check(build.data, &details.stacking, buses)?;
    }

    // Asked last, as it may go to Mojang, which is slow, once for every
    // name not known lately. A player the description names with `@name`
    // must be one, as an author must; the description keeps the names as
    // written.
    let named = details
        .description
        .as_deref()
        .map(crate::web::mentions)
        .unwrap_or_default();
    let asked: Vec<String> =
        details.credits.everyone().chain(&named).cloned().collect();
    let players = player::look_up(db, &asked).await?;
    details.credits.authors =
        mojang::respell(&details.credits.authors, &players)?;
    details.credits.contributors =
        mojang::respell(&details.credits.contributors, &players)?;
    if let Some(message) = unnamed(&mojang::unknown(&named, &players)) {
        return Err(message.into());
    }
    let evaluated = build.conventional
        && verified
        && category::behaviour(&category.name).evaluated;
    Ok(Ready {
        category,
        details,
        stacking_inferred,
        evaluated,
    })
}

/// A sentence naming the `@name`s in a description that no player has,
/// to be checked; `None` when there are none.
fn unnamed(unknown: &[String]) -> Option<String> {
    let named: Vec<String> = unknown.iter().map(|n| format!("@{n}")).collect();
    match named.as_slice() {
        [] => None,
        [one] => Some(format!(
            "The description names {one}, but no Minecraft player is named \
             that. Check the spelling."
        )),
        many => Some(format!(
            "The description names {}, but no Minecraft players are named \
             those. Check the spelling.",
            many.join(", ")
        )),
    }
}

/// For a build of a category whose measured details all come from
/// `evaluate`, drops those typed, as the simulation sets them once it is
/// stored; unless it is not recognized as one, as [`unrecognized`] says,
/// when it goes up unverified, with the details given and its width read
/// from its file when none is.
async fn evaluated(
    category: &Category,
    details: &mut Details,
    build: Candidate<'_>,
) -> Result<(), Stop> {
    if details.unverified.is_none()
        && !build.passes
        && let Some(why) = unrecognized(category, build).await?
    {
        details.unverified = Some(why);
    }
    if details.unverified.is_some() {
        if details.specs.width.is_none() {
            details.specs.width =
                simulation::derived_width(&category.name, true, build.data)
                    .ok()
                    .flatten();
        }
        return Ok(());
    }
    if !category::behaviour(&category.name).evaluated {
        return Ok(());
    }
    // Nothing measured is typed: what the file shows, its width, encoding,
    // and the way its bits run, is read from it, and the rest the
    // simulation sets once it is stored.
    details.specs.latency = None;
    details.specs.synchronized = None;
    details.specs.throughput = None;
    details.specs.logic = None;
    details.specs.pulse = None;
    details.specs.at_least = None;
    details.specs.design = None;
    details.flags.clear();
    let read = crate::schem::Blocks::read(build.data)
        .map(|blocks| {
            crate::import::derived(
                &category.name,
                &crate::recognize::interface::read(&blocks),
            )
        })
        .unwrap_or_default();
    // A lone bit shows no way its bits run, so it is horizontal until its
    // copies, if they stack, show otherwise.
    details.specs.orientation =
        read.orientation.unwrap_or(Orientation::Horizontal);
    details.specs.encoding = read.encoding;
    details.specs.width =
        simulation::derived_width(&category.name, true, build.data)
            .ok()
            .flatten();
    Ok(())
}

/// The category of its own a build of `category`, a listed one the site
/// recognizes no builds of yet, goes up under, named for what it is, such
/// as `8-bit ram`: from the category's name rather than its label, which
/// may hold characters no name may, as `NAND+NOR Gate` does.
pub fn off_list_name(category: &Category, width: Option<i64>) -> String {
    let words = category.name.replace('_', " ");
    width.map_or_else(
        || format!("unrecognized {words}"),
        |w| format!("{w}-bit {words}"),
    )
}

/// Why `build` is not recognized as a build of `category`, a blessed one,
/// see `category::Facts::listed_as`: it must be standardized, take no
/// signal-strength input, be of a width its test reads, and pass it.
/// `None` when it is, or when the category is not blessed; else a short
/// sentence about the build, without its full stop, such as `Couldn't
/// recognize inputs without levers`. The `Err` says it could not be
/// simulated at all.
pub async fn unrecognized(
    category: &Category,
    build: Candidate<'_>,
) -> Result<Option<String>, String> {
    let Some(kind) = Kind::named(&category.name).filter(|k| k.blessed()) else {
        return Ok(None);
    };
    let width = simulation::derived_width(&category.name, true, build.data)
        .ok()
        .flatten();
    let off = |why: String| Ok(Some(why));
    let Ok(blocks) = crate::schem::Blocks::read(build.data) else {
        return off("Couldn't read its file".to_owned());
    };
    // Inputs taking a strength, a hex digit per block, are only those of a
    // kind that takes digits, a hex adder's or a hex-to-binary converter's,
    // as a build kept as built says too.
    let hex = crate::recognize::interface::takes_digits(&blocks);
    if !kind.digit_inputs() && hex {
        return off(WITHOUT_LEVERS.to_owned());
    }
    if !build.conventional {
        return off("Couldn't read its inputs and outputs".to_owned());
    }
    if kind.digit_inputs() && !hex {
        return off(NO_HEX_INPUTS.to_owned());
    }
    let Some(test) = Test::of(&category.name, width) else {
        return off(match width {
            None => "Couldn't count its bits".to_owned(),
            Some(w) => format!(
                "Couldn't test {} of {w} bits yet",
                category.plural.to_lowercase()
            ),
        });
    };
    let report = simulation::run(test, build.data.to_vec())
        .await
        .map_err(|err| format!("It could not be simulated: {err}."))?;
    match &report {
        // An adder adds and a subtractor subtracts, each its own category.
        // An adder's sum reads as it is and an inverted adder's inverted.
        Report::Ran(run) if report.passed() => {
            let variant = run.adder_variant();
            match kind {
                Kind::Adder | Kind::HexAdder | Kind::Subtractor
                    if variant.sum =>
                {
                    off(INVERTS_SUM.to_owned())
                }
                Kind::InvertedAdder if !variant.sum => {
                    off(KEEPS_SUM.to_owned())
                }
                Kind::Adder | Kind::HexAdder if variant.subtract => {
                    off(SUBTRACTS.to_owned())
                }
                Kind::Subtractor if !variant.subtract => off(ADDS.to_owned()),
                _ => Ok(None),
            }
        }
        Report::Ran(run) => off(run.wrong_cases()),
        // A block the simulator does not model, as a piston.
        Report::Skipped { reason } => {
            off(reason.trim().trim_end_matches('.').to_owned())
        }
        Report::Broken { .. } => off(WRONG_OUTPUTS.to_owned()),
    }
}

/// Why a hex adder or hex-to-binary converter whose inputs are levers is
/// not verified as one.
pub const NO_HEX_INPUTS: &str = "Couldn't find inputs taking hex digits";

/// Why an adder whose B is subtracted is not verified as an adder.
pub const SUBTRACTS: &str = "It subtracts";

/// Why an adder that adds is not verified as a subtractor.
pub const ADDS: &str = "It adds";

/// Why an inverted adder is not verified as an adder.
pub const INVERTS_SUM: &str = "Its sum reads inverted, as an inverted adder's";

/// Why an adder is not verified as an inverted adder.
pub const KEEPS_SUM: &str = "Its sum reads as it is, as an adder's";

/// Why a build its test finds broken is not verified, and a build that got
/// some cases of its test wrong was, before each said which.
pub const WRONG_OUTPUTS: &str = "Gives wrong outputs for some inputs";

/// Why a build whose inputs take a signal strength, rather than a lever's
/// on or off, is not recognized.
pub const WITHOUT_LEVERS: &str = "Couldn't recognize inputs without levers";

/// Refuses details whose fields are not of the shape they must be, each
/// with a sentence saying which.
fn shaped(details: &Details) -> Result<(), String> {
    if let Some(name) = details
        .credits
        .everyone()
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
    fn the_listed_kinds_read_as_a_sentence() {
        assert!(Kind::listed().starts_with("adders, hex adders, "));
        assert!(Kind::listed().ends_with(", and T flip-flops"));
    }

    #[test]
    fn a_date_is_a_real_day_as_yyyy_mm_dd() {
        assert!(is_date("2024-02-29"));
        assert!(!is_date("2023-02-29"));
        assert!(!is_date("2024-13-01"));
        assert!(!is_date("02/29/2024"));
        assert!(!is_date("2024-2-29"));
        assert!(!is_date("2024-02-2é"));
    }

    #[test]
    fn names_a_description_gives_no_player_are_named_back() {
        assert_eq!(unnamed(&[]), None);
        assert_eq!(
            unnamed(&["Nobody_Here".to_owned()]).as_deref(),
            Some(
                "The description names @Nobody_Here, but no Minecraft player \
                 is named that. Check the spelling."
            )
        );
        assert_eq!(
            unnamed(&["abc_x".to_owned(), "abc_y".to_owned()]).as_deref(),
            Some(
                "The description names @abc_x, @abc_y, but no Minecraft \
                 players are named those. Check the spelling."
            )
        );
    }
}
