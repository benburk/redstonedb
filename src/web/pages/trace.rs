//! The logic analyzer's page: a change of a build's inputs traced, as a
//! chart of every line's waveform and every component that changed, each
//! race marked and put in words, each way it may go when another compile
//! ran it the other way; what set each change off; and how a sweep of many
//! changes settled and glitched.

use std::fmt::Write as _;

use maud::{Markup, html};

use crate::{
    models::{schematic::Schematic, user::User},
    recognize::inspect::place,
    simulate::{
        Value,
        trace::{Case, Change, Part, Race, Trace},
    },
    web::trace::{Analysis, Line, Run, becomes, case_words, verb},
};

use super::{PAGE_TITLE, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, base, grouped};

/// The chart's label column, a tick's width, and the height of a lane, a
/// section's heading, and the tick numbers along its top.
const LABEL: f64 = 290.0;
const CELL: f64 = 52.0;
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
            h1 class={ (PAGE_TITLE) " mb-2" } { "Logic analyzer" }
            p class="mb-6 max-w-calm text-muted" {
                "One change of the build's inputs, run tick by tick as the simulator runs it: "
                "each output bit's waveform, and every torch, repeater, and comparator that changed, "
                "with what set it off. Ticks are redstone ticks from the first input switching, "
                "and outputs are drawn as they arrive at their blocks, as the build's timing counts them. "
                "Places are (x, y, z) in the build as the site stores it, from its lowest north-west corner."
            }
            @match analysis {
                Err(why) => p class="text-danger" { "Could not trace it: " (why) },
                Ok(analysis) => (analyzed(&path, analysis)),
            }
        },
    )
}

fn analyzed(path: &str, a: &Analysis) -> Markup {
    let first = &a.runs[0];
    html! {
        (change_form(path, a))
        section aria-labelledby="change-heading" class="mt-10" {
            h2 id="change-heading" class={ (SECTION_TITLE) " mb-1" } { (case_words(a, &a.case)) }
            p class="mb-4 text-meta text-muted" { (switching(a)) }
            @if !first.races.is_empty() {
                (races(a, first))
            }
            @for run in &a.runs {
                @if a.runs.len() > 1 {
                    h3 class="mt-8 mb-1 font-semibold" { (order_title(a, run)) }
                }
                (outcome(a, &run.trace))
                (chart(a, run))
            }
        }
        section aria-labelledby="timeline-heading" class="mt-12" {
            h2 id="timeline-heading" class={ (SECTION_TITLE) " mb-1" } { "What happened" }
            p class="mb-3 text-meta text-muted" {
                "Each change, by tick, and the changes of what powers it that set it off"
                @if a.runs.len() > 1 { ", as the first run above went" }
                "."
            }
            (timeline(a, first))
        }
        (sweep_section(path, a))
    }
}

/// The form choosing the change: each input's value before and after, and
/// the tick it switches on.
fn change_form(path: &str, a: &Analysis) -> Markup {
    html! {
        form method="get" action={ (path) "/trace" } class="max-w-calm" {
            div class="overflow-x-auto" {
                table class="border-collapse tabular-nums" {
                    caption class="sr-only" { "The change to trace" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Input" }
                            th scope="col" class=(TABLE_HEAD) { "From" }
                            th scope="col" class=(TABLE_HEAD) { "To" }
                            th scope="col" class=(TABLE_HEAD) { "Switches on tick" }
                        }
                    }
                    tbody {
                        @for (port, line) in a.inputs.iter().enumerate() {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                    (line.name)
                                    span class="text-muted" { " · " (line.width) (if line.width == 1 { " bit" } else { " bits" }) }
                                }
                                td class=(TABLE_CELL) {
                                    input type="text" inputmode="numeric" name={ "from." (line.name) }
                                        value=(a.case.from[port]) class="input w-28" aria-label={ (line.name) " before" };
                                }
                                td class=(TABLE_CELL) {
                                    input type="text" inputmode="numeric" name={ "to." (line.name) }
                                        value=(a.case.to[port]) class="input w-28" aria-label={ (line.name) " after" };
                                }
                                td class=(TABLE_CELL) {
                                    input type="number" min="0" max="100" name={ "at." (line.name) }
                                        value=(a.at[port]) class="input w-20" aria-label={ (line.name) " switches on tick" };
                                }
                            }
                        }
                    }
                }
            }
            div class="mt-3 flex flex-wrap items-center gap-3" {
                button type="submit" class="btn btn-primary" { "Trace" }
                span class="text-meta text-muted" { "Values in decimal, or hex after 0x, or binary after 0b." }
            }
        }
    }
}

/// When each input switches.
fn switching(a: &Analysis) -> String {
    if a.at.iter().all(|&at| at == 0) {
        return "Every input switches at tick 0.".to_owned();
    }
    let each: Vec<String> = a
        .inputs
        .iter()
        .zip(&a.at)
        .map(|(line, at)| format!("{} at {at}", line.name))
        .collect();
    format!("Switching {}.", each.join(", "))
}

