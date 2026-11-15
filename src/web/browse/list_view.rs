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
    category::{Design, FlagGroup, Kind, Measure, Section},
    interface::Encoding,
    leaderboard::{Wanted, in_sentence},
    screen::{Choice, Field, Picks, Row},
    search::Term,
    web::{
        browse::query::{HomeQuery, View},
        href,
        ui::{
            PAGE_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, TOGGLE, Tab,
            Tone,
            cards::{Viewing, badges, cards, stacked_note},
            category_tabs,
            icons::{self, icon},
            layout::base,
            person::name_links,
            summary::{latency, size, volume},
        },
    },
};

/// The orders a listing offers, as `sort` values and their labels. Any
/// other order of the fields in [`Sort`] works in the address too.
const SORTS: [(&str, &str); 5] = [
    ("top", "Top"),
    ("latency", "Fastest"),
    ("volume", "Smallest"),
    ("-added_at", "Newest"),
    ("-downloads", "Downloads"),
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
    /// The terms with the one width picked among them, which a build
    /// whose copies stack to it notes.
    pub noted: &'a [Term],
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

/// The page's title and count. Within one category, a way back to every
/// build, the category's two pages, its builds and its records, and what
/// the site knows of it.
fn listing_header(listing: &Listing, here: &HomeQuery, title: &str) -> Markup {
    let kind = here.category().and_then(Kind::named);
    let every = here.picking(here.picks.any(Field::Category)).href();
    html! {
            @if here.category().is_some() {
                a href=(every) class="mb-1 inline-flex items-center gap-1 text-meta text-muted hover:text-ink" {
                    (icon(icons::ARROW_LEFT, "size-3.5")) "All builds"
                }
            }
            div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                h1 class=(PAGE_TITLE) { (title) }
                p class="mr-auto self-end pb-1 text-muted tabular-nums" {
                    (listing.shown.len()) span class="sr-only" { " builds" }
                }
                @if let Some(kind) = kind {
                    (category_tabs(Tab::Builds, &here.href(), &records_href(kind, here)))
                }
            }
            @if let Some(kind) = kind {
                (about_category(kind))
            }
    }
}

/// Where a category's records are for the builds `here` lists: its
/// leaderboard, narrowed to the one width, orientation, and design picked
/// of each, where one alone is.
fn records_href(kind: Kind, here: &HomeQuery) -> String {
    let one = |field: Field| match here.picks.of(field)[..] {
        [pick] => Some(pick),
        _ => None,
    };
    let width = one(Field::Width)
        .and_then(|width| width.parse().ok())
        .filter(|_| kind.facts().measures.first() == Some(&Measure::Width));
    let wanted = Wanted {
        measures: width.map(|n| (Measure::Width, n)).into_iter().collect(),
        orientation: one(Field::Orientation).and_then(|o| o.parse().ok()),
        design: one(Field::Design)
            .filter(|design| *design != "none")
            .map(str::to_owned),
        ..Wanted::default()
    };
    wanted.href(&format!("/leaderboards/{}", kind.facts().name))
}

/// What the site knows of a category, folded away under its title: what
/// its builds are ranked by, and the designs, lines, and operations it
/// recognizes of them, as its list gives them, so the page says what an
/// upload is read for.
fn about_category(kind: Kind) -> Markup {
    let facts = kind.facts();
    let labels = |labels: Vec<String>| labels.join(", ");
    let flags = |group: FlagGroup| {
        labels(
            facts
                .flags
                .iter()
                .filter(|flag| flag.group() == group)
                .map(|flag| flag.label.to_owned())
                .collect(),
        )
    };
    let known = [
        (
            "Named and ranked by",
            labels(
                facts
                    .measures
                    .iter()
                    .map(|m| m.label().to_owned())
                    .collect(),
            ),
        ),
        (
            "Usually",
            facts
                .logic
                .map_or_else(String::new, |l| l.label().to_owned()),
        ),
        (
            "Designs",
            labels(facts.designs.iter().map(Design::label).collect()),
        ),
        ("Optional lines", flags(FlagGroup::Lines)),
        ("Operations", flags(FlagGroup::Operations)),
        ("Features", flags(FlagGroup::Features)),
        ("Also searched as", facts.aliases.join(", ")),
    ];
    html! {
        details id="about-category" data-keep class="mb-4 max-w-prose" {
            summary class="w-fit cursor-pointer text-meta text-muted hover:text-ink" {
                "What the site recognizes of " (in_sentence(&kind.plural()))
            }
            dl class="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-meta" {
                @for (name, value) in known.iter().filter(|(_, value)| !value.is_empty()) {
                    dt class="font-semibold text-muted" { (name) }
                    dd { (value) }
                }
            }
            p class="mt-2 text-meta text-muted" {
                "The site reads each of these from a build's file and tests them. "
                a href="/about/recognized#per-category" { "How builds are recognized" }
            }
        }
    }
}

