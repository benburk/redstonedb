//! A build's checks as stages run one after another, as a CI pipeline's
//! are: its file read, its inputs and outputs found, its test, its timing,
//! and its stacking, each passing, warning, failing, or not run, and each
//! opening onto what it found, shown as tables of its lines, bits, and
//! cases rather than said in sentences. The first stage that fails is open;
//! the ones after it, which need it, say they did not run. Its file and its
//! lines are drawn here, and the stages after them in `checks_test_view`.

use maud::{Markup, html};

use crate::{
    simulate::{Report, Run},
    web::{
        build::{
            checks::{Found, Known, State, needs, problems},
            checks_test_view::{stacking_stage, test_stage, timing_stage},
            simulation_view,
        },
        ui::{
            SECTION_TITLE, TABLE_CELL, TABLE_HEAD,
            text::{dimensions, grouped},
        },
    },
};

/// One stage: its name, how it came out, what it found in a few words, and
/// what it found in full.
pub(crate) struct Stage {
    pub(crate) key: &'static str,
    pub(crate) name: &'static str,
    pub(crate) state: State,
    pub(crate) summary: String,
    pub(crate) body: Markup,
}

/// The details form's checks, the same stages as a build's page shows,
/// in the section `form.js` reads: whether its cases passed, failed, or
/// couldn't run, what they came to in a few words, and how copies stack,
/// which fill the form's measured fields.
pub fn form_checks(known: &Known) -> Markup {
    let report = known.report;
    html! {
        section id="simulation" class="max-w-calm"
            data-state=(if report.is_some_and(Report::failed) { "fail" } else { "pass" })
            data-stack=[report.and_then(simulation_view::found_stack)]
            data-measures-timing[report.is_some_and(Report::passed)]
            data-skipped[matches!(report, Some(Report::Skipped { .. }) | None)] {
            (checks_section(known))
        }
    }
}

/// Build `known.s`'s checks, a row of stages, each a button opening what
/// it found below; every stage that fails or warns open.
pub fn checks_section(known: &Known) -> Markup {
    let stages = stages(known);
    html! {
        section id="checks" aria-labelledby="checks-heading" class="mt-12 scroll-mt-4" {
            h2 id="checks-heading" class={ (SECTION_TITLE) " mb-3" } { "Checks" }
            ol class="flex flex-wrap items-center gap-x-1 gap-y-2" aria-label="Stages" {
                @for (i, stage) in stages.iter().enumerate() {
                    li class="flex items-center gap-1" {
                        @if i > 0 {
                            span aria-hidden="true" class="text-faint" { "→" }
                        }
                        a href={ "#check-" (stage.key) } data-stage=(stage.key) data-state=(stage.state.key())
                            class={ "inline-flex items-center gap-1.5 rounded-full px-3 text-meta leading-7 font-semibold hover:no-underline " (stage.state.tone()) } {
                            span aria-hidden="true" { (stage.state.mark()) }
                            (stage.name)
                        }
                    }
                }
            }
            div class="mt-4 flex flex-col divide-y divide-line rounded-overlay border border-line bg-raised" {
                @for stage in &stages {
                    details id={ "check-" (stage.key) } open[matches!(stage.state, State::Fail | State::Warn)] class="group scroll-mt-4" {
                        summary class="flex cursor-pointer items-baseline gap-3 px-4 py-3" {
                            span aria-hidden="true" class={ "inline-flex size-6 shrink-0 items-center justify-center rounded-full text-meta font-semibold " (stage.state.tone()) } {
                                (stage.state.mark())
                            }
                            span class="font-semibold" { (stage.name) }
                            span class="min-w-0 flex-1 truncate text-muted" { (stage.summary) }
                        }
                        div class="px-4 pb-4" { (stage.body) }
                    }
                }
            }
        }
    }
}

