//! What simulating a build found, as its page, the details page, and the
//! simulation routes show it: the cases, a failure's wrong bits, how copies
//! stack, and a pass's timing.

use maud::{Markup, html};

use crate::{
    models::schematic::StackDirection,
    schem::Pos,
    simulate::{
        Action, Edge, Glitch, InputTiming, OutputTiming, Report, Run, Stacking,
        Start, Takes, Timing, Value,
    },
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
        Report::Skipped { reason } => format!("untested. {reason}"),
    }
}

/// A timing in a few words, such as `synced; output settles by tick 3`,
/// or for a monostable `a 3-tick low pulse from tick 1`.
pub(super) fn timing_summary(timing: &Timing) -> String {
    if let Some(pulse) = timing.pulse {
        return format!(
            "{}a {}-tick {} pulse from tick {}.",
            if timing.synced { "" } else { "not synced; " },
            pulse.ticks,
            if pulse.inverted { "low" } else { "high" },
            timing.latency().map_or_else(|| "?".to_owned(), tick)
        );
    }
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
    /// Never measured, as for a build off the list, which is only tested
    /// to show what keeps it from being recognized.
    Untimed,
}

/// What simulating a build found: the cases tried and how many came out
/// right, then, for a failure, which output bits were wrong and the first
/// wrong cases, or, for a pass, its timing once measured.
pub fn simulation_section(report: &Report, waiting: Waiting) -> Markup {
    let untimed = matches!(waiting, Waiting::Untimed)
        && matches!(report, Report::Ran(run) if run.wrong == 0 && run.timing.is_none());
    html! {
        section id="simulation" aria-labelledby="simulation-heading" class="mt-12 max-w-calm has-[[data-timing][hidden]]:hidden"
            hidden[untimed]
            data-measures-timing[report.passed()]
            data-skipped[matches!(report, Report::Skipped { .. })]
            data-state=(if report.failed() { "fail" } else { "pass" })
            data-summary=(simulation_summary(report))
            data-stack=[found_stack(report)] {
            // Timing is measured only on a pass, and says it was simulated.
            h2 id="simulation-heading" class={ (SECTION_TITLE) " mb-2" } {
                @if report.passed() { "Timing" } @else { "Simulation" }
            }
            @match report {
                Report::Skipped { reason } => p class="text-muted" { "Untested: " (reason) },
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
                    @if run.wrong > 0 {
                        (simulation_failures(run))
                    } @else if let Some(timing) = &run.timing {
                        (timing_section(timing))
                    } @else if let Some((action, method, trigger, target, form)) = match waiting {
                        Waiting::Posted(url) => Some((url.to_owned(), "post", "load", None, Some("details-form"))),
                        Waiting::Polled(url) => Some((format!("{url}?have=cases"), "get", "poll", Some("#simulation"), None)),
                        Waiting::Untimed => None,
                    } {
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
/// How copies stack, as stacking them found, such as `2 west per copy,
/// any number of copies`, for the details form to show in place of the
/// typed stacking; `None` when no copies were stacked.
fn found_stack(report: &Report) -> Option<String> {
    let Report::Ran(run) = report else {
        return None;
    };
    let stacking = run.stacking.as_ref()?;
    if let Stacking::Not { reason, .. } = stacking {
        return Some(format!("Doesn't stack: {reason}"));
    }
    let ways: Vec<String> = stacking
        .directions()
        .iter()
        .map(StackDirection::describe)
        .collect();
    (!ways.is_empty()).then(|| ways.join(", "))
}

/// When a passing build's inputs may change and its outputs settle, from
/// t = 0, as its slowest inputs change; and, for an output that does not change cleanly, when
/// each of its bits does.
/// What the timing's ticks count from: the inputs changing, or for a
/// register, a read lever turning on, with when a write shows, or for a
/// buffer, a write.
fn timing_intro(timing: &Timing) -> Markup {
    html! {
        @if let Some(pulse) = timing.pulse {
            p class="mb-3" {
                @let latency = timing.latency().unwrap_or(0);
                strong { "A " (pulse.ticks) "-tick pulse" } ", "
                @if pulse.inverted { "low, its output resting on" } @else { "high, its output resting off" }
                ", from " (tick(latency)) " " (if latency == 1 { "tick" } else { "ticks" }) " after " (pulse.edge.words()) ". "
                @match pulse.edge {
                    Edge::Rising => span class="text-muted" { "Turning A off gives none." },
                    Edge::Falling => span class="text-muted" { "Turning A on gives none." },
                    Edge::Both => {},
                }
            }
            p class="mb-2 text-meta text-muted" { "In ticks from " (pulse.edge.turning()) ": the pulse starts at its first change and ends as it settles." }
        } @else if timing.start != Start::Input {
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
            p class="mb-2 text-meta text-muted" {
                @if timing.start == Start::Read {
                    "Reads, in ticks from a read lever turning on:"
                } @else if timing.start == Start::Clock {
                    "In ticks from a pulse of "
                    (timing.inputs.first().map_or("its clock", |input| input.name.as_str()))
                    ":"
                } @else if timing.takes == Takes::Closing {
                    "Writes, in ticks from a write ending:"
                } @else {
                    "Writes, in ticks from a write's button or lever:"
                }
            }

        }
    }
}

/// A synced build's timing, with nothing to say beyond its inputs and
/// outputs, is hidden, and its section with it.
pub fn timing_section(timing: &Timing) -> Markup {
    let quiet =
        timing.synced && timing.pulse.is_none() && timing.start == Start::Input;
    html! {
        div data-timing data-state=(if timing.synced { "pass" } else { "fail" })
            data-summary=(timing_summary(timing)) hidden[quiet]
            data-latency=[timing.latency()] data-synced=(timing.synced) {
            (timing_intro(timing))
            dl class="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1 tabular-nums" {
                dt class="text-muted" { "Synced" }
                dd {
                    @if timing.synced { "✓" } @else { strong class="text-danger" { "✗" } }
                }
                @for output in timing.outputs.iter().filter(|o| o.staggered > 0) {
                    dt class="text-muted" { "Bits apart, " (output.name) }
                    dd class="text-danger" { (grouped(output.staggered)) " changes" }
                }
                dt class="text-muted" { "Changes tested" }
                dd { (grouped(timing.changes)) }
            }
            @for output in &timing.outputs {
                @if let Some(glitch) = &output.glitch {
                    (glitch_example(&output.name, glitch, &timing.inputs))
                }
            }
            @for output in timing.outputs.iter().filter(|o| o.glitches > 0 || o.staggered > 0) {
                (bit_timing(output))
            }
        }
    }
}

/// The first change from all off in which `name` glitched: its value at
/// each tick from t = 0 until it settles, the ticks it is wrong
/// marked, and what it should hold.
fn glitch_example(
    name: &str,
    glitch: &Glitch,
    inputs: &[InputTiming],
) -> Markup {
    let case: Vec<String> = inputs
        .iter()
        .zip(&glitch.case)
        .map(|(input, value)| format!("{}={value}", input.name))
        .collect();
    let first = glitch.steps.first().map_or(0, |&(at, _)| at);
    let last = glitch.steps.last().map_or(0, |&(at, _)| at).max(0);
    let settled = glitch
        .steps
        .last()
        .map_or(glitch.before, |&(_, value)| value);
    let at = |t: i32| {
        glitch
            .steps
            .iter()
            .take_while(|&&(when, _)| when <= t)
            .last()
            .map_or(glitch.before, |&(_, value)| value)
    };
    html! {
        p class="mt-4 font-semibold" { "Glitch: " (name) ", all off → " (case.join(", ")) }
        div class="relative mt-1 overflow-x-auto" {
            table class="border-collapse tabular-nums" {
                caption class="sr-only" { "Glitch: " (name) ", tick by tick" }
                thead {
                    tr {
                        th scope="col" class={ (TABLE_HEAD) " text-left" } { "t" }
                        @for t in 0..=last {
                            th scope="col" class={ (TABLE_HEAD) " text-center" } { (t) }
                        }
                        th scope="col" class={ (TABLE_HEAD) " text-center" } { "expect" }
                    }
                }
                tbody {
                    tr {
                        th scope="row" class={ (TABLE_CELL) " text-left font-normal" } { (name) }
                        @for t in 0..=last {
                            @let value = at(t);
                            // Before its first change it holds its old value,
                            // as it should.
                            @if t < first || value == settled {
                                td class={ (TABLE_CELL) " text-center" } { (value) }
                            } @else {
                                td class={ (TABLE_CELL) " text-center font-semibold text-danger" } { (value) }
                            }
                        }
                        td class={ (TABLE_CELL) " text-center" } { (settled) }
                    }
                }
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
            div class="relative mt-2 overflow-x-auto" {
                table class="w-full border-collapse" {
                    caption class="sr-only" { (output.name) ", bit by bit" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Bit" }
                            th scope="col" class=(TABLE_HEAD) { "First changes" }
                            th scope="col" class=(TABLE_HEAD) { "Settles by" }
                            th scope="col" class=(TABLE_HEAD) { "Glitches" }
                        }
                    }
                    tbody {
                        @for (i, bit) in output.bits.iter().enumerate() {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal tabular-nums" } { (i) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(bit.first_change)) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (tick(bit.latency)) }
                                td class={ (TABLE_CELL) " tabular-nums" } {
                                    @if bit.glitches == 0 { "0" } @else { strong class="text-danger" { (grouped(bit.glitches)) } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An output's tick counted from t = 0, such as `3`; one before it, though
/// no build has shown one, as `−2`.
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
        @if !run.faults.is_empty() {
            (cell_faults(run))
        }
        // Every wrong bit of a wide build is a long list, so it starts
        // folded away; the cases below say more.
        details class="mt-6" {
            summary class="cursor-pointer font-semibold" { "Wrong bits (" (run.wrong_bits.len()) ")" }
            ul class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-meta" {
                @for (bit, count) in &run.wrong_bits {
                    li class="tabular-nums" { (bit) ", in " (grouped(*count)) " cases" }
                }
            }
        }
        h3 class="mt-6 mb-2 font-semibold" { "First wrong cases" }
        // One case to a block, each value on its own line, so a wide build's
        // cases wrap on a phone rather than overflow a table.
        ol class="flex flex-col" {
            @for wrong in &run.examples {
                li class="border-b border-line py-3 first:pt-0" {
                    dl class="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1" {
                        // A build of several operations, such as an ALU,
                        // says which each case was on.
                        @if ops {
                            dt class="text-muted" { "Operation" }
                            dd { (wrong.op.as_deref().unwrap_or("")) }
                        }
                        @for (signal, &value) in run.inputs.iter().zip(&wrong.inputs) {
                            dt class="text-muted" { (signal.name) }
                            dd class="break-all" { (shown(value, value, signal.bits)) }
                        }
                        @for ((signal, &got), &expected) in run.outputs.iter().zip(&wrong.got).zip(&wrong.expected) {
                            @if got == expected {
                                dt class="text-muted" { (signal.name) }
                                dd class="break-all" { (shown(got, expected, signal.bits)) }
                            } @else {
                                dt class="text-muted" { "Expected " (signal.name) }
                                dd class="break-all" { (shown(expected, expected, signal.bits)) }
                                dt class="text-muted" { "Got " (signal.name) }
                                dd class="break-all" { (shown(got, expected, signal.bits)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `value`, a signal of `bits` bits, as a wrong case shows it, the digits
/// that differ from `expected` marked: in binary, with its decimal beside
/// it, up to a byte; wider, in hex, as a wide build's binary would run off
/// a phone's screen.
fn shown(value: Value, expected: Value, bits: usize) -> Markup {
    if bits <= 8 {
        return value_bits(value, expected, bits);
    }
    html! {
        span class="font-mono" { "0x" (marked_hex(value, expected, bits.div_ceil(4))) }
    }
}

/// `value` in hex, `digits` digits, with each digit that differs from
/// `expected` marked.
fn marked_hex(value: Value, expected: Value, digits: usize) -> Markup {
    let digit = |v: Value, i: usize| (v >> (4 * i)) & 0xF;
    html! {
        @for i in (0..digits).rev() {
            @let d = digit(value, i);
            @let text = format!("{d:X}");
            @if d == digit(expected, i) {
                (text)
            } @else {
                mark class="bg-transparent font-semibold text-danger" { (text) }
            }
        }
    }
}

/// A failing register's cells, by the blocks their controls are on, and
/// the steps that show each fault, from the build as saved, each read's
/// wrong bits marked.
fn cell_faults(run: &Run) -> Markup {
    let width = run.inputs.first().map_or(1, |signal| signal.bits);
    let at = |[x, y, z]: Pos| format!("({x}, {y}, {z})");
    let buses = run.outputs.len() > 1;
    html! {
        h3 class="mt-6 mb-2 font-semibold" { "Cells" }
        div class="relative overflow-x-auto" {
            table class="w-full border-collapse" {
                caption class="sr-only" { "Cells" }
                thead {
                    tr {
                        th scope="col" class=(TABLE_HEAD) { "Cell" }
                        th scope="col" class=(TABLE_HEAD) { "Write" }
                        th scope="col" class=(TABLE_HEAD) { "Read" }
                    }
                }
                tbody {
                    @for (i, cell) in run.cells.iter().enumerate() {
                        tr {
                            th scope="row" class={ (TABLE_CELL) " text-left font-normal tabular-nums" } { (i + 1) }
                            td class={ (TABLE_CELL) " tabular-nums" } { (at(cell.write)) }
                            td class={ (TABLE_CELL) " tabular-nums" } { (cell.read.map(at).unwrap_or_default()) }
                        }
                    }
                }
            }
        }
        h3 class="mt-6 mb-2 font-semibold" { "Faults" }
        div class="relative overflow-x-auto" {
            table class="w-full border-collapse" {
                caption class="sr-only" { "Faults" }
                thead {
                    tr {
                        th scope="col" class=(TABLE_HEAD) { "Fault" }
                        th scope="col" class=(TABLE_HEAD) { "Step" }
                        th scope="col" class=(TABLE_HEAD) { "Cell" }
                        th scope="col" class=(TABLE_HEAD) { "Value" }
                        th scope="col" class=(TABLE_HEAD) { "Expected" }
                    }
                }
                tbody {
                    @for (n, steps) in run.faults.iter().enumerate() {
                        @for (s, step) in steps.iter().enumerate() {
                            tr {
                                @if s == 0 {
                                    th scope="rowgroup" rowspan=(steps.len()) class={ (TABLE_CELL) " text-left align-top font-normal tabular-nums" } { (n + 1) }
                                }
                                @match step {
                                    Action::Write { cell, value } => {
                                        td class=(TABLE_CELL) { "write" }
                                        td class={ (TABLE_CELL) " tabular-nums" } { (cell + 1) }
                                        td class=(TABLE_CELL) { (value_bits(*value, *value, width)) }
                                        td class=(TABLE_CELL) {}
                                    }
                                    Action::Read { cell, bus, expected, got } => {
                                        td class=(TABLE_CELL) {
                                            "read"
                                            @if buses {
                                                " " (run.outputs[*bus].name)
                                            }
                                        }
                                        td class={ (TABLE_CELL) " tabular-nums" } { (cell + 1) }
                                        td class=(TABLE_CELL) { (value_bits(*got, *expected, width)) }
                                        td class=(TABLE_CELL) {
                                            @if got != expected {
                                                span class="text-muted" { (value_bits(*expected, *expected, width)) }
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
}

/// `value` as `bits` binary digits after `0b`, those that differ from `expected`
/// marked, then in decimal when it has more than one bit.
fn value_bits(value: Value, expected: Value, bits: usize) -> Markup {
    html! {
        span class="whitespace-nowrap" {
            span class="font-mono" {
                // Marked as binary beside the hex of a wider value; a bit
                // alone is plain.
                @if bits > 1 { "0b" }
                (marked_bits(value, expected, bits))
            }
            @if bits > 1 {
                span class="ml-2 text-meta text-muted tabular-nums" { (value) }
            }
        }
    }
}

/// `value` in binary, `width` digits, with each digit that differs from
/// `expected` marked.
fn marked_bits(value: Value, expected: Value, width: usize) -> Markup {
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