/// The layout to list the builds in, cards or a table, which a phone, too
/// narrow for the table, leaves at cards.
fn layout_switch(here: &HomeQuery) -> Markup {
    let layout = |to: View, label: &str, glyph: &str| {
        html! {
            a href=(HomeQuery { view: to, ..here.clone() }.href()) aria-label=(label) title=(label)
                class={ (SWITCH) " flex items-center px-2" }
                aria-current=[(here.view == to).then_some("page")] {
                (icon(glyph, "size-4"))
            }
        }
    };
    html! {
        nav aria-label="Layout" class={ (SWITCHES) " " (PAST_PHONE) } {
            (layout(View::Cards, "Cards", icons::GRID))
            (layout(View::List, "List", icons::LIST))
        }
    }
}

/// The orders to choose from, each in sight, the one the builds are in
/// marked. An order the address asked for that the bar lacks is added to
/// it.
fn sort_bar(here: &HomeQuery) -> Markup {
    let order = here.order().name();
    let known = SORTS.iter().any(|(value, _)| *value == order);
    let choice = |value: &str, label: &str| {
        let to = here.sorted(value).href();
        html! {
            a id={ "sort-" (value) } href=(to) fx-action=(to) fx-target=(LISTING)
                class=(SWITCH) aria-current=[(value == order).then_some("page")] { (label) }
        }
    };
    html! {
        nav aria-label="Sort by" class="flex flex-wrap items-center gap-x-3 gap-y-1" {
            span class="text-meta font-semibold text-muted" aria-hidden="true" { "Sort by" }
            div class=(SWITCHES) {
                @for (value, label) in SORTS { (choice(value, label)) }
                @if !known { (choice(&order, &order)) }
            }
        }
    }
}

/// The part of the page a search or a pick redraws, as fixi targets it.
const LISTING: &str = "#listing";

/// How many choices a row of many, as of categories or people, shows
/// before the rest fold away under it.
const LEAD: usize = 6;

/// Shown only at a phone's width, and only past it.
const PHONE_ONLY: &str = "sm:hidden";
const PAST_PHONE: &str = "max-sm:hidden";

/// A row's label, as a leaderboard's rows have theirs.
fn row_label(label: &str) -> Markup {
    html! {
        span class="w-24 shrink-0 py-1 text-meta font-semibold text-muted desktop:py-2" aria-hidden="true" { (label) }
    }
}

