//! Full page templates.
//!
//! These return complete HTML documents wrapped in the base layout.

use maud::{Markup, PreEscaped, html};

use crate::{
    APP_NAME,
    interface::{Bit, Interface},
    lint::Violation,
    models::{
        schematic::{
            self, Category, Design, Flag, Schematic, StackDirection, Tag,
        },
        user::{Role, User},
    },
    search::Term,
};

use super::{
    components::base,
    details::{self, STACK_DIRECTIONS, StackRow},
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
pub use leaderboard::{LeaderboardPage, leaderboard};

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
                    Some((&Home { category: "", ..here }.href(), matched)),
                    |name| Home { category: name, ..here }.href(),
                ))
                div { (listing_body(listing, here, title, &count)) }
            }
        },
    )
}

/// The categories to narrow a page to, each with its count and its
/// address from `href`; `current` is the chosen one, and `all` the link
/// and count for every category, if the page has one.
fn category_sidebar(
    facets: &[Facet],
    current: &str,
    all: Option<(&str, usize)>,
    href: impl Fn(&str) -> String,
) -> Markup {
    let item = "flex items-center justify-between gap-3 rounded-control px-2.5 py-1.5 text-ink hover:bg-selection hover:no-underline aria-[current=page]:bg-selection aria-[current=page]:font-semibold";
    html! {
        nav aria-labelledby="categories-heading" class="desktop:sticky desktop:top-4 desktop:self-start" {
            h2 id="categories-heading" class="mb-2 px-2.5 text-meta font-semibold text-muted" { "Categories" }
            ul class="flex flex-wrap gap-1 desktop:flex-col" {
                @if let Some((all_href, matched)) = all {
                    li {
                        a href=(all_href) class=(item)
                            aria-current=[current.is_empty().then_some("page")] {
                            span { "All" }
                            span class="text-meta text-muted tabular-nums" { (matched) }
                        }
                    }
                }
                @for facet in facets {
                    li {
                        a href=(href(&facet.name)) class=(item)
                            aria-current=[(facet.name == current).then_some("page")] {
                            span { (facet.label) }
                            span class="text-meta text-muted tabular-nums" { (facet.count) }
                        }
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
    // A single standard category has a leaderboard to switch to.
    let ranked = !category.contains(',')
        && category != "other"
        && listing.facets.iter().any(|f| f.name == category);
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
                    "Every word must match: a category, orientation, tag, width such as 8b, builder such as "
                    code { "builder:embizone" } ", "
                    code { "stackable" } ", or part of the name. Put " code { "-" } " or "
                    code { "not" } " before a word to leave those builds out, and quotes around a tag with spaces."
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
                (cards(shown))
            } @else {
                (table(shown))
            }
    }
}

/// A category's builds and its leaderboard, as two tabs; `leaderboard` is
/// whether that is the one shown.
fn category_tabs(category: &str, leaderboard: bool) -> Markup {
    let builds = form_urlencoded::Serializer::new(String::new())
        .append_pair("category", category)
        .finish();
    html! {
        nav aria-label="Builds or leaderboard" class=(SWITCHES) {
            a href={ "/?" (builds) } class=(SWITCH)
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

fn cards(schematics: &[Schematic]) -> Markup {
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
                        @if !s.tags.is_empty() {
                            ul class="flex flex-wrap gap-1.5" aria-label="Tags" {
                                @for tag in &s.tags {
                                    li { (badge(tag)) }
                                }
                            }
                        }
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
            " · " (name_links(&s.builders, "builder"))
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

/// In-game names, each a link to the builds crediting it in `role`, such
/// as `builder`.
fn name_links(names: &[String], role: &str) -> Markup {
    html! {
        @for (i, name) in names.iter().enumerate() {
            @if i > 0 { ", " }
            (search_link(name, &format!("{role}:{name}")))
        }
    }
}

fn table(schematics: &[Schematic]) -> Markup {
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
                                        @if !s.tags.is_empty() {
                                            ul class="mt-1.5 flex flex-wrap gap-1.5" aria-label="Tags" {
                                                @for tag in &s.tags {
                                                    li { (badge(tag)) }
                                                }
                                            }
                                        }
                                    }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.orientation_label()) }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.encoding_label().unwrap_or_default()) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (latency(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (size(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (volume(s)) }
                                    td class=(TABLE_CELL) { (name_links(&s.builders, "builder")) }
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
}

/// One schematic's page, with an edit button for a mod.
pub fn schematic(
    viewer: Option<&User>,
    s: &Schematic,
    analysis: &Result<Analysis, String>,
) -> Markup {
    let name = s.name();
    let path = format!("/schematics/{}", s.id);
    base(
        viewer,
        &name,
        &path,
        &html! {
            div class="mb-6 flex flex-wrap items-start justify-between gap-4" {
                h1 class=(PAGE_TITLE) { (name) }
                div class="flex gap-2" {
                    @if viewer.is_some_and(User::is_admin) {
                        button type="button" commandfor="delete" command="show-modal" class="btn btn-danger" { "Delete" }
                        (delete_dialog(s))
                    }
                    @if viewer.is_some_and(User::can_edit) {
                        a href={ (path) "/edit" } class="btn btn-quiet" { "Edit" }
                    }
                    button type="button" commandfor="download" command="show-modal" class="btn btn-primary" { "Download" }
                    (download_dialog(s))
                }
            }
            (beside_viewer(&format!("{path}/download?preview=yes"), &schematic_info(s)))
            @match analysis {
                _ if !s.conventional => {
                    p class="mt-10 max-w-prose text-muted" {
                        "This build follows its own layout rather than the building conventions, so its inputs and outputs are not read from the file and it is not linted."
                    }
                }
                Ok(analysis) => (interface_section(analysis, true)),
                Err(err) => {
                    p class="mt-10 text-muted" { "Could not read the file's inputs and outputs: " (err) }
                }
            }
        },
    )
}

/// The build's inputs and outputs, and, with `with_violations`, the
/// conventions it breaks.
fn interface_section(analysis: &Analysis, with_violations: bool) -> Markup {
    let Interface { inputs, outputs } = &analysis.interface;
    html! {
        section aria-labelledby="interface-heading" class="mt-12" {
            h2 id="interface-heading" class={ (SECTION_TITLE) " mb-1" } { "Inputs and outputs" }
            p class="mb-4 text-meta text-muted" {
                "Read from the file's levers and buttons, yellow output blocks, and signs."
            }
            div class="overflow-x-auto" {
                table class="w-full max-w-calm border-collapse" {
                    caption class="sr-only" { "Inputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Input" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                            th scope="col" class=(TABLE_HEAD) { "Strength" }
                            th scope="col" class=(TABLE_HEAD) { "Drive" }
                        }
                    }
                    tbody {
                        @for port in inputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold" } { (port.name) }
                                @if port.signal_strength {
                                    // Each block is a hex digit, its strength the value.
                                    td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len() * 4) }
                                    td class=(TABLE_CELL) { "the value" }
                                    td class=(TABLE_CELL) { "signal strength" }
                                } @else {
                                    td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (port.strength) " or more" }
                                    td class=(TABLE_CELL) { (uniform(&port.bits, |b| (if b.hard { "hard" } else { "soft" }).to_owned())) }
                                }
                            }
                        }
                    }
                }
            }
            div class="mt-8 overflow-x-auto" {
                table class="w-full max-w-calm border-collapse" {
                    caption class="sr-only" { "Outputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Output" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                        }
                    }
                    tbody {
                        @for port in outputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold" } { (port.name) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                            }
                        }
                    }
                }
            }
        }
        @if with_violations && !analysis.violations.is_empty() {
            section aria-labelledby="lint-heading" class="mt-12 max-w-calm" {
                h2 id="lint-heading" class={ (SECTION_TITLE) " mb-3" } { "Breaks the building conventions" }
                (violation_list(&analysis.violations))
            }
        }
    }
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

