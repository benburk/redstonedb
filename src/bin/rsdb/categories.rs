//! The standard categories, with their measures, designs, and flags, as
//! `rsdb` lists them, and the tags builds carry, as it lists and adds
//! them.

use anyhow::{Result, bail};
use app::{build::tag, category::Kind, store::tags};
use sqlx::SqlitePool;

/// Prints every known tag with its kind and filename token.
pub(super) async fn print_tags(db: &SqlitePool) -> Result<()> {
    for tag in tags::all(db).await? {
        let (kind, token) =
            (tag.kind.name(), tag.token.as_deref().unwrap_or(""));
        println!("{kind:<12} {token:<8} {}", tag.label());
    }
    Ok(())
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
    tags::define(db, name, token, kind).await?;
    Ok(())
}

/// Prints each standard category with its measures, designs, flags, and
/// aliases, as the code's table lists them, alphabetically by label.
pub(super) fn print_categories() {
    let mut kinds: Vec<Kind> = Kind::all().collect();
    kinds.sort_by_key(|kind| kind.label().to_lowercase());
    for category in kinds {
        let facts = category.facts();
        let names = |names: Vec<&str>| names.join(", ");
        let measures = names(facts.measures.iter().map(|m| m.name()).collect());
        let mut parts = vec![
            format!("{:<20} {:<22}", facts.name, facts.label),
            format!("by {measures}"),
        ];
        if !facts.designs.is_empty() {
            let designs = facts.designs.iter().map(|d| d.name).collect();
            parts.push(format!("designs: {}", names(designs)));
        }
        if !facts.flags.is_empty() {
            let flags = facts.flags.iter().map(|f| f.name).collect();
            parts.push(format!("flags: {}", names(flags)));
        }
        if !facts.aliases.is_empty() {
            parts.push(format!("aliases: {}", names(facts.aliases.to_vec())));
        }
        println!("{}", parts.join(" ").trim_end());
    }
}
