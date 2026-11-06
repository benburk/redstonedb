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
    category::{Flag, Section},
    models::{
        schematic::{Encoding, Schematic},
        tag::{self, Tag},
        user::User,
    },
    search::Term,
};

/// How the home page lays out its schematics: cards, or `view=list` in its
/// address for a table.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum View {
    #[default]
    Cards,
    List,
}

/// A field the home page can order builds by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortField {
    AddedAt,
    Volume,
    Latency,
    Downloads,
}

impl SortField {
    const ALL: [SortField; 4] = [
        SortField::AddedAt,
        SortField::Volume,
        SortField::Latency,
        SortField::Downloads,
    ];

    /// As an address names it.
    fn name(self) -> &'static str {
        match self {
            SortField::AddedAt => "added_at",
            SortField::Volume => "volume",
            SortField::Latency => "latency",
            SortField::Downloads => "downloads",
        }
    }
}

/// The order the home page lists builds in: fields, each ascending or
/// descending, the first deciding and each after it breaking the ties
/// before. Its address writes it as `sort=-downloads,volume`, `-` for
/// descending. With no fields, or on a tie, builds go newest first.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct Sort(pub Vec<(SortField, bool)>);

impl Sort {
    /// `text` as an address gives it, leaving out any field it does not
    /// know.
    pub fn parse(text: &str) -> Self {
        Sort(
            text.split(',')
                .map(str::trim)
                .filter_map(|key| {
                    let (name, descending) = key
                        .strip_prefix('-')
                        .map_or((key, false), |name| (name, true));
                    let field = SortField::ALL
                        .into_iter()
                        .find(|field| field.name() == name)?;
                    Some((field, descending))
                })
                .collect(),
        )
    }

