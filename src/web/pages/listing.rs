//! Lists of builds, as cards or a table, and the home page that searches
//! and narrows them, by its search and its menu of categories.

use std::collections::HashSet;

use maud::{Markup, html};

use super::{
    PAGE_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, badge, base,
    icons::{self, icon},
    latency, name_links, size, volume,
};
use crate::{
    APP_NAME,
    models::{
        catalogue::Section,
        schematic::{Encoding, Schematic},
        tag::{self, Tag},
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
    ("volume", "Smallest"),
    ("latency", "Fastest"),
    ("-downloads", "Popular"),
];

/// The home page's schematics and how they were asked for.
pub struct Listing<'a> {
    /// The schematics matching the search and the chosen category.
    pub shown: &'a [Schematic],
    /// How many schematics there are in all.
    pub total: usize,
    pub view: View,
    /// Such as `-downloads`; empty for newest first.
    pub sort: &'a str,
    pub query: &'a str,
    pub terms: &'a [Term],
    /// The chosen category, such as `adder`; empty for all.
    pub category: &'a str,
    /// Tag filters, each needing a match; commas within one give choices.
    pub tags: &'a [String],
    pub unverified: Unverified,
    pub facets: &'a [Facet],
    pub viewing: &'a Viewing,
}

/// What a viewer's cards show of them: their palette's key, for the
/// pictures, if they chose one, and, once signed in, a star to favourite
/// each build with, filled on those they have.
#[derive(Default)]
pub struct Viewing {
    pub palette: Option<String>,
    pub signed_in: bool,
    pub favorites: HashSet<String>,
}

/// The builds a listing found that are not verified as their category:
/// whether they are shown, and how many there are.
#[derive(Clone, Copy)]
pub struct Unverified {
    pub shown: bool,
    pub count: usize,
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

/// The home page before anything is searched for.
pub struct Index<'a> {
    pub tiles: &'a [Tile<'a>],
    /// The builds added last, each with how many share its name.
    pub recent: &'a [(&'a Schematic, usize)],
    /// How many builds there are in all.
    pub total: usize,
    pub viewing: &'a Viewing,
}

/// Where the home page's links lead: its layout, search, and category.
#[derive(Clone, Copy)]
struct Home<'a> {
    view: View,
    sort: &'a str,
    query: &'a str,
    category: &'a str,
    tags: &'a [String],
    /// Whether builds not verified as their category are listed too.
    unverified: bool,
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
        if self.unverified {
            params.append_pair("unverified", "show");
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

    /// Whether words are searched for, or tags outside a category, whose
    /// page shows its tags chosen among its filters.
    fn searching(&self) -> bool {
        !self.query.is_empty()
            || (!self.tags.is_empty() && self.category.is_empty())
    }

    fn home(&self) -> Home<'a> {
        Home {
            view: self.view,
            sort: self.sort,
            query: self.query,
            category: self.category,
            tags: self.tags,
            unverified: self.unverified.shown,
        }
    }
}

/// The home page before anything is searched for: the search, and below it
/// [`home_index`].
pub fn home(viewer: Option<&User>, index: &Index) -> Markup {
    home_page(viewer, "/", None, &home_index(index))
}

/// The home page: the search, sent to `action` with what `listing` chose,
/// the place the dialog for finding similar builds opens in, and `below`,
/// the part a search redraws.
pub(super) fn home_page(
    viewer: Option<&User>,
    action: &str,
    listing: Option<&Listing>,
    below: &Markup,
) -> Markup {
    // Every build is the sidebar's Recent builds, and the index its Home.
    let current_path = match listing {
        None => "/",
        Some(listing) if listing.href().starts_with("/builds") => "/builds",
        Some(_) => "",
    };
    base(
        viewer,
        APP_NAME,
        current_path,
        &html! {
            (search_form(action, listing))
            div id="similar" {}
            (below)
        },
    )
}

/// The home page with something searched for or chosen, or every build:
/// the search, and below it [`home_results`].
pub fn schematics(viewer: Option<&User>, listing: &Listing) -> Markup {
    // Clearing the search on the list of every build keeps to it; anywhere
    // else it goes back to the index.
    let href = listing.href();
    let action = if href.starts_with("/builds") {
        "/builds"
    } else {
        "/"
    };
    home_page(viewer, action, Some(listing), &home_results(listing))
}

