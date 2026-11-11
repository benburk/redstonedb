//! The logic analyzer's page: a change of a build's inputs traced, as a
//! chart of every line and every component that changed, tick by tick,
//! each lane's edges and last change measured, the tick its outputs settle
//! by and each race marked, and each way a race may go when another compile
//! ran it the other way; a table of the races, and of each change with what
//! set it off; and how a sweep of many changes settled and glitched.

use std::fmt::Write as _;

use maud::{Markup, html};

use crate::{
    models::{schematic::Schematic, user::User},
    simulate::{
        Value,
        trace::{Case, Change, Part, Race, Sweep, Trace},
    },
    web::trace::{Analysis, Run, case_words, state},
};

use super::{PAGE_TITLE, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, base, grouped};

/// The chart's label column, a tick's width, the two measure columns, and
/// the height of a lane, a section's heading, and the tick numbers along
/// its top.
const LABEL: f64 = 290.0;
const CELL: f64 = 52.0;
const MEASURE: f64 = 48.0;
const LANE: f64 = 26.0;
const HEADING: f64 = 24.0;
const TOP: f64 = 22.0;

/// The most changes the timeline lists, and races the sweep's table does.
const LISTED: usize = 400;
const RACES_LISTED: usize = 12;

/// The logic analyzer for build `s`: what `analysis` found, or why it
/// could not be traced.
pub fn trace_page(
    viewer: Option<&User>,
    s: &Schematic,
    analysis: Result<&Analysis, &String>,
) -> Markup {
    let path = format!("/schematics/{}", s.id);
    let name = s.name();
    base(
        viewer,
        &format!("{name}: logic analyzer"),
        &format!("{path}/trace"),
        &html! {
            p class="mb-1 text-meta" { a href=(path) { "← " (name) } }
            h1 class={ (PAGE_TITLE) " mb-4" } { "Logic analyzer" }
            @match analysis {
                Err(why) => p class="text-danger" { (why) },
                Ok(analysis) => (analyzed(&path, analysis)),
            }
        },
    )
}

fn analyzed(path: &str, a: &Analysis) -> Markup {
    let first = &a.runs[0];
    let raced = a.runs.iter().any(|run| !run.races.is_empty());
    // Redpiler's runs are shown beside the block-by-block one where it
    // orders a tie its own way, or comes out otherwise.
    let shown: Vec<(usize, &Run)> = a
        .runs
        .iter()
        .enumerate()
        .filter(|(_, run)| {
            raced
                || a.world
                    .as_ref()
                    .is_none_or(|world| !same_outputs(a, world, run))
        })
        .collect();
    html! {
        (change_form(path, a))
        @if let Some(world) = &a.world {
            section aria-label="Block by block" class="mt-8" {
                div class="mb-3 flex flex-wrap items-end gap-x-8 gap-y-2" {
                    (figure("Run", &html! {
                        span title="MCHPRS's world, block by block: ticks scheduled for one tick run by priority, then in the order they were scheduled, as the game's do" {
                            "block by block"
                        }
                    }, false))
                    (measures(a, world))
                }
                (chart(a, world))
            }
        }
        @for (i, run) in shown {
            section aria-label={ "Redpiler run " (i + 1) } class="mt-8" {
                div class="mb-3 flex flex-wrap items-end gap-x-8 gap-y-2" {
                    (figure(&format!("Redpiler {}", i + 1), &html! {
                        @if run.races.is_empty() { "compiled" } @else { (went_first(a, run)) " first" }
                    }, false))
                    (measures(a, run))
                }
                (chart(a, run))
            }
        }
        (legend())
        @if raced {
            section aria-labelledby="races-heading" class="mt-10" {
                h2 id="races-heading" class={ (SECTION_TITLE) " mb-2" } { "Undefined order" }
                (races_table(a))
            }
        }
        section aria-labelledby="changes-heading" class="mt-10" {
            h2 id="changes-heading" class={ (SECTION_TITLE) " mb-2" } {
                "Changes" @if a.runs.len() > 1 { span class="font-normal text-muted" { " · run 1" } }
            }
            (timeline(a, first))
        }
        @if let Some(sweep) = &a.sweep {
            (sweep_section(path, a, sweep))
        }
    }
}