/// Every stage, in order, each not run when one it needs failed.
pub(crate) fn stages(known: &Known) -> Vec<Stage> {
    let mut stages = vec![file_stage(known), lines_stage(known)];
    stages.push(test_stage(known));
    stages.push(timing_stage(known));
    stages.push(stacking_stage(known));
    for i in 0..stages.len() {
        let failed = needs(stages[i].key).iter().find_map(|need| {
            stages
                .iter()
                .find(|s| s.key == *need && s.state == State::Fail)
                .map(|s| s.name.to_lowercase())
        });
        if let Some(name) = failed {
            let stage = &mut stages[i];
            stage.state = State::Skip;
            stage.summary = format!("not run: {name} failed");
            stage.body = html! { p class="text-muted" { "Runs once " (name) " passes." } };
        }
    }
    stages
}

/// Reading its file: its blocks and their box, and for an upload, what
/// uploading it takes out or swaps, a warning so its uploader sees the
/// build as it will be kept.
fn file_stage(known: &Known) -> Stage {
    match known.file {
        Ok(file) => {
            let blocks: usize = file.block_counts.iter().map(|(_, n)| n).sum();
            let size = format!(
                "{} blocks, {}",
                grouped(blocks as u64),
                dimensions(known.s.size)
            );
            let (state, summary) = match file.kept.len() {
                0 => (State::Pass, size),
                1 => (State::Warn, format!("{size}; 1 change on upload")),
                n => (State::Warn, format!("{size}; {n} changes on upload")),
            };
            Stage {
                key: "file",
                name: "File",
                state,
                summary,
                body: html! {
                    @if !file.kept.is_empty() {
                        ul class="mb-3 flex list-disc flex-col gap-1 pl-5" {
                            @for change in &file.kept { li { (change) } }
                        }
                    }
                    table class="border-collapse" {
                        tbody {
                            @for (block, n) in file.block_counts.iter().take(8) {
                                tr {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal" } { (block) }
                                    td class={ (TABLE_CELL) " text-right tabular-nums" } { (grouped(*n as u64)) }
                                }
                            }
                        }
                    }
                },
            }
        }
        Err(err) => Stage {
            key: "file",
            name: "File",
            state: State::Fail,
            summary: "couldn't read it".to_owned(),
            body: html! { pre class="whitespace-pre-wrap text-danger" { (err) } },
        },
    }
}