/// One field to screen the builds by: every choice of it a toggle, with
/// how many builds it would leave, and All, which unpicks them. The picks
/// of a row are alternatives, and the rows narrow together. A row of many
/// folds all but its first and its picked choices away. `shown` is when
/// it shows, at every width or as [`PHONE_ONLY`] says, and `copy` ends
/// the ids of a second copy of a row, so each id is one element's.
fn screen_row(row: &Row, here: &HomeQuery, shown: &str, copy: &str) -> Markup {
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
            a id={ "pick-" (field.name()) "-" (choice.key) (copy) } href=(to) fx-action=(to) fx-target=(LISTING)
                class={ (TOGGLE) @if choice.count == 0 && !choice.chosen { " opacity-50" } }
                aria-pressed=(choice.chosen) {
                (choice.label)
                span class="ml-1.5 text-meta text-muted tabular-nums" { (choice.count) }
            }
        }
    };
    html! {
        nav aria-label=(field.label()) class={ "flex flex-col gap-x-3 desktop:flex-row desktop:items-start " (shown) } {
            (row_label(field.label()))
            div class="flex min-w-0 flex-col items-start gap-1" {
                div class=(SWITCHES) {
                    @let all = here.picking(here.picks.any(field)).href();
                    a id={ "pick-" (field.name()) (copy) } href=(all) fx-action=(all) fx-target=(LISTING)
                        class=(TOGGLE) aria-pressed=(here.picks.of(field).is_empty()) {
                        // Where a build must have every pick, none asks
                        // for any build.
                        @if field.every() { "Any" } @else { "All" }
                    }
                    @for (_, choice) in &lead { (toggle(choice)) }
                }
                @if !rest.is_empty() {
                    details id={ "more-" (field.name()) (copy) } data-keep {
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
    // Within a category, clearing keeps to it, as its page is its own.
    let in_category = here.category().is_some();
    let kept = match here.category() {
        Some(category) => HomeQuery::in_category(category).picks,
        None => Picks::default(),
    };
    let narrowed = here.picks != kept || !here.tags.is_empty();
    let cleared = HomeQuery {
        picks: kept,
        tags: Vec::new(),
        ..here.clone()
    };
    // A row something is picked of shows whatever else folds away, so no
    // pick narrows the builds out of sight. Of the rest, a few lead and
    // the others fold away under them; a phone, with room for little
    // above its builds, folds those that lead away too. A category's page
    // has no row of categories: its title says which it is.
    let picked = |row: &Row| here.picks.has(row.field);
    let (above, rest): (Vec<&Row>, Vec<&Row>) = listing
        .rows
        .iter()
        .filter(|row| !(in_category && row.field == Field::Category))
        .partition(|row| picked(row) || row.field.leads());
    let lead: Vec<&Row> =
        above.iter().copied().filter(|row| !picked(row)).collect();
    let tags = tag_filters(listing, here);
    let folded = rest.len() + usize::from(!tags.0.is_empty());
    html! {
        div class="mb-6 flex flex-col gap-2" {
            @for row in &above {
                (screen_row(row, here, if picked(row) { "" } else { PAST_PHONE }, ""))
            }
            // What stands right over the builds: their order, then the
            // further filters, side by side, and the layout at the far end.
            // The order and layout float, so the filters' button sits
            // between them and its rows open full width below all three.
            div class="mt-1 flow-root" {
                div class="float-right" { (layout_switch(here)) }
                div class="float-left mr-3 mb-2" { (sort_bar(here)) }
                @if folded > 0 || !lead.is_empty() {
                    details id="more-filters" data-keep open[!here.tags.is_empty()]
                        class=(if folded == 0 { PHONE_ONLY } else { "" }) {
                        // A button in place of the marker a summary has,
                        // filled while its filters are open.
                        summary class="btn btn-quiet w-fit list-none align-top in-open:bg-selection [&::-webkit-details-marker]:hidden" {
                            (icon(icons::FUNNEL, "size-4"))
                            span class=(PAST_PHONE) { "More filters" }
                            span class=(PHONE_ONLY) { "Filters" }
                            span class={ "font-normal text-muted tabular-nums " (PAST_PHONE) } { (folded) }
                            span class={ "font-normal text-muted tabular-nums " (PHONE_ONLY) } { (folded + lead.len()) }
                        }
                        div class="clear-both flex flex-col gap-2 pt-3" {
                            @for row in &lead { (screen_row(row, here, PHONE_ONLY, "-phone")) }
                            @for row in rest { (screen_row(row, here, "", "")) }
                            (tags)
                        }
                    }
                }
            }
            @if narrowed {
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
            @if !here.sort.0.is_empty() {
                input type="hidden" form="search-form" name="sort" value=(here.sort.name());
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
                (cards(shown, listing.noted, listing.viewing))
            } @else {
                (table(shown, listing.noted))
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

/// The tags an address narrows the builds by, as a build's badge leads to
/// its category's builds with one, each to remove. The rows above offer
/// what a tag says, so none is offered here to add.
fn tag_filters(listing: &Listing, here: &HomeQuery) -> Markup {
    let mut filters = filters(listing.shown, &here.tags);
    filters.retain(|f| f.chosen.is_some());
    html! {
        @if !filters.is_empty() {
            div class="flex flex-col gap-x-3 desktop:flex-row desktop:items-start" {
                (row_label("Tags"))
                ul aria-label="Tag filters" class="flex flex-wrap gap-1.5 py-1" {
                    @for (filter, i) in filters.iter().filter_map(|f| Some((f, f.chosen?))) {
                        @let label = filter.tag.label().replace(',', " or ");
                        @let to = here.without_tag(i).href();
                        li {
                            a id={ "pick-tag-" (filter.tag.name) } href=(to) fx-action=(to) fx-target=(LISTING)
                                aria-label={ "Remove the tag filter " (label) }
                                title=(filter.tag.about())
                                class={ "badge " (filter.tag.class()) " gap-1.5 pr-2 ring-2 ring-brand ring-offset-1 ring-offset-canvas hover:no-underline" } {
                                (label)
                                span aria-hidden="true" { "×" }
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
