//! Every page's markup. This file holds the home page's listing and the
//! pieces several pages share, such as cards, badges, and the 3D viewer;
//! each other page, or part of one, is in its own module below.

use maud::{Markup, PreEscaped, html};

use crate::{
    APP_NAME,
    models::{
        catalogue::{Category, Section},
        schematic::{Encoding, Schematic},
        tag::Tag,
        user::User,
    },
    search::Term,
};

const PAGE_TITLE: &str = "text-page font-bold";
const SECTION_TITLE: &str = "text-section font-semibold";
const TABLE_HEAD: &str = "border-b border-line py-2 pr-4 text-left text-meta font-semibold text-muted";
const TABLE_CELL: &str = "border-b border-line py-2.5 pr-4 align-top";
/// One link of a segmented control, such as Cards and List; the chosen one
/// carries `aria-current`.
const SWITCH: &str = "rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink hover:no-underline aria-[current=page]:bg-raised aria-[current=page]:font-semibold aria-[current=page]:text-ink aria-[current=page]:shadow-sm";
/// The bar holding a segmented control's links.
const SWITCHES: &str =
    "flex flex-wrap items-center gap-1 rounded-control bg-sidebar p-1";

mod about;
mod build;
mod details_page;
mod form;
mod interface;
mod layout;
mod leaderboard;
mod player;
mod schemati;
mod similar;
mod simulation;
mod stack_guide;
mod upload;
mod users;
mod version;