/// The home page's search, sent to `action`. It sends as the words are
/// typed, once typing pauses, and redraws the part below it; the layout
/// and tags `listing` chose go with it, and the category and order from
/// their menus in the results. In the box's end, a button opens the dialog
/// for finding the builds most like a file.
fn search_form(action: &str, listing: Option<&Listing>) -> Markup {
    let (view, query, tags) = listing
        .map_or((View::Cards, "", &[][..]), |l| (l.view, l.query, l.tags));
    let unverified = listing.is_some_and(|l| l.unverified.shown);
    html! {
        form id="search-form" action=(action) method="get" role="search" data-live
            fx-action=(action) fx-target="#listing" class="relative mb-3" {
            @if view == View::List {
                input type="hidden" name="view" value="list";
            }
            @for tag in tags {
                input type="hidden" name="tag" value=(tag);
            }
            @if unverified {
                input type="hidden" name="unverified" value="show";
            }
            label for="search" class="sr-only" { "Search builds" }
            div class="relative" {
                input type="search" id="search" name="q" value=(query)
                    placeholder="Search builds, such as 8b adder 3t"
                    autocomplete="off"
                    class="input pr-12";
                // Inside the box's end, saying what it does while pointed
                // at or focused. The dialog comes from the server, with the
                // viewer's Schemati files, only once asked for.
                div class="group/similar absolute inset-y-0 right-1 flex items-center" {
                    button type="button" fx-action="/similar" fx-target="#similar"
                        aria-label="Find similar builds" aria-describedby="similar-tip"
                        class="flex size-8 cursor-pointer items-center justify-center rounded-control text-muted hover:bg-selection hover:text-ink" {
                        (icon(icons::UPLOAD, "size-4"))
                    }
                    span id="similar-tip" role="tooltip"
                        class="pointer-events-none absolute top-full right-0 z-20 mt-2 hidden w-64 rounded-control bg-ink px-3 py-2 text-meta text-canvas shadow-overlay group-hover/similar:block group-has-[:focus-visible]/similar:block" {
                        span class="block font-semibold" { "Find similar builds" }
                        "Choose a schematic file or a Schemati link to see which builds share the most redstone with it."
                    }
                }
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

/// The part of the home page a search redraws: the title, the terms
/// searched for, and the builds found.
pub fn home_results(listing: &Listing) -> Markup {
    let here = listing.home();
    let title = listing
        .facets
        .iter()
        .find(|facet| facet.name == listing.category)
        .map_or("Builds", |facet| facet.label.as_str());
    html! {
        div id="listing" { (listing_body(listing, here, title)) }
    }
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
                        @let href = Home {
                            view: View::Cards,
                            sort: "",
                            query: "",
                            category: &tile.category,
                            tags: &[],
                            unverified: false,
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

/// The categories to narrow a page to, as a menu named `category`: first
/// `all`, the label of the choice of every category, then each
/// category with its count, under its section, and the rest; `current` is
/// the chosen one. With `form`, it belongs to that form, wherever it is.
pub(super) fn category_menu(
    facets: &[Facet],
    current: &str,
    all: &str,
    form: Option<&str>,
) -> Markup {
    let option = |facet: &Facet| {
        html! {
            option value=(facet.name) selected[facet.name == current] {
                (facet.label) " (" (facet.count) ")"
            }
        }
    };
    let in_section = |section: Option<Section>| -> Vec<&Facet> {
        facets.iter().filter(|f| f.section == section).collect()
    };
    // A choice the address made that the menu lacks, such as two at once.
    let known = current.is_empty() || facets.iter().any(|f| f.name == current);
    html! {
        label class="flex min-w-0 items-center" {
            span class="sr-only" { "Category" }
            select id="category" name="category" form=[form] class="input min-h-9 w-full min-w-0 text-body sm:w-auto" {
                option value="" selected[current.is_empty()] { (all) }
                @if !known {
                    option value=(current) selected { (current) }
                }
                @for &section in Section::ALL {
                    @let members = in_section(Some(section));
                    @if !members.is_empty() {
                        optgroup label=(section.label()) {
                            @for facet in members { (option(facet)) }
                        }
                    }
                }
                @for facet in in_section(None) { (option(facet)) }
            }
        }
    }
}

/// The row under the search, its category, order, and layout, then the
/// page's title and count.
fn listing_header(listing: &Listing, here: Home, title: &str) -> Markup {
    let Listing {
        shown,
        view,
        category,
        ..
    } = *listing;
    let layout = |to: View, label: &str, glyph: &str| {
        html! {
            a href=(Home { view: to, ..here }.href()) aria-label=(label) title=(label)
                class={ (SWITCH) " flex items-center px-2" }
                aria-current=[(view == to).then_some("page")] {
                (icon(glyph, "size-4"))
            }
        }
    };
    html! {
            // Paired with the search above it: what it searches in, the
            // order, and the layout, which a phone, too narrow for the
            // list's table, leaves at cards.
            div class="mb-6 flex items-center gap-2" {
                div class="min-w-0 flex-1 sm:flex-none" {
                    (category_menu(listing.facets, category, "All categories", Some("search-form")))
                }
                (sort_menu(here.sort))
                div class="ml-auto hidden shrink-0 sm:block" {
                    nav aria-label="Layout" class=(SWITCHES) {
                        (layout(View::Cards, "Cards", icons::GRID))
                        (layout(View::List, "List", icons::LIST))
                    }
                }
            }
            div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                h1 class=(PAGE_TITLE) { (title) }
                p class="mr-auto self-end pb-1 text-muted tabular-nums" {
                    (shown.len()) span class="sr-only" { " builds" }
                }
                (unverified_switch(listing.unverified, here))
            }
    }
}

/// Whether the builds not verified as their category are listed, as a
/// link that turns them on or off, with how many there are; nothing when
/// there are none.
fn unverified_switch(unverified: Unverified, here: Home) -> Markup {
    let Unverified { shown, count } = unverified;
    html! {
        @if count > 0 {
            a href=(Home { unverified: !shown, ..here }.href())
                aria-pressed=(shown)
                class="flex min-h-9 items-center gap-2 self-end text-meta text-muted hover:text-ink hover:no-underline" {
                span class={ "flex size-4 items-center justify-center rounded-[4px] border "
                    (if shown { "border-brand bg-brand text-on-brand" } else { "border-boundary" }) } {
                    @if shown { (icon(icons::CHECK, "size-3")) }
                }
                span { "Show unverified (" span class="tabular-nums" { (count) } ")" }
            }
        }
    }
}

/// The orders to choose from, as a menu that belongs to the search form,
/// so choosing one sends the search again. An order the address asked for
/// that the menu lacks is added to it.
fn sort_menu(sort: &str) -> Markup {
    let known = SORTS.iter().any(|(value, _)| *value == sort);
    html! {
        label class="flex shrink-0 items-center" {
            span class="sr-only" { "Sort" }
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
            @if !here.category.is_empty() {
                (tag_filters(listing, here))
            }
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
                    @if !here.tags.is_empty() && here.category.is_empty() {
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
                    // A category's chosen tags stay, each removed from its
                    // filters.
                    @let kept: &[String] = if here.category.is_empty() { &[] } else { here.tags };
                    a href=(Home { query: "", tags: kept, ..here }.href()) class="px-1.5 text-meta" { "Clear" }
                }
            }
            @if total == 0 {
                p class="text-muted" { "No builds yet." }
            } @else if shown.is_empty() {
                p class="max-w-prose text-muted" {
                    "No builds match. " (search_help()) " Remove a term to widen the search."
                }
            } @else if view == View::Cards {
                (cards(shown, terms, listing.viewing))
            } @else {
                (table(shown, terms))
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

/// A listed build's tags on one row, but those its card makes plain:
/// any its name says, such as its design, its orientation, and its
/// carry in and out, which its picture shows. `fails simulation` comes
/// first, so a failing build stands out to be fixed or removed.
fn badges(s: &Schematic, class: &str) -> Markup {
    let tags = on_card(s);
    html! {
        @if !tags.is_empty() || !s.public {
            ul class={ "flex flex-wrap gap-1.5 " (class) } aria-label="Tags" {
                @if !s.public {
                    li { (super::private_badge()) }
                }
                @for tag in &tags {
                    li { (badge(tag, s)) }
                }
            }
        }
    }
}

/// The tags of build `s` its card shows: not its design or orientation,
/// nor a flag in [`SEEN`], nor any its name says in words, nor stackable
/// where a tag says how far apart its copies stack.
fn on_card(s: &Schematic) -> Vec<Tag> {
    let name = format!(" {} ", s.name().to_lowercase());
    let tags = s.every_tag();
    let pitched = tags.iter().any(|tag| tag.name.ends_with("_stackable"));
    tags.into_iter()
        .filter(|tag| {
            (!pitched || tag.name != "stackable")
                && !matches!(
                    tag.kind,
                    tag::Kind::Design | tag::Kind::Orientation
                )
                && tag.name != s.design
                && !SEEN.contains(&tag.name.as_str())
                && !name.contains(&format!(" {} ", tag.label()))
        })
        .collect()
}

/// Tags left off a card: flags its picture shows plainly, and synced,
/// which most builds are.
const SEEN: [&str; 5] =
    ["synced", "carry_in", "carry_out", "borrow_in", "borrow_out"];

/// A tag the builds listed carry, to narrow them by, with how many have
/// it, or one they are already narrowed by.
struct Filter {
    tag: Tag,
    count: usize,
    /// The tag filter it is, as the address gives it, when chosen.
    chosen: Option<usize>,
}

/// The tags to narrow a category's builds by: each the builds `shown`
/// carry that some but not all of them do, and each already chosen, by
/// kind, found before given, then most carried first.
fn filters(shown: &[Schematic], chosen: &[String]) -> Vec<Filter> {
    let mut filters: Vec<Filter> = Vec::new();
    for s in shown {
        for tag in s.every_tag() {
            match filters.iter_mut().find(|f| f.tag.name == tag.name) {
                Some(filter) => {
                    filter.count += 1;
                    // Found on any build, it is the site's to filter by.
                    if tag.is_found() && !filter.tag.is_found() {
                        filter.tag = tag;
                    }
                }
                None => filters.push(Filter {
                    tag,
                    count: 1,
                    chosen: None,
                }),
            }
        }
    }
    let mut unmatched = Vec::new();
    for (i, group) in chosen.iter().enumerate() {
        let wanted = tag::name(group);
        match filters.iter_mut().find(|f| f.tag.is_named(&wanted)) {
            Some(filter) => filter.chosen = Some(i),
            // Such as two tags at once, or one no build has.
            None => unmatched.push(Filter {
                tag: Tag {
                    name: wanted,
                    token: None,
                    kind: tag::Kind::Other,
                },
                count: 0,
                chosen: Some(i),
            }),
        }
    }
    filters.retain(|f| f.chosen.is_some() || f.count < shown.len());
    filters.extend(unmatched);
    let rank = |f: &Filter| {
        let kind = match f.tag.kind {
            tag::Kind::Failing | tag::Kind::NonStandard => 0,
            tag::Kind::Design => 1,
            tag::Kind::Orientation => 2,
            tag::Kind::Feature => 3,
            tag::Kind::Restriction => 4,
            tag::Kind::Other => 5,
        };
        (!f.tag.is_found(), kind, std::cmp::Reverse(f.count))
    };
    filters.sort_by_key(rank);
    filters
}

/// Under a category's title, its tags to narrow it by, each with how many
/// of its builds carry it, and those chosen marked, each to remove.
fn tag_filters(listing: &Listing, here: Home) -> Markup {
    let filters = filters(listing.shown, here.tags);
    html! {
        @if !filters.is_empty() {
            ul aria-label="Filter by tag" class="mb-6 flex flex-wrap gap-1.5" {
                @for filter in &filters {
                    @let label = filter.tag.label().replace(',', " or ");
                    li {
                        @if let Some(i) = filter.chosen {
                            @let rest: Vec<String> = here.tags.iter().enumerate()
                                .filter(|(j, _)| *j != i).map(|(_, tag)| tag.clone()).collect();
                            a href=(Home { tags: &rest, ..here }.href())
                                aria-label={ "Remove the tag filter " (label) }
                                title=(filter.tag.about())
                                class={ "badge " (filter.tag.class()) " gap-1.5 pr-2 ring-2 ring-brand ring-offset-1 ring-offset-canvas hover:no-underline" } {
                                (label)
                                span aria-hidden="true" { "×" }
                            }
                        } @else {
                            @let more: Vec<String> = here.tags.iter().cloned()
                                .chain([filter.tag.name.clone()]).collect();
                            a href=(Home { tags: &more, ..here }.href())
                                aria-label={ "Only builds tagged " (label) ", " (filter.count) }
                                title=(filter.tag.about())
                                class={ "badge " (filter.tag.class()) " gap-1.5 hover:no-underline hover:brightness-95" } {
                                (label)
                                span class="font-normal tabular-nums" { (filter.count) }
                            }
                        }
                    }
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
/// it only by stacking, as the viewer sees it.
pub(super) fn cards(
    schematics: &[Schematic],
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
const CARDS: &str = "grid grid-cols-[repeat(auto-fill,minmax(min(16rem,calc(50%_-_0.375rem)),1fr))] gap-3 sm:gap-5";

/// The picture of `s`, at an address that changes with its file and with
/// the viewer's `palette`.
fn thumbnail_src(s: &Schematic, palette: Option<&str>) -> String {
    let palette = palette.map_or(String::new(), |key| format!("&p={key}"));
    format!(
        "/schematics/{}/thumbnail.webp?v={}-{}{palette}",
        s.id,
        &s.sha256[..12.min(s.sha256.len())],
        crate::thumbnail::RENDERER
    )
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

/// Build `s` as a card, with `note` under its tags, pictured in the
/// viewer's palette, with a star when they have favourited it.
fn card(s: &Schematic, note: &Markup, viewing: &Viewing) -> Markup {
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
                                a href={ "/schematics/" (s.id) }
                                    class="text-ink group-hover:text-brand hover:no-underline after:absolute after:inset-0" { (s.name()) }
                            }
                            // Latency is what builds are compared by, so it
                            // leads.
                            p class="flex flex-wrap items-baseline gap-x-2 tabular-nums" {
                                @if s.latency.is_some() {
                                    span class="font-semibold" { (latency(s)) }
                                }
                                span class="text-meta text-muted" { (size(s)) " · " (volume(s)) }
                            }
                        }
                        (note)
                        (badges(s, ""))
                        // Its builders, then who helped, on one line; the
                        // build's page names them all.
                        @if !s.builders.is_empty() || !s.contributors.is_empty() {
                            p class="mt-auto truncate pt-1 text-meta text-muted" {
                                (name_links(&s.builders))
                                @if !s.contributors.is_empty() {
                                    @if !s.builders.is_empty() { " · " }
                                    "with " (name_links(&s.contributors))
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

/// Builds as table rows, for the home page's List layout.
fn table(schematics: &[Schematic], terms: &[Term]) -> Markup {
    // Columns a phone has no room for; the name and its tags come first.
    let secondary = "hidden desktop:table-cell";
    html! {
                div class="relative overflow-x-auto" {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{catalogue::Flag, schematic::Orientation};

    fn adder(design: &str, flags: &[&str], tags: &[&str]) -> Schematic {
        let mut s = crate::search::tests::schematic(
            "adder",
            Orientation::Horizontal,
            tags,
        );
        s.design = design.to_owned();
        s.design_abbreviation = Some("CCA".to_owned());
        s.flags = flags
            .iter()
            .map(|&name| Flag {
                category: "adder".to_owned(),
                name: name.to_owned(),
                label: name.to_owned(),
            })
            .collect();
        s
    }

    /// A card leaves out what its picture or name makes plain, or says
    /// little, such as an inverted carry in, and keeps what is hard to
    /// tell, such as how its copies stack.
    #[test]
    fn a_card_shows_only_the_tags_hard_to_tell() {
        let mut s = adder(
            "carry_cancel",
            &["carry_in", "carry_out", "inverted_carry_in"],
            &["pistons"],
        );
        s.stacking = vec![crate::models::schematic::StackDirection {
            west: 2,
            up: 0,
            north: 0,
            max_copies: None,
            faces: None,
        }];
        let names = |s: &Schematic| -> Vec<String> {
            on_card(s).into_iter().map(|tag| tag.name).collect()
        };
        // How far apart its copies stack says it stacks.
        assert_eq!(names(&s), ["2-wide_stackable", "pistons"]);
        let mut gate = adder("other", &["xnor"], &[]);
        gate.category = "xor_xnor_gate".to_owned();
        assert!(gate.name().contains("XNOR"), "{}", gate.name());
        assert!(names(&gate).is_empty());
    }

    fn shown(filters: &[Filter]) -> Vec<(&str, usize, Option<usize>)> {
        filters
            .iter()
            .map(|f| (f.tag.name.as_str(), f.count, f.chosen))
            .collect()
    }

    /// A category's page is narrowed by the tags some but not all of its
    /// builds carry, found before given; one chosen stays, however many
    /// carry it, matched by its short form too.
    #[test]
    fn a_category_is_narrowed_by_the_tags_some_of_its_builds_carry() {
        let builds = [
            adder("carry_cancel", &["carry_in"], &["pistons"]),
            adder("ripple_carry", &["carry_in", "carry_out"], &[]),
            adder("ripple_carry", &["carry_in"], &[]),
        ];
        assert_eq!(
            shown(&filters(&builds, &[])),
            [
                ("ripple_carry", 2, None),
                ("carry_cancel", 1, None),
                ("carry_out", 1, None),
                ("pistons", 1, None),
            ]
        );
        let chosen = ["cca".to_owned(), "two,three".to_owned()];
        assert_eq!(
            shown(&filters(&builds[..1], &chosen)),
            [("carry_cancel", 1, Some(0)), ("two,three", 0, Some(1))]
        );
    }
}
