//! Builds as cards, wherever a page lists some: each pictured, with its
//! few facts, the tags hard to tell from its picture, and its people.

use std::collections::HashSet;

use maud::{Markup, html};

use crate::{
    build::{
        Build,
        tag::{self, Tag},
    },
    category::Flag,
    search::Term,
    web::{
        href,
        ui::{
            icons::{self, icon},
            person::name_links,
            summary::{badge, latency, private_badge, size, volume},
            text,
        },
    },
};

/// What a viewer's cards show of them: their palette's key, for the
/// pictures, if they chose one, and, once signed in, a star to favourite
/// each build with, filled on those they have.
#[derive(Default)]
pub struct Viewing {
    pub palette: Option<String>,
    pub signed_in: bool,
    pub favorites: HashSet<String>,
}

/// Builds as cards, each noting the width `terms` ask for when it reaches
/// it only by stacking, as the viewer sees it.
pub(crate) fn cards(
    schematics: &[Build],
    terms: &[Term],
    viewing: &Viewing,
) -> Markup {
    html! {
        ul class=(CARDS) {
            @for s in schematics {
                (card(s, &stacked_note(s, terms), viewing))
            }
        }
    }
}

/// The list holding cards: as many 16rem cards to a row as fit, and two
/// on a phone.
pub(crate) const CARDS: &str = "grid grid-cols-[repeat(auto-fill,minmax(min(16rem,calc(50%_-_0.375rem)),1fr))] gap-3 sm:gap-5";

/// The picture of `s`, at an address that changes with its file, with
/// the viewer's `palette`, and as it comes to be drawn in a theme's
/// building blocks, once recognized, as a build marked as following the
/// conventions later may be, its file the same.
pub(crate) fn thumbnail_src(s: &Build, palette: Option<&str>) -> String {
    let palette = palette.map_or(String::new(), |key| format!("&p={key}"));
    let themed = if s.evaluated() && s.verified() {
        "-b"
    } else {
        ""
    };
    format!(
        "/schematics/{}/thumbnail.webp?v={}-{}{themed}{palette}",
        s.id,
        &s.file.sha256[..12.min(s.file.sha256.len())],
        crate::thumbnail::RENDERER
    )
}

/// Build `s` as a card, with `note` under its tags, pictured in the
/// viewer's palette, with a star when they have favourited it.
pub(crate) fn card(s: &Build, note: &Markup, viewing: &Viewing) -> Markup {
    html! {
                // The title's link covers the whole card; the tag and name
                // links sit above it.
                li class="group relative flex h-full flex-col overflow-hidden rounded-overlay border border-line bg-raised text-ink shadow-sm transition-colors duration-150 ease-calm hover:border-boundary" {
                    img src=(thumbnail_src(s, viewing.palette.as_deref()))
                        alt="" loading="lazy" decoding="async" width="640" height="480"
                        class="aspect-[4/3] w-full border-b border-line bg-sidebar object-contain";
                    @if viewing.signed_in {
                        (card_star(&s.id, viewing.favorites.contains(&s.id)))
                    }
                    // What every build has comes first, so cards line up:
                    // its name, then its latency and size; then what some
                    // have, its tags; and its people at the foot.
                    div class="flex flex-1 flex-col gap-2 p-3 sm:p-4" {
                        div class="flex flex-col gap-0.5" {
                            h2 class="text-section font-semibold" {
                                a href=(href::build(&s.id))
                                    class="text-ink group-hover:text-brand hover:no-underline after:absolute after:inset-0" { (s.name()) }
                            }
                            // Latency is what builds are compared by, so it
                            // leads.
                            p class="flex flex-wrap items-baseline gap-x-2 tabular-nums" {
                                @if s.specs.latency.is_some() {
                                    span class="font-semibold" { (latency(s)) }
                                }
                                span class="text-meta text-muted" { (size(s)) " · " (volume(s)) }
                                @if s.entry.favorites > 0 {
                                    @let label = text::favourites_label(s.entry.favorites);
                                    span class="ml-auto inline-flex items-center gap-1 self-center text-meta text-muted" title=(label) aria-label=(label) {
                                        (icon(icons::STAR, "size-3.5")) (s.entry.favorites)
                                    }
                                }
                            }
                        }
                        (note)
                        (badges(s, ""))
                        // Its authors, then who helped, on one line; the
                        // build's page names them all.
                        @if !s.credits.authors.is_empty() || !s.credits.contributors.is_empty() {
                            p class="mt-auto truncate pt-1 text-meta text-muted" {
                                (name_links(&s.credits.authors))
                                @if !s.credits.contributors.is_empty() {
                                    @if !s.credits.authors.is_empty() { " · " }
                                    "with " (name_links(&s.credits.contributors))
                                }
                            }
                        }
                    }
                }
    }
}

