//! Lists of builds, as cards or a table, and the home page that searches
//! and narrows them, with its category sidebar.

use maud::{Markup, html};

use super::{
    PAGE_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, badge, base, latency,
    name_links, size, volume,
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

/// A category on the home page's index, pictured by its best build.
pub struct Tile<'a> {
    /// Such as `adder`, or `other` for every category off the list.
    pub category: String,
    /// Such as `Adders`.
    pub label: String,
    pub count: usize,
    pub best: &'a Schematic,
    /// Its sidebar section; `None` for Other.
    pub section: Option<Section>,
}

/// How many categories with builds a section needs for a heading of its
/// own on the home page; a smaller one's go under Other.
const SECTION_SIZE: usize = 2;

/// The home page before anything is searched for.
pub struct Index<'a> {
    pub tiles: &'a [Tile<'a>],
    /// The builds added last, each with how many share its name.
    pub recent: &'a [(&'a Schematic, usize)],
    /// How many builds there are in all.
    pub total: usize,
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
        // With nothing to narrow it, the home page shows its index, so
        // every build has an address of its own.
        let path = if self.query.is_empty()
            && self.category.is_empty()
            && self.tags.is_empty()
        {
            "/builds"
        } else {
            "/"
        };
        let params = params.finish();
        if params.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{params}")
        }
    }
}

impl<'a> Listing<'a> {
    /// The home page's address for this listing, such as
    /// `/?category=adder&q=8b`.
    pub fn href(&self) -> String {
        self.home().href()
    }

    /// Whether words or tags are searched for, rather than a category
    /// browsed.
    fn searching(&self) -> bool {
        !self.query.is_empty() || !self.tags.is_empty()
    }

    fn home(&self) -> Home<'a> {
        Home {
            view: self.view,
            sort: self.sort,
            query: self.query,
            category: self.category,
            tags: self.tags,
        }
    }
}

/// The home page before anything is searched for: what the site is, for
/// someone not logged in, the search, and below it [`home_index`].
pub fn home(viewer: Option<&User>, index: &Index) -> Markup {
    base(
        viewer,
        APP_NAME,
        "/",
        &html! {
            (intro(viewer))
            (search_form("/", None))
            (home_index(index))
        },
    )
}

/// The home page with something searched for or chosen, or every build:
/// what the site is, for someone not logged in, the search, and below it
/// [`home_results`].
pub fn schematics(viewer: Option<&User>, listing: &Listing) -> Markup {
    // Clearing the search on the list of every build keeps to it; anywhere
    // else it goes back to the index.
    let href = listing.href();
    let action = if href.starts_with("/builds") {
        "/builds"
    } else {
        "/"
    };
    base(
        viewer,
        APP_NAME,
        "/",
        &html! {
            (intro(viewer))
            (search_form(action, Some(listing)))
            (home_results(listing))
        },
    )
}

/// What the site is, for someone not logged in.
fn intro(viewer: Option<&User>) -> Markup {
    html! {
        @if viewer.is_none() {
            p class="mb-4 max-w-prose text-muted" {
                "Minecraft redstone circuits, such as adders and multipliers, each with its "
                "latency, size, and builders, ready to download. "
                a href="/about" { "About the site" }
            }
        }
    }
}

/// The home page's search, sent to `action`. It sends as the words are
/// typed, once typing pauses, and redraws the part below it; the layout,
/// category, and tags `listing` chose go with it, and the order from its
/// menu in the results.
fn search_form(action: &str, listing: Option<&Listing>) -> Markup {
    let (view, query, category, tags) = listing
        .map_or((View::Cards, "", "", &[][..]), |l| {
            (l.view, l.query, l.category, l.tags)
        });
    html! {
        form id="search-form" action=(action) method="get" role="search" data-live
            fx-action=(action) fx-target="#listing" class="relative mb-8" {
            @if view == View::List {
                input type="hidden" name="view" value="list";
            }
            @if !category.is_empty() {
                input type="hidden" name="category" value=(category);
            }
            @for tag in tags {
                input type="hidden" name="tag" value=(tag);
            }
            label for="search" class="sr-only" { "Search builds" }
            input type="search" id="search" name="q" value=(query)
                placeholder="Search builds, such as 8b adder 3t"
                autocomplete="off" aria-describedby="search-help"
                class="peer input";
            // Shown only while the box is focused and empty, over the gap
            // below it, so nothing moves.
            p id="search-help"
                class="absolute top-full z-10 mt-1.5 hidden bg-canvas text-meta text-muted peer-focus:peer-placeholder-shown:block" {
                (search_help())
            }
        }
    }
}

/// How the search reads its words.
fn search_help() -> Markup {
    html! {
        "Each word narrows it: a category, tag, " code { "8b" } ", " code { "3t" } ", a builder, "
        code { "stackable" } ". " code { "-" } " before a word leaves it out."
    }
}

