//! Lists of builds, as cards or a table, and the home page that searches
//! and narrows them, by its search and its menu of categories.

use maud::{Markup, html};

use crate::{
    APP_NAME,
    account::User,
    build::{
        Build,
        tag::{self, Tag},
    },
    category::Section,
    interface::Encoding,
    search::Term,
    web::{
        browse::query::{HomeQuery, View},
        href,
        ui::{
            PAGE_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, Tone,
            cards::{
                CARDS, Viewing, badges, card, cards, stacked_note,
                thumbnail_src,
            },
            icons::{self, icon},
            layout::base,
            person::name_links,
            summary::{latency, size, volume},
        },
    },
};

/// The orders the home page offers, as `sort` values and their labels. Any
/// other order of the fields in [`Sort`] works in the address too.
const SORTS: [(&str, &str); 4] = [
    ("", "Newest"),
    ("volume", "Smallest"),
    ("latency", "Fastest"),
    ("-downloads", "Popular"),
];

/// The home page's schematics and how they were asked for.
pub struct Listing<'a> {
    /// The schematics matching the search and the chosen category.
    pub shown: &'a [Build],
    /// How many schematics there are in all.
    pub total: usize,
    /// What its address asked for.
    pub query: &'a HomeQuery,
    pub terms: &'a [Term],
    /// How many of the builds found are not verified as their category,
    /// shown or not.
    pub unverified: usize,
    /// How many of the builds found are unaligned, verified, shown or not.
    pub unaligned: usize,
    pub facets: &'a [Facet],
    pub viewing: &'a Viewing,
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