/// The glitch of output `output` of the build at `path`, as `a` ran it:
/// its change, its measures, and a chart of its lines and the parts on the
/// glitch's way, linking to the logic analyzer.
pub fn glitch_trace(
    path: &str,
    a: &Analysis,
    output: &str,
    title: &str,
) -> Markup {
    let run = &a.runs[0];
    let link = format!("{path}/trace?{}", case_query(a, &a.case, Some(&a.at)));
    html! {
        div class="mt-4" {
            div class="mb-2 flex flex-wrap items-baseline gap-x-3 gap-y-1" {
                strong { (title) " · " (output) }
                span class="tabular-nums text-muted" { (case_words(a, &a.case)) (switching(a)) }
                a href=(link) { "Open in the logic analyzer" }
            }
            div class="mb-3 flex flex-wrap items-end gap-x-8 gap-y-2" {
                (measures(a, run))
                @if let Some(world) = &a.world {
                    @let port = a.outputs.iter().position(|line| line.name == output);
                    @let clean = port.is_some_and(|port| (0..a.outputs[port].width).all(|bit| world.trace.bit_changes(port, bit).len() < 2));
                    @let settled = (0..a.outputs.len()).filter_map(|p| world.trace.output_steps(p).last().map(|&(t, _)| t)).max();
                    (figure("Block by block", &html! {
                        @match settled { Some(tick) => { "@" (tick) }, None => "—", }
                        @if clean { " · no glitch" } @else { " · glitches" }
                    }, !clean))
                }
            }
            (chart(a, run))
            (legend())
        }
    }
}

/// The form choosing the change: each input's value before and after, and
/// the tick it switches on.
fn change_form(path: &str, a: &Analysis) -> Markup {
    html! {
        form method="get" action={ (path) "/trace" } class="flex flex-wrap items-end gap-x-6 gap-y-3" {
            @for (port, line) in a.inputs.iter().enumerate() {
                fieldset class="flex items-end gap-1.5 tabular-nums" {
                    legend class="mb-1 text-meta text-muted" {
                        (line.name) " · " (line.width)
                    }
                    input type="text" inputmode="numeric" name={ "from." (line.name) }
                        value=(a.case.from[port]) class="input w-24" aria-label={ (line.name) " from" } title="From";
                    span class="pb-2 text-muted" aria-hidden="true" { "→" }
                    input type="text" inputmode="numeric" name={ "to." (line.name) }
                        value=(a.case.to[port]) class="input w-24" aria-label={ (line.name) " to" } title="To";
                    span class="pb-2 text-muted" aria-hidden="true" { "@" }
                    input type="number" min="0" max="100" name={ "at." (line.name) }
                        value=(a.at[port]) class="input w-16" aria-label={ (line.name) " switches on tick" } title="Switches on tick";
                }
            }
            button type="submit" class="btn btn-primary" { "Trace" }
        }
    }
}

/// Each input changing after tick 0, such as ` · A @1`.
fn switching(a: &Analysis) -> String {
    a.inputs
        .iter()
        .enumerate()
        .filter(|&(port, _)| {
            a.at[port] != 0 && a.case.from[port] != a.case.to[port]
        })
        .fold(String::new(), |mut said, (port, line)| {
            let _ = write!(said, " · {} @{}", line.name, a.at[port]);
            said
        })
}

