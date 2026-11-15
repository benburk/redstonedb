//! The builds a search finds, as cards or a table, under the search and
//! what narrows it: the rows that screen them, tags, an order, and a
//! layout.

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
    screen::{Choice, Picks, Row},
    search::Term,
    web::{
        browse::query::{HomeQuery, View},
        href,
        ui::{
            PAGE_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, TOGGLE, Tone,
            cards::{Viewing, badges, cards, stacked_note},
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
    /// The one category picked, such as `Adders`, or `Builds`.
    pub title: String,
    /// The fields to screen the builds by, each with its choices.
    pub rows: &'a [Row],
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
    let query = listing.map(|l| l.query.q.clone()).unwrap_or_default();
    html! {
        form id="search-form" action=(action) method="get" role="search" data-live
            fx-action=(action) fx-target="#listing" class="relative mb-3" {
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
    html! {
        div id="listing" { (listing_body(listing, listing.query, &listing.title)) }
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

/// The row under the search, its order and layout, then the page's title
/// and count.
fn listing_header(listing: &Listing, here: &HomeQuery, title: &str) -> Markup {
    let (shown, view) = (listing.shown, here.view);
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
            div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                h1 class=(PAGE_TITLE) { (title) }
                p class="mr-auto self-end pb-1 text-muted tabular-nums" {
                    (shown.len()) span class="sr-only" { " builds" }
                }
                // Paired with the search above it: the order, and the
                // layout, which a phone, too narrow for the list's table,
                // leaves at cards.
                (sort_menu(&here.sort.name()))
                div class="hidden shrink-0 sm:block" {
                    nav aria-label="Layout" class=(SWITCHES) {
                        (layout(View::Cards, "Cards", icons::GRID))
                        (layout(View::List, "List", icons::LIST))
                    }
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

/// The part of the page a search or a pick redraws, as fixi targets it.
const LISTING: &str = "#listing";

/// How many choices a row of many, as of categories or people, shows
/// before the rest fold away under it.
const LEAD: usize = 8;

/// A row's label, as a leaderboard's rows have theirs.
fn row_label(label: &str) -> Markup {
    html! {
        span class="w-24 shrink-0 py-1 text-meta font-semibold text-muted desktop:py-2" aria-hidden="true" { (label) }
    }
}

/// One field to screen the builds by: every choice of it a toggle, with
/// how many builds it would leave, and All, which unpicks them. The picks
/// of a row are alternatives, and the rows narrow together. A row of many
/// folds all but its first and its picked choices away.
fn screen_row(row: &Row, here: &HomeQuery) -> Markup {
    let field = row.field;
    let folds = field.by_count() && row.choices.len() > LEAD + 2;
    let (lead, rest): (Vec<_>, Vec<_>) = row
        .choices
        .iter()
        .enumerate()
        .partition(|(i, choice)| !folds || *i < LEAD || choice.chosen);
    // Each has an id, so the focus stays on it once its pick redraws the
    // listing.
    let toggle = |choice: &Choice| {
        let to = here.picking(here.picks.toggled(field, &choice.key)).href();
        html! {
            a id={ "pick-" (field.name()) "-" (choice.key) } href=(to) fx-action=(to) fx-target=(LISTING)
                class={ (TOGGLE) @if choice.count == 0 && !choice.chosen { " opacity-50" } }
                aria-pressed=(choice.chosen) {
                (choice.label)
                span class="ml-1.5 text-meta text-muted tabular-nums" { (choice.count) }
            }
        }
    };
    html! {
        nav aria-label=(field.label()) class="flex flex-col gap-x-3 desktop:flex-row desktop:items-start" {
            (row_label(field.label()))
            div class="flex min-w-0 flex-col items-start gap-1" {
                div class=(SWITCHES) {
                    @let all = here.picking(here.picks.any(field)).href();
                    a id={ "pick-" (field.name()) } href=(all) fx-action=(all) fx-target=(LISTING)
                        class=(TOGGLE) aria-pressed=(here.picks.of(field).is_empty()) { "All" }
                    @for (_, choice) in &lead { (toggle(choice)) }
                }
                @if !rest.is_empty() {
                    details id={ "more-" (field.name()) } data-keep {
                        summary class="cursor-pointer px-2 py-1 text-meta text-muted hover:text-ink" {
                            (rest.len()) " more"
                        }
                        div class={ (SWITCHES) " mt-1" } {
                            @for (_, choice) in &rest { (toggle(choice)) }
                        }
                    }
                }
            }
        }
    }
}

/// The rows the builds are screened by: each field, then the tags, and a
/// way back to every build once anything narrows them.
fn screen_rows(listing: &Listing, here: &HomeQuery) -> Markup {
    let cleared = HomeQuery {
        picks: Picks::default(),
        tags: Vec::new(),
        ..here.clone()
    };
    let (lead, rest): (Vec<&Row>, Vec<&Row>) =
        listing.rows.iter().partition(|row| row.field.leads());
    let tags = tag_filters(listing, here);
    // The rest stay open once one of them narrows the builds.
    let open = rest.iter().any(|row| row.narrowed()) || !here.tags.is_empty();
    html! {
        div class="mb-6 flex flex-col gap-2" {
            @for row in lead { (screen_row(row, here)) }
            @if !rest.is_empty() || !tags.0.is_empty() {
                details id="more-filters" data-keep open[open] {
                    summary class="w-fit cursor-pointer py-1 text-meta font-semibold text-muted hover:text-ink" {
                        "More filters"
                    }
                    div class="mt-2 flex flex-col gap-2" {
                        @for row in rest { (screen_row(row, here)) }
                        (tags)
                    }
                }
            }
            @if !here.picks.is_empty() || !here.tags.is_empty() {
                a href=(cleared.href()) class="w-fit text-meta" { "Clear filters" }
            }
        }
    }
}

/// The home page's title, the rows that screen its builds, the terms
/// searched for, and the builds found.
fn listing_body(listing: &Listing, here: &HomeQuery, title: &str) -> Markup {
    let Listing {
        shown,
        total,
        terms,
        ..
    } = *listing;
    let view = here.view;
    html! {
            // What the search keeps as it is typed, here so a pick that
            // redraws the listing brings them up to date.
            @if view == View::List {
                input type="hidden" form="search-form" name="view" value="list";
            }
            @for tag in &here.tags {
                input type="hidden" form="search-form" name="tag" value=(tag);
            }
            @for (name, picks) in here.picked() {
                input type="hidden" form="search-form" name=(name) value=(picks);
            }
            (listing_header(listing, here, title))
            @if !terms.is_empty() {
                div class="mb-4 flex flex-wrap items-center gap-1.5" {
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
                    a href=(HomeQuery { q: String::new(), ..here.clone() }.href()) class="px-1.5 text-meta" { "Clear" }
                }
            }
            @if total > 0 {
                (screen_rows(listing, here))
            }
            @if total == 0 {
                p class="text-muted" { "No builds yet." }
            } @else if shown.is_empty() {
                p class="max-w-prose text-muted" {
                    "No builds match. " (search_help()) " Remove a term or a filter to widen the search."
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
pub(crate) fn removable(
    href: &str,
    removes: &str,
    tone: &str,
    words: &str,
) -> Markup {
    html! {
        a href=(href) aria-label=(removes)
            class={ "badge gap-1.5 pr-2 hover:no-underline " (tone) } {
            (words)
            span aria-hidden="true" { "×" }
        }
    }
}

/// The query with term `index` taken out.
pub(crate) fn without(terms: &[Term], index: usize) -> String {
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
pub(crate) struct Filter {
    tag: Tag,
    count: usize,
    /// The tag filter it is, as the address gives it, when chosen.
    chosen: Option<usize>,
}

/// The tags to narrow a category's builds by: each the builds `shown`
/// carry that some but not all of them do, and each already chosen, by
/// kind, found before given, then most carried first.
pub(crate) fn filters(shown: &[Build], chosen: &[String]) -> Vec<Filter> {
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

/// Whether a row of its own says what `tag` does of the builds `shown`:
/// their design, orientation, standing, buses, inputs, timing, or how
/// their copies stack.
fn in_a_row(tag: &Tag, shown: &[Build]) -> bool {
    let named = [
        tag::FAILING,
        tag::UNTESTED,
        tag::UNALIGNED,
        tag::INTERLEAVED_INPUTS,
        tag::SYNCED,
        tag::UNSYNCED,
        tag::STACKABLE,
    ];
    named.contains(&tag.name.as_str())
        || (tag::is_pitch_stackable(&tag.name)
            && !tag::is_word_stackable(&tag.name))
        || shown.iter().any(|s| {
            s.specs.design.as_deref() == Some(tag.name.as_str())
                || s.specs.orientation.label().eq_ignore_ascii_case(&tag.name)
        })
}

/// Whether `tag` means something only within a category of the builds
/// `shown`: one of its flags, as an adder's carry in or an ALU's `xor`, or
/// how far apart a register's copies add words.
fn of_a_category(tag: &Tag, shown: &[Build]) -> bool {
    tag::is_word_stackable(&tag.name)
        || shown
            .iter()
            .any(|s| s.flags.iter().any(|flag| flag.name == tag.name))
}

/// The last row the builds are screened by: the tags some of them carry
/// that no row above says, each with how many carry it, and those chosen
/// marked, each to remove. Across categories, only the tags that mean the
/// same of any build show, as `torchless` does; a category's own join
/// them within it. Unlike a row's picks, the tags chosen narrow together,
/// a build carrying them all.
fn tag_filters(listing: &Listing, here: &HomeQuery) -> Markup {
    let shown = listing.shown;
    let in_category = here.category().is_some();
    let mut filters = filters(shown, &here.tags);
    filters.retain(|f| {
        f.chosen.is_some()
            || !(in_a_row(&f.tag, shown)
                || !in_category && of_a_category(&f.tag, shown))
    });
    html! {
        @if !filters.is_empty() {
            div class="flex flex-col gap-x-3 desktop:flex-row desktop:items-start" {
            (row_label("Tags"))
            ul aria-label="Filter by tag" class="flex flex-wrap gap-1.5 py-1" {
                @for filter in &filters {
                    @let label = filter.tag.label().replace(',', " or ");
                    li {
                        @if let Some(i) = filter.chosen {
                            @let to = here.without_tag(i).href();
                            a id={ "pick-tag-" (filter.tag.name) } href=(to) fx-action=(to) fx-target=(LISTING)
                                aria-label={ "Remove the tag filter " (label) }
                                title=(filter.tag.about())
                                class={ "badge " (filter.tag.class()) " gap-1.5 pr-2 ring-2 ring-brand ring-offset-1 ring-offset-canvas hover:no-underline" } {
                                (label)
                                span aria-hidden="true" { "×" }
                            }
                        } @else {
                            @let more: Vec<String> = here.tags.iter().cloned()
                                .chain([filter.tag.name.clone()]).collect();
                            @let to = HomeQuery { tags: more, ..here.clone() }.href();
                            a id={ "pick-tag-" (filter.tag.name) } href=(to) fx-action=(to) fx-target=(LISTING)
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
}

/// Builds as table rows, for the home page's List layout.
pub(crate) fn table(schematics: &[Build], terms: &[Term]) -> Markup {
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