/// One value shared by every bit, or `mixed`.
fn uniform(bits: &[Bit], value: impl Fn(&Bit) -> String) -> String {
    let mut values = bits.iter().map(value);
    let first = values.next().unwrap_or_default();
    if values.all(|v| v == first) {
        first
    } else {
        "mixed".to_owned()
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
/// glass corner markers for `WorldEdit`.
fn download_dialog(s: &Schematic) -> Markup {
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
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Download" }
                    button type="button" commandfor="download" command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}

/// A schematic's tags and details, beside its viewer.
fn schematic_info(s: &Schematic) -> Markup {
    html! {
        div class="flex flex-col gap-8" {
            section aria-labelledby="tags-heading" {
                h2 id="tags-heading" class={ (SECTION_TITLE) " mb-3" } { "Tags" }
                @if s.tags.is_empty() {
                    p class="text-muted" { "No tags yet." }
                } @else {
                    ul class="flex flex-wrap gap-1.5" {
                        @for tag in &s.tags {
                            li { (badge(tag)) }
                        }
                    }
                }
            }
            section aria-labelledby="details-heading" {
                h2 id="details-heading" class={ (SECTION_TITLE) " mb-3" } { "Details" }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-2" {
                    // A category off the list has no list of its own to link to.
                    (field("Category", html! {
                        @if s.category_label.is_some() {
                            a href={ "/?category=" (s.category) } { (s.category_label()) }
                        } @else {
                            (s.category_label())
                        }
                    }))
                    @if let Some(design) = s.design_label() {
                        (field("Design", search_link(&design, &s.design)))
                    }
                    (field("Width", s.width.map_or_else(
                        || html! { "unknown" },
                        |w| search_link(&s.width_label().unwrap_or_default(), &format!("{w}b")),
                    )))
                    (field("Orientation", search_link(s.orientation_label(), &s.orientation_label().to_lowercase())))
                    @if let Some(encoding) = s.encoding_label() {
                        (field("Encoding", search_link(encoding, &encoding.to_lowercase())))
                    }
                    (field("Latency", s.latency.map_or_else(|| "unknown".to_owned(), |t| format!("{t} ticks"))))
                    (field("Synchronized", match s.synchronized {
                        Some(true) => search_link("yes", "sync"),
                        Some(false) => search_link("no", "unsync"),
                        None => html! { "unknown" },
                    }))
                    (field("Stacks", if s.stacking.is_empty() {
                        html! { "no" }
                    } else {
                        search_link(&stacking(s), "stackable")
                    }))
                    (field("Size", format!("{} (HWD)", size(s))))
                    (field("Volume", volume(s)))
                    (field(&plural("Builder", &s.builders), if s.builders.is_empty() {
                        html! { "unknown" }
                    } else {
                        name_links(&s.builders, "builder")
                    }))
                    @if !s.contributors.is_empty() {
                        (field(&plural("Contributor", &s.contributors), name_links(&s.contributors, "contributor")))
                    }
                    (field("Built", s.built_on.as_deref().unwrap_or("unknown")))
                    @for flag in &s.flags {
                        (field(&flag.label, search_link("yes", &flag.name)))
                    }
                    (field("Uploaded", s.added_at.get(..10).unwrap_or(&s.added_at)))
                    @if let Some(edited) = &s.edited_at {
                        (field("Last edited", edited.get(..10).unwrap_or(edited)))
                    }
                    (field("File", html! { code class="break-all" { (s.filename()) } }))
                    (field("Downloads", s.downloads.to_string()))
                    (field("Imported by", s.importer().map_or_else(
                        || html! { "unknown" },
                        |name| search_link(name, &format!("importer:{name}")),
                    )))
                    (field("Conventions", if s.conventional {
                        html! { "followed" }
                    } else {
                        search_link("not followed", "unconventional")
                    }))
                }
            }
            @if let Some(description) = &s.description {
                section aria-labelledby="description-heading" {
                    h2 id="description-heading" class={ (SECTION_TITLE) " mb-3" } { "Description" }
                    p class="leading-prose whitespace-pre-line" { (description) }
                }
            }
        }
    }
}

/// What the edit page shows.
pub struct EditSchematic<'a> {
    pub s: &'a Schematic,
    /// The values sent, when a save was refused; else the build's own.
    pub sent: Option<&'a details::Form>,
    pub choices: &'a Choices,
    /// Every tag, to suggest.
    pub tags: &'a [Tag],
    pub message: &'a str,
}