impl Listing<'_> {
    /// Whether words are searched for, or tags outside a category, whose
    /// page shows its tags chosen among its filters.
    fn searching(&self) -> bool {
        !self.query.q.is_empty()
            || (!self.query.tags.is_empty() && self.query.category.is_empty())
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
pub(crate) fn home_page(
    viewer: Option<&User>,
    action: &str,
    listing: Option<&Listing>,
    below: &Markup,
) -> Markup {
    // Every build is the sidebar's Recent builds, and the index its Home.
    let current_path = match listing {
        None => "/",
        Some(listing) if !listing.query.narrows() => "/builds",
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
    let action = if listing.query.narrows() {
        "/"
    } else {
        "/builds"
    };
    home_page(viewer, action, Some(listing), &home_results(listing))
}

/// The home page's search, sent to `action`. It sends as the words are
/// typed, once typing pauses, and redraws the part below it; the layout
/// and tags `listing` chose go with it, and the category and order from
/// their menus in the results. In the box's end, a button opens the dialog
/// for finding the builds most like a file.
fn search_form(action: &str, listing: Option<&Listing>) -> Markup {
    let query = listing.map(|l| l.query).cloned().unwrap_or_default();
    let HomeQuery {
        view,
        q: query,
        tags,
        unverified,
        hide_unaligned,
        ..
    } = query;
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
            @if hide_unaligned {
                input type="hidden" name="unaligned" value="hide";
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
        "Each word narrows it: a category, tag, " code { "8b" } ", " code { "3t" } ", an author, "
        code { "stackable" } ". " code { "-" } " before a word leaves it out."
    }
}

/// The part of the home page a search redraws: the title, the terms
/// searched for, and the builds found.
pub fn home_results(listing: &Listing) -> Markup {
    let here = listing.query;
    let title = listing
        .facets
        .iter()
        .find(|facet| facet.name == here.category)
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

/// The categories to narrow a page to, as a menu named `category`: first
/// `all`, the label of the choice of every category, then each
/// category with its count, under its section, and the rest; `current` is
/// the chosen one. With `form`, it belongs to that form, wherever it is.
pub(crate) fn category_menu(
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
fn listing_header(listing: &Listing, here: &HomeQuery, title: &str) -> Markup {
    let (shown, view, category) = (listing.shown, here.view, &here.category);
    let layout = |to: View, label: &str, glyph: &str| {
        html! {
            a href=(HomeQuery { view: to, ..here.clone() }.href()) aria-label=(label) title=(label)
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
                (sort_menu(&here.sort.name()))
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
                (switch(
                    listing.unaligned,
                    here.hide_unaligned,
                    "Hide unaligned",
                    &HomeQuery {
                        hide_unaligned: !here.hide_unaligned,
                        ..here.clone()
                    },
                ))
                (switch(
                    listing.unverified,
                    here.unverified,
                    "Show failing and untested",
                    &HomeQuery { unverified: !here.unverified, ..here.clone() },
                ))
            }
    }
}

/// A switch for some builds found, `count` of them, ticked when `on`, as
/// a link going to `other`, the listing with it turned the other way,
/// saying `label` and how many there are; nothing when there are none.
/// Builds not verified as their category show when one is on, and
/// unaligned builds hide when another is.
fn switch(count: usize, on: bool, label: &str, other: &HomeQuery) -> Markup {
    html! {
        @if count > 0 {
            a href=(other.href())
                aria-pressed=(on)
                class="flex min-h-9 items-center gap-2 self-end text-meta text-muted hover:text-ink hover:no-underline" {
                span class={ "flex size-4 items-center justify-center rounded-[4px] border "
                    (if on { "border-brand bg-brand text-on-brand" } else { "border-boundary" }) } {
                    @if on { (icon(icons::CHECK, "size-3")) }
                }
                span { (label) " (" span class="tabular-nums" { (count) } ")" }
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
fn listing_body(listing: &Listing, here: &HomeQuery, title: &str) -> Markup {
    let Listing {
        shown,
        total,
        terms,
        ..
    } = *listing;
    let view = here.view;
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
                        (removable(
                            &HomeQuery { category: String::new(), ..here.clone() }.href(),
                            &format!("Search every category, not only {title}"),
                            PLAIN_CHIP,
                            &format!("in {title}"),
                        ))
                    }
                    @if !terms.is_empty() {
                        ul class="contents" aria-label="Search terms" {
                            @for (i, term) in terms.iter().enumerate() {
                                li {
                                    (removable(
                                        &HomeQuery { q: without(terms, i), ..here.clone() }.href(),
                                        &format!("Remove {}", term.display()),
                                        if term.negated { Tone::Bad.class() } else { PLAIN_CHIP },
                                        &format!("{}{}", if term.negated { "not " } else { "" }, term.text),
                                    ))
                                }
                            }
                        }
                    }
                    @if !here.tags.is_empty() && here.category.is_empty() {
                        ul class="contents" aria-label="Tag filters" {
                            @for (i, group) in here.tags.iter().enumerate() {
                                @let shown = group.replace('_', " ").replace(',', " or ");
                                li {
                                    (removable(
                                        &here.without_tag(i).href(),
                                        &format!("Remove the tag filter {shown}"),
                                        PLAIN_CHIP,
                                        &format!("tag: {shown}"),
                                    ))
                                }
                            }
                        }
                    }
                    // A category's chosen tags stay, each removed from its
                    // filters.
                    @let kept = if here.category.is_empty() { Vec::new() } else { here.tags.clone() };
                    a href=(HomeQuery { q: String::new(), tags: kept, ..here.clone() }.href()) class="px-1.5 text-meta" { "Clear" }
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

/// The colours of a chip that is neither a tag nor a warning.
const PLAIN_CHIP: &str = "bg-selection text-ink";

/// A chip naming one thing the search is narrowed by, `words`, in the
/// colours of `tone`, which takes it off the search at `href`; `removes`
/// says so to a screen reader.
fn removable(href: &str, removes: &str, tone: &str, words: &str) -> Markup {
    html! {
        a href=(href) aria-label=(removes)
            class={ "badge gap-1.5 pr-2 hover:no-underline " (tone) } {
            (words)
            span aria-hidden="true" { "×" }
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
fn filters(shown: &[Build], chosen: &[String]) -> Vec<Filter> {
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
            tag::Kind::Failing | tag::Kind::Untested => 0,
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
fn tag_filters(listing: &Listing, here: &HomeQuery) -> Markup {
    let filters = filters(listing.shown, &here.tags);
    html! {
        @if !filters.is_empty() {
            ul aria-label="Filter by tag" class="mb-6 flex flex-wrap gap-1.5" {
                @for filter in &filters {
                    @let label = filter.tag.label().replace(',', " or ");
                    li {
                        @if let Some(i) = filter.chosen {
                            a href=(here.without_tag(i).href())
                                aria-label={ "Remove the tag filter " (label) }
                                title=(filter.tag.about())
                                class={ "badge " (filter.tag.class()) " gap-1.5 pr-2 ring-2 ring-brand ring-offset-1 ring-offset-canvas hover:no-underline" } {
                                (label)
                                span aria-hidden="true" { "×" }
                            }
                        } @else {
                            @let more: Vec<String> = here.tags.iter().cloned()
                                .chain([filter.tag.name.clone()]).collect();
                            a href=(HomeQuery { tags: more, ..here.clone() }.href())
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

/// Builds as table rows, for the home page's List layout.
fn table(schematics: &[Build], terms: &[Term]) -> Markup {
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
                                th scope="col" class=(TABLE_HEAD) { "Authors" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Built" }
                            }
                        }
                        tbody {
                            @for s in schematics {
                                tr class="hover:bg-selection/50" {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                        a href=(href::build(&s.id)) class="font-semibold" { (s.name()) }
                                        (stacked_note(s, terms))
                                        (badges(s, "mt-1.5"))
                                    }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.specs.orientation.label()) }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.specs.encoding.map_or("", Encoding::label)) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (latency(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (size(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (volume(s)) }
                                    td class=(TABLE_CELL) { (name_links(&s.credits.authors)) }
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
    use crate::{category::Flag, interface::Orientation};

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
            adder(Some("carry_cancel"), &["carry_in"], &["pistons"]),
            adder(Some("ripple_carry"), &["carry_in", "carry_out"], &[]),
            adder(Some("ripple_carry"), &["carry_in"], &[]),
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