/// The query tracing `case`, every input switching at tick 0.
fn case_query(a: &Analysis, case: &Case) -> String {
    let pairs: Vec<(String, String)> = a
        .inputs
        .iter()
        .enumerate()
        .flat_map(|(port, line)| {
            [
                (format!("from.{}", line.name), case.from[port].to_string()),
                (format!("to.{}", line.name), case.to[port].to_string()),
            ]
        })
        .collect();
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

/// Each output's values over the run, and the bits that glitched.
fn outcome(a: &Analysis, trace: &Trace) -> Markup {
    html! {
        ul class="mb-4 flex flex-col gap-1 tabular-nums" {
            @for (port, line) in a.outputs.iter().enumerate() {
                @let steps = trace.output_steps(port);
                @let glitched: Vec<usize> = (0..line.width)
                    .filter(|&bit| trace.bit_changes(port, bit).len() > 1)
                    .collect();
                li {
                    strong { (line.name) } ": " (trace.output(port, -1))
                    @if steps.is_empty() {
                        span class="text-muted" { ", unchanged" }
                    }
                    @for (tick, value) in &steps {
                        ", " (value) span class="text-muted" { " on tick " (tick) }
                    }
                    @if let Some(&(last, _)) = steps.last() {
                        ". Settled by tick " (last) "."
                    }
                    @if !glitched.is_empty() {
                        " "
                        strong class="text-danger" {
                            "Glitched in "
                            (glitched.iter().map(|&b| line.bit_name(b)).collect::<Vec<_>>().join(", "))
                            "."
                        }
                    }
                }
            }
        }
    }
}

fn label(a: &Analysis, node: usize) -> String {
    a.label(node)
}

/// The races of `run`, each in words, the first set apart as the one the
/// rest likely follow from.
fn races(a: &Analysis, run: &Run) -> Markup {
    let first = run.races[0].tick;
    html! {
        div class="mb-6 rounded-overlay border border-danger/40 bg-danger/5 p-4" {
            p class="mb-2 font-semibold text-danger" {
                (run.races.len()) (if run.races.len() == 1 { " race" } else { " races" })
            }
            p class="mb-3 text-meta text-muted" {
                "A race is a component running its tick on the same tick as an input of its own changes, "
                "both at the same priority, so which goes first decides what it does. "
                "The game runs them in the order they were scheduled, which comes of the order it updates blocks in, "
                "so of where the build stands and which way it faces: a build may work in one place and glitch in another. "
                "To fix one, have the signals meet a tick apart, or hand a line over with an overlap rather than a gap."
            }
            ul class="flex flex-col gap-3" {
                @for race in &run.races {
                    li {
                        (race_words(a, run, race))
                        @if race.tick > first {
                            span class="text-meta text-muted" { " Likely the doing of a race before it." }
                        }
                    }
                }
            }
        }
    }
}

fn race_words(a: &Analysis, run: &Run, race: &Race) -> Markup {
    let part = a.graph.nodes[race.node].part;
    let node = label(a, race.node);
    let rivals: Vec<String> =
        race.rivals.iter().map(|&r| label(a, r)).collect();
    let rivals = rivals.join(" and ");
    let was = run.trace.strength(race.node, race.tick - 1);
    let who = |ahead: &[usize]| {
        if ahead.is_empty() {
            format!("the {}", part.name())
        } else {
            ahead
                .iter()
                .map(|&r| label(a, r))
                .collect::<Vec<_>>()
                .join(" and ")
        }
    };
    html! {
        strong { "Tick " (race.tick) ", " (node) ". " }
        @if race.pulse {
            "It is off, and its inputs " (rivals) " change on this tick at the same priority. "
            "In one order one of them powers it for an instant, setting it off to turn on "
            (part.delay()) (if part.delay() == 1 { " tick" } else { " ticks" }) " later, a pulse; in another it never is. "
            "This run, it " (if race.ran > 0 { "pulsed." } else { "stayed off." })
        } @else {
            "It runs its tick on the same tick as " (rivals) ", which "
            (if race.rivals.len() == 1 { "powers" } else { "power" })
            " it, changes, at the same priority. "
            @for (i, (ahead, outcome)) in race.outcomes.iter().enumerate() {
                @if i > 0 { "; if " } @else { "If " }
                (who(ahead)) " goes first, it " (becomes(part, was, *outcome))
            }
            ". This run, "
            (race.outcomes.iter().find(|(_, o)| *o == race.ran).map_or_else(|| "it".to_owned(), |(ahead, _)| who(ahead)))
            " went first."
        }
    }
}

/// Which way the first race went in `run`, as a heading.
fn order_title(a: &Analysis, run: &Run) -> String {
    let Some(race) = run.races.first() else {
        return "Another run".to_owned();
    };
    let part = a.graph.nodes[race.node].part;
    let went = race
        .outcomes
        .iter()
        .find(|(_, outcome)| *outcome == race.ran)
        .map(|(ahead, _)| ahead.clone())
        .unwrap_or_default();
    let first = if went.is_empty() {
        label(a, race.node)
    } else {
        went.iter()
            .map(|&r| label(a, r))
            .collect::<Vec<_>>()
            .join(" and ")
    };
    let was = run.trace.strength(race.node, race.tick - 1);
    let it = if went.is_empty() {
        "it".to_owned()
    } else {
        label(a, race.node)
    };
    format!(
        "When {first} goes first, {it} {}",
        becomes(part, was, race.ran)
    )
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

/// The chart of `run`: its inputs and outputs, each output's changed bits,
/// then every part that changed, each race marked down the chart at its
/// tick.
fn chart(a: &Analysis, run: &Run) -> Markup {
    // A column before the change, then one for each tick.
    let columns = usize::try_from(run.trace.ticks() + 1).unwrap_or(1);
    let sections = [
        ("Inputs", input_lanes(a, &run.trace)),
        ("Outputs", output_lanes(a, &run.trace)),
        ("Components that changed", part_lanes(a, run)),
    ];
    let count = |n: usize| f64::from(u32::try_from(n).unwrap_or(0));
    let width = LABEL + CELL * count(columns) + 8.0;
    let lanes: usize = sections.iter().map(|(_, lanes)| lanes.len()).sum();
    let height =
        TOP + HEADING * count(sections.len()) + LANE * count(lanes) + 6.0;
    let x = |column: usize| LABEL + CELL * count(column);
    let mut svg = String::new();
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
        let _ = write!(
            svg,
            r#"<text x="{:.1}" y="14" text-anchor="middle" class="fill-muted" font-size="11">{text}</text>"#,
            x(column) + CELL / 2.0
        );
    }
    for race in &run.races {
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
            lane_svg(&mut svg, lane, y, &x);
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
                    format!("  {}", bit_label(line, bit)),
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

/// Each part that changed, by the tick it first did, then each race's part
/// that never did, the order having gone its way; a race's part and its
/// rivals marked.
fn part_lanes(a: &Analysis, run: &Run) -> Vec<Lane> {
    let trace = &run.trace;
    let ticks = trace.ticks();
    let racing: Vec<usize> = run.races.iter().map(|r| r.node).collect();
    let rivals: Vec<usize> =
        run.races.iter().flat_map(|r| r.rivals.clone()).collect();
    let mut changed: Vec<(i32, usize)> = (0..a.graph.nodes.len())
        .filter(|&node| a.graph.nodes[node].part.ticks())
        .filter_map(|node| {
            (0..ticks)
                .find(|&t| trace.changed(node, t))
                .or_else(|| racing.contains(&node).then_some(ticks))
                .map(|t| (t, node))
        })
        .collect();
    changed.sort_unstable();
    changed
        .into_iter()
        .map(|(_, node)| {
            let part = a.graph.nodes[node].part;
            let strengths =
                (-1..ticks).map(|t| trace.strength(node, t)).collect();
            let analog = matches!(part, Part::Comparator { .. });
            Lane {
                danger: racing.contains(&node),
                note: if racing.contains(&node) {
                    Some("race")
                } else if rivals.contains(&node) {
                    Some("rival")
                } else {
                    None
                },
                marks: run
                    .races
                    .iter()
                    .filter(|r| r.node == node)
                    .map(|r| r.tick)
                    .collect(),
                ..plain(label(a, node), Draw::Wave { strengths, analog })
            }
        })
        .collect()
}

/// An output bit's lane label, such as `bit 3 at (7, 8, 8)`.
fn bit_label(line: &Line, bit: usize) -> String {
    format!("bit {bit} at {}", place(line.blocks[bit]))
}

/// The top, middle, and bottom of a lane's drawing, its top at `top`.
fn levels(top: f64) -> (f64, f64, f64) {
    let (high, low) = (top + 5.0, top + LANE - 5.0);
    (high, f64::midpoint(high, low), low)
}

/// Draws `lane` into `svg` with its top at `top`, column `c` starting at
/// `x(c)`.
fn lane_svg(
    svg: &mut String,
    lane: &Lane,
    top: f64,
    x: &impl Fn(usize) -> f64,
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
                caption class="sr-only" { "What happened, tick by tick" }
                thead {
                    tr {
                        th scope="col" class=(TABLE_HEAD) { "Tick" }
                        th scope="col" class=(TABLE_HEAD) { "What changed" }
                        th scope="col" class=(TABLE_HEAD) { "Set off by" }
                    }
                }
                tbody {
                    @for change in changes.iter().take(LISTED) {
                        @let part = a.graph.nodes[change.node].part;
                        @let causes = run.trace.causes(&a.graph, change);
                        tr class=[racing(change).then_some("text-danger")] {
                            td class=(TABLE_CELL) { (change.tick) }
                            td class=(TABLE_CELL) {
                                (label(a, change.node)) " " (verb(part, change, false))
                                @if racing(change) { strong { " · race" } }
                            }
                            td class={ (TABLE_CELL) " text-muted" } {
                                @if matches!(part, Part::Switch { .. }) {
                                    "the change"
                                } @else if causes.is_empty() {
                                    "—"
                                } @else {
                                    @for (i, cause) in causes.iter().enumerate() {
                                        @if i > 0 { ", " }
                                        (label(a, cause.node)) " "
                                        (verb(a.graph.nodes[cause.node].part, cause, true))
                                        " on tick " (cause.tick)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        @if changes.len() > LISTED {
            p class="mt-2 text-meta text-muted" { "And " (grouped(u64::try_from(changes.len() - LISTED).unwrap_or(0))) " more." }
        }
    }
}

/// How the sweep's changes settled, where they glitched, and its races,
/// each with a link to trace it.
fn sweep_section(path: &str, a: &Analysis) -> Markup {
    let sweep = &a.sweep;
    let trace = |case: &Case| format!("{path}/trace?{}", case_query(a, case));
    html! {
        section aria-labelledby="sweep-heading" class="mt-12 max-w-calm" {
            h2 id="sweep-heading" class={ (SECTION_TITLE) " mb-1" } {
                "Across " (grouped(u64::try_from(sweep.cases).unwrap_or(0))) " changes"
            }
            p class="mb-4 text-meta text-muted" {
                "Every input switching at tick 0: each input alone from all off to each of its values, "
                "and " (grouped(u64::try_from(a.random).unwrap_or(0))) " changes at random. "
                "The sweep is one compile, so each race goes one way in it, and only the races that come first in a change are listed."
            }
            @for (port, line) in a.outputs.iter().enumerate() {
                @let settling = &sweep.outputs[port];
                div class="mb-4" {
                    p {
                        strong { (line.name) } " settles by "
                        @for (i, (tick, count)) in settling.settled.iter().enumerate() {
                            @if i > 0 { ", " }
                            "tick " (tick) " in " (grouped(u64::try_from(*count).unwrap_or(0)))
                        }
                        @if settling.settled.is_empty() { "—, never changing" }
                        "."
                        @if let Some((tick, case)) = &settling.slowest {
                            " Slowest, tick " (tick) ": "
                            a href=(trace(case)) { (case_words(a, case)) }
                            "."
                        }
                    }
                    @if !settling.glitches.is_empty() {
                        ul class="mt-1 flex flex-col gap-0.5 text-danger" {
                            @for (bit, (count, case)) in &settling.glitches {
                                li {
                                    (line.bit_name(*bit)) " glitched in " (grouped(u64::try_from(*count).unwrap_or(0)))
                                    (if *count == 1 { " change" } else { " changes" }) ", as "
                                    a href=(trace(case)) { (case_words(a, case)) }
                                }
                            }
                        }
                    }
                }
            }
            @if sweep.races.is_empty() {
                p class="text-muted" { "No races." }
            } @else {
                div class="overflow-x-auto" {
                    table class="border-collapse tabular-nums" {
                        caption class="sr-only" { "Races" }
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Race" }
                                th scope="col" class=(TABLE_HEAD) { "Against" }
                                th scope="col" class=(TABLE_HEAD) { "In changes" }
                                th scope="col" class=(TABLE_HEAD) { "First" }
                            }
                        }
                        tbody {
                            @for seen in sweep.races.iter().take(RACES_LISTED) {
                                tr {
                                    td class=(TABLE_CELL) {
                                        (label(a, seen.race.node))
                                        @if seen.race.pulse { span class="text-muted" { ", a pulse" } }
                                    }
                                    td class=(TABLE_CELL) {
                                        (seen.race.rivals.iter().map(|&r| label(a, r)).collect::<Vec<_>>().join(", "))
                                    }
                                    td class=(TABLE_CELL) { (grouped(u64::try_from(seen.count).unwrap_or(0))) }
                                    td class=(TABLE_CELL) {
                                        a href=(trace(&seen.case)) { (case_words(a, &seen.case)) }
                                        span class="text-muted" { ", tick " (seen.race.tick) }
                                    }
                                }
                            }
                        }
                    }
                }
                @if sweep.races.len() > RACES_LISTED {
                    p class="mt-2 text-meta text-muted" {
                        "And " (sweep.races.len() - RACES_LISTED) " more, each in as few changes or fewer."
                    }
                }
            }
        }
    }
}
