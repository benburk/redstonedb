//! A category's leaderboard page.

use maud::{Markup, html};

use crate::{
    category::{Category, Group, Measure},
    leaderboard::{
        self, Board, Choice, Inputs, Placed, Ruler, SHOWN, Standing, Wanted,
        in_sentence,
    },
    models::{schematic::Schematic, tag, user::User},
};

use super::{
    PAGE_TITLE, SECTION_TITLE, SWITCH, SWITCHES, TABLE_CELL, TABLE_HEAD, base,
    listing::{Facet, category_menu},
};

/// A leaderboard and the categories beside it.
pub struct LeaderboardPage<'a> {
    pub category: &'a Category,
    pub board: &'a Board<'a>,
    /// Categories with builds to rank.
    pub facets: &'a [Facet],
}

/// The leaderboards' front page: who built and who added the most builds.
pub struct PeoplePage<'a> {
    pub built: &'a [Standing],
    pub added: &'a [Standing],
    /// Categories with builds to rank, for the sidebar.
    pub facets: &'a [Facet],
}

/// The menu that goes to another leaderboard: the front page, and each
/// category's, `current` the one shown. Choosing one goes there at once;
/// without scripts, its button does.
fn board_menu(facets: &[Facet], current: &str) -> Markup {
    html! {
        form action="/leaderboards" method="get" data-live class="flex items-center gap-2" {
            (category_menu(facets, current, "Top boards", None))
            noscript { button type="submit" class="btn btn-quiet min-h-9" { "Go" } }
        }
    }
}

