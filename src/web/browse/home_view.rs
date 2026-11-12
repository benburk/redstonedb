//! The home page before a search narrows it: each category pictured by its
//! best build, under its section, and the builds added last.

use maud::{Markup, html};

use crate::{
    APP_NAME,
    account::User,
    build::Build,
    category::Section,
    web::{
        browse::{list_view::home_page, query::HomeQuery},
        ui::{
            PAGE_TITLE,
            cards::{CARDS, Viewing, card, thumbnail_src},
        },
    },
};

/// A category on the home page's index, pictured by its best build.
pub struct Tile<'a> {
    /// Such as `adder`, or `other` for every category off the list.
    pub category: String,
    /// Such as `Adders`.
    pub label: String,
    pub count: usize,
    pub best: &'a Build,
    /// Its sidebar section; `None` for Other.
    pub section: Option<Section>,
}

/// The home page before anything is searched for.
pub struct Index<'a> {
    pub tiles: &'a [Tile<'a>],
    /// The builds added last, each with how many share its name.
    pub recent: &'a [(&'a Build, usize)],
    /// How many builds there are in all.
    pub total: usize,
    pub viewing: &'a Viewing,
}

/// The home page before anything is searched for: the search, and below it
/// [`home_index`].
pub fn home(viewer: Option<&User>, index: &Index) -> Markup {
    home_page(viewer, "/", None, &home_index(index))
}

/// The part of the home page below its search before anything is searched
/// for: each category as a tile, and the builds added last. A search
/// cleared redraws it.
pub fn home_index(index: &Index) -> Markup {
    html! {
        div id="listing" class="pt-5" {
            h1 class="sr-only" { (APP_NAME) }
            section aria-labelledby="index-heading" class="mb-12" {
                // The section headings say what the tiles are, so this one
                // is for a screen reader's outline only.
                h2 id="index-heading" class="sr-only" { "Categories" }
                @for (heading, id, tiles) in tile_groups(index.tiles) {
                    h3 id={ "tiles-" (id) } class={ (PAGE_TITLE) " mt-12 mb-4 first-of-type:mt-0" } { (heading) }
                    // Five to a row at the page's full width, fewer as it
                    // narrows, and never fewer than two.
                    ul aria-labelledby={ "tiles-" (id) } class="grid grid-cols-[repeat(auto-fill,minmax(min(13rem,calc(50%_-_0.5rem)),1fr))] gap-4" {
                    @for tile in tiles {
                        @let href = HomeQuery {
                            category: tile.category.clone(),
                            ..HomeQuery::default()
                        }.href();
                        li class="group relative flex flex-col overflow-hidden rounded-overlay border border-line bg-raised shadow-sm transition-colors duration-150 ease-calm hover:border-boundary" {
                            img src=(thumbnail_src(tile.best, index.viewing.palette.as_deref())) alt="" loading="lazy" decoding="async" width="640" height="480"
                                class="aspect-[4/3] w-full border-b border-line bg-sidebar object-contain";
                            div class="flex flex-1 flex-col gap-0.5 px-3 py-2.5" {
                                h4 class="flex items-baseline justify-between gap-2 font-semibold" {
                                    a href=(href) class="text-ink group-hover:text-brand hover:no-underline after:absolute after:inset-0" { (tile.label) }
                                    span class="text-meta font-normal text-muted tabular-nums" {
                                        (tile.count) span class="sr-only" { " builds" }
                                    }
                                }
                            }
                        }
                    }
                    }
                }
            }
            section aria-labelledby="recent-heading" {
                div class="mb-4 flex flex-wrap items-baseline gap-x-4 gap-y-1" {
                    h2 id="recent-heading" class={ (PAGE_TITLE) " mr-auto" } { "Recently added" }
                    a href="/builds" { "All " (index.total) " builds" }
                }
                ul class=(CARDS) {
                    @for &(s, named) in index.recent {
                        (card(s, &html! {
                            @if named > 1 {
                                p class="text-meta text-muted" { (named) " builds by this name" }
                            }
                        }, index.viewing))
                    }
                }
            }
        }
    }
}

/// The tiles under their sections' headings, each with its heading and
/// the id it goes by, in the sidebar's order, then under Other those off
/// the list, keeping the order of `tiles`.
fn tile_groups<'t>(
    tiles: &'t [Tile<'t>],
) -> Vec<(&'static str, &'static str, Vec<&'t Tile<'t>>)> {
    let in_section = |section: Section| -> Vec<&Tile> {
        tiles
            .iter()
            .filter(|t| t.section == Some(section))
            .collect()
    };
    // Every section with a category that has builds keeps its heading, as
    // the sidebar does.
    let headed: Vec<Section> = Section::ALL
        .iter()
        .copied()
        .filter(|&section| !in_section(section).is_empty())
        .collect();
    let mut groups: Vec<_> = headed
        .iter()
        .map(|&section| (section.label(), section.name(), in_section(section)))
        .collect();
    let rest: Vec<&Tile> =
        tiles.iter().filter(|t| t.section.is_none()).collect();
    if !rest.is_empty() {
        groups.push(("Other", "other", rest));
    }
    groups
}
