//! The later stages of a build's checks, each opening onto what it found:
//! its test, as the cases it tried and, for a failure, the bits and cases
//! it got wrong; its timing; and how its copies stack.

use maud::{Markup, html};

use crate::{
    simulate::{BitCount, Report, Run, Stacking},
    stack::StackDirection,
    web::{
        build::{
            checks::{Known, State, bit_counts},
            checks_view::Stage,
            simulation_view::{cell_faults, shown, timing_section},
        },
        ui::{self, TABLE_CELL, TABLE_HEAD, text::grouped},
    },
};

/// Its test: every case it tried and how many came out right; for a
/// failure, how often each output bit was wrong, and the first wrong cases
/// as a truth table.
pub(crate) fn test_stage(known: &Known) -> Stage {
    let (state, summary, body) = match known.report {
        Some(Report::Ran(run)) if run.wrong == 0 => (
            State::Pass,
            format!("all {} cases right · {}", grouped(run.cases), run.pattern),
            tried(run),
        ),
        Some(Report::Ran(run)) => (
            State::Fail,
            format!(
                "{} of {} cases wrong · {}",
                grouped(run.wrong),
                grouped(run.cases),
                run.pattern
            ),
            html! {
                (tried(run))
                @if let Some(hint) = &run.hint {
                    p class="mt-3" { strong { "Likely cause: " } (hint) }
                }
                (bit_strip(run))
                @if !run.faults.is_empty() {
                    (cell_faults(run))
                } @else {
                    (truth_table(run))
                }
            },
        ),
        Some(Report::Skipped { reason }) => (
            State::Skip,
            reason.clone(),
            html! { p class="text-muted" { (reason) } },
        ),
        Some(Report::Broken { .. }) | None => (
            State::Skip,
            "not run".to_owned(),
            html! { p class="text-muted" { "No test ran on it." } },
        ),
    };
    Stage {
        key: "test",
        name: "Test",
        state,
        summary,
        body,
    }
}

/// Which cases a run tried.
pub(crate) fn tried(run: &Run) -> Markup {
    html! {
        p class="text-meta text-muted" {
            (grouped(run.cases)) " cases of " (run.pattern) ": "
            @if run.exhaustive { "every combination" } @else {
                "every case of each " @if run.by_digit { "hex digit" } @else { "byte" }
                " and " (grouped(run.patterns)) " pairings of test patterns"
            }
            "."
        }
    }
}

/// Every output bit as a cell shaded by how often it was wrong, highest
/// first as a number is written, each bit that did one thing throughout,
/// such as never come on, named under it.
fn bit_strip(run: &Run) -> Markup {
    let counts = bit_counts(run);
    let mut start = 0;
    let outputs: Vec<(&str, &[BitCount])> = run
        .outputs
        .iter()
        .map(|line| {
            let end = (start + line.bits).min(counts.len());
            let bits = &counts[start.min(end)..end];
            start = end;
            (line.name.as_str(), bits)
        })
        .collect();
    let patterned: Vec<(&BitCount, &str)> = counts
        .iter()
        .filter_map(|bit| Some((bit, bit.pattern(run.cases)?)))
        .collect();
    html! {
        h3 class="mt-5 mb-2 font-semibold" { "Wrong bits" }
        div class="flex flex-wrap items-end gap-x-6 gap-y-3" {
            @for (name, bits) in &outputs {
                figure {
                    div class="flex gap-0.5" {
                        @for (i, bit) in bits.iter().enumerate().rev() {
                            @let share = (bit.wrong * 100).checked_div(run.cases).unwrap_or(0);
                            div class="flex flex-col items-center gap-0.5" title={ (bit.name) ": wrong in " (grouped(bit.wrong)) " cases" } {
                                div class={ "size-6 rounded-sm border border-line " (if bit.wrong == 0 { "bg-success/12" } else { "" }) }
                                    style=[(bit.wrong > 0).then(|| format!("background: color-mix(in oklch, var(--color-danger) {}%, transparent)", 25 + share * 3 / 4))] {}
                                span class="text-[10px] leading-none text-muted tabular-nums" { (i) }
                            }
                        }
                    }
                    figcaption class="mt-1 text-meta text-muted" { (name) }
                }
            }
        }
        @if !patterned.is_empty() {
            ul class="mt-3 flex flex-col gap-1" {
                @for (bit, pattern) in &patterned {
                    li { strong { (bit.name) } ": " span class="text-danger" { (pattern) } span class="text-muted" { ", wrong in " (grouped(bit.wrong)) " of " (grouped(run.cases)) " cases" } }
                }
            }
        }
    }
}

