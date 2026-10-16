//! Lists of builds, as cards or a table, and the home page that searches
//! and narrows them, with its category sidebar.

use maud::{Markup, html};

use super::{
    PAGE_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, badge, base, facts,
    latency, name_links, size, volume,
};
use crate::{
    APP_NAME,
    models::{
        catalogue::Section,
        schematic::{Encoding, Schematic},
        user::User,
    },
    search::Term,
};

/// How the home page lays out its schematics.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Cards,
    List,
}

/// The orders the home page offers, as `sort` values and their labels. Any
/// other value in the address works too; see `schematics::order`.
const SORTS: [(&str, &str); 4] = [
    ("", "Newest"),
    ("volume", "Volume"),
    ("latency", "Latency"),
    ("-downloads", "Downloads"),
];

/// The home page's schematics and how they were asked for.
pub struct Listing<'a> {
    /// The schematics matching the search and the chosen category.
    pub shown: &'a [Schematic],
    /// How many schematics there are in all.
    pub total: usize,
    /// How many match the search, of any category.
    pub matched: usize,
    pub view: View,
    /// Such as `-downloads`; empty for newest first.
    pub sort: &'a str,
    pub query: &'a str,
    pub terms: &'a [Term],
    /// The chosen category, such as `adder`; empty for all.
    pub category: &'a str,
    /// Tag filters, each needing a match; commas within one give choices.
    pub tags: &'a [String],
    pub facets: &'a [Facet],
}

/// One category in the home page's sidebar.
pub struct Facet {
    pub name: String,
    /// Such as `Adders`.
    pub label: String,
    /// How many builds of it match the search.
    pub count: usize,
    /// Its sidebar section; `None` for Other.
    pub section: Option<Section>,
}

/// Where the home page's links lead: its layout, search, and category.
#[derive(Clone, Copy)]
struct Home<'a> {
    view: View,
    sort: &'a str,
    query: &'a str,
    category: &'a str,
    tags: &'a [String],
}

impl Home<'_> {
    fn href(self) -> String {
        let mut params = form_urlencoded::Serializer::new(String::new());
        if self.view == View::List {
            params.append_pair("view", "list");
        }
        if !self.sort.is_empty() {
            params.append_pair("sort", self.sort);
        }
        if !self.category.is_empty() {
            params.append_pair("category", self.category);
        }
        for tag in self.tags {
            params.append_pair("tag", tag);
        }
        if !self.query.is_empty() {
            params.append_pair("q", self.query);
        }
        let params = params.finish();
        if params.is_empty() {
            "/".to_owned()
        } else {
            format!("/?{params}")
        }
    }
}

/// The home page: the category sidebar beside the search and the builds
/// it finds.
pub fn schematics(viewer: Option<&User>, listing: &Listing) -> Markup {
    let Listing {
        shown,
        total,
        matched,
        view,
        sort,
        query,
        category,
        tags,
        facets,
        ..
    } = *listing;
    let here = Home {
        view,
        sort,
        query,
        category,
        tags,
    };
    let title = facets
        .iter()
        .find(|facet| facet.name == category)
        .map_or("Schematics", |facet| facet.label.as_str());
    let builds = if total == 1 { "build" } else { "builds" };
    let count = if shown.len() == total {
        format!("{total} {builds}")
    } else {
        format!("{} of {total} {builds}", shown.len())
    };
    base(
        viewer,
        APP_NAME,
        "/",
        &html! {
            div class="grid gap-x-10 gap-y-6 desktop:grid-cols-[12rem_minmax(0,1fr)]" {
                (category_sidebar(
                    facets,
                    category,
                    Some(("All", &Home { category: "", ..here }.href(), matched)),
                    |name| Home { category: name, ..here }.href(),
                ))
                div { (listing_body(listing, here, title, &count)) }
            }
        },
    )
}

/// The categories to narrow a page to, each with its count and its
/// address from `href`, under their sections' headings and then the rest;
/// `current` is the chosen one, and `all` the label, link, and count of the
/// first entry, for every category, if the page has one.
pub(super) fn category_sidebar(
    facets: &[Facet],
    current: &str,
    all: Option<(&str, &str, usize)>,
    href: impl Fn(&str) -> String,
) -> Markup {
    let item = "flex items-center justify-between gap-3 rounded-control px-2.5 py-1 text-nav text-ink hover:bg-selection hover:no-underline aria-[current=page]:bg-selection aria-[current=page]:font-semibold";
    let list = "flex flex-wrap gap-0.5 desktop:flex-col";
    let link = |to: &str, label: &str, count: usize, chosen: bool| {
        html! {
            li {
                a href=(to) class=(item) aria-current=[chosen.then_some("page")] {
                    span { (label) }
                    span class="text-meta text-muted tabular-nums" { (count) }
                }
            }
        }
    };
    let in_section = |section: Option<Section>| -> Vec<&Facet> {
        facets.iter().filter(|f| f.section == section).collect()
    };
    let rest = in_section(None);
    html! {
        // Its own scroller on a wide screen, since it stays in view and
        // may be taller than the window: sidebar.js fits it to the window
        // below its top, and without it, it fits below the header.
        nav aria-labelledby="categories-heading" data-sidebar
            class="desktop:sticky desktop:top-4 desktop:max-h-[calc(100vh-6.5rem)] desktop:self-start desktop:overflow-y-auto" {
            script defer src=(crate::web::SIDEBAR_JS.url()) {}
            h2 id="categories-heading" class="mb-2 px-2.5 text-meta font-semibold text-muted" { "Categories" }
            @if let Some((label, all_href, count)) = all {
                ul class=(list) { (link(all_href, label, count, current.is_empty())) }
            }
            @for &section in Section::ALL {
                @let members = in_section(Some(section));
                @if !members.is_empty() {
                    h3 id={ "section-" (section.name()) } class="mt-5 mb-1 px-2.5 text-section font-semibold" { (section.label()) }
                    ul aria-labelledby={ "section-" (section.name()) } class=(list) {
                        @for facet in members {
                            (link(&href(&facet.name), &facet.label, facet.count, facet.name == current))
                        }
                    }
                }
            }
            // Other, off the list, needs no heading of its own.
            @if !rest.is_empty() {
                ul class={ "mt-4 " (list) } {
                    @for facet in rest {
                        (link(&href(&facet.name), &facet.label, facet.count, facet.name == current))
                    }
                }
            }
        }
    }
}

