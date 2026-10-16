//! What simulating a build found, as its page, the details page, and the
//! simulation routes show it: the cases, a failure's wrong bits, how copies
//! stack, and a pass's timing.

use maud::{Markup, html};

use crate::simulate::{
    Glitch, InputTiming, OutputTiming, Report, Run, Stacking, Takes, Timing,
};

use super::{SECTION_TITLE, TABLE_CELL, TABLE_HEAD, grouped, spinner};

/// The details form's simulation result, a live panel the form redraws
/// whenever its category or width changes: `content`, the section for the
/// form's test or why there is none, posted for to `simulate`.
pub fn simulation_results(simulate: &str, content: &Markup) -> Markup {
    html! {
        div id="simulation-results" fx-action=(simulate) fx-method="post" fx-trigger="redraw"
            data-form="details-form" {
            (content)
        }
    }
}

/// A build's simulation section while its cases run in the background,
/// as just after it is published: it asks `poll` after it every few seconds,
/// then reloads the page, since whether it fails shows above too.
pub(super) fn cases_pending(poll: &str) -> Markup {
    html! {
        section id="simulation" aria-labelledby="simulation-heading" class="mt-12 max-w-calm" {
            h2 id="simulation-heading" class={ (SECTION_TITLE) " mb-1" } { "Simulation" }
            div fx-action={ (poll) "?have=none" } fx-trigger="poll" data-every="3000"
                fx-target="#simulation" class="mt-3" {
                p class="flex items-center gap-2 text-muted" {
                    (spinner())
                    "Simulating every case, which takes a few seconds…"
                }
                noscript { p class="text-muted" { "Reload the page to see it once it is done." } }
            }
        }
    }
}

/// A simulation result in a few words, such as `all 131,072 cases right`.
pub(super) fn simulation_summary(report: &Report) -> String {
    match report {
        Report::Ran(run) if run.wrong == 0 => {
            format!("all {} cases right.", grouped(run.cases))
        }
        Report::Ran(run) => format!(
            "{} of {} cases wrong.",
            grouped(run.wrong),
            grouped(run.cases)
        ),
        Report::Broken { reason } => format!("broken. {reason}"),
        Report::Skipped { reason } => format!("not simulated. {reason}"),
    }
}

/// A timing in a few words, such as `synced; output settles by tick 3`.
pub(super) fn timing_summary(timing: &Timing) -> String {
    // Such as `output settles by tick 3, carry out by tick 4`.
    let outputs: Vec<String> = timing
        .outputs
        .iter()
        .enumerate()
        .map(|(i, output)| {
            let verb = if i == 0 { " settles" } else { "" };
            format!("{}{verb} by tick {}", output.name, tick(output.latency))
        })
        .collect();
    let glitching: Vec<&str> = timing
        .outputs
        .iter()
        .filter(|output| output.glitches > 0)
        .map(|output| output.name.as_str())
        .collect();
    let synced = if timing.synced {
        "synced".to_owned()
    } else if glitching.is_empty() {
        "not synced".to_owned()
    } else {
        format!("not synced: {} glitches", glitching.join(" and "))
    };
    format!("{synced}; {}.", outputs.join(", "))
}