/// The first wrong cases as a truth table: a column for each input, then
/// each output expected and got, the bits that differ marked.
fn truth_table(run: &Run) -> Markup {
    let ops = run.examples.iter().any(|wrong| wrong.op.is_some());
    html! {
        h3 class="mt-5 mb-2 font-semibold" { "First wrong cases" }
        div class="relative overflow-x-auto" {
            table class="border-collapse" {
                thead {
                    tr {
                        @if ops { th scope="col" class=(TABLE_HEAD) { "Operation" } }
                        @for line in &run.inputs {
                            th scope="col" class=(TABLE_HEAD) { (line.name) }
                        }
                        @for line in &run.outputs {
                            th scope="col" class={ (TABLE_HEAD) " border-l border-line pl-4" } { (line.name) " expected" }
                            th scope="col" class=(TABLE_HEAD) { "got" }
                        }
                    }
                }
                tbody {
                    @for wrong in &run.examples {
                        tr {
                            @if ops { td class=(TABLE_CELL) { (wrong.op.as_deref().unwrap_or("")) } }
                            @for (line, &value) in run.inputs.iter().zip(&wrong.inputs) {
                                td class=(TABLE_CELL) { (shown(value, value, line.bits)) }
                            }
                            @for ((line, &got), &expected) in run.outputs.iter().zip(&wrong.got).zip(&wrong.expected) {
                                td class={ (TABLE_CELL) " border-l border-line pl-4" } { (shown(expected, expected, line.bits)) }
                                td class=(TABLE_CELL) { (shown(got, expected, line.bits)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// When its outputs settle, measured once its test has run, right or
/// wrong: a warning when they settle on different ticks or glitch, the
/// glitch drawn as it runs.
pub(crate) fn timing_stage(known: &Known) -> Stage {
    let timing = match known.report {
        Some(Report::Ran(run)) => run.timing.as_ref(),
        _ => None,
    };
    // On the details form, measured once the page has loaded, the
    // section it gives taking this one's place.
    if let (None, Some(url), Some(Report::Ran(_))) =
        (timing, known.timing, known.report)
    {
        return Stage {
            key: "timing",
            name: "Timing",
            state: State::Skip,
            summary: "measuring…".to_owned(),
            body: html! {
                div data-timing-pending fx-action=(url) fx-method="post" fx-trigger="load"
                    data-form="details-form" {
                    (ui::waiting("Measuring when each output settles, which takes up to a minute…"))
                }
            },
        };
    }
    let Some(timing) = timing else {
        let why = "Timing is measured once its test has run.";
        return Stage {
            key: "timing",
            name: "Timing",
            state: State::Skip,
            summary: "not measured".to_owned(),
            body: html! { p class="text-muted" { (why) } },
        };
    };
    let latency = timing.latency().map_or_else(
        || "settles".to_owned(),
        |t| format!("settles by tick {t}"),
    );
    let glitching: Vec<&str> = timing
        .outputs
        .iter()
        .filter(|o| o.glitches > 0)
        .map(|o| o.name.as_str())
        .collect();
    let (state, summary) = if timing.synced {
        (State::Pass, format!("{latency}, every bit together"))
    } else if glitching.is_empty() {
        (
            State::Warn,
            format!("{latency}; its bits settle on different ticks"),
        )
    } else {
        (
            State::Warn,
            format!("{latency}; {} glitches", glitching.join(" and ")),
        )
    };
    let body = html! {
        table class="border-collapse" {
            thead { tr {
                th scope="col" class=(TABLE_HEAD) { "Output" }
                th scope="col" class=(TABLE_HEAD) { "Settles by tick" }
                th scope="col" class=(TABLE_HEAD) { "Glitches" }
            } }
            tbody {
                @for output in &timing.outputs {
                    tr {
                        td class=(TABLE_CELL) { (output.name) }
                        td class={ (TABLE_CELL) " tabular-nums" } { (output.latency) }
                        td class={ (TABLE_CELL) " tabular-nums" } {
                            @if output.glitches == 0 { span class="text-muted" { "none" } }
                            @else { strong class="text-danger" { (grouped(output.glitches)) } " of " (grouped(timing.changes)) " changes" }
                        }
                    }
                }
            }
        }
        @if !timing.synced {
            p class="mt-3 text-meta text-muted" {
                "Synced means every bit of an output settles on the same tick, so it can be read on that tick. "
                "A glitch is a bit that changes and changes back on its way there."
            }
        }
        div class="mt-2" { (timing_section(timing, known.trace)) }
    };
    Stage {
        key: "timing",
        name: "Timing",
        state,
        summary,
        body,
    }
}

/// Whether copies of it stack into a wider one: each way they do, or why
/// not, which holds nothing up.
pub(crate) fn stacking_stage(known: &Known) -> Stage {
    let stacking = match known.report {
        Some(Report::Ran(run)) if run.wrong == 0 => run.stacking.as_ref(),
        _ => None,
    };
    let (state, summary) = match stacking {
        None => (State::Skip, "not tried".to_owned()),
        Some(Stacking::Not { reason, .. }) => {
            (State::Skip, format!("doesn't stack: {reason}"))
        }
        Some(found) => {
            let ways: Vec<String> = found
                .directions()
                .iter()
                .map(StackDirection::describe)
                .collect();
            (State::Pass, ways.join(", "))
        }
    };
    Stage {
        key: "stacking",
        name: "Stacking",
        state,
        summary: summary.clone(),
        body: html! {
            p { (summary) }
            p class="mt-1 text-meta text-muted" { a href="#stacking" { "How it stacks" } }
        },
    }
}
