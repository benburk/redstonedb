//! A build's details as people give them, held to the site's rules in one
//! place whichever way they arrive: the upload page, the edit page, or
//! `rsdb set`.

use crate::{
    build::{Details, Prepared, Ready, tag, valid_player_name},
    category::{self, Category, Kind, Measure, Test},
    external::mojang,
    ingest::{jobs, results},
    interface::Orientation,
    players,
    recognize::fault::{
        ADDS, INVERTS_SUM, KEEPS_SUM, NO_HEX_INPUTS, SUBTRACTS, WITHOUT_LEVERS,
        WRONG_OUTPUTS,
    },
    simulate::Report,
    stack::{self},
    store::tags,
};
use sqlx::SqlitePool;

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
    /// A file as `prepare::prepare_for` made it.
    pub fn of(prepared: &'a Prepared) -> Self {
        Candidate {
            data: &prepared.data,
            conventional: prepared.conventional,
            passes: false,
        }
    }
}

/// Holds `details` of `build` to every rule, and returns them ready to
/// store, or a sentence for the person saying what to change. Only when
/// `may_create_tags`, as for a mod or admin, may a tag not yet on the list be
/// named, to be made under `other`.
///
/// A category off the list, such as a piston door, has no width, encoding,
/// or throughput; only one ranked by a measure, as a threshold is by the
/// fewest inputs on, keeps it.
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
/// width, throughput, or encoding where it isn't `listed`, a measure it
/// isn't ranked by, and flags not of it. A number kept is above zero.
fn keep_what_applies(details: &mut Details, category: &Category, listed: bool) {
    let positive = |n: Option<i64>| n.filter(|n| *n > 0);
    let specs = &mut details.specs;
    specs.width = positive(specs.width).filter(|_| listed);
    specs.throughput = positive(specs.throughput).filter(|_| listed);
    specs.encoding = specs.encoding.filter(|_| listed);
    specs.at_least =
        positive(specs.at_least).filter(|_| category.has(Measure::AtLeast));
    specs.pulse =
        positive(specs.pulse).filter(|_| category.has(Measure::Pulse));
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
        &mut details.specs.design,
    ] {
        *text = text
            .take()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
    }
    details.tags = details.tags.iter().map(|name| tag::name(name)).collect();
    shaped(&details)?;
    tags::check_new(db, &details.tags, may_create_tags).await??;

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
    keep_what_applies(&mut details, &category, listed);

    // Only a build of a category the site recognizes builds of is
    // unverified as one.
    if !listed {
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
            category::behaviour(&category.name).copies,
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
        .map(crate::build::mentions)
        .unwrap_or_default();
    let asked: Vec<String> =
        details.credits.everyone().chain(&named).cloned().collect();
    let players = players::look_up(db, &asked).await?;
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
                results::derived_width(&category.name, true, build.data)
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
            crate::ingest::read::derived(
                &category.name,
                &crate::interface::read(&blocks),
            )
        })
        .unwrap_or_default();
    // A lone bit shows no way its bits run, so it is horizontal until its
    // copies, if they stack, show otherwise.
    details.specs.orientation =
        read.orientation.unwrap_or(Orientation::Horizontal);
    details.specs.encoding = read.encoding;
    details.specs.width =
        results::derived_width(&category.name, true, build.data)
            .ok()
            .flatten();
    Ok(())
}

/// Why `build` is not recognized as a build of `category`, a standard
/// one: it must be standardized, take no
/// signal-strength input, be of a width its test reads, and pass it.
/// `None` when it is, or when the category is one of its author's own;
/// else a short
/// sentence about the build, without its full stop, such as `Couldn't
/// recognize inputs without levers`. The `Err` says it could not be
/// simulated at all.
pub async fn unrecognized(
    category: &Category,
    build: Candidate<'_>,
) -> Result<Option<String>, String> {
    let Some(kind) = Kind::named(&category.name) else {
        return Ok(None);
    };
    let width = results::derived_width(&category.name, true, build.data)
        .ok()
        .flatten();
    let off = |why: String| Ok(Some(why));
    let Ok(blocks) = crate::schem::Blocks::read(build.data) else {
        return off("Couldn't read its file".to_owned());
    };
    // Inputs taking a strength, a hex digit per block, are only those of a
    // kind that takes digits, a hex adder's or a hex-to-binary converter's,
    // as a build kept as built says too.
    let hex = crate::interface::takes_digits(&blocks);
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
    let report = jobs::run(test, build.data.to_vec())
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

/// Refuses details whose fields are not of the shape they must be, each
/// with a sentence saying which.
fn shaped(details: &Details) -> Result<(), String> {
    if let Some(name) =
        details.credits.everyone().find(|n| !valid_player_name(n))
    {
        return Err(format!(
            "{name} is not a Minecraft name: letters, digits, and \
             underscores, up to 16."
        ));
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
