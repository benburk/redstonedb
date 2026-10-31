//! A category's leaderboard page.

use maud::{Markup, html};

use crate::{
    leaderboard::{self, Board, Choice, Placed, SHOWN, Standing, State},
    models::{
        catalogue::{Category, Measure},
        schematic::{Orientation, Schematic},
        user::User,
    },
};

use super::{
    PAGE_TITLE, SECTION_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, base,
    listing::{Facet, category_menu, category_tabs},
};

/// A leaderboard and the categories beside it.
pub struct LeaderboardPage<'a> {
    pub category: &'a Category,
    pub board: &'a Board<'a>,
    /// Categories with builds to rank.
    pub facets: &'a [Facet],
    /// How many people the people leaderboard ranks.
    pub people: usize,
}

/// The people leaderboard: who built, added, and holds the records for
/// the most builds.
pub struct PeoplePage<'a> {
    pub built: &'a [Standing],
    pub added: &'a [Standing],
    pub records: &'a [Standing],
    /// Categories with builds to rank, for the sidebar.
    pub facets: &'a [Facet],
}

/// The menu that goes to another leaderboard: the people leaderboard, which
/// ranks `people`, and each category's, `current` the one shown. Choosing
/// one goes there at once; without scripts, its button does.
fn board_menu(facets: &[Facet], current: &str, people: usize) -> Markup {
    html! {
        form action="/leaderboards" method="get" data-live class="flex items-center gap-2" {
            (category_menu(facets, current, ("People", people), None))
            noscript { button type="submit" class="btn btn-quiet min-h-9" { "Go" } }
        }
    }
}

pub fn people(viewer: Option<&User>, page: &PeoplePage) -> Markup {
    let PeoplePage {
        built,
        added,
        records,
        facets,
    } = *page;
    let builder = |s: &Standing| super::player_href(&s.name);
    // An upload's account has a profile; an importer is only a name.
    let adder = |name: &str, account: Option<&str>| match account {
        Some(username) => format!("/users/{username}"),
        None => super::player_href(name),
    };
    base(
        viewer,
        "People leaderboard",
        "/leaderboards",
        &html! {
            div {
                div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                    h1 class={ (PAGE_TITLE) " mr-auto" } { "People" }
                    (board_menu(facets, "", built.len()))
                }
                p class="mb-6 max-w-prose text-muted" {
                    "A record is first place, as fastest or as smallest, on a category's leaderboard "
                    "with at least two builds; choose a category to see them."
                }
                div class="grid gap-x-10 gap-y-8 desktop:grid-cols-3" {
                    (standings("most-built", "Most builds", "Builds", built, builder))
                    (standings("most-added", "Most added", "Added", added, |s| adder(&s.name, s.account.as_deref())))
                    (standings("most-records", "Most records", "Records", records, builder))
                }
            }
        },
    )
}

/// One people ranking under `heading`: the first [`SHOWN`], each linking
/// where `href` says, with their count under `unit`.
fn standings(
    id: &str,
    heading: &str,
    unit: &str,
    people: &[Standing],
    href: impl Fn(&Standing) -> String,
) -> Markup {
    html! {
        section aria-labelledby=(id) {
            h2 id=(id) class={ (SECTION_TITLE) " mb-2" } { (heading) }
            @if people.is_empty() {
                p class="text-muted" { "No one yet." }
            } @else {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class={ (TABLE_HEAD) " w-8" } { "#" }
                            th scope="col" class=(TABLE_HEAD) { "Name" }
                            th scope="col" class={ (TABLE_HEAD) " text-right" } { (unit) }
                        }
                    }
                    tbody {
                        @for (place, person) in places(people).into_iter().take(SHOWN) {
                            tr class="hover:bg-selection/50" {
                                td class={ (TABLE_CELL) " tabular-nums text-muted" } { (place) }
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                    (super::person_link(&person.name, &href(person)))
                                }
                                td class={ (TABLE_CELL) " text-right tabular-nums" } { (person.count) }
                            }
                        }
                    }
                }
                @if people.len() > SHOWN {
                    p class="mt-2 text-meta text-muted" { "And " (people.len() - SHOWN) " more." }
                }
            }
        }
    }
}

/// Each person with their place, a tie sharing one.
fn places(people: &[Standing]) -> Vec<(usize, &Standing)> {
    let mut place = 0;
    people
        .iter()
        .enumerate()
        .map(|(i, person)| {
            if i == 0 || people[i - 1].count != person.count {
                place = i + 1;
            }
            (place, person)
        })
        .collect()
}