/// A mod's page for changing a build's details and tags: the form on the
/// left, the build on the right, as when uploading.
pub fn edit_schematic(user: &User, e: &EditSchematic) -> Markup {
    let s = e.s;
    let path = format!("/schematics/{}", s.id);
    let number = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
    let (width, latency) = (number(s.width), number(s.latency));
    let own_tags: Vec<String> =
        s.tags.iter().map(|tag| tag.name.clone()).collect();
    let own_flags: Vec<String> =
        s.flags.iter().map(|flag| flag.name.clone()).collect();
    // A build that does not stack yet starts with a likely direction.
    let stack_rows: Vec<StackRow> = if s.stacking.is_empty() {
        vec![StackRow::suggested(&s.orientation, s.size)]
    } else {
        s.stacking.iter().map(StackRow::of).collect()
    };
    let (values, chosen_tags) = match e.sent {
        Some(form) => (form.fields(), form.tags.as_slice()),
        None => (
            Fields {
                category: &s.category,
                width: &width,
                orientation: &s.orientation,
                encoding: s.encoding.as_deref().unwrap_or_default(),
                latency: &latency,
                synchronized: s.synchronized,
                builders: &s.builders,
                contributors: &s.contributors,
                built_on: s.built_on.as_deref().unwrap_or(""),
                description: s.description.as_deref().unwrap_or(""),
                given_name: s.given_name.as_deref().unwrap_or(""),
                design: &s.design,
                flags: &own_flags,
                stackable: !s.stacking.is_empty(),
                stack_rows: &stack_rows,
            },
            own_tags.as_slice(),
        ),
    };
    base(
        Some(user),
        &format!("Edit {}", s.name()),
        &path,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { "Edit " (s.name()) }
            (beside_viewer(&format!("{path}/download?preview=yes"), &html! {
                form action={ (path) "/edit" } method="post" class="flex flex-col gap-4" {
                    (fields(&values, e.choices))
                    (tag_picker(e.tags, chosen_tags))
                    @if !e.message.is_empty() {
                        p role="alert" class="text-danger" { (e.message) }
                    }
                    div class="flex items-center gap-4" {
                        button type="submit" class="btn btn-primary" { "Save" }
                        a href=(path) { "Cancel" }
                    }
                }
            }))
            script defer src=(super::form_js_url()) {}
        },
    )
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
        // A category off the standard list shows as other, with its name.
        @let standard = f.category.is_empty()
            || choices.categories.iter().any(|category| category.name == f.category);
        @let named = choices
            .categories
            .iter()
            .any(|category| category.name == f.category && category.named);
        @let has_designs = choices
            .designs
            .iter()
            .any(|design| design.category == f.category);
        label class="field" {
            span { "Category" }
            select name="category" required class="input" data-category {
                @if f.category.is_empty() {
                    option value="" selected { "Choose a category" }
                }
                @for category in &choices.categories {
                    option value=(category.name) selected[f.category == category.name]
                        data-named[category.named] {
                        (category.label)
                    }
                }
                option value="other" selected[!standard] { "Other" }
            }
        }
        label class="field" data-category-name {
            span { "Category name, if other" }
            input name="category_name" value=(if standard { "" } else { f.category })
                placeholder="such as line drawer" pattern="[A-Za-z0-9][A-Za-z0-9_ \\-]*"
                title="Words, numbers, and hyphens, such as 8-bit horizontal diode"
                class="input";
        }
        // For a category whose builds each have a name, such as bussing.
        label class="field" data-named-field hidden[!named] {
            span { "Name" }
            input name="given_name" value=(f.given_name) maxlength="60"
                placeholder="such as Bus Crossing" class="input";
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
                @let chosen = category.name == f.category;
                div class="flex flex-wrap items-center gap-x-6 gap-y-2" data-flags-for=(category.name) hidden[!chosen] {
                    @for flag in flags {
                        label class="flex items-center gap-2" {
                            input type="checkbox" name={ "flag:" (category.name) } value=(flag.name)
                                checked[f.flags.contains(&flag.name)] class="size-4 appearance-auto";
                            (flag.label)
                        }
                    }
                }
            }
        }
    }
}