/// The home page's title and count, with its tabs, layouts, and sorts.
fn listing_header(
    listing: &Listing,
    here: Home,
    title: &str,
    count: &str,
) -> Markup {
    let Listing { view, category, .. } = *listing;
    // Every build, and a single standard category, has a leaderboard to
    // switch to.
    let ranked = category.is_empty()
        || (!category.contains(',')
            && category != "other"
            && listing.facets.iter().any(|f| f.name == category));
    html! {
            div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                h1 class=(PAGE_TITLE) { (title) }
                p class="mr-auto self-end pb-1 text-muted tabular-nums" { (count) }
                @if ranked {
                    (category_tabs(category, false))
                }
                nav aria-label="Layout" class=(SWITCHES) {
                    a href=(Home { view: View::Cards, ..here }.href()) class=(SWITCH) aria-current=[(view == View::Cards).then_some("page")] { "Cards" }
                    a href=(Home { view: View::List, ..here }.href()) class=(SWITCH) aria-current=[(view == View::List).then_some("page")] { "List" }
                }
                nav aria-label="Sort" class=(SWITCHES) {
                    span class="px-2 text-meta text-muted" aria-hidden="true" { "Sort" }
                    @for (value, label) in SORTS {
                        a href=(Home { sort: value, ..here }.href()) class=(SWITCH)
                            aria-current=[(here.sort == value).then_some("page")] { (label) }
                    }
                }
            }
    }
}

/// The home page's title, search, and schematics.
fn listing_body(
    listing: &Listing,
    here: Home,
    title: &str,
    count: &str,
) -> Markup {
    let Listing {
        shown,
        total,
        view,
        query,
        terms,
        category,
        ..
    } = *listing;
    html! {
            (listing_header(listing, here, title, count))
            form action="/" method="get" role="search" class="mb-6 flex flex-col gap-2" {
                @if view == View::List {
                    input type="hidden" name="view" value="list";
                }
                @if !category.is_empty() {
                    input type="hidden" name="category" value=(category);
                }
                @if !here.sort.is_empty() {
                    input type="hidden" name="sort" value=(here.sort);
                }
                @for tag in here.tags {
                    input type="hidden" name="tag" value=(tag);
                }
                div class="flex gap-2" {
                    label for="search" class="sr-only" { "Search schematics" }
                    input type="search" id="search" name="q" value=(query)
                        placeholder="register vertical alpha -torchless"
                        aria-describedby="search-help"
                        class="input flex-1";
                    button type="submit" class="btn btn-primary" { "Search" }
                }
                p id="search-help" class="text-meta text-muted" {
                    "Every word narrows it: a category, tag, " code { "8b" } ", " code { "3t" } ", a builder, "
                    code { "stackable" } ". " code { "-" } " before a word leaves it out."
                }
                @if !terms.is_empty() {
                    ul class="flex flex-wrap items-center gap-1.5" aria-label="Search terms" {
                        @for (i, term) in terms.iter().enumerate() {
                            li {
                                a href=(Home { query: &without(terms, i), ..here }.href())
                                    aria-label={ "Remove " (term.display()) }
                                    class={ "badge gap-1.5 pr-2 hover:no-underline "
                                        (if term.negated { "bg-danger/10 text-danger" } else { "bg-selection text-ink" }) } {
                                    (if term.negated { "not " } else { "" }) (term.text)
                                    span aria-hidden="true" { "×" }
                                }
                            }
                        }
                        li {
                            a href=(Home { query: "", ..here }.href()) class="px-1.5 text-meta" { "Clear" }
                        }
                    }
                }
                @if !here.tags.is_empty() {
                    ul class="flex flex-wrap items-center gap-1.5" aria-label="Tag filters" {
                        @for (i, group) in here.tags.iter().enumerate() {
                            @let rest: Vec<String> = here.tags.iter().enumerate()
                                .filter(|(j, _)| *j != i).map(|(_, tag)| tag.clone()).collect();
                            @let shown = group.replace('_', " ").replace(',', " or ");
                            li {
                                a href=(Home { tags: &rest, ..here }.href())
                                    aria-label={ "Remove the tag filter " (shown) }
                                    class="badge gap-1.5 bg-selection pr-2 text-ink hover:no-underline" {
                                    "tag: " (shown)
                                    span aria-hidden="true" { "×" }
                                }
                            }
                        }
                    }
                }
            }
            @if total == 0 {
                p class="text-muted" { "No schematics yet." }
            } @else if shown.is_empty() {
                p class="text-muted" { "No builds match. Remove a term to widen the search." }
            } @else if view == View::Cards {
                (cards(shown, listing.terms))
            } @else {
                (table(shown, listing.terms))
            }
    }
}