/// Finding its inputs and outputs: each line, its bits, and whether it
/// reads inverted, as its test read them; what keeps them from fitting its
/// category, if anything; and what bends the building conventions, each
/// kind once with every place it is.
fn lines_stage(known: &Known) -> Stage {
    let problems = problems(known);
    // What keeps a working build off the leaderboards is its lines' doing.
    let unranked = known.s.unranked.clone();
    let (state, summary) = match known.report {
        Some(Report::Broken { reason }) => (State::Fail, reason.clone()),
        Some(Report::Ran(run)) => {
            let ins: Vec<String> =
                run.inputs.iter().map(|l| l.bits.to_string()).collect();
            let outs: Vec<String> =
                run.outputs.iter().map(|l| l.bits.to_string()).collect();
            let found =
                format!("{} in → {} out", ins.join(" + "), outs.join(" + "));
            // Unaligned buses are said whether it works or not, as what
            // its lines are; unranked only of one that works.
            match (&unranked, known.s.unaligned, problems.is_empty()) {
                (Some(why), _, _) => {
                    (State::Warn, format!("{found}; unranked: {why}"))
                }
                (None, true, _) => {
                    (State::Warn, format!("{found}; unaligned buses"))
                }
                (None, false, false) => {
                    (State::Warn, format!("{found}; bends a convention"))
                }
                (None, false, true) => (State::Pass, found),
            }
        }
        // Not tested, its lines say why only when they are what failed.
        _ if !problems.is_empty() => (
            State::Fail,
            match problems.len() {
                1 => "1 kind of problem".to_owned(),
                n => format!("{n} kinds of problem"),
            },
        ),
        _ => match known.s.unverified {
            Some(why) => (State::Fail, why.to_owned()),
            None => (State::Skip, "not read".to_owned()),
        },
    };
    let lines = match known.report {
        Some(Report::Ran(run)) => Some(run),
        _ => None,
    };
    let found = known
        .file
        .as_ref()
        .ok()
        .map(|file| &file.analysis.interface);
    let body = html! {
        @if let Some(Report::Broken { reason }) = known.report {
            p class="mb-3 text-danger" { (reason) }
        }
        // Untested, what the last check stored is all there is to say
        // why, as for a build that never comes to rest.
        @if let (None, Some(why)) = (known.report, known.s.unverified) {
            p class="mb-3 text-danger" { (why) }
        }
        @if let Some(run) = lines {
            (lines_table(run))
        } @else if let Some(found) = found.filter(|f| !f.inputs.is_empty() || !f.outputs.is_empty()) {
            p class="mb-2 text-meta text-muted" { "Lines found in its file:" }
            table class="border-collapse" {
                thead { tr {
                    th scope="col" class=(TABLE_HEAD) { "Line" }
                    th scope="col" class=(TABLE_HEAD) { "Bits" }
                } }
                tbody {
                    @for (way, port) in found.inputs.iter().map(|p| ("in", p)).chain(found.outputs.iter().map(|p| ("out", p))) {
                        tr {
                            td class=(TABLE_CELL) { (way) " · " (port.name) }
                            td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                        }
                    }
                }
            }
        }
        @if let Some(why) = &unranked {
            p class="mt-3 rounded-md bg-restriction px-3 py-2 text-on-restriction" {
                strong { "Unranked: " (why) ". " }
                @if known.s.unaligned {
                    "Its buses aren't evenly spaced straight lines. "
                } @else if !known.s.conventional {
                    "It is kept as built, its lines read loosely. "
                }
                "It works and is listed, but ranks on no leaderboard."
            }
        } @else if known.s.unaligned {
            p class="mt-3 rounded-md bg-restriction px-3 py-2 text-on-restriction" {
                strong { "Unaligned buses. " }
                "Its buses aren't evenly spaced straight lines, so once it works it lists, but ranks on no leaderboard."
            }
        }
        @if !problems.is_empty() {
            (problems_table(&problems))
        }
    };
    Stage {
        key: "lines",
        name: "Inputs & outputs",
        state,
        summary,
        body,
    }
}

/// Each line its test drove or read, its bits, and whether it reads
/// inverted.
fn lines_table(run: &Run) -> Markup {
    html! {
        table class="border-collapse" {
            thead { tr {
                th scope="col" class=(TABLE_HEAD) { "" }
                th scope="col" class=(TABLE_HEAD) { "Line" }
                th scope="col" class=(TABLE_HEAD) { "Bits" }
                th scope="col" class=(TABLE_HEAD) { "Reads" }
            } }
            tbody {
                @for (way, line) in run.inputs.iter().map(|l| ("in", l)).chain(run.outputs.iter().map(|l| ("out", l))) {
                    tr {
                        td class={ (TABLE_CELL) " text-muted" } { (way) }
                        td class=(TABLE_CELL) { (line.name) }
                        td class={ (TABLE_CELL) " tabular-nums" } { (line.bits) }
                        td class=(TABLE_CELL) {
                            @if run.inverted.contains(&line.name) { "inverted" } @else { span class="text-muted" { "as is" } }
                        }
                    }
                }
            }
        }
    }
}

/// Each kind of problem, how to fix it, and where it is.
fn problems_table(problems: &[Found]) -> Markup {
    html! {
        h3 class="mt-4 mb-2 font-semibold" { "Problems" }
        ul class="flex flex-col gap-3" {
            @for problem in problems {
                li {
                    span class="block" { (problem.kind) }
                    @if let Some(advice) = problem.rule.advice() {
                        span class="block text-meta text-muted" {
                            "Fix: " (advice.fix)
                            @if let Some(section) = advice.section {
                                " " a href={ "/about/recognized#" (section) } { "See the rule" }
                            }
                        }
                    }
                    @if !problem.places.is_empty() {
                        p class="mt-1 font-mono text-meta text-muted" {
                            (problem.places.len()) " at: " (problem.places.join(", "))
                        }
                    }
                }
            }
        }
    }
}