/// The inputs for a build's details, shared by editing and uploading.
fn fields(f: &Fields, choices: &Choices) -> Markup {
    // A category off the list, such as a piston door, has no width or encoding.
    let other = !f.category.is_empty()
        && !choices
            .categories
            .iter()
            .any(|category| category.name == f.category);
    html! {
        (category_fields(f, choices))
        label class="field" data-standard-only hidden[other] {
            span { "Width in bits" }
            input name="width" type="number" min="1" value=(f.width) class="input tabular-nums";
        }
        label class="field" {
            span { "Orientation" }
            select name="orientation" class="input" {
                @for (value, label) in [("horz", "Horizontal"), ("vert", "Vertical"), ("diag", "Diagonal")] {
                    option value=(value) selected[f.orientation == value] { (label) }
                }
            }
        }
        label class="field" data-standard-only hidden[other] {
            span { "Encoding" }
            select name="encoding" class="input" {
                @for (value, label) in [("bin", "Binary"), ("hex", "Hex")] {
                    option value=(value) selected[f.encoding == value] { (label) }
                }
            }
        }
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
        (stacking_fields(f))
        (person_field("Builders", "builder", "Add a builder's in-game name", f.builders, choices))
        (person_field("Contributors", "contributor", "Add someone who helped", f.contributors, choices))
        div class="field" {
            label for="built-on" class="text-meta font-semibold text-muted" { "Built on" }
            div class="flex items-center gap-4" data-date-field {
                input id="built-on" name="built_on" type="date" value=(f.built_on) class="input flex-1";
                label class="flex items-center gap-2 whitespace-nowrap" {
                    input type="checkbox" name="built_on_unknown" value="yes"
                        checked[f.built_on.is_empty()] class="size-4 appearance-auto";
                    "Don't know"
                }
            }
        }
        label class="field" {
            span { "Description" }
            textarea name="description" rows="3" class="input py-2 leading-prose" { (f.description) }
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
        fieldset class="flex flex-col gap-3" data-stacking {
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
        li class="group" {
            span class={ "badge pr-1 " (p.class) } {
                span data-label { (p.label) }
                button type="button" data-remove
                    aria-label={ "Remove " (if p.label.is_empty() { "this" } else { &p.label }) }
                    class="flex size-4.5 cursor-pointer items-center justify-center rounded-full opacity-0 group-hover:opacity-70 hover:bg-current/15 hover:opacity-100 focus-visible:opacity-100 pointer-coarse:opacity-70" {
                    (cross())
                }
            }
            input type="hidden" name=(name) value=(p.value);
        }
    }
}

/// The tag field of the upload form.
fn tag_picker(tags: &[Tag], chosen: &[String]) -> Markup {
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
        pattern: "^[A-Za-z0-9 _]+$",
        hint: "cannot be a tag: use letters, digits, and spaces",
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
        "timing" => "Timing",
        _ => "Other",
    }
}

fn field(name: &str, value: impl maud::Render) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}