/// The query tracing `case`, each input switching on its tick of `at`, or
/// at 0 with none given.
fn case_query(a: &Analysis, case: &Case, at: Option<&[i32]>) -> String {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (port, line) in a.inputs.iter().enumerate() {
        pairs
            .push((format!("from.{}", line.name), case.from[port].to_string()));
        pairs.push((format!("to.{}", line.name), case.to[port].to_string()));
        if let Some(&tick) = at.and_then(|at| at.get(port))
            && tick != 0
        {
            pairs.push((format!("at.{}", line.name), tick.to_string()));
        }
    }
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

/// A measure: its name small over its value.
fn figure(name: &str, value: &Markup, danger: bool) -> Markup {
    html! {
        div class="flex flex-col" {
            span class="text-meta text-muted" { (name) }
            span class={ "font-semibold tabular-nums" (if danger { " text-danger" } else { "" }) } { (value) }
        }
    }
}

/// The tick each output settled by, its glitched bits, and the races of
/// `run`.
fn measures(a: &Analysis, run: &Run) -> Markup {
    let trace = &run.trace;
    html! {
        @for (port, line) in a.outputs.iter().enumerate() {
            @let settled = trace.output_steps(port).last().map(|&(tick, _)| tick);
            @let glitched: Vec<String> = (0..line.width)
                .filter(|&bit| trace.bit_changes(port, bit).len() > 1)
                .map(|bit| if line.width == 1 { line.name.clone() } else { format!("bit {bit}") })
                .collect();
            (figure(&format!("{} settles", line.name), &html! {
                @match settled { Some(tick) => { "@" (tick) }, None => "—", }
            }, false))
            @if !glitched.is_empty() {
                (figure(&format!("{} glitches", line.name), &html! { (glitched.join(", ")) }, true))
            }
        }
        (figure("Undefined order", &html! { (run.races.len()) }, !run.races.is_empty()))
    }
}

fn label(a: &Analysis, node: usize) -> String {
    a.label(node)
}

/// Whether runs `x` and `y` gave every output the same values on the same
/// ticks.
fn same_outputs(a: &Analysis, x: &Run, y: &Run) -> bool {
    (0..a.outputs.len())
        .all(|port| x.trace.output_steps(port) == y.trace.output_steps(port))
}

/// What went first in `run`'s first race: the part, or its rivals.
fn went_first(a: &Analysis, run: &Run) -> String {
    let Some(race) = run.races.first() else {
        return "—".to_owned();
    };
    let went = race
        .outcomes
        .iter()
        .find(|(_, outcome)| *outcome == race.ran)
        .map(|(ahead, _)| ahead.clone())
        .unwrap_or_default();
    if went.is_empty() {
        label(a, race.node)
    } else {
        names(a, &went)
    }
}

fn names(a: &Analysis, nodes: &[usize]) -> String {
    nodes
        .iter()
        .map(|&n| label(a, n))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a race's part comes to, `outcome`: its state, or for a pulse,
/// whether it gives one.
fn result(race: &Race, part: Part, outcome: u8) -> String {
    if race.pulse {
        if outcome > 0 { "pulse" } else { "none" }.to_owned()
    } else {
        state(part, outcome)
    }
}

/// Every race of every run, once: its tick, part, and rivals, what the part
/// comes to with each going first, and what it came to in each run.
fn races_table(a: &Analysis) -> Markup {
    let mut rows: Vec<&Race> = Vec::new();
    for run in &a.runs {
        for race in &run.races {
            let same = |r: &&Race| {
                r.tick == race.tick
                    && r.node == race.node
                    && r.rivals == race.rivals
            };
            if !rows.iter().any(same) {
                rows.push(race);
            }
        }
    }
    rows.sort_by_key(|race| race.tick);
    html! {
        div class="overflow-x-auto" {
            table class="border-collapse tabular-nums" {
                thead {
                    tr {
                        th scope="col" class=(TABLE_HEAD) { "Tick" }
                        th scope="col" class=(TABLE_HEAD) { "Part" }
                        th scope="col" class=(TABLE_HEAD) { "Against" }
                        th scope="col" class=(TABLE_HEAD) { "First → it ends" }
                        @if a.world.is_some() {
                            th scope="col" class=(TABLE_HEAD) { "Block by block" }
                        }
                        @for i in 0..a.runs.len() {
                            th scope="col" class=(TABLE_HEAD) { "Redpiler " (i + 1) }
                        }
                    }
                }
                tbody {
                    @for race in rows {
                        @let part = a.graph.nodes[race.node].part;
                        tr {
                            td class=(TABLE_CELL) { (race.tick) }
                            td class={ (TABLE_CELL) " text-danger" } { (label(a, race.node)) }
                            td class=(TABLE_CELL) { (names(a, &race.rivals)) }
                            td class=(TABLE_CELL) {
                                @for (ahead, outcome) in &race.outcomes {
                                    div {
                                        @if race.pulse {
                                            (if ahead.is_empty() { "none of them".to_owned() } else { names(a, ahead) })
                                        } @else if ahead.is_empty() {
                                            "the " (part.name())
                                        } @else {
                                            (names(a, ahead))
                                        }
                                        " → " strong { (result(race, part, *outcome)) }
                                    }
                                }
                            }
                            @for run in a.world.iter().chain(&a.runs) {
                                @let ran = run.races.iter().find(|r| r.tick == race.tick && r.node == race.node && r.rivals == race.rivals);
                                td class=(TABLE_CELL) {
                                    @match ran {
                                        Some(ran) => (result(ran, part, ran.ran)),
                                        None => span class="text-muted" { "—" },
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

/// What the chart's marks mean.
fn legend() -> Markup {
    html! {
        div class="mt-2 flex flex-wrap items-center gap-x-5 gap-y-1 text-meta text-muted" {
            span class="flex items-center gap-1.5" {
                svg width="18" height="10" aria-hidden="true" { line x1="9" x2="9" y1="0" y2="10" class="stroke-danger" stroke-width="1.5" stroke-dasharray="3 2" {} }
                "race"
            }
            span class="flex items-center gap-1.5" {
                svg width="18" height="10" aria-hidden="true" { line x1="9" x2="9" y1="0" y2="10" class="stroke-brand" stroke-width="2" {} }
                "settled"
            }
            span class="flex items-center gap-1.5" {
                svg width="18" height="10" aria-hidden="true" { rect x="1" y="1" width="16" height="8" class="fill-danger/15 stroke-danger" {} }
                "wrong value"
            }
            span { "ticks: redstone · places: (x, y, z) · outputs as they reach their blocks" }
        }
    }
}

/// What a lane of the chart draws.
enum Draw {
    /// A bus's value at each column, before the change first.
    Bus(Vec<Value>),
    /// A part's strength at each column, a comparator's drawn by height.
    Wave { strengths: Vec<u8>, analog: bool },
}

/// One lane of the chart.
struct Lane {
    label: String,
    draw: Draw,
    /// Drawn in the danger colour: a glitched bit, or a race's part.
    danger: bool,
    /// A note after its label, such as `race`.
    note: Option<&'static str>,
    /// The ticks of its races, marked on it.
    marks: Vec<i32>,
}

/// The chart of `run`: its inputs and outputs, and each output's changed
/// bits, each race marked down the chart at its tick. The parts inside the
/// build are left out, as a part's place says nothing a reader can find it
/// by.
fn chart(a: &Analysis, run: &Run) -> Markup {
    // A column before the change, then one for each tick.
    let columns = usize::try_from(run.trace.ticks() + 1).unwrap_or(1);
    let sections = [
        ("Inputs", input_lanes(a, &run.trace)),
        ("Outputs", output_lanes(a, &run.trace)),
    ];
    let count = |n: usize| f64::from(u32::try_from(n).unwrap_or(0));
    let width = LABEL + CELL * count(columns) + 8.0 + 2.0 * MEASURE;
    let lanes: usize = sections.iter().map(|(_, lanes)| lanes.len()).sum();
    let height =
        TOP + HEADING * count(sections.len()) + LANE * count(lanes) + 6.0;
    let x = |column: usize| LABEL + CELL * count(column);
    let measured = x(columns) + 8.0;
    // The tick by which every output has settled.
    let settled = (0..a.outputs.len())
        .filter_map(|port| run.trace.output_steps(port).last().map(|&(t, _)| t))
        .max();
    let mut svg = axes(columns, height, settled, &run.races);
    let mut y = TOP;
    for (title, lanes) in &sections {
        let _ = write!(
            svg,
            r#"<text x="0" y="{:.1}" class="fill-muted" font-size="11" font-weight="600">{}</text>"#,
            y + 16.0,
            escape(title)
        );
        y += HEADING;
        for lane in lanes {
            lane_svg(&mut svg, lane, y, &x, measured);
            y += LANE;
        }
    }
    let svg = format!(
        r#"<svg viewBox="0 0 {width:.0} {height:.0}" width="{width:.0}" height="{height:.0}" role="img" aria-label="Waveforms, tick by tick" style="font-variant-numeric: tabular-nums">{svg}</svg>"#
    );
    html! {
        div class="overflow-x-auto rounded-overlay border border-line bg-raised p-3" {
            (maud::PreEscaped(svg))
        }
    }
}

/// A lane drawing `draw`, labelled `label`, and nothing more.
fn plain(label: String, draw: Draw) -> Lane {
    Lane {
        label,
        draw,
        danger: false,
        note: None,
        marks: Vec::new(),
    }
}

/// Each input's value, a column before the change and one a tick.
fn input_lanes(a: &Analysis, trace: &Trace) -> Vec<Lane> {
    a.inputs
        .iter()
        .enumerate()
        .map(|(port, line)| {
            let value = |t: i32| {
                if t >= a.at[port] {
                    a.case.to[port]
                } else {
                    a.case.from[port]
                }
            };
            let values = (-1..trace.ticks()).map(value).collect();
            plain(line.name.clone(), Draw::Bus(values))
        })
        .collect()
}

/// Each output's value, then each of its bits that changed, one that
/// changed more than once marked as glitching.
fn output_lanes(a: &Analysis, trace: &Trace) -> Vec<Lane> {
    let ticks = trace.ticks();
    let mut lanes = Vec::new();
    for (port, line) in a.outputs.iter().enumerate() {
        let values = (-1..ticks).map(|t| trace.output(port, t)).collect();
        lanes.push(plain(line.name.clone(), Draw::Bus(values)));
        if line.width == 1 {
            continue;
        }
        for bit in 0..line.width {
            let changes = trace.bit_changes(port, bit).len();
            if changes == 0 {
                continue;
            }
            let strengths = (-1..ticks)
                .map(|t| 15 * u8::from(trace.output(port, t) >> bit & 1 == 1))
                .collect();
            lanes.push(Lane {
                danger: changes > 1,
                note: (changes > 1).then_some("glitch"),
                ..plain(
                    format!("  {}", bit_label(bit)),
                    Draw::Wave {
                        strengths,
                        analog: false,
                    },
                )
            });
        }
    }
    lanes
}

/// The chart's frame, `columns` wide and `height` high: each tick's column,
/// lit while pointed at, its grid and numbers, the measures' headings, and
/// down it the tick every output settled by, `settled`, and each of
/// `races`'.
fn axes(
    columns: usize,
    height: f64,
    settled: Option<i32>,
    races: &[Race],
) -> String {
    let count = |n: usize| f64::from(u32::try_from(n).unwrap_or(0));
    let x = |column: usize| LABEL + CELL * count(column);
    let measured = x(columns) + 8.0;
    let mut svg = String::new();
    // Under everything, each tick's column, lit while pointed at.
    for column in 1..columns {
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.1}" width="{CELL:.1}" height="{:.1}" class="fill-transparent hover:fill-selection"><title>tick {}</title></rect>"#,
            x(column),
            TOP - 4.0,
            height - TOP + 4.0,
            column - 1
        );
    }
    for column in 0..=columns {
        let _ = write!(
            svg,
            r#"<line x1="{0:.1}" x2="{0:.1}" y1="{1:.1}" y2="{2:.1}" class="stroke-line" stroke-width="1"/>"#,
            x(column),
            TOP - 4.0,
            height
        );
    }
    for column in 0..columns {
        let text = if column == 0 {
            "before".to_owned()
        } else {
            (column - 1).to_string()
        };
        let settles = settled
            .is_some_and(|t| usize::try_from(t + 1).ok() == Some(column));
        let (class, weight) = if settles {
            ("fill-brand", "700")
        } else {
            ("fill-muted", "400")
        };
        let _ = write!(
            svg,
            r#"<text x="{:.1}" y="14" text-anchor="middle" class="{class}" font-size="11" font-weight="{weight}">{text}</text>"#,
            x(column) + CELL / 2.0
        );
    }
    for (offset, name) in [(0.5, "edges"), (1.5, "last")] {
        let _ = write!(
            svg,
            r#"<text x="{:.1}" y="14" text-anchor="middle" class="fill-muted" font-size="11">{name}</text>"#,
            measured + MEASURE * offset
        );
    }
    if let Some(tick) = settled {
        let column = usize::try_from(tick + 1).unwrap_or(0);
        let _ = write!(
            svg,
            r#"<line x1="{0:.1}" x2="{0:.1}" y1="{1:.1}" y2="{2:.1}" class="stroke-brand" stroke-width="2"><title>Settled by tick {3}</title></line>"#,
            x(column),
            TOP - 4.0,
            height,
            tick
        );
    }
    for race in races {
        let column = usize::try_from(race.tick + 1).unwrap_or(0);
        let _ = write!(
            svg,
            r#"<line x1="{0:.1}" x2="{0:.1}" y1="{1:.1}" y2="{2:.1}" class="stroke-danger" stroke-width="1.5" stroke-dasharray="4 3"><title>Race on tick {3}</title></line>"#,
            x(column),
            TOP - 4.0,
            height,
            race.tick
        );
    }
    svg
}

/// An output bit's lane label, such as `bit 3`.
fn bit_label(bit: usize) -> String {
    format!("bit {bit}")
}

/// The top, middle, and bottom of a lane's drawing, its top at `top`.
fn levels(top: f64) -> (f64, f64, f64) {
    let (high, low) = (top + 5.0, top + LANE - 5.0);
    (high, f64::midpoint(high, low), low)
}

/// Draws `lane` into `svg` with its top at `top`, column `c` starting at
/// `x(c)`, and its edges and last change from `measured` on.
fn lane_svg(
    svg: &mut String,
    lane: &Lane,
    top: f64,
    x: &impl Fn(usize) -> f64,
    measured: f64,
) {
    let label = match lane.note {
        Some(note) => format!("{} · {note}", lane.label),
        None => lane.label.clone(),
    };
    let label_class = if lane.danger {
        "fill-danger"
    } else {
        "fill-ink"
    };
    let _ = write!(
        svg,
        r#"<text x="0" y="{:.1}" class="{label_class}" font-size="12" xml:space="preserve">{}<title>{}</title></text>"#,
        top + 16.0,
        escape(&truncate(&label, 46)),
        escape(&label)
    );
    match &lane.draw {
        Draw::Bus(values) => bus_svg(svg, values, top, x),
        Draw::Wave { strengths, analog } => {
            wave_svg(svg, strengths, *analog, lane.danger, top, x);
        }
    }
    let (_, mid, _) = levels(top);
    let (edges, last) = match &lane.draw {
        Draw::Bus(values) => edges(values),
        Draw::Wave { strengths, .. } => edges(strengths),
    };
    let last = last.map_or_else(|| "—".to_owned(), |tick| format!("@{tick}"));
    let class = if lane.danger {
        "fill-danger"
    } else {
        "fill-ink"
    };
    for (offset, text) in [(0.5, edges.to_string()), (1.5, last)] {
        let _ = write!(
            svg,
            r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" class="{class}" font-size="12">{text}</text>"#,
            measured + MEASURE * offset,
            mid + 4.0
        );
    }
    for &tick in &lane.marks {
        let column = usize::try_from(tick + 1).unwrap_or(0);
        let _ = write!(
            svg,
            r#"<circle cx="{:.1}" cy="{mid:.1}" r="4.5" class="fill-danger"><title>Race on tick {tick}</title></circle>"#,
            x(column)
        );
    }
}

/// A bus's `values`, a column each, as a run of cells, each value shown
/// where it fits; after the bus first changes, a value other than the one
/// it settles on is drawn as wrong.
fn bus_svg(
    svg: &mut String,
    values: &[Value],
    top: f64,
    x: &impl Fn(usize) -> f64,
) {
    let (high, mid, low) = levels(top);
    let last = values.last().copied().unwrap_or(0);
    let first_change = values.windows(2).position(|w| w[0] != w[1]);
    let mut start = 0;
    while start < values.len() {
        let value = values[start];
        let mut end = start + 1;
        while end < values.len() && values[end] == value {
            end += 1;
        }
        let wrong = first_change.is_some_and(|c| start > c) && value != last;
        let (x0, x1) = (x(start), x(end));
        let (fill, stroke, text_class) = if wrong {
            ("fill-danger/15", "stroke-danger", "fill-danger")
        } else {
            ("fill-none", "stroke-ink", "fill-ink")
        };
        let _ = write!(
            svg,
            r#"<polygon points="{:.1},{mid:.1} {:.1},{high:.1} {:.1},{high:.1} {:.1},{mid:.1} {:.1},{low:.1} {:.1},{low:.1}" class="{fill} {stroke}" stroke-width="1.25"/>"#,
            x0 + 1.0,
            x0 + 5.0,
            x1 - 5.0,
            x1 - 1.0,
            x1 - 5.0,
            x0 + 5.0
        );
        let text = value.to_string();
        #[allow(clippy::cast_precision_loss, reason = "a short label")]
        let fits = (x1 - x0) > 7.0 * text.len() as f64 + 10.0;
        if fits {
            let _ = write!(
                svg,
                r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" class="{text_class}" font-size="12">{text}</text>"#,
                f64::midpoint(x0, x1),
                mid + 4.0
            );
        }
        start = end;
    }
}

/// A part's `strengths`, a column each: on or off, or for an `analog`
/// one, a comparator's, its height and number.
fn wave_svg(
    svg: &mut String,
    strengths: &[u8],
    analog: bool,
    danger: bool,
    top: f64,
    x: &impl Fn(usize) -> f64,
) {
    let (high, _, low) = levels(top);
    let level = |s: u8| {
        if analog {
            low - (low - high) * f64::from(s) / 15.0
        } else if s > 0 {
            high
        } else {
            low
        }
    };
    let mut d = format!("M{:.1},{:.1}", x(0), level(strengths[0]));
    for (column, &s) in strengths.iter().enumerate() {
        let _ = write!(
            d,
            " L{:.1},{:.1} L{:.1},{:.1}",
            x(column),
            level(s),
            x(column + 1),
            level(s)
        );
    }
    if analog {
        let area = format!(
            "{d} L{:.1},{low:.1} L{:.1},{low:.1} Z",
            x(strengths.len()),
            x(0)
        );
        let _ = write!(svg, r#"<path d="{area}" class="fill-brand/10"/>"#);
        for (column, &s) in strengths.iter().enumerate() {
            let changed = column == 0 || strengths[column - 1] != s;
            if changed && s > 0 {
                // Above the line where there is room, else below it.
                let y = if level(s) - high > 9.0 {
                    level(s) - 2.0
                } else {
                    level(s) + 10.0
                };
                let _ = write!(
                    svg,
                    r#"<text x="{:.1}" y="{y:.1}" class="fill-muted" font-size="10">{s}</text>"#,
                    x(column) + 3.0
                );
            }
        }
    }
    let stroke = if danger {
        "stroke-danger"
    } else {
        "stroke-ink"
    };
    let _ = write!(
        svg,
        r#"<path d="{d}" class="fill-none {stroke}" stroke-width="1.75" stroke-linejoin="round"/>"#
    );
}

/// How many times `values`, a column each from before the change, changed,
/// and the tick of the last.
fn edges<T: PartialEq>(values: &[T]) -> (usize, Option<i32>) {
    let flips: Vec<usize> = (1..values.len())
        .filter(|&column| values[column] != values[column - 1])
        .collect();
    let last = flips
        .last()
        .and_then(|&column| i32::try_from(column).ok())
        .map(|column| column - 1);
    (flips.len(), last)
}

fn truncate(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let kept: String = text.chars().take(most - 1).collect();
    format!("{kept}…")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Each change of `run`, by tick, with what set it off.
fn timeline(a: &Analysis, run: &Run) -> Markup {
    let changes = run.trace.changes();
    let racing = |change: &Change| {
        run.races
            .iter()
            .any(|r| r.node == change.node && r.tick == change.tick)
    };
    html! {
        div class="overflow-x-auto" {
            table class="border-collapse tabular-nums" {
                thead {
                    tr {
                        th scope="col" class=(TABLE_HEAD) { "Tick" }
                        th scope="col" class=(TABLE_HEAD) { "Part" }
                        th scope="col" class=(TABLE_HEAD) { "Change" }
                        th scope="col" class=(TABLE_HEAD) { "Set off by" }
                    }
                }
                tbody {
                    @for change in changes.iter().take(LISTED) {
                        @let part = a.graph.nodes[change.node].part;
                        @let causes = run.trace.causes(&a.graph, change);
                        tr class=[racing(change).then_some("text-danger")] {
                            td class=(TABLE_CELL) { (change.tick) }
                            td class=(TABLE_CELL) { (label(a, change.node)) }
                            td class=(TABLE_CELL) {
                                (state(part, change.was)) " → " (state(part, change.now))
                                @if racing(change) { strong { " race" } }
                            }
                            td class={ (TABLE_CELL) " text-muted" } {
                                @if matches!(part, Part::Switch { .. }) {
                                    "input"
                                } @else if causes.is_empty() {
                                    "—"
                                } @else {
                                    @for cause in &causes {
                                        div { (label(a, cause.node)) " @" (cause.tick) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        @if changes.len() > LISTED {
            p class="mt-2 text-meta text-muted" { "+" (grouped(u64::try_from(changes.len() - LISTED).unwrap_or(0))) }
        }
    }
}

/// How the sweep's changes settled, a bar a tick, where they glitched, and
/// its races, each with a link to trace it.
fn sweep_section(path: &str, a: &Analysis, sweep: &Sweep) -> Markup {
    let trace =
        |case: &Case| format!("{path}/trace?{}", case_query(a, case, None));
    let count = |n: usize| grouped(u64::try_from(n).unwrap_or(0));
    html! {
        section aria-labelledby="sweep-heading" class="mt-12" {
            h2 id="sweep-heading" class={ (SECTION_TITLE) " mb-3" } {
                "Sweep " span class="font-normal text-muted tabular-nums" {
                    "· " (count(sweep.cases)) " changes, all @0: each input alone, " (count(a.random)) " random"
                }
            }
            div class="flex flex-wrap gap-x-12 gap-y-6" {
                @for (port, line) in a.outputs.iter().enumerate() {
                    @let settling = &sweep.outputs[port];
                    @let most = settling.settled.values().copied().max().unwrap_or(1).max(1);
                    div {
                        p class="mb-1 font-semibold" { (line.name) " settles" }
                        table class="border-collapse tabular-nums" {
                            tbody {
                                @for (tick, n) in &settling.settled {
                                    @let slowest = settling.slowest.as_ref().filter(|(t, _)| t == tick);
                                    tr {
                                        td class="pr-2 text-right text-muted" { "@" (tick) }
                                        td class="w-48 py-0.5" {
                                            div class="h-3 rounded-sm bg-brand/70"
                                                style={ "width: " ((100 * n / most).max(1)) "%" } {}
                                        }
                                        td class="pl-2" { (count(*n)) }
                                        td class="pl-3" {
                                            @if let Some((_, case)) = slowest {
                                                a href=(trace(case)) title=(case_words(a, case)) { "slowest" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        @if settling.settled.is_empty() {
                            p class="text-muted" { "—" }
                        }
                        @if !settling.glitches.is_empty() {
                            table class="mt-2 border-collapse tabular-nums" {
                                thead {
                                    tr {
                                        th scope="col" class=(TABLE_HEAD) { "Glitch" }
                                        th scope="col" class=(TABLE_HEAD) { "Changes" }
                                        th scope="col" class=(TABLE_HEAD) { "First" }
                                    }
                                }
                                tbody {
                                    @for (bit, (n, case)) in &settling.glitches {
                                        tr class="text-danger" {
                                            td class=(TABLE_CELL) { (line.bit_name(*bit)) }
                                            td class=(TABLE_CELL) { (count(*n)) }
                                            td class=(TABLE_CELL) { a href=(trace(case)) { (case_words(a, case)) } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            @if !sweep.races.is_empty() {
                div class="mt-6 overflow-x-auto" {
                    table class="border-collapse tabular-nums" {
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Race" }
                                th scope="col" class=(TABLE_HEAD) { "Against" }
                                th scope="col" class=(TABLE_HEAD) { "Changes" }
                                th scope="col" class=(TABLE_HEAD) { "Tick" }
                                th scope="col" class=(TABLE_HEAD) { "First" }
                            }
                        }
                        tbody {
                            @for seen in sweep.races.iter().take(RACES_LISTED) {
                                tr {
                                    td class={ (TABLE_CELL) " text-danger" } {
                                        (label(a, seen.race.node))
                                        @if seen.race.pulse { span class="text-muted" { " · pulse" } }
                                    }
                                    td class=(TABLE_CELL) { (names(a, &seen.race.rivals)) }
                                    td class=(TABLE_CELL) { (count(seen.count)) }
                                    td class=(TABLE_CELL) { "@" (seen.race.tick) }
                                    td class=(TABLE_CELL) { a href=(trace(&seen.case)) { (case_words(a, &seen.case)) } }
                                }
                            }
                        }
                    }
                }
                @if sweep.races.len() > RACES_LISTED {
                    p class="mt-2 text-meta text-muted" { "+" (sweep.races.len() - RACES_LISTED) " in fewer changes" }
                }
            }
        }
    }
}