    /// As an address writes it, such as `-downloads,volume`.
    pub fn name(&self) -> String {
        self.0
            .iter()
            .map(|&(field, descending)| {
                if descending {
                    format!("-{}", field.name())
                } else {
                    field.name().to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

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
    pub shown: &'a [Schematic],
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

/// What a viewer's cards show of them: their palette's key, for the
/// pictures, if they chose one, and, once signed in, a star to favourite
/// each build with, filled on those they have.
#[derive(Default)]
pub struct Viewing {
    pub palette: Option<String>,
    pub signed_in: bool,
    pub favorites: HashSet<String>,
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

/// What the home page's address asks for, and where its links lead.
/// Separate parameters narrow together; within one, commas mean any of
/// them, as in `?category=adder,subtractor`. `tag` may repeat, each needing
/// a match: `?tag=alpha&tag=torchless` is both, and `?tag=alpha,torchless`
/// either.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct HomeQuery {
    pub view: View,
    pub sort: Sort,
    /// A search such as `register vertical alpha -torchless`.
    pub q: String,
    /// The chosen category, such as `adder`; empty for all.
    pub category: String,
    /// Tag filters, each needing a match; commas within one give choices.
    pub tags: Vec<String>,
    /// Whether builds not verified as their category are listed too.
    pub unverified: bool,
    /// Whether unaligned builds, whose buses aren't laid as the
    /// conventions lay them, are left out; verified, they list as any
    /// other unless hidden.
    pub hide_unaligned: bool,
}

impl HomeQuery {
    /// The query string `raw`, without its `?`. Asking for the unverified
    /// by their tag shows them.
    pub fn parse(raw: &str) -> Self {
        let mut query = HomeQuery::default();
        for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
            let value = value.trim().to_owned();
            match key.as_ref() {
                "view" => {
                    query.view = if value == "list" {
                        View::List
                    } else {
                        View::Cards
                    };
                }
                "q" => query.q = value,
                "category" => query.category = value,
                "sort" => query.sort = Sort::parse(&value),
                "tag"
                    if [tag::FAILING, tag::UNTESTED]
                        .contains(&tag::name(&value).as_str()) =>
                {
                    query.unverified = true;
                    query.tags.push(value);
                }
                "tag" if !value.is_empty() => query.tags.push(value),
                "unverified" => query.unverified = value == "show",
                "unaligned" => query.hide_unaligned = value == "hide",
                _ => {}
            }
        }
        query
    }

    /// Whether it searches or chooses anything. With nothing to narrow it,
    /// the home page shows its index, so every build has an address of its
    /// own, `/builds`.
    pub fn narrows(&self) -> bool {
        !self.q.is_empty() || !self.category.is_empty() || !self.tags.is_empty()
    }

    /// Its address, such as `/?category=adder&q=8b`, as
    /// [`HomeQuery::parse`] reads it.
    pub fn href(&self) -> String {
        let mut params = form_urlencoded::Serializer::new(String::new());
        if self.view == View::List {
            params.append_pair("view", "list");
        }
        let sort = self.sort.name();
        if !sort.is_empty() {
            params.append_pair("sort", &sort);
        }
        if !self.category.is_empty() {
            params.append_pair("category", &self.category);
        }
        for tag in &self.tags {
            params.append_pair("tag", tag);
        }
        if self.unverified {
            params.append_pair("unverified", "show");
        }
        if self.hide_unaligned {
            params.append_pair("unaligned", "hide");
        }
        if !self.q.is_empty() {
            params.append_pair("q", &self.q);
        }
        let path = if self.narrows() { "/" } else { "/builds" };
        let params = params.finish();
        if params.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{params}")
        }
    }
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
pub(super) fn home_page(
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
                        a href=(HomeQuery { category: String::new(), ..here.clone() }.href())
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
                                    a href=(HomeQuery { q: without(terms, i), ..here.clone() }.href())
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
                                    a href=(HomeQuery { tags: rest, ..here.clone() }.href())
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
/// nor a flag in [`SEEN`], nor any its name says in words, nor one another
/// tag implies, see [`tag::implied`].
fn on_card(s: &Schematic) -> Vec<Tag> {
    let name = format!(" {} ", s.name().to_lowercase());
    let tags = s.every_tag();
    tags.iter()
        .filter(|tag| {
            !tag::implied(&tag.name, &tags)
                && !matches!(
                    tag.kind,
                    tag::Kind::Design | tag::Kind::Orientation
                )
                && s.design.as_deref() != Some(tag.name.as_str())
                && !SEEN.contains(&tag.name.as_str())
                && !name.contains(&format!(" {} ", tag.label()))
        })
        .cloned()
        .collect()
}

/// Tags left off a card: synced, which most builds are, and the flags its
/// picture shows plainly.
const SEEN: [&str; 5] = [
    tag::SYNCED,
    Flag::CARRY_IN,
    Flag::CARRY_OUT,
    Flag::BORROW_IN,
    Flag::BORROW_OUT,
];

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
                            @let rest: Vec<String> = here.tags.iter().enumerate()
                                .filter(|(j, _)| *j != i).map(|(_, tag)| tag.clone()).collect();
                            a href=(HomeQuery { tags: rest, ..here.clone() }.href())
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

/// The picture of `s`, at an address that changes with its file, with
/// the viewer's `palette`, and as it comes to be drawn in a theme's
/// building blocks, once recognized, as a build marked as following the
/// conventions later may be, its file the same.
pub(super) fn thumbnail_src(s: &Schematic, palette: Option<&str>) -> String {
    let palette = palette.map_or(String::new(), |key| format!("&p={key}"));
    let themed = if s.evaluated() && s.verified() {
        "-b"
    } else {
        ""
    };
    format!(
        "/schematics/{}/thumbnail.webp?v={}-{}{themed}{palette}",
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
                                @if s.favorites > 0 {
                                    @let label = super::favourites_label(s.favorites);
                                    span class="ml-auto inline-flex items-center gap-1 self-center text-meta text-muted" title=(label) aria-label=(label) {
                                        (icon(icons::STAR, "size-3.5")) (s.favorites)
                                    }
                                }
                            }
                        }
                        (note)
                        (badges(s, ""))
                        // Its authors, then who helped, on one line; the
                        // build's page names them all.
                        @if !s.authors.is_empty() || !s.contributors.is_empty() {
                            p class="mt-auto truncate pt-1 text-meta text-muted" {
                                (name_links(&s.authors))
                                @if !s.contributors.is_empty() {
                                    @if !s.authors.is_empty() { " · " }
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
                                th scope="col" class=(TABLE_HEAD) { "Authors" }
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
                                    td class=(TABLE_CELL) { (name_links(&s.authors)) }
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
    use crate::{category::Flag, models::schematic::Orientation};

    /// An address reads back as it was written, but for a sort field the
    /// site does not know, which is left out.
    #[test]
    fn a_home_address_reads_back_as_written() {
        for href in [
            "/builds",
            "/builds?view=list&sort=volume",
            "/?category=adder",
            "/?view=list&sort=-downloads%2Cvolume&category=adder&tag=alpha%2Ctorchless&tag=synced&unverified=show&q=8b+adder",
        ] {
            let raw = href.split_once('?').map_or("", |(_, raw)| raw);
            assert_eq!(HomeQuery::parse(raw).href(), href);
        }
        // Unaligned builds list unless the switch hides them.
        let hidden = HomeQuery::parse("category=adder&unaligned=hide");
        assert!(hidden.hide_unaligned);
        assert_eq!(hidden.href(), "/?category=adder&unaligned=hide");
        assert!(!HomeQuery::parse("category=adder").hide_unaligned);
        let query = HomeQuery::parse("q=add&sort=bogus,-latency");
        assert_eq!(query.href(), "/?sort=-latency&q=add");
        assert!(!HomeQuery::parse("view=list").narrows());
    }

    fn adder(
        design: Option<&str>,
        flags: &[&'static str],
        tags: &[&str],
    ) -> Schematic {
        let mut s = crate::search::tests::schematic(
            "adder",
            Orientation::Horizontal,
            tags,
        );
        s.design = design.map(str::to_owned);
        s.design_abbreviation = Some("CCA".to_owned());
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
        s.stacking = vec![crate::models::schematic::StackDirection {
            west: 2,
            up: 0,
            max_copies: None,
            faces: None,
            pitch: Some(2),
        }];
        let names = |s: &Schematic| -> Vec<String> {
            on_card(s).into_iter().map(|tag| tag.name).collect()
        };
        // How far apart its copies stack says it stacks.
        assert_eq!(names(&s), ["2-wide_stackable", "pistons"]);
        let mut gate = adder(None, &[], &[]);
        gate.category = "xnor_gate".to_owned();
        gate.category_label = Some("XNOR Gate".to_owned());
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
