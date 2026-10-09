//! A category's leaderboard page.

use maud::{Markup, html};

use crate::{
    leaderboard::{self, Board, Choice, Measure, Placed, SHOWN},
    models::{
        schematic::{Category, Schematic},
        user::User,
    },
};

use super::{
    Facet, PAGE_TITLE, SECTION_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD,
    base, category_sidebar, category_tabs, name_links,
};

/// A leaderboard and the categories beside it.
pub struct LeaderboardPage<'a> {
    pub category: &'a Category,
    pub board: &'a Board<'a>,
    /// Categories with builds to rank.
    pub facets: &'a [Facet],
}

pub fn leaderboard(viewer: Option<&User>, page: &LeaderboardPage) -> Markup {
    let LeaderboardPage {
        category,
        board,
        facets,
    } = *page;
    let path = format!("/leaderboards/{}", category.name);
    base(
        viewer,
        &format!("{} leaderboard", category.label),
        &path,
        &html! {
            div class="grid gap-x-10 gap-y-6 desktop:grid-cols-[12rem_minmax(0,1fr)]" {
                (category_sidebar(facets, &category.name, None, |name| {
                    format!("/leaderboards/{name}")
                }))
                div {
                    div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                        h1 class={ (PAGE_TITLE) " mr-auto" } { (category.plural) }
                        (category_tabs(&category.name, true))
                    }
                    @if board.measures.is_empty() {
                        p class="text-muted" {
                            "No " (category.plural.to_lowercase())
                            " of known " (board.by.name()) " yet, so none to rank."
                        }
                    } @else {
                        (choices(&path, board))
                        (rankings(category, board))
                    }
                }
            }
        },
    )
}

/// The board's address with these choices; `None` leaves one out. The
/// measure goes under its own name, `width` or `period`.
fn board_href(
    path: &str,
    measure: (Measure, Option<i64>),
    orientation: Option<&str>,
    design: Option<&str>,
) -> String {
    let mut params = form_urlencoded::Serializer::new(String::new());
    if let (by, Some(n)) = measure {
        params.append_pair(by.name(), &n.to_string());
    }
    if let Some(orientation) = orientation {
        params.append_pair("orientation", orientation);
    }
    if let Some(design) = design {
        params.append_pair("design", design);
    }
    let params = params.finish();
    if params.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{params}")
    }
}

/// The width or period, orientation, and design the board is narrowed to.
/// Changing one resets those after it, which the new choice may not have.
fn choices(path: &str, board: &Board) -> Markup {
    let orientation = board.orientation.as_deref();
    let measure = (board.by, board.measure);
    let heading = if board.by == Measure::Width {
        "Width"
    } else {
        "Period"
    };
    html! {
        div class="mb-6 flex flex-col gap-2" {
            (choice_row(heading, &board.measures, |c| {
                (board.measure.map(|n| n.to_string()).as_deref() == Some(c.value.as_str()),
                    board_href(path, (board.by, c.value.parse().ok()), None, None))
            }, None))
            (choice_row("Orientation", &board.orientations, |c| {
                (orientation == Some(c.value.as_str()),
                    board_href(path, measure, Some(&c.value), None))
            }, None))
            @if !board.designs.is_empty() {
                @let all = board.all_designs.then(|| (
                    board.design.is_none(),
                    board_href(path, measure, orientation, None),
                ));
                (choice_row("Design", &board.designs, |c| {
                    (board.design.as_deref() == Some(c.value.as_str()),
                        board_href(path, measure, orientation, Some(&c.value)))
                }, all))
            }
        }
    }
}

/// A labelled segmented control of `options`. `link` gives each option
/// whether it is chosen and its address; `all` adds an "All" option first.
fn choice_row(
    label: &str,
    options: &[Choice],
    link: impl Fn(&Choice) -> (bool, String),
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
    let orientation = board
        .orientations
        .iter()
        .find(|c| Some(&c.value) == board.orientation.as_ref())
        .map(|c| c.label.to_lowercase());
    let design = board.design.as_ref().and_then(|d| {
        board
            .designs
            .iter()
            .find(|c| &c.value == d)
            .map(|c| c.label.clone())
    });
    board
        .measure
        .map(|n| board.by.label(n))
        .into_iter()
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
    format!("{} blocks", s.volume())
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
        div class="overflow-x-auto" {
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
                                @if !s.builders.is_empty() {
                                    p class="text-meta text-muted" {
                                        "by " (name_links(&s.builders, "builder"))
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