/// Each direction a build stacks along, or `no`.
fn stacking(s: &Schematic) -> String {
    if s.stacking.is_empty() {
        return "no".to_owned();
    }
    s.stacking
        .iter()
        .map(StackDirection::describe)
        .collect::<Vec<_>>()
        .join("; ")
}

fn latency(s: &Schematic) -> String {
    s.latency.map(|t| format!("{t} ticks")).unwrap_or_default()
}

fn volume(s: &Schematic) -> String {
    format!("{} blocks", s.volume())
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
                h2 class={ (SECTION_TITLE) " mt-8 mb-3" } { "Tags" }
                p class="mb-3" { "A tag's colour gives its kind." }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2" {
                    dt { span class="badge badge-restriction" { "Restriction" } }
                    dd { "Components the build does without." }
                    dt { span class="badge badge-timing" { "Timing" } }
                    dd { "How the build is clocked, when it differs from what the category implies." }
                    dt { span class="badge badge-other" { "Other" } }
                    dd { "Anything else worth searching by." }
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
pub fn users(admin: &User, users: &[User]) -> Markup {
    base(
        Some(admin),
        "Users",
        "/users",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-2" } { "Users" }
            p class="mb-6 max-w-prose text-muted" {
                "A user uploads builds. A mod also edits any build's details and tags. An admin also deletes builds and sets roles."
            }
            div class="max-w-calm overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Account" }
                            th scope="col" class=(TABLE_HEAD) { "Role" }
                        }
                    }
                    tbody {
                        @for user in users {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                    (account_link(user))
                                }
                                td class=(TABLE_CELL) {
                                    @if user.id == admin.id {
                                        span class="text-muted" { "admin (you)" }
                                    } @else {
                                        form action={ "/users/" (user.username) "/role" } method="post" class="flex gap-2" {
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
                }
            }
        },
    )
}

