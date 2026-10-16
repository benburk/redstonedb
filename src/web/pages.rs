//! Full page templates.
//!
//! These return complete HTML documents wrapped in the base layout.

use maud::{Markup, PreEscaped, html};

use crate::{
    APP_NAME, import,
    interface::{Bit, Interface, Port},
    lint::Violation,
    models::{
        schematic::{self, Category, Design, Flag, Measure, Schematic, Tag},
        user::{Role, User},
    },
    search::Term,
    simulate::{
        self, Glitch, InputTiming, OutputTiming, Report, Run, Stacking, Takes,
        Timing,
    },
};

use super::{
    components::base,
    details::{STACK_DIRECTIONS, StackRow},
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

mod leaderboard;
pub use leaderboard::{LeaderboardPage, PeoplePage, leaderboard, people};
mod player;
pub use player::{PlayerPage, player};
mod details_page;
mod sorting;
pub use sorting::sorting;
mod build;
pub use build::{build_banner, build_info, schematic_page};
mod schemati;
mod similar;
pub use similar::{Found, similar_results, similar_search};
mod stack_guide;
pub use schemati::{Connection, schemati, schemati_folder, schemati_sent};
mod version;
pub use details_page::{DetailsPage, File, Stored, details_page};

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
    /// Its sidebar section, one of [`schematic::SECTIONS`]; `None` for
    /// Other.
    pub section: Option<String>,
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
    let in_section = |section: Option<&str>| -> Vec<&Facet> {
        facets
            .iter()
            .filter(|f| f.section.as_deref() == section)
            .collect()
    };
    let rest = in_section(None);
    html! {
        // Its own scroller on a wide screen, since it stays in view and
        // may be taller than the window: sidebar.js fits it to the window
        // below its top, and without it, it fits below the header.
        nav aria-labelledby="categories-heading" data-sidebar
            class="desktop:sticky desktop:top-4 desktop:max-h-[calc(100vh-6.5rem)] desktop:self-start desktop:overflow-y-auto" {
            script defer src=(super::sidebar_js_url()) {}
            h2 id="categories-heading" class="mb-2 px-2.5 text-meta font-semibold text-muted" { "Categories" }
            @if let Some((label, all_href, count)) = all {
                ul class=(list) { (link(all_href, label, count, current.is_empty())) }
            }
            @for (name, heading) in schematic::SECTIONS {
                @let members = in_section(Some(name));
                @if !members.is_empty() {
                    h3 id={ "section-" (name) } class="mt-5 mb-1 px-2.5 text-section font-semibold" { (heading) }
                    ul aria-labelledby={ "section-" (name) } class=(list) {
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
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.orientation_label()) }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.encoding_label().unwrap_or_default()) }
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

/// What the stored file says about a build.
pub struct Analysis {
    pub interface: Interface,
    pub violations: Vec<Violation>,
    /// How a stackable build's stacking looks, each way it stacks.
    pub stacking: Vec<crate::stack::Review>,
}

/// Whether the simulation found how build `s` stacks, rather than its
/// builder giving it: an adder's always, and a register's when its copies
/// stored and read right.
fn stacking_found(s: &Schematic, report: Option<&Report>) -> bool {
    let category = s.category.as_str();
    let found = report.is_some_and(|report| {
        matches!(report, Report::Ran(run) if matches!(
            run.stacking,
            Some(
                Stacking::Stacks { .. }
                    | Stacking::Registers { .. }
                    | Stacking::Wider { .. }
            )
        ))
    });
    s.conventional
        && (simulate::INFERS_STACKING.contains(&category)
            || (simulate::FINDS_STACKING.contains(&category) && found))
}

/// The details form's simulation result, a live panel the form redraws
/// whenever its category or width changes: `content`, the section for the
/// form's test or why there is none, posted for to `simulate`.
pub fn simulation_results(simulate: &str, content: &Markup) -> Markup {
    html! {
        div id="simulation-results" fx-action=(simulate) fx-method="post" fx-trigger="redraw"
            data-form="details-form" {
            (content)
        }
    }
}

/// A build's simulation section while its cases run in the background,
/// as just after it is published: `simulation.js` asks `poll` after it,
/// then reloads the page, since whether it fails shows above too.
fn cases_pending(poll: &str) -> Markup {
    html! {
        section id="simulation" aria-labelledby="simulation-heading" class="mt-12 max-w-calm" {
            h2 id="simulation-heading" class={ (SECTION_TITLE) " mb-1" } { "Simulation" }
            div fx-action={ (poll) "?have=none" } fx-trigger="poll" data-every="3000"
                fx-target="#simulation" class="mt-3" {
                p class="flex items-center gap-2 text-muted" {
                    (spinner())
                    "Simulating every case, which takes a few seconds…"
                }
                noscript { p class="text-muted" { "Reload the page to see it once it is done." } }
            }
        }
    }
}

/// A simulation result in a few words, such as `all 131,072 cases right`.
pub fn simulation_summary(report: &Report) -> String {
    match report {
        Report::Ran(run) if run.wrong == 0 => {
            format!("all {} cases right.", grouped(run.cases))
        }
        Report::Ran(run) => format!(
            "{} of {} cases wrong.",
            grouped(run.wrong),
            grouped(run.cases)
        ),
        Report::Broken { reason } => format!("broken. {reason}"),
        Report::Skipped { reason } => format!("not simulated. {reason}"),
    }
}

/// A timing in a few words, such as `synced; output settles by tick 3`.
pub fn timing_summary(timing: &Timing) -> String {
    // Such as `output settles by tick 3, carry out by tick 4`.
    let outputs: Vec<String> = timing
        .outputs
        .iter()
        .enumerate()
        .map(|(i, output)| {
            let verb = if i == 0 { " settles" } else { "" };
            format!("{}{verb} by tick {}", output.name, tick(output.latency))
        })
        .collect();
    let glitching: Vec<&str> = timing
        .outputs
        .iter()
        .filter(|output| output.glitches > 0)
        .map(|output| output.name.as_str())
        .collect();
    let synced = if timing.synced {
        "synced".to_owned()
    } else if glitching.is_empty() {
        "not synced".to_owned()
    } else {
        format!("not synced: {} glitches", glitching.join(" and "))
    };
    format!("{synced}; {}.", outputs.join(", "))
}

/// A small spinning ring, for something under way; still where motion is
/// reduced.
pub fn spinner() -> Markup {
    html! {
        span aria-hidden="true"
            class="inline-block size-4 shrink-0 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
    }
}

/// How a section's timing arrives while it is measured.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Waiting<'a> {
    /// On an upload or edit page: posted the details form to this address
    /// once on the page.
    Posted(&'a str),
    /// On a build's page, measured in the background: asked of this
    /// address every few seconds until it is ready.
    Polled(&'a str),
}

/// What simulating a build found: the cases tried and how many came out
/// right, then, for a failure, which output bits were wrong and the first
/// wrong cases, or, for a pass, its timing once measured.
pub fn simulation_section(report: &Report, waiting: Waiting) -> Markup {
    html! {
        section id="simulation" aria-labelledby="simulation-heading" class="mt-12 max-w-calm"
            data-measures-timing[report.passed()]
            data-skipped[matches!(report, Report::Skipped { .. })]
            data-state=(if report.failed() { "fail" } else { "pass" })
            data-summary=(simulation_summary(report)) {
            h2 id="simulation-heading" class={ (SECTION_TITLE) " mb-2" } { "Simulation" }
            @match report {
                Report::Skipped { reason } => p class="text-muted" { "Not simulated: " (reason) },
                Report::Broken { reason } => p class="text-danger" { strong { "Broken: " } (reason) },
                Report::Ran(run) => {
                    // A pass is the build's badge; a failure says how badly
                    // over the cases it got wrong.
                    @if run.wrong > 0 {
                        p class="text-danger" {
                            span aria-hidden="true" { "✗ " }
                            strong { (grouped(run.wrong)) " of " (grouped(run.cases)) " cases wrong" }
                        }
                        p class="text-meta text-muted" {
                            (run.pattern) ", "
                            @if run.exhaustive { "every combination" } @else {
                                "every case of each " @if run.by_digit { "hex digit" } @else { "byte" }
                                " and " (grouped(run.patterns)) " pairings of test patterns"
                            }
                            "."
                        }
                    }
                    @if let Some(stacking) = &run.stacking {
                        (stacking_note(stacking))
                    }
                    @if run.wrong > 0 {
                        (simulation_failures(run))
                    } @else if let Some(timing) = &run.timing {
                        (timing_section(timing))
                    } @else {
                        @let (action, method, trigger, target, form) = match waiting {
                            Waiting::Posted(url) => (url.to_owned(), "post", "load", None, Some("details-form")),
                            Waiting::Polled(url) => (format!("{url}?have=cases"), "get", "poll", Some("#simulation"), None),
                        };
                        div data-timing-pending fx-action=(action) fx-method=(method) fx-trigger=(trigger)
                            fx-target=[target] data-form=[form] data-every="3000" class="mt-6" {
                            p class="flex items-center gap-2 text-muted" {
                                (spinner())
                                "Measuring when each output settles, which takes up to a minute…"
                            }
                            @if let Waiting::Polled(_) = waiting {
                                noscript { p class="text-muted" { "Reload the page to see it once it is done." } }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Whether copies of a passing adder stack into a wider one, or of a
/// register into a bigger register file.
/// What stacking copies found, in a sentence, for the hover on a build's
/// Stacks field.
fn stacking_words(stacking: &Stacking) -> String {
    let limit = |most: Option<usize>, copies: usize| match most {
        Some(_) => format!(", at most {copies}"),
        None => String::new(),
    };
    match stacking {
        Stacking::Stacks {
            step, copies, bits, ..
        } => format!(
            "Simulated: {copies} copies {} apart make a {bits}-bit adder.",
            step.describe()
        ),
        Stacking::Registers {
            step,
            copies,
            registers,
            most,
            ..
        } => format!(
            "Simulated: copies {} apart make more registers, {registers} from {copies}{}.",
            step.describe(),
            limit(*most, *copies)
        ),
        Stacking::Wider {
            step,
            copies,
            bits,
            most,
            ..
        } => format!(
            "Simulated: copies {} apart make a wider register, {bits} bits from {copies}{}.",
            step.describe(),
            limit(*most, *copies)
        ),
        Stacking::Not { step, reason } => match step {
            Some(step) => format!(
                "Copies {} apart don't stack as saved: {reason}",
                step.describe()
            ),
            None => format!("Copies don't stack as saved: {reason}"),
        },
    }
}

fn stacking_note(stacking: &Stacking) -> Markup {
    let limit = |copies: usize, most: Option<usize>| match most {
        Some(_) => format!("; a copy more fails, so at most {copies}"),
        None => ", so any number stack".to_owned(),
    };
    html! {
        @match stacking {
            Stacking::Stacks { step, copies, bits, cases } => p class="mt-3" {
                strong { "Stacks: " }
                (copies) " copies " (step.describe()) " apart make a " (bits) "-bit adder, all "
                (grouped(*cases)) " cases right"
                @if *copies >= 3 { ", so any number stack." } @else { "." }
            },
            Stacking::Registers { step, copies, registers, cases, most } => p class="mt-3" data-found-stacking {
                strong { "Stacks: " }
                (copies) " copies " (step.describe()) " apart make " (registers) " registers sharing their data buses, all "
                (grouped(*cases)) " cases right" (limit(*copies, *most)) "."
            },
            Stacking::Wider { step, copies, bits, cases, most } => p class="mt-3" data-found-stacking {
                strong { "Stacks: " }
                (copies) " copies " (step.describe()) " apart make a " (bits) "-bit register sharing its control lines, all "
                (grouped(*cases)) " cases right" (limit(*copies, *most)) "."
            },
            // Why not is on the build's Stacks field, on hover.
            Stacking::Not { .. } => {},
        }
    }
}

/// When a passing build's inputs may change and its outputs settle, from
/// input A's change; and, for an output that does not change cleanly, when
/// each of its bits does.
/// What the timing's ticks count from: input A changing, or for a
/// register, a read lever turning on, with when a write shows.
fn timing_intro(timing: &Timing) -> Markup {
    html! {
        @if timing.from_read {
            p class="mb-3" {
                @match (timing.data, timing.write, timing.takes) {
                    (Some(data), Some(write), Takes::Closing) => {
                        strong { "Latency " (data) " ticks" } ", data in to data out, the data changing as the write ends. "
                        span class="text-muted" { "A write shows " (write) " ticks after it ends." }
                    }
                    (Some(data), Some(write), Takes::Open) => {
                        strong { "Latency " (data) " ticks" } ", data in to data out with the write held open. "
                        span class="text-muted" { "A write shows " (write) " ticks after its button." }
                    }
                    (None, Some(write), _) => {
                        strong { "Latency " (write) " ticks" } ", from a write's button until the bus shows it; "
                        span class="text-muted" { "a cell takes its data only as its write opens." }
                    }
                    _ => {}
                }
            }
            p class="mb-2 text-meta text-muted" { "Reads, in ticks from a read lever turning on:" }
        } @else {
            @let late: Vec<&InputTiming> = timing.inputs.iter().skip(1).filter(|input| input.tick != Some(0)).collect();
            p class="mb-2 text-meta text-muted" {
                "In ticks from A changing."
                @for input in late {
                    " " (input.name)
                    @match input.tick {
                        Some(tick) if tick > 0 => { " may change up to " (tick) " later." }
                        Some(tick) => { " must change " (-tick) " earlier." }
                        None => { " leads to no output." }
                    }
                }
            }
        }
    }
}

pub fn timing_section(timing: &Timing) -> Markup {
    let case: Vec<String> = timing
        .inputs
        .iter()
        .zip(&timing.slowest_case)
        .map(|(input, value)| format!("{} = {value}", input.name))
        .collect();
    html! {
        section aria-labelledby="timing-heading" class="mt-6"
            data-timing data-state=(if timing.synced { "pass" } else { "fail" })
            data-summary=(timing_summary(timing)) {
            h3 id="timing-heading" class="mb-2 font-semibold" { "Timing" }
            (timing_intro(timing))
            div class="overflow-x-auto" {
                table class="w-full border-collapse" {
                    caption class="sr-only" { "When each output settles" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Output" }
                            th scope="col" class=(TABLE_HEAD) { "First changes" }
                            th scope="col" class=(TABLE_HEAD) { "Settles by" }
                            th scope="col" class=(TABLE_HEAD) { "Glitches" }
                        }
                    }
                    tbody {
                        @for output in &timing.outputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } { (output.name) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(output.first_change)) }
                                td class={ (TABLE_CELL) " tabular-nums font-semibold" } { (tick(output.latency)) }
                                td class={ (TABLE_CELL) " tabular-nums" } {
                                    @if output.glitches == 0 {
                                        "none"
                                    } @else {
                                        strong class="text-danger" { "in " (grouped(output.glitches)) " changes" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (carried_notes(&timing.outputs))
            p class="mt-3" {
                @if timing.synced {
                    strong { "Synced: " } "each output changes once, all its bits together."
                } @else {
                    strong class="text-danger" { "Not synced: " } "an output glitches or its bits change apart."
                }
            }
            @for output in &timing.outputs {
                @if let Some(glitch) = &output.glitch {
                    (glitch_example(&output.name, glitch, &timing.inputs))
                }
                @if output.staggered > 0 {
                    p class="mt-2" {
                        "The bits of " (output.name) " change on different ticks in " (grouped(output.staggered))
                        " changes, so it shows a value between the old and the new."
                    }
                }
            }
            p class="mt-2 text-meta text-muted" {
                "Half the changes settle by tick " (timing.median) ", 99% by " (timing.p99)
                "; the slowest, " (case.join(", ")) ", by " (timing.slowest) "."
            }
            @for output in timing.outputs.iter().filter(|o| o.glitches > 0 || o.staggered > 0) {
                (bit_timing(output))
            }
            p class="mt-1 text-meta text-muted" {
                "Over " (grouped(timing.changes)) " changes, each input alone to "
                @if timing.patterns { "each test pattern" } @else { "every value" }
                " and back, and all together; a slower one may hide among those not tried."
            }
        }
    }
}

/// What each output timed short of the repeaters or comparators carrying
/// it out of the build leaves out.
fn carried_notes(outputs: &[OutputTiming]) -> Markup {
    html! {
        @for output in outputs.iter().filter(|o| o.carried > 0) {
            p class="mt-2 text-muted" {
                (output.name) " is timed where it reaches the repeaters or comparators standing on its blocks, which carry it out of the build "
                (output.carried) (if output.carried == 1 { " tick" } else { " ticks" }) " later."
            }
        }
    }
}

/// The first change from all off in which `name` glitched, tick by tick,
/// to try in-game.
fn glitch_example(
    name: &str,
    glitch: &Glitch,
    inputs: &[InputTiming],
) -> Markup {
    let case: Vec<String> = inputs
        .iter()
        .zip(&glitch.case)
        .map(|(input, value)| format!("{} = {value}", input.name))
        .collect();
    let steps: Vec<String> = glitch
        .steps
        .iter()
        .map(|&(at, value)| format!("{value} at tick {}", tick(at)))
        .collect();
    html! {
        p class="mt-2" {
            "For example, from all inputs off to " (case.join(", ")) ", " (name)
            " starts at " (glitch.before) " and becomes " (steps.join(", then "))
            @if glitch.pulse() {
                ": a pulse, where it should stay " (glitch.before) "."
            } @else {
                ": it flips back and forth on its way to its new value."
            }
        }
    }
}

/// When each bit of an output that does not change cleanly first changes
/// and settles, folded away under its name.
fn bit_timing(output: &OutputTiming) -> Markup {
    html! {
        details class="mt-4" {
            summary class="cursor-pointer font-semibold" { (output.name) ", bit by bit" }
            div class="mt-2 overflow-x-auto" {
                table class="w-full border-collapse" {
                    caption class="sr-only" { (output.name) ", bit by bit" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Bit" }
                            th scope="col" class=(TABLE_HEAD) { "First changes" }
                            th scope="col" class=(TABLE_HEAD) { "Settles by" }
                            th scope="col" class=(TABLE_HEAD) { "Changes more than once" }
                        }
                    }
                    tbody {
                        @for (i, bit) in output.bits.iter().enumerate() {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal tabular-nums" } { (i) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(bit.first_change)) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(bit.latency)) }
                                td class={ (TABLE_CELL) " tabular-nums" } {
                                    @if bit.glitches == 0 { "never" } @else { "in " (grouped(bit.glitches)) " changes" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An output's tick counted from A's change, such as `3`; one before A's,
/// though no build has shown one, as `−2`.
fn tick(tick: i32) -> String {
    match tick {
        t if t < 0 => format!("−{}", t.unsigned_abs()),
        t => t.to_string(),
    }
}

/// Which bits a failing build got wrong, and its first wrong cases, a
/// column for each input and output.
fn simulation_failures(run: &Run) -> Markup {
    let ops = run.examples.iter().any(|wrong| wrong.op.is_some());
    html! {
        @if let Some(hint) = &run.hint {
            p class="mt-4" { strong { "Likely cause: " } (hint) }
        }
        h3 class="mt-6 mb-2 font-semibold" { "Wrong bits" }
        ul class="flex flex-col gap-1" {
            @for (bit, count) in &run.wrong_bits {
                li class="tabular-nums" { (bit) ", in " (grouped(*count)) " cases" }
            }
        }
        h3 class="mt-6 mb-2 font-semibold" { "First wrong cases" }
        div class="overflow-x-auto" {
            table class="w-full border-collapse" {
                caption class="sr-only" { "First wrong cases" }
                thead {
                    tr {
                        // A build of several operations, such as an ALU,
                        // says which each case was on.
                        @if ops {
                            th scope="col" class=(TABLE_HEAD) { "Operation" }
                        }
                        @for signal in run.inputs.iter().chain(&run.outputs) {
                            th scope="col" class=(TABLE_HEAD) { (signal.name) }
                        }
                    }
                }
                tbody {
                    @for wrong in &run.examples {
                        tr {
                            @if ops {
                                td class=(TABLE_CELL) { (wrong.op.as_deref().unwrap_or("")) }
                            }
                            @for (signal, &value) in run.inputs.iter().zip(&wrong.inputs) {
                                td class=(TABLE_CELL) { (value_bits(value, value, signal.bits)) }
                            }
                            @for ((signal, &got), &expected) in run.outputs.iter().zip(&wrong.got).zip(&wrong.expected) {
                                td class=(TABLE_CELL) {
                                    @if got == expected {
                                        (value_bits(got, expected, signal.bits))
                                    } @else {
                                        div class="grid grid-cols-[auto_auto] items-baseline gap-x-2" {
                                            span class="text-meta text-muted" { "got" }
                                            (value_bits(got, expected, signal.bits))
                                            span class="text-meta text-muted" { "expected" }
                                            span class="text-muted" { (value_bits(expected, expected, signal.bits)) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `value` as `bits` binary digits, those that differ from `expected`
/// marked, then in decimal when it has more than one bit.
fn value_bits(value: u64, expected: u64, bits: usize) -> Markup {
    html! {
        span class="whitespace-nowrap" {
            span class="font-mono" { (marked_bits(value, expected, bits)) }
            @if bits > 1 {
                span class="ml-2 text-meta text-muted tabular-nums" { (value) }
            }
        }
    }
}

/// `value` in binary, `width` digits, with each digit that differs from
/// `expected` marked.
fn marked_bits(value: u64, expected: u64, width: usize) -> Markup {
    html! {
        @for bit in (0..width).rev() {
            @let digit = value >> bit & 1;
            @if digit == expected >> bit & 1 {
                (digit)
            } @else {
                mark class="bg-transparent font-semibold text-danger" { (digit) }
            }
        }
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

/// The build's inputs and outputs, read from the file, a short list of
/// each: a bus by its bits, and a row of numbered lines, such as a
/// memory's `read 0` to `read 15`, as one entry.
fn interface_section(analysis: &Analysis) -> Markup {
    let Interface { inputs, outputs } = &analysis.interface;
    html! {
        section aria-labelledby="interface-heading" {
            h2 id="interface-heading" class={ (SECTION_TITLE) " mb-3" } { "Inputs and outputs" }
            div class="grid gap-6 sm:grid-cols-2" {
                (port_list("Inputs", &port_entries(inputs), true))
                (port_list("Outputs", &port_entries(outputs), false))
            }
        }
    }
}

/// The building conventions the build's file breaks, if any.
fn violations_section(analysis: &Analysis) -> Markup {
    html! {
        @if !analysis.violations.is_empty() {
            section aria-labelledby="lint-heading" class="mt-12 max-w-calm" {
                h2 id="lint-heading" class={ (SECTION_TITLE) " mb-3" } { "Breaks the building conventions" }
                (violation_list(&analysis.violations))
            }
        }
    }
}

/// One entry in a list of inputs or outputs: a port, or numbered lines of
/// one bit each that share a word and a colour, such as `read 0` to
/// `read 15`.
struct PortEntry<'a> {
    name: String,
    ports: Vec<&'a Port>,
    /// Names more than one line is signed with, such as `read 5`, as a
    /// mislabelled sign leaves.
    repeated: Vec<String>,
}

/// Sorted, distinct `numbers` as runs, such as `0–14` or `0–3, 6`.
fn ranges(numbers: &[u32]) -> String {
    let mut runs: Vec<String> = Vec::new();
    let mut start = 0;
    for i in 1..=numbers.len() {
        if i == numbers.len() || numbers[i] != numbers[i - 1] + 1 {
            let (first, last) = (numbers[start], numbers[i - 1]);
            runs.push(if first == last {
                first.to_string()
            } else {
                format!("{first}–{last}")
            });
            start = i;
        }
    }
    runs.join(", ")
}

/// Ports, by the word and colour numbered lines share; one apiece for any
/// other port.
type PortGroup<'p> = (Option<(String, String)>, Vec<&'p Port>);

/// `ports` as entries, in order of first appearance, numbered lines
/// gathered under their shared word.
fn port_entries(ports: &[Port]) -> Vec<PortEntry<'_>> {
    let numbered = |port: &Port| {
        let (word, number) = port.name.rsplit_once(' ')?;
        let number: u32 = number.parse().ok()?;
        (port.bits.len() == 1).then(|| (word.to_owned(), number))
    };
    let mut entries: Vec<PortGroup> = Vec::new();
    for port in ports {
        let key = numbered(port).map(|(word, _)| (word, port.block.clone()));
        match entries.iter_mut().find(|(k, _)| key.is_some() && *k == key) {
            Some((_, group)) => group.push(port),
            None => entries.push((key, vec![port])),
        }
    }
    entries
        .into_iter()
        .map(|(key, ports)| {
            let (name, repeated) = match (&key, ports.as_slice()) {
                (Some((word, _)), [_, _, ..]) => {
                    let mut numbers: Vec<u32> = ports
                        .iter()
                        .filter_map(|p| numbered(p).map(|(_, n)| n))
                        .collect();
                    numbers.sort_unstable();
                    let mut repeated: Vec<String> = numbers
                        .windows(2)
                        .filter(|pair| pair[0] == pair[1])
                        .map(|pair| format!("{word} {}", pair[0]))
                        .collect();
                    repeated.dedup();
                    numbers.dedup();
                    (format!("{word} {}", ranges(&numbers)), repeated)
                }
                _ => (ports[0].name.clone(), Vec::new()),
            };
            PortEntry {
                name,
                ports,
                repeated,
            }
        })
        .collect()
}

/// A list of input or output entries under `heading`, each with a swatch
/// of its marking colour, its bits or lines, and, for an input, what it
/// takes beyond any power.
fn port_list(heading: &str, entries: &[PortEntry], inputs: bool) -> Markup {
    html! {
        div {
            h3 class="mb-2 text-meta font-semibold text-muted" { (heading) }
            ul class="flex flex-col gap-1.5" {
                @for entry in entries {
                    @let port = entry.ports[0];
                    @let dye = port.block.strip_suffix(" concrete").unwrap_or(&port.block).replace(' ', "_");
                    @let [r, g, b, _] = crate::thumbnail::dye_color(&dye);
                    li class="flex items-start gap-2" {
                        span aria-hidden="true" class="mt-1.5 inline-block size-3 shrink-0 rounded-[3px] border border-line"
                            style={ "background: rgb(" (r) " " (g) " " (b) ")" } {}
                        span {
                            span class="font-semibold" { (entry.name) }
                            " "
                            span class="text-meta text-muted" { (port_detail(entry, inputs)) }
                        }
                    }
                }
            }
        }
    }
}

/// What an entry carries, such as `8 bits` or `16 lines`, and for an
/// input, the strength its signs ask for and whether it is inverted, as
/// its levers are saved on.
fn port_detail(entry: &PortEntry, inputs: bool) -> String {
    let port = entry.ports[0];
    let mut said = vec![if entry.ports.len() > 1 {
        format!("{} lines", entry.ports.len())
    } else if port.signal_strength {
        format!(
            "{} bits, a hex digit per block's strength",
            port.bits.len() * 4
        )
    } else if port.bits.len() == 1 {
        "1 bit".to_owned()
    } else {
        format!("{} bits", port.bits.len())
    }];
    for name in &entry.repeated {
        said.push(format!("{name} signed more than once"));
    }
    if inputs && !port.signal_strength {
        let bits: Vec<&Bit> =
            entry.ports.iter().flat_map(|p| p.bits.iter()).collect();
        let strength =
            entry.ports.iter().map(|p| p.strength).max().unwrap_or(1);
        if strength > 1 {
            said.push(format!("strength {strength} or more"));
        }
        // An input saved with its lever on is inverted: off means 1.
        if bits.iter().all(|b| b.inverted) {
            said.push("inverted".to_owned());
        } else if bits.iter().any(|b| b.inverted) {
            said.push("some bits inverted".to_owned());
        }
    }
    said.join(" · ")
}
/// Each broken convention with where in the file it is.
fn violation_list(violations: &[Violation]) -> Markup {
    html! {
        ul class="flex flex-col gap-1.5" {
            @for violation in violations {
                li class="flex gap-3" {
                    span class="w-24 shrink-0 text-muted tabular-nums" {
                        @if let Some([x, y, z]) = violation.pos { (x) ", " (y) ", " (z) }
                    }
                    span { (violation.message) }
                }
            }
        }
    }
}

/// An admin's confirmation before deleting a build.
fn delete_dialog(s: &Schematic) -> Markup {
    html! {
        dialog id="delete" aria-labelledby="delete-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action={ "/schematics/" (s.id) "/delete" } method="post" class="flex flex-col gap-4" {
                h2 id="delete-heading" class=(SECTION_TITLE) { "Delete " (s.name()) "?" }
                p { "The build, its file, tags, and credits are removed for good." }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-danger" { "Delete" }
                    button type="button" commandfor="delete" command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}

/// The choices for downloading a build: copies of a stackable one, and
/// glass corner markers for `WorldEdit`; and, for someone `signed_in`, a
/// button that sends the file so made to their Schemati instead.
fn download_dialog(s: &Schematic, signed_in: bool) -> Markup {
    let path = format!("/schematics/{}", s.id);
    html! {
        dialog id="download" aria-labelledby="download-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action={ (path) "/download" } method="get" class="flex flex-col gap-4" {
                h2 id="download-heading" class=(SECTION_TITLE) { "Download" }
                @if !s.stacking.is_empty() {
                    div class="flex gap-2" {
                        @for direction in &s.stacking {
                            label class="field flex-1" {
                                span { "Copies " (direction.name().replace('-', " and ")) }
                                input type="number" name=(direction.name()) min="1"
                                    max=[direction.max_copies] value="1" required
                                    class="input tabular-nums";
                            }
                        }
                    }
                }
                label class="flex items-start gap-2" {
                    input type="checkbox" name="markers" value="yes" class="mt-1 size-4 appearance-auto";
                    span { "Add a glass block off each corner, for WorldEdit to select the build by" }
                }
                div class="flex flex-wrap items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Download" }
                    @if signed_in {
                        button type="submit" formaction={ (path) "/schemati" } formmethod="post"
                            fx-action={ (path) "/schemati" } fx-method="post" fx-target="#sent" fx-swap="innerHTML"
                            class="btn btn-quiet" {
                            "Send to Schemati"
                        }
                    }
                    button type="button" commandfor="download" command="close" class="btn btn-quiet" { "Cancel" }
                }
                @if signed_in {
                    div id="sent" aria-live="polite" class="text-meta empty:hidden" {}
                }
            }
        }
    }
}

/// The builds of its category a build is similar to, the most alike first.
fn similar_section(similar: &[(Schematic, u32)]) -> Markup {
    html! {
        section aria-labelledby="similar-heading" {
            h2 id="similar-heading" class={ (SECTION_TITLE) " mb-1" } { "Similar builds" }
            p class="mb-3 text-meta text-muted" {
                "Share of their redstone that lines up, turned or mirrored as need be."
            }
            ul class="flex flex-col gap-1.5" {
                @for (other, permille) in similar {
                    li class="flex items-baseline justify-between gap-4" {
                        span {
                            a href={ "/schematics/" (other.id) } { (other.title()) }
                            span class="text-muted" {
                                @if !other.builders.is_empty() { " by " (other.builders.join(", ")) }
                                ", " (size(other))
                            }
                        }
                        span class="tabular-nums text-muted whitespace-nowrap" { (percent(*permille)) }
                    }
                }
            }
        }
    }
}

/// Tenths of a percent as a whole percent, such as `98%`.
fn percent(permille: u32) -> String {
    format!("{}%", permille / 10)
}

/// The standard `categories` as the sidebar lists them: each section, by
/// its key and heading, with its categories, then any in none under Other.
/// A section with no categories is left out.
fn sections(categories: &[Category]) -> Vec<(&str, &str, Vec<&Category>)> {
    let of = |section: Option<&str>| -> Vec<&Category> {
        categories
            .iter()
            .filter(|c| c.section.as_deref() == section)
            .collect()
    };
    schematic::SECTIONS
        .iter()
        .map(|&(key, heading)| (key, heading, of(Some(key))))
        .chain([("other", "Other", of(None))])
        .filter(|(_, _, members)| !members.is_empty())
        .collect()
}

/// The values a details form starts with.
pub struct Fields<'a> {
    pub category: &'a str,
    pub width: &'a str,
    pub orientation: &'a str,
    pub encoding: &'a str,
    pub latency: &'a str,
    /// `None` when unknown, which shows the toggle as neither on nor off.
    pub synchronized: Option<bool>,
    pub builders: &'a [String],
    pub contributors: &'a [String],
    pub built_on: &'a str,
    pub description: &'a str,
    /// The build's own name, for a category that takes one.
    pub given_name: &'a str,
    /// `comb` or `seq`; empty follows the category.
    pub logic: &'a str,
    /// The fewest ticks from one input to the next.
    pub throughput: &'a str,
    /// How many bytes a memory holds.
    pub bytes: &'a str,
    /// A threshold's k.
    pub at_least: &'a str,
    pub design: &'a str,
    /// The ticked flags, such as `carry_in`.
    pub flags: &'a [String],
    pub stackable: bool,
    /// The stacking directions so far, or a suggested first one.
    pub stack_rows: &'a [StackRow],
}

/// The lists a details form chooses from or suggests.
pub struct Choices {
    /// The standard categories.
    pub categories: Vec<Category>,
    pub designs: Vec<Design>,
    /// Every category's flags.
    pub flags: Vec<Flag>,
    /// Everyone credited on a stored build, to suggest as builders and
    /// contributors.
    pub people: Vec<String>,
}

impl Choices {
    /// The standard category `typed` names, by its name or label, whatever
    /// the capitals, as the server reads the form.
    fn category(&self, typed: &str) -> Option<&Category> {
        let typed = typed.trim();
        self.categories.iter().find(|c| {
            c.name.eq_ignore_ascii_case(typed)
                || c.label.eq_ignore_ascii_case(typed)
                || c.name == schematic::tag_name(typed)
        })
    }

    fn sections(&self) -> Vec<(&str, &str, Vec<&Category>)> {
        sections(&self.categories)
    }

    pub async fn load(db: &sqlx::SqlitePool) -> Result<Self, sqlx::Error> {
        Ok(Choices {
            categories: schematic::categories(db).await?,
            designs: schematic::designs(db).await?,
            flags: schematic::flags(db).await?,
            people: schematic::all_people(db).await?,
        })
    }
}

/// The category, its name when other, its design, and its carry lines.
fn category_fields(f: &Fields, choices: &Choices) -> Markup {
    html! {
        @let chosen = choices.category(f.category);
        @let named = chosen.is_some_and(|c| c.named);
        @let has = |measure| chosen.is_some_and(|c: &Category| c.has(measure));
        @let chosen_name = chosen.map_or("", |c| c.name.as_str());
        @let has_designs = choices
            .designs
            .iter()
            .any(|design| design.category == chosen_name);
        // Typed, with a menu of the standard categories under their
        // sections, as in the sidebar, which form.js opens and narrows as
        // the box is typed in; a name of the builder's own makes a category
        // off the list. Without the script, the box takes typing alone.
        div class="field" {
            label for="category" class="text-meta font-semibold text-muted" { "Category" }
            div class="relative" {
                input id="category" name="category" required autocomplete="off"
                    role="combobox" aria-controls="category-menu" aria-expanded="false"
                    aria-autocomplete="list"
                    value=(chosen.map_or_else(|| f.category.replace('_', " "), |c| c.label.clone()))
                    placeholder="Type to search, such as adder" aria-describedby="category-help"
                    class="input" data-category;
                div id="category-menu" role="listbox" aria-label="Categories" hidden
                    class="absolute inset-x-0 top-full z-20 mt-1 max-h-80 overflow-y-auto rounded-control border border-line bg-raised p-1 shadow-overlay"
                    data-category-menu {
                    @for (key, heading, members) in choices.sections() {
                        // Headings stand apart from the options under them:
                        // bold, with the options indented and a line between
                        // sections.
                        div role="group" aria-labelledby={ "category-group-" (key) } data-group
                            class="menu-section" {
                            div id={ "category-group-" (key) } role="presentation"
                                class="px-2.5 pt-2 pb-1 text-meta font-bold text-ink" { (heading) }
                            @for category in members {
                                div id={ "category-option-" (category.name) } role="option"
                                    aria-selected="false" data-value=(category.label)
                                    data-name=(category.name) data-named[category.named]
                                    data-logic=(category.logic.as_deref().map_or("unknown", schematic::logic_label).to_lowercase())
                                    data-reads-file[import::READ_FROM_FILE.contains(&category.name.as_str())]
                                    data-infers-stacking[simulate::INFERS_STACKING.contains(&category.name.as_str())]
                                    data-finds-stacking[simulate::FINDS_STACKING.contains(&category.name.as_str())]
                                    data-tested[simulate::Test::of(&category.name, Some(1)).is_some()]
                                    data-measures=(category.measures.iter().map(|m| m.name()).collect::<Vec<_>>().join(" "))
                                    class="cursor-pointer rounded-control py-1.5 pr-2.5 pl-5 text-nav hover:bg-selection aria-selected:bg-selection" {
                                    (category.label)
                                }
                            }
                        }
                    }
                    p role="presentation" hidden class="px-2.5 py-1.5 text-meta text-muted" data-no-match {
                        "No category matches; this name makes a new one."
                    }
                }
            }
            span id="category-help" class="text-meta font-normal text-muted" {
                "Or type your own, such as piston door."
            }
        }
        // For a category whose builds each have a name, such as bussing.
        label class="field" data-named-field hidden[!named] {
            span { "Name" }
            input name="given_name" value=(f.given_name) maxlength="60"
                placeholder="such as Bus Crossing" class="input";
        }
        // For a memory, such as RAM.
        label class="field" data-measure-field="bytes" hidden[!has(Measure::Bytes)] {
            span { "Capacity (bytes)" }
            input type="number" name="bytes" value=(f.bytes) min="1"
                inputmode="numeric" placeholder="such as 32" class="input";
        }
        // For a threshold.
        label class="field" data-measure-field="at_least" hidden[!has(Measure::AtLeast)] {
            span { "Turns on with at least (inputs)" }
            input type="number" name="at_least" value=(f.at_least) min="1"
                inputmode="numeric" placeholder="such as 3" class="input";
        }
        // Shown once the chosen category has designs to choose from.
        label class="field" data-design-field hidden[!has_designs] {
            span { "Design" }
            select name="design" class="input" data-design {
                option value="other" selected[f.design.is_empty() || f.design == "other"] { "Other" }
                @for category in &choices.categories {
                    @let designs: Vec<&Design> = choices.designs.iter().filter(|d| d.category == category.name).collect();
                    @if !designs.is_empty() {
                        optgroup label=(category.label) data-for=(category.name) {
                            @for design in designs {
                                option value=(design.name) selected[f.design == design.name] { (design.label()) }
                            }
                        }
                    }
                }
            }
        }
        // Each category's flags, sent under its own name; form.js shows only
        // the chosen category's. A flag starts ticked in every group that has
        // it, as the carry lines a file's signs show do, so it stays ticked
        // whichever category is chosen.
        @for category in &choices.categories {
            @let flags: Vec<&Flag> = choices.flags.iter().filter(|flag| flag.category == category.name).collect();
            @if !flags.is_empty() {
                (flag_group(category, &flags, f.flags, category.name == chosen_name))
            }
        }
    }
}

/// `category`'s flags, ticked as `ticked` has them, shown when it is the
/// chosen category. An adder's are read from its file, so it says so
/// instead.
fn flag_group(
    category: &Category,
    flags: &[&Flag],
    ticked: &[String],
    chosen: bool,
) -> Markup {
    html! {
        div class="flex flex-wrap items-center gap-x-6 gap-y-2" data-flags-for=(category.name) hidden[!chosen] {
            @if import::FLAGS_FROM_FILE.contains(&category.name.as_str()) {
                p class="text-muted" {
                    "Carry in and carry out are read from the file."
                }
            } @else {
                @for flag in flags {
                    label class="flex items-center gap-2" {
                        input type="checkbox" name={ "flag:" (category.name) } value=(flag.name)
                            checked[ticked.contains(&flag.name)] class="size-4 appearance-auto";
                        (flag.label)
                    }
                }
            }
        }
    }
}

/// An orientation as people read it, such as `Vertical` for `vert`.
fn orientation_label(orientation: &str) -> &'static str {
    match orientation {
        "horz" => "Horizontal",
        "vert" => "Vertical",
        _ => "Diagonal",
    }
}

/// The inputs for a build's details, shared by editing and uploading.
/// `derived` is the width and orientation the file gives a category that
/// reads them from the file, such as an adder's from its lime inputs,
/// shown in place of the typed ones once such a category is chosen.
fn fields(f: &Fields, choices: &Choices, derived: &import::Derived) -> Markup {
    // A category off the list, such as a piston door, has no width or encoding.
    let other =
        !f.category.is_empty() && choices.category(f.category).is_none();
    html! {
        (category_fields(f, choices))
        label class="field" data-standard-only data-typed-width hidden[other] {
            span { "Width in bits" }
            input name="width" type="number" min="1" value=(f.width) class="input tabular-nums";
        }
        @if let Some(width) = derived.width {
            div class="field" data-derived-width hidden {
                span { "Width in bits" }
                p class="tabular-nums" { (width) span class="text-muted" { ", counted from the lime inputs" } }
            }
        }
        @if let Some(orientation) = derived.orientation {
            div class="field" data-derived-orientation hidden {
                span { "Orientation" }
                p {
                    (orientation_label(orientation))
                    span class="text-muted" { ", from the way the lime inputs run" }
                }
            }
        }
        label class="field" data-typed-orientation {
            span { "Orientation" }
            select name="orientation" class="input" {
                @for (value, label) in [("horz", "Horizontal"), ("vert", "Vertical"), ("diag", "Diagonal")] {
                    option value=(value) selected[f.orientation == value] { (label) }
                }
            }
        }
        @if let Some(encoding) = derived.encoding {
            div class="field" data-derived-encoding hidden {
                span { "Encoding" }
                p {
                    (if encoding == "hex" { "Hex" } else { "Binary" })
                    span class="text-muted" {
                        @if encoding == "hex" { ", as the lime inputs take a signal strength" } @else { ", as the lime inputs take levers" }
                    }
                }
            }
        }
        label class="field" data-standard-only data-typed-encoding hidden[other] {
            span { "Encoding" }
            select name="encoding" class="input" {
                @for (value, label) in [("bin", "Binary"), ("hex", "Hex")] {
                    option value=(value) selected[f.encoding == value] { (label) }
                }
            }
        }
        // Hidden by form.js once a passing simulation measures them.
        p data-timing-note hidden class="text-muted" {
            "Latency and synchronized are measured by the simulation."
            // Sent in place of the hidden fields, so nothing typed is kept.
            input type="hidden" name="synchronized_unknown" value="yes" disabled data-measured-marker;
        }
        div class="contents" data-timing-fields {
        label class="field" {
            span { "Latency in ticks" }
            input name="latency" type="number" min="0" value=(f.latency) class="input tabular-nums";
        }
        div class="field" {
            span { "Synchronized" }
            // An unknown build's toggle stays unknown until it is clicked;
            // form.js shows it as neither on nor off and drops the marker.
            label class="flex items-center gap-2" {
                input type="checkbox" name="synchronized" value="yes"
                    checked[f.synchronized == Some(true)]
                    data-unknown[f.synchronized.is_none()]
                    class="size-4 appearance-auto";
                "Every output bit settles on the same tick"
            }
            @if f.synchronized.is_none() {
                input type="hidden" name="synchronized_unknown" value="yes";
            }
        }
        }
        label class="field" data-standard-only hidden[other] {
            span { "Throughput in ticks" }
            input type="number" name="throughput" value=(f.throughput) min="1" max="1000"
                inputmode="numeric" placeholder="such as 3" aria-describedby="throughput-help"
                class="input tabular-nums";
            span id="throughput-help" class="text-meta font-normal" {
                "The fewest ticks between inputs that still come out right."
            }
        }
        (logic_field(f, choices))
        (stacking_fields(f))
        (person_field("Builders", "builder", "Add a builder's in-game name", f.builders, choices))
        (person_field("Contributors", "contributor", "Add someone who helped", f.contributors, choices))
        (date_field(f.built_on))
        label class="field" {
            span { "Description" }
            textarea name="description" rows="3" class="input py-2 leading-prose" { (f.description) }
        }
    }
}

/// Stateless or stateful, or as the category usually is, which `form.js`
/// names as the category changes.
fn logic_field(f: &Fields, choices: &Choices) -> Markup {
    let usual = choices
        .category(f.category)
        .and_then(|c| c.logic.as_deref())
        .map_or("unknown", schematic::logic_label)
        .to_lowercase();
    html! {
        label class="field" {
            span { "Stateless" }
            select name="logic" class="input" aria-describedby="logic-help" {
                option value="" selected[f.logic.is_empty()] data-usual-logic {
                    "As the category usually is (" (usual) ")"
                }
                @for (value, label) in [("comb", "Stateless"), ("seq", "Stateful")] {
                    option value=(value) selected[f.logic == value] { (label) }
                }
            }
            span id="logic-help" class="text-meta font-normal text-muted" {
                "Stateful builds remember something, as a register does."
            }
        }
    }
}

/// The build date, as a year, month, and day box each, in that order
/// whatever the browser's locale, which `form.js` moves between as they
/// fill and checks is a real date. The server joins them.
fn date_field(built_on: &str) -> Markup {
    let mut parts = built_on.splitn(3, '-');
    let [year, month, day] = [(); 3].map(|()| parts.next().unwrap_or(""));
    let part = |name: &str, hint: &str, value: &str, width: &str| {
        html! {
            input id={ "built-on-" (name) } name={ "built_on_" (name) } type="text" value=(value)
                aria-label=(name) aria-describedby="built-on-message"
                inputmode="numeric" maxlength=(hint.len()) placeholder=(hint)
                pattern={ "[0-9]{1," (hint.len()) "}" } autocomplete="off"
                class={ "input px-2 text-center tabular-nums " (width) };
        }
    };
    html! {
        div class="field" data-date-field {
            span id="built-on-label" { "Built on" }
            div class="flex items-center gap-4" {
                div role="group" aria-labelledby="built-on-label" class="flex items-center gap-1" {
                    (part("year", "yyyy", year, "w-18"))
                    span aria-hidden="true" class="text-muted" { "-" }
                    (part("month", "mm", month, "w-12"))
                    span aria-hidden="true" class="text-muted" { "-" }
                    (part("day", "dd", day, "w-12"))
                }
                label class="flex items-center gap-2 whitespace-nowrap" {
                    input type="checkbox" name="built_on_unknown" value="yes"
                        checked[built_on.is_empty()] class="size-4 appearance-auto";
                    "Don't know"
                }
            }
            p id="built-on-message" class="message" hidden {}
        }
    }
}

/// One choice in a pill field.
struct Pill {
    label: String,
    value: String,
    /// The badge colour, such as `badge-restriction`.
    class: String,
}

fn tag_pill(tag: &Tag) -> Pill {
    Pill {
        label: tag.label(),
        value: tag.name.clone(),
        class: format!("badge-{}", tag.kind),
    }
}

/// Whether the build stacks, and how: two rows, each a direction with its
/// step and most copies. The second row is for a build that also stacks
/// another way, and is left empty otherwise. `form.js` hides the rows
/// while the toggle is off.
fn stacking_fields(f: &Fields) -> Markup {
    let blank = StackRow::default();
    let rows = f
        .stack_rows
        .iter()
        .chain(std::iter::repeat(&blank))
        .take(f.stack_rows.len().max(2));
    html! {
        // For a category such as adder, the simulation stacks copies the way
        // the bits run and decides, so the rows give way to a note; for a
        // register, once it finds copies that work.
        div class="field" data-derived-stacking hidden {
            span { "Stacking" }
            p class="text-muted" {
                "Found by the simulation."
            }
        }
        fieldset class="flex flex-col gap-3" data-stacking data-typed-stacking {
            legend class="mb-1.5 text-meta font-semibold text-muted" { "Stacking" }
            label class="flex items-start gap-2" {
                input type="checkbox" name="stackable" value="yes" checked[f.stackable]
                    class="mt-1 size-4 appearance-auto";
                span { "Stackable: copies placed one after another make a bigger build" }
            }
            div class="flex flex-col gap-3" data-stack-rows {
                @for (i, row) in rows.enumerate() {
                    @let n = i + 1;
                    // Wraps between phrases, never inside one.
                    div class="flex flex-wrap items-center gap-x-2 gap-y-2" {
                        span class="flex items-center gap-2" {
                            "Each copy is"
                            input type="number" name="stack_step" min="1" value=(row.step)
                                aria-label={ "Blocks per copy, direction " (n) }
                                class="input w-18 tabular-nums";
                            "blocks"
                        }
                        select name="stack_direction" aria-label={ "Direction " (n) } class="input w-auto" {
                            @for (value, label) in STACK_DIRECTIONS {
                                option value=(value) selected[row.direction == value] { (label) }
                            }
                        }
                        span class="flex items-center gap-2" {
                            "up to"
                            input type="number" name="stack_max" min="2" value=(row.max) placeholder="any"
                                aria-label={ "Most copies, direction " (n) }
                                class="input w-18 tabular-nums";
                            "copies"
                        }
                    }
                }
                p class="text-meta text-muted" {
                    "Fill in the second row only for a build that also stacks another way, such as north for "
                    "more registers side by side. Leave the most copies empty for no limit. Saving stacks two "
                    "copies each way to check the step."
                }
            }
        }
    }
}

/// A pill field of in-game names, suggesting everyone already credited.
fn person_field(
    label: &'static str,
    name: &'static str,
    placeholder: &'static str,
    chosen: &[String],
    choices: &Choices,
) -> Markup {
    pill_field(&PillField {
        label,
        name,
        placeholder,
        suggestions: choices
            .people
            .iter()
            .map(|name| person_pill(name))
            .collect(),
        chosen: chosen.iter().map(|name| person_pill(name)).collect(),
        new_class: "badge-builder",
        pattern: "^[A-Za-z0-9_]{1,16}$",
        hint: "is not a Minecraft name: letters, digits, and underscores, up to 16",
        keep_case: true,
    })
}

/// `Builder` for one name, else `Builders`.
fn plural(singular: &str, names: &[String]) -> String {
    if names.len() == 1 {
        singular.to_owned()
    } else {
        format!("{singular}s")
    }
}

fn person_pill(name: &str) -> Pill {
    Pill {
        label: name.to_owned(),
        value: name.to_owned(),
        class: "badge-builder".to_owned(),
    }
}

/// A field whose values show as pills inside one box, with the text cursor
/// after them and suggestions as you type. `form.js` turns a picked or
/// typed value into a pill; without it, the typed text is sent as
/// `<name>_entry`.
struct PillField<'a> {
    label: &'a str,
    /// The name each chosen value is sent under.
    name: &'a str,
    placeholder: &'a str,
    suggestions: Vec<Pill>,
    chosen: Vec<Pill>,
    /// The colour of a pill for a value not among the suggestions.
    new_class: &'a str,
    /// What a new value must match; the server checks it too.
    pattern: &'a str,
    /// Said after a typed value that does not match.
    hint: &'a str,
    /// Whether a new value keeps its capitals, as in-game names do; else
    /// it is lowercased, as tag names are.
    keep_case: bool,
}

fn pill_field(f: &PillField) -> Markup {
    let entry = format!("{}-entry", f.name);
    let list = format!("{}-suggestions", f.name);
    let message = format!("{}-message", f.name);
    html! {
        div class="flex flex-col gap-1.5" data-pill-field
            data-name=(f.name) data-new-class=(f.new_class)
            data-pattern=(f.pattern) data-hint=(f.hint) data-keep-case[f.keep_case] {
            label for=(entry) class="text-meta font-semibold text-muted" { (f.label) }
            div class="flex min-h-10 flex-wrap items-center gap-1.5 rounded-control border border-boundary bg-raised px-2 py-1.5 focus-within:border-brand focus-within:ring-1 focus-within:ring-brand" {
                ul class="contents" aria-label={ "Chosen: " (f.label) } data-chosen {
                    @for chosen in &f.chosen {
                        (pill(chosen, f.name))
                    }
                }
                input id=(entry) name={ (f.name) "_entry" } list=(list) autocomplete="off"
                    placeholder=(f.placeholder) aria-describedby=(message)
                    class="min-w-40 flex-1 bg-transparent px-1 py-0.5 text-ink outline-none placeholder:text-muted";
            }
            datalist id=(list) {
                @for suggestion in &f.suggestions {
                    option value=(suggestion.label) data-value=(suggestion.value) data-class=(suggestion.class) {}
                }
            }
            p id=(message) role="alert" class="message" data-message {}
            // A pill, cloned by form.js for each value added.
            template data-template {
                (pill(&Pill { label: String::new(), value: String::new(), class: String::new() }, f.name))
            }
        }
    }
}

/// A chosen value: a pill whose remove button shows on hover or focus, and
/// on touch screens always, with the hidden input that sends it.
fn pill(p: &Pill, name: &str) -> Markup {
    html! {
        li {
            // Fits its name and its ×, with even space all round: the left
            // is wider by what the × keeps inside its own box. Names have
            // capitals, so the text is centred as it is, without the lift
            // a lowercase badge takes.
            span data-pill class={ "inline-flex items-center gap-1 rounded-full py-1 pr-1 pl-2 text-meta font-medium whitespace-nowrap " (p.class) } {
                span data-label { (p.label) }
                button type="button" data-remove
                    aria-label={ "Remove " (if p.label.is_empty() { "this" } else { &p.label }) }
                    class="flex size-4 cursor-pointer items-center justify-center rounded-full opacity-60 hover:bg-current/15 hover:opacity-100 focus-visible:opacity-100" {
                    (cross())
                }
            }
            input type="hidden" name=(name) value=(p.value);
        }
    }
}

/// The tag field of the details forms. Only an admin, `may_create`, may
/// name a tag not yet on the list.
fn tag_picker(tags: &[Tag], chosen: &[String], may_create: bool) -> Markup {
    pill_field(&PillField {
        label: "Tags",
        name: "tag",
        placeholder: "Add a tag, such as torchless",
        suggestions: tags.iter().map(tag_pill).collect(),
        chosen: tags
            .iter()
            .filter(|t| chosen.contains(&t.name))
            .map(tag_pill)
            .chain(
                // New tags the person named, not yet stored.
                chosen
                    .iter()
                    .filter(|name| tags.iter().all(|t| &t.name != *name))
                    .map(|name| Pill {
                        label: name.replace('_', " "),
                        value: name.clone(),
                        class: "badge-other".to_owned(),
                    }),
            )
            .collect(),
        new_class: "badge-other",
        // A pattern nothing matches, so only listed tags can be added.
        pattern: if may_create {
            "^[A-Za-z0-9 _]+$"
        } else {
            "(?!)"
        },
        hint: if may_create {
            "cannot be a tag: use letters, digits, and spaces"
        } else {
            "is not a tag; only an admin can make a new one"
        },
        keep_case: false,
    })
}

/// The × on a removable badge.
fn cross() -> Markup {
    html! {
        svg aria-hidden="true" viewBox="0 0 12 12" class="size-2.5" {
            path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" {}
        }
    }
}

/// A tag, linking to the builds that carry it.
fn badge(tag: &Tag) -> Markup {
    html! {
        a href={ "/?" (form_urlencoded::Serializer::new(String::new()).append_pair("tag", &tag.name).finish()) }
            title=(kind_label(&tag.kind))
            class={ "badge badge-" (tag.kind) " relative z-10 hover:no-underline hover:brightness-95" } {
            (tag.label())
        }
    }
}

/// A tag kind as people read it, such as `Restriction`.
fn kind_label(kind: &str) -> &str {
    match kind {
        "restriction" => "Restriction",
        "failing" => "Simulation",
        _ => "Other",
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

/// A 3D view of the schematic at `file`, an address the browser can fetch,
/// drawn by schematic-renderer with the vanilla resource pack it publishes
/// alongside its own build.
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
        script type="module" { (PreEscaped(VIEWER_SCRIPT)) }
    }
}

const VIEWER_IMPORTS: &str = r#"{ "imports": {
  "three": "https://cdn.jsdelivr.net/npm/three@0.184.0/build/three.module.js",
  "nucleation": "https://cdn.jsdelivr.net/npm/nucleation@0.2.18/nucleation.js"
} }"#;

const VIEWER_SCRIPT: &str = r#"
const CDN = "https://cdn.jsdelivr.net/npm";
const { SchematicRenderer } = await import(`${CDN}/schematic-renderer@1.6.1/dist/schematic-renderer.es.js`);
const canvas = document.getElementById("viewer");
const status = document.getElementById("viewer-status");
const say = (text) => { status.textContent = text; };
new SchematicRenderer(canvas, {}, {
  vanilla: () => fetch(`${CDN}/schematic-renderer@1.6.1/dist/pack.zip`).then((r) => r.blob()),
}, {
  enableDragAndDrop: false,
  callbacks: {
    onRendererInitialized: async (renderer) => {
      try {
        await renderer.schematicManager.loadSchematicFromURL(canvas.dataset.src, "schematic");
        renderer.cameraManager.focusOnSchematics();
        status.hidden = true;
      } catch (error) {
        say(`Could not load the schematic: ${error.message}`);
      }
    },
  },
});
"#;

pub fn about(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "About",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "About" }
                p class="mb-4" {
                    "A collection of Minecraft redstone schematics. Each build is stored with its facts, "
                    "so it can be found, compared, and downloaded under a name that describes it."
                }
                p class="mb-4" {
                    a href="/about/categories" { "How builds are sorted" }
                    " explains categories, designs, measures, tags, and what search and the leaderboards go by."
                }
            }
        },
    )
}

/// The page for an address that names nothing.
pub fn not_found(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "Not found",
        "",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "Not found" }
                p class="mb-4" {
                    "Nothing is here. The build or person may have been removed, or the address mistyped."
                }
                p { a href="/" { "See every build" } }
            }
        },
    )
}

/// The login page. `enabled` is false when the Discord settings are
/// missing; `message` explains a failed login.
pub fn login(enabled: bool, message: &str) -> Markup {
    base(
        None,
        "Log in",
        "/sessions/new",
        &html! {
            section class="mx-auto mt-8 flex max-w-sm flex-col gap-5 rounded-overlay border border-line bg-raised p-6 shadow-sm" {
                h1 class=(PAGE_TITLE) { "Log in" }
                p class="text-muted" {
                    "Log in with your Discord account to upload builds. Your account is made the first time you log in."
                }
                @if !message.is_empty() {
                    p role="alert" class="text-danger" { (message) }
                }
                @if enabled {
                    a href="/auth/discord" class="btn btn-primary" { "Log in with Discord" }
                } @else {
                    p role="alert" class="text-danger" {
                        "Discord login is not set up on this server: DISCORD_CLIENT_ID, DISCORD_CLIENT_SECRET, and DISCORD_REDIRECT_URI must be set."
                    }
                }
            }
        },
    )
}

/// The admin's list of accounts, each with a form to set its role.
/// The admin's list of accounts; `message` says why a change was refused.
pub fn users(admin: &User, users: &[User], message: &str) -> Markup {
    base(
        Some(admin),
        "Users",
        "/users",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-2" } { "Users" }
            p class="mb-6 max-w-prose text-muted" {
                "Mods edit any build; admins also delete builds and set roles. Confirm a Minecraft name only once you know it is theirs: it lets them edit the builds crediting it."
            }
            @if !message.is_empty() {
                p role="alert" class="mb-6 text-danger" { (message) }
            }
            div class="max-w-calm overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Account" }
                            th scope="col" class=(TABLE_HEAD) { "Minecraft name" }
                            th scope="col" class=(TABLE_HEAD) { "Role" }
                        }
                    }
                    tbody {
                        @for user in users {
                            (user_row(admin, user))
                        }
                    }
                }
            }
        },
    )
}

/// One account's row on the users page, redrawn by fixi when one of its
/// forms changes it.
pub fn user_row(admin: &User, user: &User) -> Markup {
    let row = format!("user-{}", user.username);
    let target = format!("#{row}");
    html! {
        tr id=(row) {
            th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                (account_link(user))
            }
            td class=(TABLE_CELL) { (minecraft_cell(user, &target)) }
            td class=(TABLE_CELL) {
                @if user.id == admin.id {
                    span class="text-muted" { "admin (you)" }
                } @else {
                    @let action = format!("/users/{}/role", user.username);
                    form action=(action) method="post"
                        fx-action=(action) fx-method="post" fx-target=(target) class="flex gap-2" {
                        label for={ "role-" (user.username) } class="sr-only" {
                            "Role for " (user.name())
                        }
                        select id={ "role-" (user.username) } name="role" class="input min-h-9 w-auto" {
                            @for role in Role::ALL {
                                option value=(role.as_str()) selected[role == user.role] { (role.as_str()) }
                            }
                        }
                        button type="submit" class="btn btn-quiet" { "Set" }
                    }
                }
            }
        }
    }
}

/// An account's Minecraft name on the users page, with the button that
/// confirms it or withdraws the confirmation.
fn minecraft_cell(user: &User, target: &str) -> Markup {
    let Some(name) = &user.minecraft_name else {
        // None given yet, so the admin may give one.
        let id = format!("minecraft-{}", user.username);
        let action = format!("/users/{}/minecraft/name", user.username);
        return html! {
            form action=(action) method="post"
                fx-action=(action) fx-method="post" fx-target=(target)
                class="flex items-center gap-2" {
                label for=(id) class="sr-only" { "Minecraft name for " (user.name()) }
                input id=(id) name="minecraft_name" required
                    pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                    placeholder="None given" class="input min-h-8 w-40";
                button type="submit" class="btn btn-quiet min-h-8 px-3" { "Set" }
            }
        };
    };
    let (value, action) = if user.minecraft_verified {
        ("no", "Withdraw")
    } else {
        ("yes", "Confirm")
    };
    let verify = format!("/users/{}/minecraft/verify", user.username);
    html! {
        form action=(verify) method="post"
            fx-action=(verify) fx-method="post" fx-target=(target)
            class="flex flex-wrap items-center gap-x-3 gap-y-1" {
            span { (name) }
            span class="text-meta text-muted" {
                (if user.minecraft_verified { "confirmed" } else { "not confirmed" })
            }
            input type="hidden" name="verified" value=(value);
            button type="submit" class="btn btn-quiet min-h-8 px-3"
                aria-label={ (action) " " (name) " as " (user.name()) "'s Minecraft name" } { (action) }
        }
    }
}

/// An account's avatar and name, linking to its page.
fn account_link(user: &User) -> Markup {
    html! {
        a href={ "/users/" (user.username) } class="inline-flex items-center gap-2" {
            @if let Some(picture) = user.picture(24) { (picture) }
            (user.name())
        }
        // The Discord username, which says who it is when its name does not.
        @if user.name() != user.username {
            span class="ml-2 text-meta text-muted" { "@" (user.username) }
        }
    }
}

/// An account's page: who they are and what they uploaded.
/// The builds a profile lists.
pub struct ProfileBuilds {
    /// Credited to the account's Minecraft name as builder.
    pub built: Vec<Schematic>,
    /// Credited to it as contributor.
    pub contributed: Vec<Schematic>,
    /// Uploaded by the account through the site.
    pub uploaded: Vec<Schematic>,
}

/// An account's page: the builds it built, helped with, and uploaded. Its
/// owner can set their Minecraft name here; `message` says why a name was
/// refused.
pub fn profile(
    viewer: Option<&User>,
    account: &User,
    builds: &ProfileBuilds,
    message: &str,
) -> Markup {
    let path = format!("/users/{}", account.username);
    let own = viewer.is_some_and(|viewer| viewer.id == account.id);
    base(
        viewer,
        account.name(),
        &path,
        &html! {
            div class="mb-8 flex items-center gap-4" {
                @if let Some(picture) = account.picture(64) { (picture) }
                div {
                    h1 class=(PAGE_TITLE) { (account.name()) }
                    p class="text-muted" {
                        "@" (account.username) " · " (account.role.as_str())
                    }
                    @if let Some(name) = &account.minecraft_name {
                        p {
                            "Minecraft: "
                            a href=(player_href(name)) { (name) }
                            @if !account.minecraft_verified {
                                span class="text-meta text-muted" { " · not yet confirmed" }
                            }
                        }
                    }
                }
            }
            @if own {
                form action={ (path) "/minecraft" } method="post" class="mb-8 flex max-w-form flex-col gap-2" {
                    label for="minecraft-name" class="text-meta font-semibold text-muted" { "Your Minecraft name" }
                    div class="flex gap-2" {
                        input id="minecraft-name" name="minecraft_name"
                            value=(account.minecraft_name.as_deref().unwrap_or_default())
                            pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                            placeholder="Your in-game name"
                            aria-invalid=[(!message.is_empty()).then_some("true")]
                            aria-describedby="minecraft-help"
                            class="input flex-1";
                        button type="submit" class="btn btn-quiet" { "Save" }
                    }
                    @if message.is_empty() {
                        p id="minecraft-help" class="text-meta text-muted" {
                            "Links you to your builds. Once an admin confirms it, you can edit them."
                        }
                    } @else {
                        p id="minecraft-help" role="alert" class="text-meta text-danger" { (message) }
                    }
                }
            }
            section aria-labelledby="built-heading" class="mb-10" {
                h2 id="built-heading" class={ (SECTION_TITLE) " mb-3" } { "Built" }
                @match (&account.minecraft_name, builds.built.is_empty()) {
                    (None, _) if own => {
                        p class="text-muted" { "Give your Minecraft name above to list the builds credited to it." }
                    }
                    (None, _) => {
                        p class="text-muted" { "No Minecraft name given, so their builds can't be listed." }
                    }
                    (Some(name), true) => {
                        p class="text-muted" { "No builds credit " (name) " as builder yet." }
                    }
                    (Some(_), false) => { (cards(&builds.built, &[])) }
                }
            }
            @if !builds.contributed.is_empty() {
                section aria-labelledby="contributed-heading" class="mb-10" {
                    h2 id="contributed-heading" class={ (SECTION_TITLE) " mb-3" } { "Contributed to" }
                    (cards(&builds.contributed, &[]))
                }
            }
            section aria-labelledby="uploads-heading" {
                h2 id="uploads-heading" class={ (SECTION_TITLE) " mb-3" } { "Uploads" }
                @if builds.uploaded.is_empty() {
                    p class="text-muted" { "No uploads yet." }
                } @else {
                    (cards(&builds.uploaded, &[]))
                }
            }
        },
    )
}

/// The upload page's first step: choosing a file.
/// The fields for choosing a file, the same wherever one is chosen: the
/// file or a Schemati link, and how to prepare it. `form` names the form
/// they belong to when they stand outside it; `keep_paste` offers keeping
/// where the file pastes from, which a new version takes from its build.
pub fn file_fields(
    form: Option<&str>,
    keep_paste: bool,
    schemati: &Connection,
) -> Markup {
    html! {
        label class="field" {
            span { "Schematic file" }
            input type="file" name="file" accept=".schem" form=[form] class="input py-1.5";
        }
        label class="field" {
            span { "Or a Schemati link" }
            input type="url" name="schemati" inputmode="url" autocomplete="off" form=[form]
                placeholder="https://schemati.openredstone.org/download?file=…"
                aria-describedby="schemati-help" class="input";
            span id="schemati-help" class="text-meta font-normal" {
                "Run " code { "//download <name>" } " in game to get one. It works for seven days."
            }
        }
        (schemati::own_file_field(schemati, form))
        @if keep_paste {
            label class="flex items-start gap-2" {
                input type="checkbox" name="keep_paste" value="yes" form=[form] class="mt-1 size-4 appearance-auto";
                span {
                    "Keep its paste position"
                    span class="block text-meta text-muted" {
                        "Unticked, " code { "//paste" } " puts you one block south of the build's south-east bottom corner, as for every build here. "
                        "Tick it to paste the build where you copied it from."
                    }
                }
            }
        }
        label class="flex items-start gap-2" {
            input type="checkbox" name="standardize" value="yes" checked form=[form] class="mt-1 size-4 appearance-auto";
            span {
                "Standardize it"
                span class="block text-meta text-muted" {
                    "Turns every block that isn't redstone, an input, or an output white. Glass, slabs, and stairs become white stained glass."
                }
            }
        }
        label class="flex items-start gap-2 pl-6" {
            input type="checkbox" name="whiten_gray" value="yes" checked form=[form] class="mt-1 size-4 appearance-auto";
            span {
                "Turn gray blocks white too"
                span class="block text-meta text-muted" {
                    "Gray marks parts only the first copy of a stack keeps, such as an input's driver. "
                    "Untick this if your stackable build uses gray that way."
                }
            }
        }
    }
}

pub fn upload(user: &User, message: &str, connection: &Connection) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-6" } { "Upload a build" }

                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    class="mb-10 flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
                    (file_fields(None, true, connection))
                    @if !message.is_empty() {
                        p role="alert" class="text-danger" { (message) }
                    }
                    div { button type="submit" class="btn btn-primary" { "Preview" } }
                }
                (schemati_guide())
                (marking_guide())
            }
        },
    )
}

/// How to get a Schemati link for a build on the Open Redstone Engineers
/// server.
fn schemati_guide() -> Markup {
    html! {
        section aria-labelledby="schemati-heading" class="mb-10 max-w-prose leading-prose" {
            h2 id="schemati-heading" class={ (SECTION_TITLE) " mb-3" } { "From Open Redstone" }
            ol class="flex list-decimal flex-col gap-1.5 pl-5" {
                li { "Select the build with " code { "//wand" } ", then run " code { "//copy" } "." }
                li { "Run " code { "//save <name>" } " to keep it in Schemati." }
                li {
                    "Pick it from your Schemati above. Without Schemati connected, run "
                    code { "//download <name>" } " and paste the link instead."
                }
            }
        }
    }
}

/// One role in the marking guide: its colours, as swatch and name, and
/// what goes with them.
struct Marking {
    role: &'static str,
    colours: &'static [(&'static str, &'static str)],
    block: &'static str,
    rule: &'static str,
}

/// Each part of a build and how to mark it, with Minecraft's concrete
/// colours for the swatches.
const MARKINGS: [Marking; 7] = [
    Marking {
        role: "Input A",
        colours: &[("bg-[#5ea818]", "lime")],
        block: "concrete",
        rule: "A lever on the south face of each bit.",
    },
    Marking {
        role: "Inputs B, C, D",
        colours: &[
            ("bg-[#2389c6]", "light blue"),
            ("bg-[#157788]", "cyan"),
            ("bg-[#2c2e8f]", "blue"),
        ],
        block: "concrete",
        rule: "A lever on any side of each bit.",
    },
    Marking {
        role: "Control line",
        colours: &[("bg-[#a9309f]", "magenta")],
        block: "concrete",
        rule: "A lever or button, and a sign naming it, such as carry in or reset. The only one on an adder, incrementer, or decrementer needs no sign.",
    },
    Marking {
        role: "Outputs 1, 2, 3",
        colours: &[
            ("bg-[#f0af15]", "yellow"),
            ("bg-[#e06100]", "orange"),
            ("bg-[#603b1f]", "brown"),
        ],
        block: "concrete or stained glass",
        rule: "When they make one value, yellow holds the lowest bits.",
    },
    Marking {
        role: "Status line",
        colours: &[("bg-[#d5658e]", "pink")],
        block: "concrete or stained glass",
        rule: "A sign naming it, such as carry out or overflow. The only one on an adder, incrementer, or decrementer needs no sign.",
    },
    Marking {
        role: "First copy only",
        colours: &[("bg-[#373a3e]", "gray")],
        block: "concrete or stained glass",
        rule: "Parts only the first copy of a stack keeps, such as an input's driver. Stacking leaves them out of later copies.",
    },
    Marking {
        role: "Everything else",
        colours: &[("bg-[#cfd5d6]", "white")],
        block: "concrete or stained glass",
        rule: "Use no other colours or blocks. Redstone parts stay as they are.",
    },
];

/// How a build marks its inputs and outputs so the site can read it, from
/// `conventions.md`, kept to what an upload needs.
fn marking_guide() -> Markup {
    html! {
        section aria-labelledby="guide-heading" {
            h2 id="guide-heading" class={ (SECTION_TITLE) " mb-1" } { "How to mark a build" }
            p class="mb-4 max-w-prose text-muted" {
                "The site reads a build's inputs and outputs from its colours and signs."
            }
            div class="overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Part" }
                            th scope="col" class=(TABLE_HEAD) { "Block" }
                            th scope="col" class=(TABLE_HEAD) { "Rule" }
                        }
                    }
                    tbody {
                        @for marking in &MARKINGS {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold whitespace-nowrap" } { (marking.role) }
                                td class=(TABLE_CELL) {
                                    span class="flex flex-wrap items-center gap-x-3 gap-y-1" {
                                        @for (swatch, name) in marking.colours {
                                            span class="flex items-center gap-1.5 whitespace-nowrap" {
                                                span aria-hidden="true" class={ "size-3.5 shrink-0 rounded-[3px] border border-black/15 " (swatch) } {}
                                                (name)
                                            }
                                        }
                                    }
                                    span class="text-meta text-muted" { (marking.block) }
                                }
                                td class=(TABLE_CELL) { (marking.rule) }
                            }
                        }
                    }
                }
            }
            ul class="mt-4 flex max-w-prose list-disc flex-col gap-1.5 pl-5" {
                li { "Each colour is one bus. Its first bit is the one nearest the south-east bottom corner." }
                li {
                    "To start a bus from the other end, put a sign reading " code { "lsb" } " on its first bit. "
                    "The buses running alongside it follow, unless they have their own."
                }
                li { "To name a bus, put a sign on any of its blocks, such as " code { "addend" } "." }
                li { "Place each lever so it touches only its input block." }
                li { "Save every lever off. A lever saved on makes its input inverted." }
                li { "A build with its own layout, such as a display, needs none of this. Tick “follows its own layout” on the next page." }
            }
        }
    }
}
