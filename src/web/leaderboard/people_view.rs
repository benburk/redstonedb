//! The leaderboards' front page: the records most contested, and who
//! built, added, and holds the records for the most builds.

use maud::{Markup, html};

use crate::{
    account::User,
    build::tag::{self, Kind},
    leaderboard::{self, Inputs, Rank, Record, SHOWN, Standing, Wanted},
    web::{
        browse::list_view::Facet,
        href,
        leaderboard::view::{and_more, blocks, heading},
        ui::{
            SECTION_TITLE, TABLE_CELL, TABLE_HEAD, layout::base, person,
            summary::latency, text::count,
        },
    },
};

/// The leaderboards' front page: the records most contested, and who
/// built, added, and holds the most records.
pub struct PeoplePage<'a> {
    pub records: &'a [Record<'a>],
    pub built: &'a [Standing],
    pub added: &'a [Standing],
    pub holders: &'a [Standing],
    /// Categories with builds to rank, for the sidebar.
    pub facets: &'a [Facet],
}

pub fn people(viewer: Option<&User>, page: &PeoplePage) -> Markup {
    let PeoplePage {
        records,
        built,
        added,
        holders,
        facets,
    } = *page;
    let author = |s: &Standing| href::player(&s.name);
    // An upload's account has a profile; an importer is only a name.
    let adder = |name: &str, account: Option<&str>| match account {
        Some(username) => href::user(username),
        None => href::player(name),
    };
    base(
        viewer,
        "Leaderboards",
        "/leaderboards",
        &html! {
            div {
                (heading("Leaderboards", facets, ""))
                (contested(records))
                h2 class={ (SECTION_TITLE) " mt-10 mb-4" } { "People" }
                div class="grid grid-cols-1 gap-x-10 gap-y-8 desktop:grid-cols-3" {
                    (standings("most-built", "Most builds", "Builds", built, author))
                    (standings("most-added", "Most added", "Added", added, |s| adder(&s.name, s.account.as_deref())))
                    (standings("most-records", "Most records", "Records", holders, author))
                }
            }
        },
    )
}

/// The records most contested, each build's under the board with the
/// most authors it is first on.
pub(crate) fn contested(records: &[Record]) -> Markup {
    let shown = leaderboard::one_for_each_holder(records);
    html! {
        section aria-labelledby="records" {
            h2 id="records" class={ (SECTION_TITLE) " mb-1" } { "Records" }
            p class="mb-2 max-w-prose text-meta text-muted" {
                "First place on boards of two or more builds, each tag or latency changing who is first. Most authors first."
            }
            @if shown.is_empty() {
                p class="text-muted" { "No board has two builds yet." }
            } @else {
                (record_table(&shown, None))
                (and_more(shown.len()))
            }
        }
    }
}

/// The first [`SHOWN`] of `records`, each linking to its board, with how
/// many built for it and the record itself, linking to the build holding
/// it; or, for a `player`, every one, with the name of the build of theirs
/// holding it.
pub(crate) fn record_table(
    records: &[&Record],
    player: Option<&str>,
) -> Markup {
    let named = player.is_some();
    html! {
        div class="relative overflow-x-auto" {
            table class="w-full border-collapse" {
                thead {
                    tr {
                        th scope="col" class=(TABLE_HEAD) { "Record" }
                        @if named {
                            th scope="col" class=(TABLE_HEAD) { "Build" }
                        }
                        th scope="col" class=(TABLE_HEAD) { "Holds at" }
                    }
                }
                tbody {
                    @for record in records.iter().take(if named { usize::MAX } else { SHOWN }) {
                        @let holder = record.holder(player);
                        @let href = href::build(&holder.id);
                        tr class="hover:bg-selection/50" {
                            th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                a href=(record_href(record)) class="font-semibold" { (record.name()) }
                                div class="mt-1 text-meta text-muted" {
                                    (count(record.authors, "author")) " · " (count(record.field.len(), "build"))
                                }
                            }
                            @if named {
                                td class=(TABLE_CELL) {
                                    a href=(href) { (holder.title()) }
                                }
                            }
                            td class={ (TABLE_CELL) " tabular-nums" } {
                                a href=(href) {
                                    @match record.rank {
                                        Rank::Fastest => (latency(holder)),
                                        Rank::Smallest => (blocks(holder, record.ruler)),
                                    }
                                }
                                @if record.holders.len() > 1 {
                                    div class="mt-1 text-meta text-muted" { (record.holders.len()) " tied" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The address of the board a record is first on: its interleaved inputs
/// and, for cells, its pitch class, chosen apart from its tags.
fn record_href(record: &Record) -> String {
    let of = |kind: Kind| record.tags.iter().find(|tag| tag.kind == kind);
    let cells = leaderboard::as_cells(leaderboard::group_of(record.category));
    let class = |name: &str| {
        let (pitch, shape) = leaderboard::pitch_class(name)?;
        cells.then(|| format!("{pitch}-{shape}"))
    };
    let interleaved = record
        .tags
        .iter()
        .any(|t| t.name == tag::INTERLEAVED_INPUTS);
    Wanted {
        measures: record.measures.clone(),
        logic: Some(record.logic),
        orientation: of(Kind::Orientation)
            .map(|_| record.holders[0].specs.orientation),
        design: of(Kind::Design).map(|tag| tag.name.clone()),
        ticks: record.ticks,
        tags: record
            .tags
            .iter()
            .filter(|tag| !matches!(tag.kind, Kind::Design | Kind::Orientation))
            .filter(|tag| tag.name != tag::INTERLEAVED_INPUTS)
            .filter(|tag| class(&tag.name).is_none())
            .map(|tag| tag.name.clone())
            .collect(),
        inputs: interleaved.then_some(Inputs::Interleaved),
        stacking: record.tags.iter().find_map(|tag| class(&tag.name)),
    }
    .href(&format!("/leaderboards/{}", record.category))
}

/// One people ranking under `heading`: the first [`SHOWN`], each linking
/// where `href` says, with their count under `unit`.
pub(crate) fn standings(
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
                        @for (place, person) in leaderboard::places(people).into_iter().take(SHOWN) {
                            tr class="hover:bg-selection/50" {
                                td class={ (TABLE_CELL) " tabular-nums text-muted" } { (place) }
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                    (person::person_link(&person.name, &href(person)))
                                }
                                td class={ (TABLE_CELL) " text-right tabular-nums" } { (person.count) }
                            }
                        }
                    }
                }
                (and_more(people.len()))
            }
        }
    }
}