/// An account's avatar and name, linking to its page.
fn account_link(user: &User) -> Markup {
    html! {
        a href={ "/users/" (user.username) } class="inline-flex items-center gap-2" {
            @if let Some(avatar) = user.avatar_url() {
                img src=(avatar) alt="" width="24" height="24" class="size-6 rounded-full";
            }
            (user.name())
        }
        @if user.display_name.is_some() {
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
                @if let Some(avatar) = account.avatar_url() {
                    img src=(avatar) alt="" width="64" height="64" class="size-16 rounded-full";
                }
                div {
                    h1 class=(PAGE_TITLE) { (account.name()) }
                    p class="text-muted" {
                        "@" (account.username) " · " (account.role.as_str())
                    }
                    @if let Some(name) = &account.minecraft_name {
                        p {
                            "Minecraft: "
                            a href=(search_href(&format!("builder:{name}"))) { (name) }
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
                            "Shown on your profile, linked to the builds you built. Leave it empty to remove it."
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
                    (Some(_), false) => { (cards(&builds.built)) }
                }
            }
            @if !builds.contributed.is_empty() {
                section aria-labelledby="contributed-heading" class="mb-10" {
                    h2 id="contributed-heading" class={ (SECTION_TITLE) " mb-3" } { "Contributed to" }
                    (cards(&builds.contributed))
                }
            }
            section aria-labelledby="uploads-heading" {
                h2 id="uploads-heading" class={ (SECTION_TITLE) " mb-3" } { "Uploads" }
                @if builds.uploaded.is_empty() {
                    p class="text-muted" { "No uploads yet." }
                } @else {
                    (cards(&builds.uploaded))
                }
            }
        },
    )
}

/// The upload page's first step: choosing a file.
pub fn upload(user: &User, message: &str) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-2" } { "Upload a build" }
                p class="mb-6 max-w-prose text-muted" {
                    "A Sponge " code { ".schem" } " file, yours or someone else's. "
                    "Next you'll see what the site reads from it, and describe it."
                }
                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    class="mb-10 flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
                    label class="field" {
                        span { "Schematic file" }
                        input type="file" name="file" accept=".schem" required class="input py-1.5";
                    }
                    label class="flex items-start gap-2" {
                        input type="checkbox" name="standardize" value="yes" checked class="mt-1 size-4 appearance-auto";
                        span {
                            "Standardize it: every block that isn't redstone, an input, or an output turns white, "
                            "and slabs and glass white stained glass. The inputs and outputs must already be marked, as below."
                        }
                    }
                    @if !message.is_empty() {
                        p role="alert" class="text-danger" { (message) }
                    }
                    div { button type="submit" class="btn btn-primary" { "Preview" } }
                }
                (marking_guide())
            }
        },
    )
}

/// One role in the marking guide: its colours, as swatch and name, and
/// what goes with them.
struct Marking {
    role: &'static str,
    colours: &'static [(&'static str, &'static str)],
    block: &'static str,
    rule: &'static str,
}