pub fn leaderboard(viewer: Option<&User>, page: &LeaderboardPage) -> Markup {
    let LeaderboardPage {
        category,
        board,
        facets,
        people,
    } = *page;
    let path = format!("/leaderboards/{}", category.name);
    base(
        viewer,
        &format!("{} leaderboard", category.label),
        &path,
        &html! {
            div {
                div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                    h1 class={ (PAGE_TITLE) " mr-auto" } { (category.plural) }
                    (category_tabs(&category.name, true))
                    (board_menu(facets, &category.name, people))
                }
                @if board.splits.first().is_none_or(|split| split.choices.is_empty()) {
                    p class="text-muted" {
                        "No " (category.plural.to_lowercase()) " of known "
                        (board.splits.first().map_or("width", |split| split.measure.label()).to_lowercase())
                        " yet, so none to rank."
                    }
                } @else {
                    (choices(&path, board))
                    (rankings(category, board))
                }
            }
        },
    )
}

/// A board's choices as its address gives them; `None` leaves one out.
#[derive(Clone, Default)]
struct Address<'a> {
    /// A value of each measure chosen, such as a width of 8.
    measures: Vec<(Measure, i64)>,
    logic: Option<State>,
    orientation: Option<Orientation>,
    design: Option<&'a str>,
}

impl Address<'_> {
    /// The address under `path`. Each measure goes under its own name, such
    /// as `width` or `throughput`.
    fn href(&self, path: &str) -> String {
        let mut params = form_urlencoded::Serializer::new(String::new());
        for (measure, n) in &self.measures {
            params.append_pair(measure.name(), &n.to_string());
        }
        let named = [
            ("logic", self.logic.map(State::name)),
            ("orientation", self.orientation.map(Orientation::name)),
            ("design", self.design),
        ];
        for (name, value) in named {
            if let Some(value) = value {
                params.append_pair(name, value);
            }
        }
        let params = params.finish();
        if params.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{params}")
        }
    }
}

/// The values chosen of the board's first `count` measures.
fn chosen_measures(board: &Board, count: usize) -> Vec<(Measure, i64)> {
    board.splits[..count]
        .iter()
        .filter_map(|split| Some((split.measure, split.chosen?)))
        .collect()
}

/// The measures, such as width, then the logic, orientation, and design
/// the board is narrowed to. Changing one resets those after it,
/// which the new choice may not have.
fn choices(path: &str, board: &Board) -> Markup {
    let measured = Address {
        measures: chosen_measures(board, board.splits.len()),
        ..Address::default()
    };
    let logic = Address {
        logic: board.logic,
        ..measured.clone()
    };
    let orientation = Address {
        orientation: board.orientation,
        ..logic.clone()
    };
    html! {
        div class="mb-6 flex flex-col gap-2" {
            @for (i, split) in board.splits.iter().enumerate() {
                (choice_row(split.measure.label(), &split.choices, |c| {
                    let mut values = chosen_measures(board, i);
                    values.push((split.measure, c.value));
                    (split.chosen == Some(c.value),
                        Address { measures: values, ..Address::default() }.href(path))
                }, None))
            }
            @if !board.logics.is_empty() {
                (choice_row("State", &board.logics, |c| {
                    (logic.logic == Some(c.value),
                        Address { logic: Some(c.value), ..measured.clone() }.href(path))
                }, None))
            }
            (choice_row("Orientation", &board.orientations, |c| {
                (orientation.orientation == Some(c.value),
                    Address { orientation: Some(c.value), ..logic.clone() }.href(path))
            }, None))
            @if !board.designs.is_empty() {
                @let all = board.all_designs.then(|| (board.design.is_none(), orientation.href(path)));
                (choice_row("Design", &board.designs, |c| {
                    (board.design.as_deref() == Some(c.value.as_str()),
                        Address { design: Some(&c.value), ..orientation.clone() }.href(path))
                }, all))
            }
        }
    }
}

/// A labelled segmented control of `options`. `link` gives each option
/// whether it is chosen and its address; `all` adds an "All" option first.
fn choice_row<V>(
    label: &str,
    options: &[Choice<V>],
    link: impl Fn(&Choice<V>) -> (bool, String),
    all: Option<(bool, String)>,
) -> Markup {
    html! {
        nav aria-label=(label) class="flex flex-wrap items-center gap-x-3 gap-y-1" {
            span class="w-24 text-meta font-semibold text-muted" aria-hidden="true" { (label) }
            div class=(SWITCHES) {
                @if let Some((chosen, href)) = all {
                    a href=(href) class=(SWITCH) aria-current=[chosen.then_some("page")] { "All" }
                }
                @for option in options {
                    @let (chosen, href) = link(option);
                    a href=(href) class=(SWITCH) aria-current=[chosen.then_some("page")] {
                        (option.label)
                        span class="ml-1.5 text-meta text-muted tabular-nums" { (option.count) }
                    }
                }
            }
        }
    }
}

