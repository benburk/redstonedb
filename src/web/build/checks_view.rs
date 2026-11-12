//! A build's checks as stages run one after another, as a CI pipeline's
//! are: its file read, its inputs and outputs found, its test, its timing,
//! and its stacking, each passing, warning, failing, or not run, and each
//! opening onto what it found, shown as tables of its lines, bits, and
//! cases rather than said in sentences. The first stage that fails is open;
//! the ones after it, which need it, say they did not run.

use std::collections::BTreeMap;

use maud::{Markup, html};

use crate::{
    build::Build,
    recognize::fault,
    simulate::{BitCount, Report, Run, Stacking},
    stack::StackDirection,
    web::{
        build::{
            detail_view::FileRead,
            simulation_view::{self, cell_faults, shown, timing_section},
        },
        ui::{
            self, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, Tone, text::grouped,
        },
    },
};

/// How a stage came out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Pass,
    /// Passed, with something worth a look that holds nothing up.
    Warn,
    Fail,
    /// Not run, as a stage before it failed or there is nothing to run.
    Skip,
}

impl State {
    fn key(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Warn => "warn",
            State::Fail => "fail",
            State::Skip => "skip",
        }
    }

    fn mark(self) -> &'static str {
        match self {
            State::Pass => "✓",
            State::Warn => "!",
            State::Fail => "✗",
            State::Skip => "–",
        }
    }

    fn tone(self) -> Tone {
        match self {
            State::Pass => Tone::Good,
            State::Warn => Tone::Warn,
            State::Fail => Tone::Bad,
            State::Skip => Tone::Plain,
        }
    }
}

/// One stage: its name, how it came out, what it found in a few words, and
/// what it found in full.
struct Stage {
    key: &'static str,
    name: &'static str,
    state: State,
    summary: String,
    body: Markup,
}

/// What its checks say of the build, stored or being uploaded: its box,
/// its category, why it ranks on no board though it works, whether its
/// buses are unaligned, whether it keeps to the conventions, and the
/// reason stored against it, if any.
pub struct Subject<'a> {
    pub size: (i64, i64, i64),
    pub category: &'a str,
    pub unranked: Option<String>,
    pub unaligned: bool,
    pub conventional: bool,
    pub unverified: Option<&'a str>,
}