pub use about::{about, sorting};
pub use build::{FileRead, build_banner, build_info, schematic_page};
pub use details_page::{DetailsPage, File, details_page};
pub use form::Choices;
use layout::base;
pub use layout::{forbidden, not_found};
pub use leaderboard::{LeaderboardPage, PeoplePage, leaderboard, people};
pub use player::{PlayerPage, player};
pub use schemati::{Connection, schemati, schemati_folder, schemati_sent};
pub use similar::{Found, similar_results, similar_search};
pub use simulation::{
    Waiting, simulation_results, simulation_section, timing_section,
};
pub use upload::upload;
pub use users::{ProfileBuilds, login, profile, user_row, users};
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
fn category_sidebar(
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
fn category_tabs(category: &str, leaderboard: bool) -> Markup {
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
fn cards(schematics: &[Schematic], terms: &[Term]) -> Markup {
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

/// The few facts a card has room for, such as `8 ticks · 11×19×18 ·
/// 3762 blocks · embizone`.
fn facts(s: &Schematic) -> Markup {
    let measures: Vec<String> = [
        s.latency.map(|t| format!("{t} ticks")),
        Some(size(s)),
        Some(volume(s)),
    ]
    .into_iter()
    .flatten()
    .collect();
    html! {
        (measures.join(" · "))
        @if !s.builders.is_empty() {
            " · " (name_links(&s.builders))
        }
    }
}

/// The home page searching for `term`, such as a tag or a builder.
fn search_href(term: &str) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("q", term)
        .finish();
    format!("/?{query}")
}

/// `text` linking to the builds matching `term`. It sits above a card's
/// own link.
fn search_link(text: &str, term: &str) -> Markup {
    html! {
        a href=(search_href(term)) class="relative z-10" { (text) }
    }
}

/// In-game names, each a link to its player's page, which lists what
/// they built and helped with. The links sit above a card's own link.
fn name_links(names: &[String]) -> Markup {
    html! {
        @for (i, name) in names.iter().enumerate() {
            @if i > 0 { ", " }
            a href=(player_href(name)) class="relative z-10" { (name) }
        }
    }
}

/// The page of the Minecraft player `name`.
/// A person as a badge: their Minecraft head, blank until it loads or when
/// there is none, and their name, linking to `href`.
fn person_badge(name: &str, href: &str) -> Markup {
    html! {
        a href=(href)
            class="relative z-10 inline-flex items-center gap-1.5 rounded-full border border-line bg-raised py-0.5 pr-2.5 pl-1 text-meta text-ink hover:bg-selection hover:no-underline" {
            img src={ "/players/" (name) "/head.png" } alt="" width="18" height="18" loading="lazy"
                class="size-[18px] rounded-[4px] bg-line [image-rendering:pixelated]";
            span { (name) }
        }
    }
}

/// Each of `names` as a badge linking to their player page.
fn name_badges(names: &[String]) -> Markup {
    html! {
        span class="flex flex-wrap gap-1.5" {
            @for name in names {
                (person_badge(name, &player_href(name)))
            }
        }
    }
}

fn player_href(name: &str) -> String {
    format!("/players/{name}")
}

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

/// A small spinning ring, for something under way; still where motion is
/// reduced.
pub fn spinner() -> Markup {
    html! {
        span aria-hidden="true"
            class="inline-block size-4 shrink-0 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
    }
}

/// `n` with its thousands separated by commas, such as `131,072`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Tenths of a percent as a whole percent, such as `98%`.
fn percent(permille: u32) -> String {
    format!("{}%", permille / 10)
}

/// The standard `categories` as the sidebar lists them: each section, by
/// its key and heading, with its categories, then any in none under Other.
/// A section with no categories is left out.
fn sections(categories: &[Category]) -> Vec<(&str, &str, Vec<&Category>)> {
    let of = |section: Option<Section>| -> Vec<&Category> {
        categories.iter().filter(|c| c.section == section).collect()
    };
    Section::ALL
        .iter()
        .map(|&section| (section.name(), section.label(), of(Some(section))))
        .chain([("other", "Other", of(None))])
        .filter(|(_, _, members)| !members.is_empty())
        .collect()
}

/// `Builder` for one name, else `Builders`.
fn plural(singular: &str, names: &[String]) -> String {
    if names.len() == 1 {
        singular.to_owned()
    } else {
        format!("{singular}s")
    }
}

/// A tag, linking to the builds that carry it.
fn badge(tag: &Tag) -> Markup {
    html! {
        a href={ "/?" (form_urlencoded::Serializer::new(String::new()).append_pair("tag", &tag.name).finish()) }
            title=(tag.kind.label())
            class={ "badge badge-" (tag.kind.name()) " relative z-10 hover:no-underline hover:brightness-95" } {
            (tag.label())
        }
    }
}

fn field(name: &str, value: impl maud::Render) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}

fn latency(s: &Schematic) -> String {
    s.latency.map(|t| format!("{t} ticks")).unwrap_or_default()
}

/// The blocks a build takes as it is compared, said per copy for a
/// stackable build, such as `48 blocks per copy`.
fn volume(s: &Schematic) -> String {
    let per = if s.per_copy().is_some() {
        " per copy"
    } else {
        ""
    };
    format!("{} blocks{per}", s.volume())
}

/// Height × width × depth, as the build looks from the south.
fn size(s: &Schematic) -> String {
    let (x, y, z) = s.size;
    format!("{y}×{x}×{z}")
}

/// The layout of a build's page and of an upload's preview: `side`, its
/// facts or form, on the left, and the 3D view of the file at `file` on
/// the right, staying in sight while the side scrolls. On a phone the view
/// comes first.
fn beside_viewer(file: &str, side: &Markup) -> Markup {
    html! {
        div class="grid items-start gap-8 desktop:grid-cols-[var(--container-form)_minmax(0,1fr)]" {
            div class="desktop:sticky desktop:top-4 desktop:order-last" {
                (viewer_3d(file))
            }
            div class="flex flex-col gap-8" { (side) }
        }
    }
}

/// Where `static/viewer.js` finds the renderer's own imports.
const VIEWER_IMPORTS: &str = r#"{ "imports": {
  "three": "https://cdn.jsdelivr.net/npm/three@0.184.0/build/three.module.js",
  "nucleation": "https://cdn.jsdelivr.net/npm/nucleation@0.2.18/nucleation.js"
} }"#;

/// A 3D view of the schematic at `file`, an address the browser can fetch,
/// drawn by `static/viewer.js`.
fn viewer_3d(file: &str) -> Markup {
    html! {
        script type="importmap" { (PreEscaped(VIEWER_IMPORTS)) }
        // The renderer sizes its canvas to the parent, so the parent's height
        // must be fixed or the two grow each other without bound.
        div class="relative h-[60vh] overflow-hidden rounded-overlay border border-line bg-sidebar desktop:h-[calc(100vh-12rem)]" {
            canvas id="viewer" data-src=(file) aria-label="3D view of the schematic"
                class="absolute inset-0 size-full" {}
            p id="viewer-status" class="absolute inset-0 flex items-center justify-center text-muted" {
                "Loading viewer…"
            }
        }
        script type="module" src=(crate::web::VIEWER_JS.url()) {}
    }
}