/// The board's three rankings.
fn rankings(category: &Category, board: &Board) -> Markup {
    let builds = &board.builds;
    if builds.is_empty() {
        return html! {
            p class="text-muted" { "No builds match these choices." }
        };
    }
    let fastest = leaderboard::fastest(builds);
    let smallest = leaderboard::smallest(builds);
    let tradeoffs = leaderboard::tradeoffs(builds);
    let competing = if builds.len() == 1 {
        "1 build so far".to_owned()
    } else {
        format!("{} builds compete", builds.len())
    };
    let what = match (&board.design, board.all_designs) {
        (None, true) => " of every design".to_owned(),
        _ => String::new(),
    };
    html! {
        p class="mb-6 text-muted" {
            (competing) ": " (board_name(category, board)) (what) "."
        }
        div class="grid gap-x-10 gap-y-8 desktop:grid-cols-2" {
            section aria-labelledby="fastest" {
                h2 id="fastest" class={ (SECTION_TITLE) " mb-2" } { "Fastest" }
                @if fastest.is_empty() {
                    p class="text-muted" { "No latencies known yet." }
                } @else {
                    (ranking(&fastest, ["Latency", "Volume"], |s| {
                        [ticks(s), blocks(s)]
                    }))
                }
            }
            section aria-labelledby="smallest" {
                h2 id="smallest" class={ (SECTION_TITLE) " mb-2" } { "Smallest" }
                (ranking(&smallest, ["Volume", "Latency"], |s| {
                    [blocks(s), ticks(s)]
                }))
            }
        }
        @if tradeoffs.len() > 1 {
            section aria-labelledby="tradeoffs" class="mt-8" {
                h2 id="tradeoffs" class={ (SECTION_TITLE) " mb-1" } { "Most compact at each latency" }
                p class="mb-2 text-meta text-muted" {
                    "Each is smaller than every build at least as fast, so going slower buys space."
                }
                (ranking(
                    &tradeoffs.iter().map(|s| (0, *s)).collect::<Vec<_>>(),
                    ["Latency", "Volume"],
                    |s| [ticks(s), blocks(s)],
                ))
            }
        }
    }
}

/// What the board holds, such as `8-bit horizontal Carry Cancel (CCA)
/// adders`.
fn board_name(category: &Category, board: &Board) -> String {
    let orientation = board.orientation.map(|o| o.label().to_lowercase());
    let design = board.design.as_ref().and_then(|d| {
        board
            .designs
            .iter()
            .find(|c| &c.value == d)
            .map(|c| c.label.clone())
    });
    board
        .splits
        .iter()
        .filter_map(|split| split.chosen.map(|n| split.measure.lead(n)))
        // Named only where the category has builds of each.
        .chain(
            board
                .logic
                .filter(|_| !board.logics.is_empty())
                .map(|logic| logic.name().to_owned()),
        )
        .chain(orientation)
        .chain(design)
        .chain([category.plural.to_lowercase()])
        .collect::<Vec<_>>()
        .join(" ")
}

fn ticks(s: &Schematic) -> String {
    s.latency.map(|t| format!("{t} ticks")).unwrap_or_default()
}

fn blocks(s: &Schematic) -> String {
    super::volume(s)
}

/// A table of the first [`SHOWN`] of `placed`, with their places unless all
/// are zero, and the two measures `values` gives under `headings`.
fn ranking(
    placed: &[Placed],
    headings: [&str; 2],
    values: impl Fn(&Schematic) -> [String; 2],
) -> Markup {
    let numbered = placed.iter().any(|(place, _)| *place > 0);
    html! {
        div class="relative overflow-x-auto" {
            table class="w-full border-collapse" {
                thead {
                    tr {
                        @if numbered {
                            th scope="col" class={ (TABLE_HEAD) " w-8" } { "#" }
                        }
                        th scope="col" class=(TABLE_HEAD) { "Build" }
                        @for heading in headings {
                            th scope="col" class={ (TABLE_HEAD) " whitespace-nowrap" } { (heading) }
                        }
                    }
                }
                tbody {
                    @for (place, s) in placed.iter().take(SHOWN) {
                        tr class="hover:bg-selection/50" {
                            @if numbered {
                                td class={ (TABLE_CELL) " tabular-nums text-muted" } { (place) }
                            }
                            th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                a href={ "/schematics/" (s.id) } class="font-semibold" { (s.title()) }
                                @if let (Some(from), Some(width)) = (s.stacked_from, s.width) {
                                    div class="mt-1 text-meta text-muted" {
                                        (width / from.max(1)) " copies of a " (from) "-bit build, stacked"
                                    }
                                }
                                @if !s.builders.is_empty() {
                                    div class="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-meta" {
                                        @for name in &s.builders {
                                            (super::person_link(name, &super::player_href(name)))
                                        }
                                    }
                                }
                            }
                            @for value in values(s) {
                                td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap" } { (value) }
                            }
                        }
                    }
                }
            }
        }
        @if placed.len() > SHOWN {
            p class="mt-2 text-meta text-muted" {
                "And " (placed.len() - SHOWN) " more."
            }
        }
    }
}