/// The part of the home page a search redraws: the category sidebar, each
/// category with its count of matches, beside the title, the terms
/// searched for, and the builds found. A search shows only what it finds,
/// with no sidebar.
pub fn home_results(listing: &Listing) -> Markup {
    let here = listing.home();
    let title = listing
        .facets
        .iter()
        .find(|facet| facet.name == listing.category)
        .map_or("Builds", |facet| facet.label.as_str());
    if listing.searching() {
        return html! {
            div id="listing" { (listing_body(listing, here, title)) }
        };
    }
    html! {
        div id="listing" class="grid gap-x-10 gap-y-6 desktop:grid-cols-[12rem_minmax(0,1fr)]" {
            // A phone has no room beside the builds, so it goes back to
            // the index to choose another category.
            (category_sidebar(
                listing.facets,
                listing.category,
                Some(("All", &Home { category: "", ..here }.href(), listing.matched)),
                |name| Home { category: name, ..here }.href(),
                "hidden desktop:block",
            ))
            div {
                a href="/" class="mb-2 inline-block text-meta desktop:hidden" { "← All categories" }
                (listing_body(listing, here, title))
            }
        }
    }
}

/// The part of the home page below its search before anything is searched
/// for: each category as a tile, and the builds added last. A search
/// cleared redraws it.
pub fn home_index(index: &Index) -> Markup {
    html! {
        div id="listing" {
            h1 class="sr-only" { (APP_NAME) }
            section aria-labelledby="index-heading" class="mb-12" {
                // The section headings say what the tiles are, so this one
                // is for a screen reader's outline only.
                h2 id="index-heading" class="sr-only" { "Categories" }
                @for (heading, id, tiles) in tile_groups(index.tiles) {
                    h3 id={ "tiles-" (id) } class="mt-8 mb-3 text-section font-semibold first-of-type:mt-0" { (heading) }
                    ul aria-labelledby={ "tiles-" (id) } class="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-4" {
                    @for tile in tiles {
                        @let href = Home {
                            view: View::Cards,
                            sort: "",
                            query: "",
                            category: &tile.category,
                            tags: &[],
                        }.href();
                        li class="group relative flex flex-col overflow-hidden rounded-overlay border border-line bg-raised shadow-sm transition-colors duration-150 ease-calm hover:border-boundary" {
                            img src=(thumbnail_src(tile.best)) alt="" loading="lazy" width="640" height="480"
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
                div class="mb-3 flex flex-wrap items-baseline gap-x-4 gap-y-1" {
                    h2 id="recent-heading" class="mr-auto text-section font-semibold" { "Recently added" }
                    a href="/builds" { "All " (index.total) " builds" }
                }
                ul class=(CARDS) {
                    @for &(s, named) in index.recent {
                        (card(s, &html! {
                            @if named > 1 {
                                p class="text-meta text-muted" { (named) " builds by this name" }
                            }
                        }))
                    }
                }
            }
        }
    }
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
    class: &str,
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
            class={ (class) " desktop:sticky desktop:top-4 desktop:max-h-[calc(100vh-6.5rem)] desktop:self-start desktop:overflow-y-auto" } {
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

/// The home page's title and count, with its tabs, layouts, and order.
fn listing_header(listing: &Listing, here: Home, title: &str) -> Markup {
    let Listing {
        shown,
        view,
        category,
        ..
    } = *listing;
    // A single standard category has a leaderboard to switch to.
    let ranked = !category.is_empty()
        && !category.contains(',')
        && category != "other"
        && listing.facets.iter().any(|f| f.name == category);
    html! {
            div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                h1 class=(PAGE_TITLE) { (title) }
                p class="mr-auto self-end pb-1 text-muted tabular-nums" {
                    (shown.len()) span class="sr-only" { " builds" }
                }
                @if ranked {
                    (category_tabs(category, false))
                }
                nav aria-label="Layout" class=(SWITCHES) {
                    a href=(Home { view: View::Cards, ..here }.href()) class=(SWITCH) aria-current=[(view == View::Cards).then_some("page")] { "Cards" }
                    a href=(Home { view: View::List, ..here }.href()) class=(SWITCH) aria-current=[(view == View::List).then_some("page")] { "List" }
                }
                (sort_menu(here.sort))
            }
    }
}

/// The orders to choose from, as a menu that belongs to the search form,
/// so choosing one sends the search again. An order the address asked for
/// that the menu lacks is added to it.
fn sort_menu(sort: &str) -> Markup {
    let known = SORTS.iter().any(|(value, _)| *value == sort);
    html! {
        label class="flex items-center gap-2 text-meta text-muted" {
            "Sort"
            select id="sort" name="sort" form="search-form" class="input min-h-9 w-auto text-body" {
                @for (value, label) in SORTS {
                    option value=(value) selected[value == sort] { (label) }
                }
                @if !known {
                    option value=(sort) selected { (sort) }
                }
            }
        }
    }
}

/// The home page's title, the terms and tags searched for, and the builds
/// found.
fn listing_body(listing: &Listing, here: Home, title: &str) -> Markup {
    let Listing {
        shown,
        total,
        view,
        terms,
        ..
    } = *listing;
    html! {
            (listing_header(listing, here, title))
            @if listing.searching() {
                div class="mb-6 flex flex-wrap items-center gap-1.5" {
                    // With no sidebar, the category searched in is one of
                    // the things to remove.
                    @if !here.category.is_empty() {
                        a href=(Home { category: "", ..here }.href())
                            aria-label={ "Search every category, not only " (title) }
                            class="badge gap-1.5 bg-selection pr-2 text-ink hover:no-underline" {
                            "in " (title)
                            span aria-hidden="true" { "×" }
                        }
                    }
                    @if !terms.is_empty() {
                        ul class="contents" aria-label="Search terms" {
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
                        }
                    }
                    @if !here.tags.is_empty() {
                        ul class="contents" aria-label="Tag filters" {
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
                    a href=(Home { query: "", tags: &[], ..here }.href()) class="px-1.5 text-meta" { "Clear" }
                }
            }
            @if total == 0 {
                p class="text-muted" { "No builds yet." }
            } @else if shown.is_empty() {
                p class="max-w-prose text-muted" {
                    "No builds match. " (search_help()) " Remove a term to widen the search."
                }
            } @else if view == View::Cards {
                (cards(shown, terms))
            } @else {
                (table(shown, terms))
            }
    }
}

/// A category's builds and its leaderboard, as two tabs; `leaderboard` is
/// whether that is the one shown.
pub(super) fn category_tabs(category: &str, leaderboard: bool) -> Markup {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("category", category)
        .finish();
    html! {
        nav aria-label="Builds or leaderboard" class=(SWITCHES) {
            a href={ "/?" (query) } class=(SWITCH)
                aria-current=[(!leaderboard).then_some("page")] { "Builds" }
            a href={ "/leaderboards/" (category) } class=(SWITCH)
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
        ul class=(CARDS) {
            @for s in schematics {
                (card(s, &stacked_note(s, terms)))
            }
        }
    }
}

/// The list holding cards.
const CARDS: &str =
    "grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-5";

/// The picture of `s`, at an address that changes with its file.
fn thumbnail_src(s: &Schematic) -> String {
    format!(
        "/schematics/{}/thumbnail.png?v={}",
        s.id,
        &s.sha256[..12.min(s.sha256.len())]
    )
}

/// The tiles under their sections' headings, each with its heading and
/// the id it goes by, in the sidebar's order, then under Other the tiles of
/// every section too small for a heading and those off the list, keeping
/// the order of `tiles`.
fn tile_groups<'t>(
    tiles: &'t [Tile<'t>],
) -> Vec<(&'static str, &'static str, Vec<&'t Tile<'t>>)> {
    let in_section = |section: Section| -> Vec<&Tile> {
        tiles
            .iter()
            .filter(|t| t.section == Some(section))
            .collect()
    };
    let headed: Vec<Section> = Section::ALL
        .iter()
        .copied()
        .filter(|&section| in_section(section).len() >= SECTION_SIZE)
        .collect();
    let mut groups: Vec<_> = headed
        .iter()
        .map(|&section| (section.label(), section.name(), in_section(section)))
        .collect();
    let rest: Vec<&Tile> = tiles
        .iter()
        .filter(|t| t.section.is_none_or(|section| !headed.contains(&section)))
        .collect();
    if !rest.is_empty() {
        groups.push(("Other", "other", rest));
    }
    groups
}

/// Build `s` as a card, with `note` under its tags.
fn card(s: &Schematic, note: &Markup) -> Markup {
    html! {
                // The title's link covers the whole card; the tag and name
                // links sit above it.
                li class="group relative flex h-full flex-col overflow-hidden rounded-overlay border border-line bg-raised text-ink shadow-sm transition-colors duration-150 ease-calm hover:border-boundary" {
                    img src=(thumbnail_src(s))
                        alt="" loading="lazy" width="640" height="480"
                        class="aspect-[4/3] w-full border-b border-line bg-sidebar object-contain";
                    div class="flex flex-1 flex-col gap-2 p-4" {
                        h2 class="text-section font-semibold" {
                            a href={ "/schematics/" (s.id) }
                                class="text-ink group-hover:text-brand hover:no-underline after:absolute after:inset-0" { (s.name()) }
                        }
                        (badges(s, ""))
                        (note)
                        // Latency is what builds are compared by, so it
                        // leads.
                        p class="mt-auto flex flex-wrap items-baseline gap-x-2 pt-1 tabular-nums" {
                            @if s.latency.is_some() {
                                span class="font-semibold" { (latency(s)) }
                            }
                            span class="text-meta text-muted" { (size(s)) " · " (volume(s)) }
                        }
                        // The first builder, and how many more, on one line;
                        // the build's page names them all.
                        @if let Some((first, more)) = s.builders.split_first() {
                            p class="truncate text-meta text-muted" {
                                (name_links(std::slice::from_ref(first)))
                                @if !more.is_empty() { " +" (more.len()) }
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
