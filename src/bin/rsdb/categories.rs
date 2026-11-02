//! The standard categories, with their measures, designs, and flags, and
//! the tags builds carry, as `rsdb` lists and adds them.

use anyhow::{Result, bail};
use app::models::{
    catalogue::{self, Measure, Section},
    schematic::Logic,
    tag,
};
use clap::Args;
use sqlx::SqlitePool;

#[derive(Args)]
pub(super) struct NewCategory {
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

/// Adds a tag, unless the build itself works it out, as it does torchless.
pub(super) async fn define_tag(
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

/// Adds a standard category, its label being its name's words and its
/// plural the label with an s unless given.
pub(super) async fn define_category(
    db: &SqlitePool,
    args: NewCategory,
) -> Result<()> {
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
pub(super) async fn define_flag(
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

/// Prints each category with its designs.
pub(super) async fn print_categories(db: &SqlitePool) -> Result<()> {
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