impl<'a> Subject<'a> {
    /// Stored build `s`.
    pub fn of(s: &'a Build) -> Subject<'a> {
        Subject {
            size: s.file.size,
            category: &s.category,
            unranked: s.unranked(),
            unaligned: s.unaligned,
            conventional: s.file.conventional,
            unverified: s.unverified.as_deref(),
        }
    }
}

/// What a page knows of a build, which its stages are worked out from.
pub struct Known<'a> {
    pub s: Subject<'a>,
    pub file: &'a Result<FileRead, String>,
    pub report: Option<&'a Report>,
    /// Where its traced glitch is asked for, under `/glitch`; `None` for a
    /// build not yet stored.
    pub trace: Option<&'a str>,
    /// Where the details form asks for its timing, when it is measured
    /// once the page has loaded rather than stored.
    pub timing: Option<&'a str>,
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
fn stages(known: &Known) -> Vec<Stage> {
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

/// The stages the one keyed `key` reads from: the test and timing its
/// lines, and stacking a test that passes. Timing needs no right
/// answers, so it runs on a build that gets cases wrong.
fn needs(key: &str) -> &'static [&'static str] {
    match key {
        "lines" => &["file"],
        "test" | "timing" => &["file", "lines"],
        "stacking" => &["file", "lines", "test"],
        _ => &[],
    }
}

/// Reading its file: its blocks and their box, and for an upload, what
/// uploading it takes out or swaps, a warning so its uploader sees the
/// build as it will be kept.
fn file_stage(known: &Known) -> Stage {
    let (x, y, z) = known.s.size;
    match known.file {
        Ok(file) => {
            let blocks: usize = file.block_counts.iter().map(|(_, n)| n).sum();
            let size =
                format!("{} blocks, {y}×{x}×{z}", grouped(blocks as u64));
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

/// What the check by function finds wrong with its lines, less the cases
/// its test got wrong, which the test shows: each kind of problem once,
/// with every place it is, as `Lever at (5, 2, 3): touches repeater at (4,
/// 2, 3)` five times over is one problem at five places.
fn problems(known: &Known) -> Vec<Problem> {
    let Ok(file) = known.file else {
        return Vec::new();
    };
    // Only a kind that takes digits may take a signal strength.
    let strength = file.strength_inputs
        && !crate::category::Kind::named(known.s.category)
            .is_some_and(crate::category::Kind::digit_inputs);
    let strength =
        strength.then(|| crate::recognize::fault::WITHOUT_LEVERS.to_owned());
    let mut kinds: BTreeMap<String, Problem> = BTreeMap::new();
    for said in strength.iter().chain(&file.unrecognized) {
        if said.is_empty() || fault::wrong_outputs(said) {
            continue;
        }
        let (kind, places) = placeless(said);
        let problem = kinds.entry(kind.clone()).or_insert_with(|| Problem {
            kind,
            first: said.clone(),
            places: Vec::new(),
        });
        if !places.is_empty() {
            problem.places.push(places);
        }
    }
    kinds.into_values().collect()
}

/// One kind of problem: as said with its places taken out, as first said,
/// and each place it is.
struct Problem {
    kind: String,
    first: String,
    places: Vec<String>,
}

/// `problem` with its places taken out, as the kind of problem it is, and
/// its places: `Lever at (5, 2, 3): touches repeater at (4, 2, 3)` is
/// `Lever at …: touches repeater at …` at `(5, 2, 3) → (4, 2, 3)`.
fn placeless(problem: &str) -> (String, String) {
    let mut kind = String::new();
    let mut places = Vec::new();
    let mut rest = problem;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        if inner.split(',').all(|n| n.trim().parse::<i32>().is_ok())
            && inner.contains(',')
        {
            kind.push_str(&rest[..open]);
            kind.push('…');
            places.push(&rest[open..=open + close]);
        } else {
            kind.push_str(&rest[..=open + close]);
        }
        rest = &rest[open + close + 1..];
    }
    kind.push_str(rest);
    (kind, places.join(" → "))
}

/// Each kind of problem, how to fix it, and where it is.
fn problems_table(problems: &[Problem]) -> Markup {
    html! {
        h3 class="mt-4 mb-2 font-semibold" { "Problems" }
        ul class="flex flex-col gap-3" {
            @for problem in problems {
                li {
                    span class="block" { (problem.kind) }
                    @if let Some(advice) = fault::advice(&problem.first) {
                        span class="block text-meta text-muted" {
                            "Fix: " (advice.fix)
                            @if let Some(rule) = advice.rule {
                                " " a href={ "/about/recognized#" (rule) } { "See the rule" }
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

/// Its test: every case it tried and how many came out right; for a
/// failure, how often each output bit was wrong, and the first wrong cases
/// as a truth table.
fn test_stage(known: &Known) -> Stage {
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
fn tried(run: &Run) -> Markup {
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

/// Each result bit's counts, or for a test that doesn't count them, each
/// bit with how often it was wrong alone.
fn bit_counts(run: &Run) -> Vec<BitCount> {
    if !run.bit_counts.is_empty() {
        return run.bit_counts.clone();
    }
    run.outputs
        .iter()
        .flat_map(|line| {
            (0..line.bits).map(move |bit| match line.bits {
                1 => line.name.clone(),
                _ => format!("{} bit {bit}", line.name),
            })
        })
        .map(|name| {
            let wrong = run
                .wrong_bits
                .iter()
                .find(|(bit, _)| *bit == name)
                .map_or(0, |(_, n)| *n);
            // Neither count is known, so no pattern is claimed.
            BitCount {
                name,
                wrong,
                got_on: 1,
                expected_on: 1,
            }
        })
        .collect()
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
fn timing_stage(known: &Known) -> Stage {
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
fn stacking_stage(known: &Known) -> Stage {
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