/// How a build marks its inputs and outputs so the site can read it, from
/// `conventions.md`, kept to what an upload needs.
fn marking_guide() -> Markup {
    // Minecraft's concrete colours, for the swatches.
    const MARKINGS: [Marking; 6] = [
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
            rule: "A lever or button, and a sign naming it, such as carry in or reset.",
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
            rule: "A sign naming it, such as carry out or overflow.",
        },
        Marking {
            role: "Everything else",
            colours: &[("bg-[#cfd5d6]", "white")],
            block: "concrete or stained glass",
            rule: "No other colours or blocks, besides redstone parts.",
        },
    ];
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
                li { "Each colour is one bus. A sign on any of its blocks names it, such as addend, low, or high; unnamed, inputs are A to D and outputs 1 to 3." }
                li { "A bus counts up from the bit nearest the south-east bottom corner." }
                li { "A lever touches nothing but its input block. Save inputs off, or on for an inverted input." }
                li { "Corner markers aren't needed; the box is trimmed to the build." }
                li { "A build with its own layout, such as a display, can skip all this: tick “follows its own layout” on the next page." }
            }
        }
    }
}

/// What the upload preview shows.
pub struct UploadPreview<'a> {
    /// The prepared file, base64, carried to publishing.
    pub data: &'a str,
    pub analysis: &'a Analysis,
    /// The build less its interface layers.
    pub size: [u16; 3],
    /// The id of a stored build with the same file, if any.
    pub duplicate: Option<&'a str>,
    pub fields: &'a Fields<'a>,
    pub choices: &'a Choices,
    pub tags: &'a [Tag],
    pub chosen_tags: &'a [String],
    pub unconventional: bool,
    pub message: &'a str,
}

/// The upload page's second step: checking what the site read and filling
/// in the details.
pub fn upload_preview(user: &User, p: &UploadPreview) -> Markup {
    let [x, y, z] = p.size;
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { "Check and describe it" }
            // The viewer fetches the file from a data address, since nothing
            // is stored until publishing.
            (beside_viewer(&format!("data:application/octet-stream;base64,{}", p.data), &html! {
                div class="flex flex-col gap-3" {
                    p { "Size: " span class="tabular-nums" { (y) "×" (x) "×" (z) " (HWD)" } }
                    @if let Some(id) = p.duplicate {
                        p role="alert" class="text-danger" {
                            "This exact file is already on the site: "
                            a href={ "/schematics/" (id) } { "see it here" } "."
                        }
                    }
                    a href="/upload" { "Choose a different file" }
                }
                // A refused publish comes back here, so say why first.
                @if !p.message.is_empty() {
                    p id="refusal" role="alert" tabindex="-1" autofocus
                        class="rounded-control border border-danger p-3 text-danger" {
                        strong { "Not published. " } (p.message)
                    }
                }
                @if !p.analysis.violations.is_empty() {
                    section aria-labelledby="lint-heading" class="flex flex-col gap-3" {
                        h2 id="lint-heading" class={ (SECTION_TITLE) " text-danger" } {
                            @match p.analysis.violations.len() {
                                1 => "Breaks a building convention",
                                count => { "Breaks " (count) " building conventions" }
                            }
                        }
                        p class="text-muted" {
                            "It can't be published until these are fixed: choose the file again with Standardize ticked, "
                            "fix them in-game, or tick “follows its own layout” below if it is a display or similar."
                        }
                        (violation_list(&p.analysis.violations))
                    }
                }
                (upload_form(p))
            }))
            (interface_section(p.analysis, false))
            script defer src=(super::form_js_url()) {}
        },
    )
}

/// The upload page's details form, one field to a line.
fn upload_form(p: &UploadPreview) -> Markup {
    html! {
            form action="/upload" method="post" class="flex flex-col gap-4" {
                h2 class=(SECTION_TITLE) { "Details" }
                input type="hidden" name="data" value=(p.data);
                (fields(p.fields, p.choices))
                (tag_picker(p.tags, p.chosen_tags))
                @if !p.analysis.violations.is_empty() || p.unconventional {
                    label class="flex items-start gap-2" {
                        input type="checkbox" name="unconventional" value="yes"
                            checked[p.unconventional] class="mt-1 size-4 appearance-auto";
                        span {
                            "This build follows its own layout, such as a display, so the building conventions do not apply to it."
                        }
                    }
                }
                div {
                    button type="submit" class="btn btn-primary" disabled[p.duplicate.is_some()] { "Publish" }
                }
            }
    }
}