pub fn people(viewer: Option<&User>, page: &PeoplePage) -> Markup {
    let PeoplePage {
        built,
        added,
        facets,
    } = *page;
    let author = |s: &Standing| super::player_href(&s.name);
    // An upload's account has a profile; an importer is only a name.
    let adder = |name: &str, account: Option<&str>| match account {
        Some(username) => format!("/users/{username}"),
        None => super::player_href(name),
    };
    base(
        viewer,
        "Leaderboards",
        "/leaderboards",
        &html! {
            div {
                div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                    h1 class={ (PAGE_TITLE) " mr-auto" } { "Leaderboards" }
                    (board_menu(facets, ""))
                }
                div class="grid grid-cols-1 gap-x-10 gap-y-8 desktop:grid-cols-2" {
                    (standings("most-built", "Most builds", "Builds", built, author))
                    (standings("most-added", "Most added", "Added", added, |s| adder(&s.name, s.account.as_deref())))
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
                    (board_menu(facets, &category.name))
                }
                @if !board.ranked {
                    p class="text-muted" {
                        "No " (in_sentence(&category.plural)) " of known "
                        (category.measures.first().map_or("width", |m| m.label()).to_lowercase())
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

/// The values chosen of the board's first `count` measures.
fn chosen_measures(board: &Board, count: usize) -> Vec<(Measure, i64)> {
    board.splits[..count]
        .iter()
        .filter_map(|split| Some((split.measure, split.chosen?)))
        .collect()
}

/// The measures, such as width, then the logic, inputs, a cell's stacking,
/// orientation, design, latency, and tags the board is narrowed to.
/// Changing a measure or the logic resets those after it, which the new
/// choice may not have; the rest keep one another.
fn choices(path: &str, board: &Board) -> Markup {
    let measured = Wanted {
        measures: chosen_measures(board, board.splits.len()),
        ..Wanted::default()
    };
    // Split inputs are the default, unless no build has them.
    let split_default = board
        .inputs_choices
        .iter()
        .any(|c| c.value == Inputs::Split);
    let inputs = |inputs: Inputs| {
        (inputs != Inputs::Split || !split_default).then_some(inputs)
    };
    let current = Wanted {
        logic: board.logic,
        inputs: inputs(board.inputs),
        stacking: board.stacking.clone(),
        orientation: board.orientation,
        design: board.design.clone().filter(|_| board.all_designs),
        ticks: board.ticks,
        tags: board.tags.clone(),
        ..measured.clone()
    };
    let toggled = |tag: &str| {
        let mut tags = current.tags.clone();
        match tags.iter().position(|t| t == tag) {
            Some(i) => {
                tags.remove(i);
            }
            None => tags.push(tag.to_owned()),
        }
        Wanted {
            tags,
            ..current.clone()
        }
        .href(path)
    };
    html! {
        div class="mb-6 flex flex-col gap-2" {
            @for (i, split) in board.splits.iter().enumerate() {
                (choice_row(split.measure.label(), &split.choices, |c| {
                    let mut values = chosen_measures(board, i);
                    values.push((split.measure, c.value));
                    (split.chosen == Some(c.value),
                        Wanted { measures: values, ..Wanted::default() }.href(path))
                }, None))
            }
            @if !board.logics.is_empty() {
                (choice_row("State", &board.logics, |c| {
                    (board.logic == Some(c.value),
                        Wanted { logic: Some(c.value), ..measured.clone() }.href(path))
                }, None))
            }
            @if !board.inputs_choices.is_empty() {
                @let all = ("All", board.inputs == Inputs::All, Wanted { inputs: Some(Inputs::All), ..current.clone() }.href(path));
                (choice_row("Inputs", &board.inputs_choices, |c| {
                    (board.inputs == c.value,
                        Wanted { inputs: inputs(c.value), ..current.clone() }.href(path))
                }, Some(all)))
            }
            @if !board.stackings.is_empty() || board.stacking.is_some() {
                @let all = ("Alone", board.stacking.is_none(), Wanted { stacking: None, ..current.clone() }.href(path));
                (choice_row("Stacks", &board.stackings, |c| {
                    (board.stacking.as_ref() == Some(&c.value),
                        Wanted { stacking: Some(c.value.clone()), ..current.clone() }.href(path))
                }, Some(all)))
            }
            @if board.orientations.len() > 1 || board.orientation.is_some() {
                @let all = ("All", board.orientation.is_none(), Wanted { orientation: None, ..current.clone() }.href(path));
                (choice_row("Orientation", &board.orientations, |c| {
                    (board.orientation == Some(c.value),
                        Wanted { orientation: Some(c.value), ..current.clone() }.href(path))
                }, Some(all)))
            }
            @if !board.designs.is_empty() {
                @let all = board.all_designs.then(|| ("All", board.design.is_none(), Wanted { design: None, ..current.clone() }.href(path)));
                (choice_row("Design", &board.designs, |c| {
                    (board.design.as_deref() == Some(c.value.as_str()),
                        Wanted { design: Some(c.value.clone()), ..current.clone() }.href(path))
                }, all))
            }
            @if board.latencies.len() > 1 || board.ticks.is_some() {
                @let all = ("All", board.ticks.is_none(), Wanted { ticks: None, ..current.clone() }.href(path));
                (choice_row("Ticks", &board.latencies, |c| {
                    (board.ticks == Some(c.value),
                        Wanted { ticks: Some(c.value), ..current.clone() }.href(path))
                }, Some(all)))
            }
            @if !board.tag_choices.is_empty() {
                (choice_row("Tags", &board.tag_choices, |c| {
                    (board.tags.contains(&c.value), toggled(&c.value))
                }, None))
            }
        }
    }
}

/// A labelled segmented control of `options`. `link` gives each option
/// whether it is chosen and its address; `all` adds an option first, its
/// label, as "All", whether it is chosen, and its address.
fn choice_row<V>(
    label: &str,
    options: &[Choice<V>],
    link: impl Fn(&Choice<V>) -> (bool, String),
    all: Option<(&str, bool, String)>,
) -> Markup {
    html! {
        nav aria-label=(label) class="flex flex-wrap items-center gap-x-3 gap-y-1" {
            span class="w-24 text-meta font-semibold text-muted" aria-hidden="true" { (label) }
            div class=(SWITCHES) {
                @if let Some((all, chosen, href)) = all {
                    a href=(href) class=(SWITCH) aria-current=[chosen.then_some("page")] { (all) }
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

/// The board's three rankings. Cells on shared lines show how many copies
/// still work together, as a shared line reaches only so far.
fn rankings(category: &Category, board: &Board) -> Markup {
    let builds = &board.builds;
    if builds.is_empty() {
        return html! {
            p class="text-muted" { "No builds match these choices." }
        };
    }
    let ruler = board.ruler;
    let fastest = leaderboard::fastest(builds, ruler);
    let smallest = leaderboard::smallest(builds, ruler);
    let tradeoffs = leaderboard::tradeoffs(builds, ruler);
    let shared = board.group == Group::SharedLines;
    let by_speed = |s: &Schematic| {
        let mut values = vec![ticks(s), blocks(s, ruler)];
        values.extend(shared.then(|| copies(s, ruler)));
        values
    };
    let by_size = |s: &Schematic| {
        let mut values = vec![blocks(s, ruler), ticks(s)];
        values.extend(shared.then(|| copies(s, ruler)));
        values
    };
    let heads = |first: &'static str, second: &'static str| {
        let mut heads = vec![first, second];
        heads.extend(shared.then_some("Copies"));
        heads
    };
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
        div class="grid grid-cols-1 gap-x-10 gap-y-8 desktop:grid-cols-2" {
            section aria-labelledby="fastest" {
                h2 id="fastest" class={ (SECTION_TITLE) " mb-2" } { "Fastest" }
                @if fastest.is_empty() {
                    p class="text-muted" { "No latencies known yet." }
                } @else {
                    (ranking(&fastest, &heads("Latency", "Volume"), by_speed))
                }
            }
            section aria-labelledby="smallest" {
                h2 id="smallest" class={ (SECTION_TITLE) " mb-2" } { "Smallest" }
                (ranking(&smallest, &heads("Volume", "Latency"), by_size))
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
                    &heads("Latency", "Volume"),
                    by_speed,
                ))
            }
        }
    }
}

/// What the board holds, such as `3-tick 8-bit synced horizontal Carry
/// Cancel (CCA) adders`, or `2-wide stackable XOR gates`.
fn board_name(category: &Category, board: &Board) -> String {
    let orientation = board.orientation.map(|o| o.label().to_lowercase());
    let design = board.design.as_ref().and_then(|d| {
        board
            .designs
            .iter()
            .find(|c| &c.value == d)
            .map(|c| c.label.clone())
    });
    let tags = board.tags.iter().map(|t| {
        board
            .tag_choices
            .iter()
            .find(|c| &c.value == t)
            .map_or_else(|| t.replace('_', " "), |c| c.label.clone())
    });
    board
        .ticks
        .map(|t| format!("{t}-tick"))
        .into_iter()
        .chain(
            board.splits.iter().filter_map(|split| {
                split.chosen.map(|n| split.measure.lead(n))
            }),
        )
        // Named only where the category has builds of each.
        .chain(
            board
                .logic
                .filter(|_| !board.logics.is_empty())
                .map(|logic| logic.name().to_owned()),
        )
        .chain(
            board
                .stacking
                .as_deref()
                .map(tag::pitch_stackable)
                .map(|c| c.replace('_', " ")),
        )
        .chain(match board.inputs {
            Inputs::Split => None,
            Inputs::Interleaved => Some("interleaved inputs".to_owned()),
            Inputs::All => (!board.inputs_choices.is_empty())
                .then(|| "split and interleaved inputs".to_owned()),
        })
        .chain(tags)
        .chain(orientation)
        .chain(design)
        .chain([in_sentence(&category.plural)])
        .collect::<Vec<_>>()
        .join(" ")
}

fn ticks(s: &Schematic) -> String {
    s.latency.map(|t| format!("{t} ticks")).unwrap_or_default()
}

/// Its size as `ruler` measures it: its volume, or a cell's blocks, those
/// of each copy where its copies stack, or a bit where it holds more.
fn blocks(s: &Schematic, ruler: Ruler) -> String {
    if ruler == Ruler::Whole {
        return super::volume(s);
    }
    let size = ruler.size(s);
    if size.bits == 1 {
        let per = if ruler.stacked() { " per copy" } else { "" };
        return format!("{} blocks{per}", size.blocks);
    }
    // Tenths, but where a whole number of blocks.
    let tenths = (size.blocks * 10 + size.bits / 2) / size.bits;
    let n = if tenths % 10 == 0 {
        (tenths / 10).to_string()
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    };
    format!("{n} blocks a bit")
}

/// How many copies of `s` still work stacked the ways its board, as
/// `ruler` measures it, counts, the most of them: any number, up to a
/// limit, or a dash for a build whose copies add no bits.
fn copies(s: &Schematic, ruler: Ruler) -> String {
    let (west, up) = match ruler {
        Ruler::Cell { west, up } if west || up => (west, up),
        _ => (true, true),
    };
    let limits: Vec<Option<i64>> = s
        .stacking
        .iter()
        .filter(|way| way.pitch.is_some())
        .filter(|way| (west && way.west > 0) || (up && way.up > 0))
        .map(|way| way.max_copies)
        .collect();
    if limits.is_empty() {
        "—".to_owned()
    } else if limits.contains(&None) {
        "Any number".to_owned()
    } else {
        let most = limits.into_iter().flatten().max().unwrap_or_default();
        format!("Up to {most}")
    }
}

/// A table of the first [`SHOWN`] of `placed`, with their places unless all
/// are zero, and the measures `values` gives under `headings`.
fn ranking(
    placed: &[Placed],
    headings: &[&str],
    values: impl Fn(&Schematic) -> Vec<String>,
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
                                @if !s.authors.is_empty() {
                                    div class="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-meta" {
                                        @for name in &s.authors {
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