/// A category's builds and its leaderboard, as two tabs, or for no
/// category, every build and the people leaderboard; `leaderboard` is
/// whether that is the one shown.
pub(super) fn category_tabs(category: &str, leaderboard: bool) -> Markup {
    let (builds, board) = if category.is_empty() {
        ("/".to_owned(), "/leaderboards".to_owned())
    } else {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("category", category)
            .finish();
        (format!("/?{query}"), format!("/leaderboards/{category}"))
    };
    html! {
        nav aria-label="Builds or leaderboard" class=(SWITCHES) {
            a href=(builds) class=(SWITCH)
                aria-current=[(!leaderboard).then_some("page")] { "Builds" }
            a href=(board) class=(SWITCH)
                aria-current=[leaderboard.then_some("page")] { "Leaderboard" }
        }
    }
}

/// The query with term `index` taken out.
fn without(terms: &[Term], index: usize) -> String {
    terms
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, term)| term.display())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A listed build's tags on one row. `fails simulation` comes first, so a
/// failing build stands out to be fixed or removed.
fn badges(s: &Schematic, class: &str) -> Markup {
    html! {
        @if !s.tags.is_empty() {
            ul class={ "flex flex-wrap gap-1.5 " (class) } aria-label="Tags" {
                @for tag in &s.tags {
                    li { (badge(tag)) }
                }
            }
        }
    }
}

/// That build `s` makes the width `terms` ask for as a stack of its
/// copies, if it does only so.
fn stacked_note(s: &Schematic, terms: &[Term]) -> Markup {
    html! {
        @if let Some((width, copies)) = crate::search::stacked_width(terms, s) {
            p class="text-meta text-brand" { (width) " bits as " (copies) " stacked copies" }
        }
    }
}

/// Builds as cards, each noting the width `terms` ask for when it reaches
/// it only by stacking.
pub(super) fn cards(schematics: &[Schematic], terms: &[Term]) -> Markup {
    html! {
        ul class="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-5" {
            @for s in schematics {
                // The title's link covers the whole card; the tag and name
                // links sit above it.
                li class="group relative flex h-full flex-col overflow-hidden rounded-overlay border border-line bg-raised text-ink shadow-sm transition-colors duration-150 ease-calm hover:border-boundary" {
                    img src={ "/schematics/" (s.id) "/thumbnail.png?v=" (&s.sha256[..12.min(s.sha256.len())]) }
                        alt="" loading="lazy" width="640" height="480"
                        class="aspect-[4/3] w-full border-b border-line bg-sidebar object-contain";
                    div class="flex flex-1 flex-col gap-2 p-4" {
                        h2 class="text-section font-semibold" {
                            a href={ "/schematics/" (s.id) }
                                class="text-ink group-hover:text-brand hover:no-underline after:absolute after:inset-0" { (s.name()) }
                        }
                        (badges(s, ""))
                        (stacked_note(s, terms))
                        p class="mt-auto pt-1 text-meta text-muted tabular-nums" { (facts(s)) }
                    }
                }
            }
        }
    }
}

/// Builds as table rows, for the home page's List layout.
fn table(schematics: &[Schematic], terms: &[Term]) -> Markup {
    // Columns a phone has no room for; the name and its tags come first.
    let secondary = "hidden desktop:table-cell";
    html! {
                div class="overflow-x-auto" {
                    table class="w-full border-collapse" {
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Name" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Orientation" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Encoding" }
                                th scope="col" class=(TABLE_HEAD) { "Latency" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Size" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Volume" }
                                th scope="col" class=(TABLE_HEAD) { "Builders" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Built" }
                            }
                        }
                        tbody {
                            @for s in schematics {
                                tr class="hover:bg-selection/50" {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                        a href={ "/schematics/" (s.id) } class="font-semibold" { (s.name()) }
                                        (stacked_note(s, terms))
                                        (badges(s, "mt-1.5"))
                                    }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.orientation.label()) }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.encoding.map_or("", Encoding::label)) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (latency(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (size(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (volume(s)) }
                                    td class=(TABLE_CELL) { (name_links(&s.builders)) }
                                    td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap " (secondary) } {
                                        (s.built_on.as_deref().unwrap_or(""))
                                    }
                                }
                            }
                        }
                    }
                }
    }
}