/// How a section's timing arrives while it is measured.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Waiting<'a> {
    /// On an upload or edit page: posted the details form to this address
    /// once on the page.
    Posted(&'a str),
    /// On a build's page, measured in the background: asked of this
    /// address every few seconds until it is ready.
    Polled(&'a str),
}

/// What simulating a build found: the cases tried and how many came out
/// right, then, for a failure, which output bits were wrong and the first
/// wrong cases, or, for a pass, its timing once measured.
pub fn simulation_section(report: &Report, waiting: Waiting) -> Markup {
    html! {
        section id="simulation" aria-labelledby="simulation-heading" class="mt-12 max-w-calm"
            data-measures-timing[report.passed()]
            data-skipped[matches!(report, Report::Skipped { .. })]
            data-state=(if report.failed() { "fail" } else { "pass" })
            data-summary=(simulation_summary(report)) {
            h2 id="simulation-heading" class={ (SECTION_TITLE) " mb-2" } { "Simulation" }
            @match report {
                Report::Skipped { reason } => p class="text-muted" { "Not simulated: " (reason) },
                Report::Broken { reason } => p class="text-danger" { strong { "Broken: " } (reason) },
                Report::Ran(run) => {
                    // A pass is the build's badge; a failure says how badly
                    // over the cases it got wrong.
                    @if run.wrong > 0 {
                        p class="text-danger" {
                            span aria-hidden="true" { "✗ " }
                            strong { (grouped(run.wrong)) " of " (grouped(run.cases)) " cases wrong" }
                        }
                        p class="text-meta text-muted" {
                            (run.pattern) ", "
                            @if run.exhaustive { "every combination" } @else {
                                "every case of each " @if run.by_digit { "hex digit" } @else { "byte" }
                                " and " (grouped(run.patterns)) " pairings of test patterns"
                            }
                            "."
                        }
                    }
                    @if let Some(stacking) = &run.stacking {
                        (stacking_note(stacking))
                    }
                    @if run.wrong > 0 {
                        (simulation_failures(run))
                    } @else if let Some(timing) = &run.timing {
                        (timing_section(timing))
                    } @else {
                        @let (action, method, trigger, target, form) = match waiting {
                            Waiting::Posted(url) => (url.to_owned(), "post", "load", None, Some("details-form")),
                            Waiting::Polled(url) => (format!("{url}?have=cases"), "get", "poll", Some("#simulation"), None),
                        };
                        div data-timing-pending fx-action=(action) fx-method=(method) fx-trigger=(trigger)
                            fx-target=[target] data-form=[form] data-every="3000" class="mt-6" {
                            p class="flex items-center gap-2 text-muted" {
                                (spinner())
                                "Measuring when each output settles, which takes up to a minute…"
                            }
                            @if let Waiting::Polled(_) = waiting {
                                noscript { p class="text-muted" { "Reload the page to see it once it is done." } }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Whether copies of a passing adder stack into a wider one, or of a
/// register into a bigger register file.
/// What stacking copies found, in a sentence, for the hover on a build's
/// Stacks field.
pub(super) fn stacking_words(stacking: &Stacking) -> String {
    let limit = |most: Option<usize>, copies: usize| match most {
        Some(_) => format!(", at most {copies}"),
        None => String::new(),
    };
    match stacking {
        Stacking::Stacks {
            step, copies, bits, ..
        } => format!(
            "Simulated: {copies} copies {} apart make a {bits}-bit adder.",
            step.describe()
        ),
        Stacking::Registers {
            step,
            copies,
            registers,
            most,
            ..
        } => format!(
            "Simulated: copies {} apart make more registers, {registers} from {copies}{}.",
            step.describe(),
            limit(*most, *copies)
        ),
        Stacking::Wider {
            step,
            copies,
            bits,
            most,
            ..
        } => format!(
            "Simulated: copies {} apart make a wider register, {bits} bits from {copies}{}.",
            step.describe(),
            limit(*most, *copies)
        ),
        Stacking::Not { step, reason } => match step {
            Some(step) => format!(
                "Copies {} apart don't stack as saved: {reason}",
                step.describe()
            ),
            None => format!("Copies don't stack as saved: {reason}"),
        },
    }
}

fn stacking_note(stacking: &Stacking) -> Markup {
    let limit = |copies: usize, most: Option<usize>| match most {
        Some(_) => format!("; a copy more fails, so at most {copies}"),
        None => ", so any number stack".to_owned(),
    };
    html! {
        @match stacking {
            Stacking::Stacks { step, copies, bits, cases } => p class="mt-3" {
                strong { "Stacks: " }
                (copies) " copies " (step.describe()) " apart make a " (bits) "-bit adder, all "
                (grouped(*cases)) " cases right"
                @if *copies >= 3 { ", so any number stack." } @else { "." }
            },
            Stacking::Registers { step, copies, registers, cases, most } => p class="mt-3" data-found-stacking {
                strong { "Stacks: " }
                (copies) " copies " (step.describe()) " apart make " (registers) " registers sharing their data buses, all "
                (grouped(*cases)) " cases right" (limit(*copies, *most)) "."
            },
            Stacking::Wider { step, copies, bits, cases, most } => p class="mt-3" data-found-stacking {
                strong { "Stacks: " }
                (copies) " copies " (step.describe()) " apart make a " (bits) "-bit register sharing its control lines, all "
                (grouped(*cases)) " cases right" (limit(*copies, *most)) "."
            },
            // Why not is on the build's Stacks field, on hover.
            Stacking::Not { .. } => {},
        }
    }
}

/// When a passing build's inputs may change and its outputs settle, from
/// input A's change; and, for an output that does not change cleanly, when
/// each of its bits does.
/// What the timing's ticks count from: input A changing, or for a
/// register, a read lever turning on, with when a write shows.
fn timing_intro(timing: &Timing) -> Markup {
    html! {
        @if timing.from_read {
            p class="mb-3" {
                @match (timing.data, timing.write, timing.takes) {
                    (Some(data), Some(write), Takes::Closing) => {
                        strong { "Latency " (data) " ticks" } ", data in to data out, the data changing as the write ends. "
                        span class="text-muted" { "A write shows " (write) " ticks after it ends." }
                    }
                    (Some(data), Some(write), Takes::Open) => {
                        strong { "Latency " (data) " ticks" } ", data in to data out with the write held open. "
                        span class="text-muted" { "A write shows " (write) " ticks after its button." }
                    }
                    (None, Some(write), _) => {
                        strong { "Latency " (write) " ticks" } ", from a write's button until the bus shows it; "
                        span class="text-muted" { "a cell takes its data only as its write opens." }
                    }
                    _ => {}
                }
            }
            p class="mb-2 text-meta text-muted" { "Reads, in ticks from a read lever turning on:" }
        } @else {
            @let late: Vec<&InputTiming> = timing.inputs.iter().skip(1).filter(|input| input.tick != Some(0)).collect();
            p class="mb-2 text-meta text-muted" {
                "In ticks from A changing."
                @for input in late {
                    " " (input.name)
                    @match input.tick {
                        Some(tick) if tick > 0 => { " may change up to " (tick) " later." }
                        Some(tick) => { " must change " (-tick) " earlier." }
                        None => { " leads to no output." }
                    }
                }
            }
        }
    }
}

pub fn timing_section(timing: &Timing) -> Markup {
    let case: Vec<String> = timing
        .inputs
        .iter()
        .zip(&timing.slowest_case)
        .map(|(input, value)| format!("{} = {value}", input.name))
        .collect();
    html! {
        section aria-labelledby="timing-heading" class="mt-6"
            data-timing data-state=(if timing.synced { "pass" } else { "fail" })
            data-summary=(timing_summary(timing)) {
            h3 id="timing-heading" class="mb-2 font-semibold" { "Timing" }
            (timing_intro(timing))
            div class="overflow-x-auto" {
                table class="w-full border-collapse" {
                    caption class="sr-only" { "When each output settles" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Output" }
                            th scope="col" class=(TABLE_HEAD) { "First changes" }
                            th scope="col" class=(TABLE_HEAD) { "Settles by" }
                            th scope="col" class=(TABLE_HEAD) { "Glitches" }
                        }
                    }
                    tbody {
                        @for output in &timing.outputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } { (output.name) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(output.first_change)) }
                                td class={ (TABLE_CELL) " tabular-nums font-semibold" } { (tick(output.latency)) }
                                td class={ (TABLE_CELL) " tabular-nums" } {
                                    @if output.glitches == 0 {
                                        "none"
                                    } @else {
                                        strong class="text-danger" { "in " (grouped(output.glitches)) " changes" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (carried_notes(&timing.outputs))
            p class="mt-3" {
                @if timing.synced {
                    strong { "Synced: " } "each output changes once, all its bits together."
                } @else {
                    strong class="text-danger" { "Not synced: " } "an output glitches or its bits change apart."
                }
            }
            @for output in &timing.outputs {
                @if let Some(glitch) = &output.glitch {
                    (glitch_example(&output.name, glitch, &timing.inputs))
                }
                @if output.staggered > 0 {
                    p class="mt-2" {
                        "The bits of " (output.name) " change on different ticks in " (grouped(output.staggered))
                        " changes, so it shows a value between the old and the new."
                    }
                }
            }
            p class="mt-2 text-meta text-muted" {
                "Half the changes settle by tick " (timing.median) ", 99% by " (timing.p99)
                "; the slowest, " (case.join(", ")) ", by " (timing.slowest) "."
            }
            @for output in timing.outputs.iter().filter(|o| o.glitches > 0 || o.staggered > 0) {
                (bit_timing(output))
            }
            p class="mt-1 text-meta text-muted" {
                "Over " (grouped(timing.changes)) " changes, each input alone to "
                @if timing.patterns { "each test pattern" } @else { "every value" }
                " and back, and all together; a slower one may hide among those not tried."
            }
        }
    }
}

/// What each output timed short of the repeaters or comparators carrying
/// it out of the build leaves out.
fn carried_notes(outputs: &[OutputTiming]) -> Markup {
    html! {
        @for output in outputs.iter().filter(|o| o.carried > 0) {
            p class="mt-2 text-muted" {
                (output.name) " is timed where it reaches the repeaters or comparators standing on its blocks, which carry it out of the build "
                (output.carried) (if output.carried == 1 { " tick" } else { " ticks" }) " later."
            }
        }
    }
}

/// The first change from all off in which `name` glitched, tick by tick,
/// to try in-game.
fn glitch_example(
    name: &str,
    glitch: &Glitch,
    inputs: &[InputTiming],
) -> Markup {
    let case: Vec<String> = inputs
        .iter()
        .zip(&glitch.case)
        .map(|(input, value)| format!("{} = {value}", input.name))
        .collect();
    let steps: Vec<String> = glitch
        .steps
        .iter()
        .map(|&(at, value)| format!("{value} at tick {}", tick(at)))
        .collect();
    html! {
        p class="mt-2" {
            "For example, from all inputs off to " (case.join(", ")) ", " (name)
            " starts at " (glitch.before) " and becomes " (steps.join(", then "))
            @if glitch.pulse() {
                ": a pulse, where it should stay " (glitch.before) "."
            } @else {
                ": it flips back and forth on its way to its new value."
            }
        }
    }
}

/// When each bit of an output that does not change cleanly first changes
/// and settles, folded away under its name.
fn bit_timing(output: &OutputTiming) -> Markup {
    html! {
        details class="mt-4" {
            summary class="cursor-pointer font-semibold" { (output.name) ", bit by bit" }
            div class="mt-2 overflow-x-auto" {
                table class="w-full border-collapse" {
                    caption class="sr-only" { (output.name) ", bit by bit" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Bit" }
                            th scope="col" class=(TABLE_HEAD) { "First changes" }
                            th scope="col" class=(TABLE_HEAD) { "Settles by" }
                            th scope="col" class=(TABLE_HEAD) { "Changes more than once" }
                        }
                    }
                    tbody {
                        @for (i, bit) in output.bits.iter().enumerate() {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal tabular-nums" } { (i) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(bit.first_change)) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(bit.latency)) }
                                td class={ (TABLE_CELL) " tabular-nums" } {
                                    @if bit.glitches == 0 { "never" } @else { "in " (grouped(bit.glitches)) " changes" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An output's tick counted from A's change, such as `3`; one before A's,
/// though no build has shown one, as `−2`.
fn tick(tick: i32) -> String {
    match tick {
        t if t < 0 => format!("−{}", t.unsigned_abs()),
        t => t.to_string(),
    }
}

/// Which bits a failing build got wrong, and its first wrong cases, a
/// column for each input and output.
fn simulation_failures(run: &Run) -> Markup {
    let ops = run.examples.iter().any(|wrong| wrong.op.is_some());
    html! {
        @if let Some(hint) = &run.hint {
            p class="mt-4" { strong { "Likely cause: " } (hint) }
        }
        h3 class="mt-6 mb-2 font-semibold" { "Wrong bits" }
        ul class="flex flex-col gap-1" {
            @for (bit, count) in &run.wrong_bits {
                li class="tabular-nums" { (bit) ", in " (grouped(*count)) " cases" }
            }
        }
        h3 class="mt-6 mb-2 font-semibold" { "First wrong cases" }
        div class="overflow-x-auto" {
            table class="w-full border-collapse" {
                caption class="sr-only" { "First wrong cases" }
                thead {
                    tr {
                        // A build of several operations, such as an ALU,
                        // says which each case was on.
                        @if ops {
                            th scope="col" class=(TABLE_HEAD) { "Operation" }
                        }
                        @for signal in run.inputs.iter().chain(&run.outputs) {
                            th scope="col" class=(TABLE_HEAD) { (signal.name) }
                        }
                    }
                }
                tbody {
                    @for wrong in &run.examples {
                        tr {
                            @if ops {
                                td class=(TABLE_CELL) { (wrong.op.as_deref().unwrap_or("")) }
                            }
                            @for (signal, &value) in run.inputs.iter().zip(&wrong.inputs) {
                                td class=(TABLE_CELL) { (value_bits(value, value, signal.bits)) }
                            }
                            @for ((signal, &got), &expected) in run.outputs.iter().zip(&wrong.got).zip(&wrong.expected) {
                                td class=(TABLE_CELL) {
                                    @if got == expected {
                                        (value_bits(got, expected, signal.bits))
                                    } @else {
                                        div class="grid grid-cols-[auto_auto] items-baseline gap-x-2" {
                                            span class="text-meta text-muted" { "got" }
                                            (value_bits(got, expected, signal.bits))
                                            span class="text-meta text-muted" { "expected" }
                                            span class="text-muted" { (value_bits(expected, expected, signal.bits)) }
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
}

/// `value` as `bits` binary digits, those that differ from `expected`
/// marked, then in decimal when it has more than one bit.
fn value_bits(value: u64, expected: u64, bits: usize) -> Markup {
    html! {
        span class="whitespace-nowrap" {
            span class="font-mono" { (marked_bits(value, expected, bits)) }
            @if bits > 1 {
                span class="ml-2 text-meta text-muted tabular-nums" { (value) }
            }
        }
    }
}

/// `value` in binary, `width` digits, with each digit that differs from
/// `expected` marked.
fn marked_bits(value: u64, expected: u64, width: usize) -> Markup {
    html! {
        @for bit in (0..width).rev() {
            @let digit = value >> bit & 1;
            @if digit == expected >> bit & 1 {
                (digit)
            } @else {
                mark class="bg-transparent font-semibold text-danger" { (digit) }
            }
        }
    }
}