/// The star in a card's corner that favourites build `id`, or takes it
/// back, as its page's does: filled on a favourite, and otherwise shown
/// while the card is pointed at or focused, or always where nothing can
/// point. Through fixi it is redrawn in place.
pub fn card_star(id: &str, favorited: bool) -> Markup {
    let label = if favorited {
        "Remove from favourites"
    } else {
        "Add to favourites"
    };
    let action = format!("/schematics/{id}/favorite");
    let shown = if favorited {
        "text-brand"
    } else {
        "text-muted opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 hover:text-brand [@media(hover:none)]:opacity-100"
    };
    html! {
        form action=(action) method="post" fx-action=(action) fx-method="post"
            class="absolute top-2 right-2 z-10" {
            input type="hidden" name="card" value="yes";
            @if !favorited {
                input type="hidden" name="on" value="yes";
            }
            button type="submit" aria-label=(label) title=(label) aria-pressed=(favorited)
                class={ "flex size-8 cursor-pointer items-center justify-center rounded-full bg-raised/90 shadow-sm transition-opacity duration-150 ease-calm " (shown) } {
                (icon(icons::STAR, if favorited { "size-4 fill-current" } else { "size-4" }))
            }
        }
    }
}

/// A listed build's tags on one row, but those its card makes plain:
/// any its name says, such as its design, its orientation, and its
/// carry in and out, which its picture shows. `fails simulation` comes
/// first, so a failing build stands out to be fixed or removed.
pub(crate) fn badges(s: &Build, class: &str) -> Markup {
    let tags = on_card(s);
    html! {
        @if !tags.is_empty() || !s.entry.public {
            ul class={ "flex flex-wrap gap-1.5 " (class) } aria-label="Tags" {
                @if !s.entry.public {
                    li { (private_badge()) }
                }
                @for tag in &tags {
                    li { (badge(tag, s)) }
                }
            }
        }
    }
}

/// The tags of build `s` its card shows: not its design or orientation,
/// nor a flag in [`SEEN`], nor any its name says in words, nor one another
/// tag implies, see [`tag::implied`].
pub(crate) fn on_card(s: &Build) -> Vec<Tag> {
    let name = format!(" {} ", s.name().to_lowercase());
    let tags = s.every_tag();
    tags.iter()
        .filter(|tag| {
            !tag::implied(&tag.name, &tags)
                && !matches!(
                    tag.kind,
                    tag::Kind::Design | tag::Kind::Orientation
                )
                && s.specs.design.as_deref() != Some(tag.name.as_str())
                && !SEEN.contains(&tag.name.as_str())
                && !name.contains(&format!(" {} ", tag.label()))
        })
        .cloned()
        .collect()
}

/// Tags left off a card: synced, which most builds are, and the flags its
/// picture shows plainly.
pub(crate) const SEEN: [&str; 5] = [
    tag::SYNCED,
    Flag::CARRY_IN,
    Flag::CARRY_OUT,
    Flag::BORROW_IN,
    Flag::BORROW_OUT,
];

/// That build `s` makes the width `terms` ask for as a stack of its
/// copies, if it does only so.
pub(crate) fn stacked_note(s: &Build, terms: &[Term]) -> Markup {
    html! {
        @if let Some((width, copies)) = crate::search::stacked_width(terms, s) {
            p class="text-meta text-brand" { (width) " bits as " (copies) " stacked copies" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::Orientation;

    fn adder(
        design: Option<&str>,
        flags: &[&'static str],
        tags: &[&str],
    ) -> Build {
        let mut s = crate::search::tests::schematic(
            "adder",
            Orientation::Horizontal,
            tags,
        );
        s.specs.design = design.map(str::to_owned);
        s.flags = flags
            .iter()
            .map(|&name| Flag { name, label: name })
            .collect();
        s
    }

    /// A card leaves out what its picture or name makes plain, or says
    /// little, such as an inverted carry in, and keeps what is hard to
    /// tell, such as how its copies stack.
    #[test]
    fn a_card_shows_only_the_tags_hard_to_tell() {
        let mut s = adder(
            Some("carry_cancel"),
            &["carry_in", "carry_out", "inverted_carry_in"],
            &["pistons"],
        );
        s.stacking = vec![crate::stack::StackDirection {
            west: 2,
            up: 0,
            max_copies: None,
            faces: None,
            pitch: Some(2),
        }];
        let names = |s: &Build| -> Vec<String> {
            on_card(s).into_iter().map(|tag| tag.name).collect()
        };
        // How far apart its copies stack says it stacks.
        assert_eq!(names(&s), ["2-wide_stackable", "pistons"]);
        let mut gate = adder(None, &[], &[]);
        gate.category = "xnor_gate".to_owned();
        assert!(gate.name().contains("XNOR"), "{}", gate.name());
        assert!(names(&gate).is_empty());
    }
}
