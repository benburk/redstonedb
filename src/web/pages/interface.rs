//! A build's inputs and outputs as its file marks them, and the building
//! conventions it breaks, shown on its page and on the details page.

use maud::{Markup, html};

use crate::{
    import::Analysis,
    interface::{Bit, Interface, Port},
    lint::{self, Violation},
    simulate::{InputTiming, OutputTiming, Timing},
};

use super::{SECTION_TITLE, TABLE_CELL, TABLE_HEAD, build::hover_note};

/// The lines a flag says are read inverted.
pub(super) enum Inverts {
    /// The input of this colour.
    Input(&'static str),
    /// The output of this colour.
    Output(&'static str),
    /// Every output but the one of this colour.
    OutputsBut(&'static str),
    /// Every output.
    Outputs,
}

/// Which lines `flag` on a build of `category` says are read inverted,
/// if it says any are.
pub(super) fn inverts(category: &str, flag: &str) -> Option<Inverts> {
    Some(match (category, flag) {
        ("adder", "subtract") => Inverts::Input("light blue"),
        ("adder", "inverted_carry_in") => Inverts::Input(lint::CONTROL),
        ("adder", "inverted_output") => Inverts::OutputsBut(lint::STATUS),
        ("adder", "inverted_carry_out") => Inverts::Output(lint::STATUS),
        (
            "register"
            | "buffer"
            | "decoder"
            | "monostable"
            | "dual_edge_monostable",
            "inverted",
        ) => Inverts::Outputs,
        _ => return None,
    })
}

/// The flag, of the build's `flags`, that says `port` is read inverted.
fn inverted_by<'f>(
    category: &str,
    flags: &'f [String],
    port: &Port,
    input: bool,
) -> Option<&'f str> {
    let coloured = |colour: &str| port.block.starts_with(colour);
    flags.iter().map(String::as_str).find(|flag| {
        match inverts(category, flag) {
            Some(Inverts::Input(colour)) => input && coloured(colour),
            Some(Inverts::Output(colour)) => !input && coloured(colour),
            Some(Inverts::OutputsBut(colour)) => !input && !coloured(colour),
            Some(Inverts::Outputs) => !input,
            None => false,
        }
    })
}

/// The build's inputs and outputs, read from the file, each a row: a bus
/// by its bits, and a row of numbered lines, such as a memory's `read 0`
/// to `read 15`, as one. With its `timing`, each input says when it
/// switches for the build to stay synced and each output when it changes
/// and settles, in ticks from the first input, t = 0. When its `flags`
/// say a line of its `category` is read inverted, each line says whether
/// it is.
pub(super) fn interface_section(
    analysis: &Analysis,
    timing: Option<&Timing>,
    (category, flags): (&str, &[String]),
) -> Markup {
    let Interface { inputs, outputs } = &analysis.interface;
    let inverted = |entry: &PortEntry, input: bool| {
        inverted_by(category, flags, entry.ports[0], input)
    };
    let any_inverted = port_entries(inputs)
        .iter()
        .any(|e| inverted(e, true).is_some())
        || port_entries(outputs)
            .iter()
            .any(|e| inverted(e, false).is_some());
    let timed_in = |entry: &PortEntry| {
        let timing = timing?;
        let i = timing
            .inputs
            .iter()
            .position(|input| input.name == entry.ports[0].name)?;
        Some((i, &timing.inputs[i]))
    };
    let timed_out = |entry: &PortEntry| {
        timing?
            .outputs
            .iter()
            .find(|output| output.name == entry.ports[0].name)
    };
    // One table, inputs then outputs, so their columns line up: each
    // line's name, its bits, whether it is inverted, then its power and
    // delay or its first change and settling.
    html! {
        section aria-labelledby="interface-heading" class="flex flex-col gap-6" {
            h2 id="interface-heading" class=(SECTION_TITLE) { "Inputs and outputs" }
            div class="relative max-desktop:overflow-x-auto" {
                table class="w-full border-collapse tabular-nums" {
                    tbody {
                        tr {
                            (head("in-name", "Input", None))
                            (head("in-bits", "Bits", Some(BITS_NOTE)))
                            @if any_inverted { (head("in-inverted", "Inverted", Some(INVERTED_NOTE))) }
                            (head("in-power", "Power", Some(POWER_NOTE)))
                            @if timing.is_some() { (head("in-delay", "Delay", Some(DELAY_NOTE))) }
                        }
                        @for entry in port_entries(inputs) {
                            tr {
                                (line_cell(&entry, true))
                                td class=(TABLE_CELL) { (bits(&entry)) }
                                @if any_inverted { (inverted_cell(inverted(&entry, true))) }
                                td class=(TABLE_CELL) { (power(&entry)) }
                                @if timing.is_some() {
                                    td class=(TABLE_CELL) {
                                        @match timed_in(&entry) {
                                            Some((i, input)) => (switch_by(i, input)),
                                            None => span class="text-muted" { "—" },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    tbody {
                        tr aria-hidden="true" { td class="h-6" {} }
                        tr {
                            (head("out-name", "Output", None))
                            (head("out-bits", "Bits", Some(BITS_NOTE)))
                            @if any_inverted { (head("out-inverted", "Inverted", Some(INVERTED_NOTE))) }
                            @if timing.is_some() {
                                (head("out-first", "First change", Some(FIRST_CHANGE_NOTE)))
                                (head("out-settles", "Settles", Some(SETTLES_NOTE)))
                            }
                        }
                        @for entry in port_entries(outputs) {
                            tr {
                                (line_cell(&entry, false))
                                td class=(TABLE_CELL) { (bits(&entry)) }
                                @if any_inverted { (inverted_cell(inverted(&entry, false))) }
                                @if timing.is_some() {
                                    @match timed_out(&entry) {
                                        Some(output) => (output_ticks(output)),
                                        None => {
                                            td class={ (TABLE_CELL) " text-muted" } { "—" }
                                            td class={ (TABLE_CELL) " text-muted" } { "—" }
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

const BITS_NOTE: &str = "Bits on the bus, or lines in a numbered row";
const INVERTED_NOTE: &str = "Yes: powered reads as 0";
const POWER_NOTE: &str = "Hard: dust touches the input block, so a repeater, comparator, \
    or torch must drive it. Soft: dust may drive it.";
const DELAY_NOTE: &str =
    "Tick to switch it, from the first input, for the outputs to stay synced";
const FIRST_CHANGE_NOTE: &str = "Ticks from t = 0 until a bit first changes";
const SETTLES_NOTE: &str =
    "Ticks from t = 0 until every bit holds its final value";

/// A column's heading, its `note` shown on hover or focus.
fn head(id: &str, label: &str, note: Option<&str>) -> Markup {
    html! {
        th scope="col" class=(TABLE_HEAD) {
            @match note {
                Some(note) => (hover_note(id, &html! {
                    span class="underline decoration-boundary decoration-dotted underline-offset-4" { (label) }
                }, note)),
                None => (label),
            }
        }
    }
}

/// A line's name beside a swatch of its marking colour, with what else it
/// takes, such as a strength or being inverted, beneath.
fn line_cell(entry: &PortEntry, input: bool) -> Markup {
    let port = entry.ports[0];
    let dye = port
        .block
        .strip_suffix(" concrete")
        .unwrap_or(&port.block)
        .replace(' ', "_");
    let [r, g, b, _] = blockrender::dye_color(&dye);
    let notes = notes(entry, input);
    html! {
        th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
            span class="flex items-start gap-2" {
                span aria-hidden="true" class="mt-1.5 inline-block size-3 shrink-0 rounded-[3px] border border-line"
                    style={ "background: rgb(" (r) " " (g) " " (b) ")" } {}
                span {
                    span class="font-semibold" { (entry.name) }
                    @if !notes.is_empty() {
                        span class="block text-meta text-muted" { (notes) }
                    }
                }
            }
        }
    }
}

/// Whether a line is read inverted, by the `flag` that says so, which
/// finds the builds that share it.
fn inverted_cell(flag: Option<&str>) -> Markup {
    html! {
        @match flag {
            Some(flag) => td class=(TABLE_CELL) {
                a href=(super::search_href(flag)) title="Find builds that share it" { "Yes" }
            },
            None => td class={ (TABLE_CELL) " text-muted" } { "No" },
        }
    }
}

/// How many bits or lines an entry carries.
fn bits(entry: &PortEntry) -> String {
    let port = entry.ports[0];
    if entry.ports.len() > 1 {
        format!("{} lines", entry.ports.len())
    } else if port.signal_strength {
        format!("{} (hex)", port.bits.len() * 4)
    } else {
        port.bits.len().to_string()
    }
}

/// What powers an input: hard, when dust touches its block, so only a
/// repeater, comparator, or torch drives it; soft, when dust may.
fn power(entry: &PortEntry) -> &'static str {
    let bits: Vec<&Bit> =
        entry.ports.iter().flat_map(|p| p.bits.iter()).collect();
    if entry.ports[0].signal_strength {
        "strength"
    } else if bits.iter().all(|bit| bit.hard) {
        "hard"
    } else if bits.iter().any(|bit| bit.hard) {
        "mixed"
    } else {
        "soft"
    }
}

/// When an input, the `i`th timed, switches so its change lands with the
/// first's: the first at t = 0, another at its offset from it.
fn switch_by(i: usize, input: &InputTiming) -> String {
    match (i, input.tick) {
        (0, _) => "t = 0".to_owned(),
        (_, Some(tick)) => ticks(tick),
        (_, None) => "no effect".to_owned(),
    }
}

/// Ticks from t = 0, signed, such as `+3t`, `−1t`, or `0t`.
fn ticks(t: i32) -> String {
    match t {
        0 => "0t".to_owned(),
        t if t < 0 => format!("−{}t", t.unsigned_abs()),
        t => format!("+{t}t"),
    }
}

/// An output's first change and settling tick, as cells, the settling
/// noting on hover the ticks repeaters on its blocks take to carry it out.
fn output_ticks(output: &OutputTiming) -> Markup {
    html! {
        td class=(TABLE_CELL) { (output.first_change) "t" }
        td class={ (TABLE_CELL) " font-semibold" } {
            @if output.carried > 0 {
                (hover_note(
                    &format!("carried-{}", output.name.replace(' ', "-")),
                    &html! {
                        span class="underline decoration-boundary decoration-dotted underline-offset-4" { (output.latency) "t" }
                    },
                    &format!(
                        "{}t out of the build: +{}t through the repeater on its output block",
                        output.latency + output.carried,
                        output.carried,
                    ),
                ))
            } @else {
                (output.latency) "t"
            }
        }
    }
}

/// The building conventions the build's file breaks, if any.
pub(super) fn violations_section(analysis: &Analysis) -> Markup {
    html! {
        @if !analysis.violations.is_empty() {
            section aria-labelledby="lint-heading" class="mt-12 max-w-calm" {
                h2 id="lint-heading" class={ (SECTION_TITLE) " mb-3" } { "Breaks the building conventions" }
                (violation_list(&analysis.violations))
            }
        }
    }
}

/// One entry in a list of inputs or outputs: a port, or numbered lines of
/// one bit each that share a word and a colour, such as `read 0` to
/// `read 15`.
struct PortEntry<'a> {
    name: String,
    ports: Vec<&'a Port>,
    /// Names more than one line is signed with, such as `read 5`, as a
    /// mislabelled sign leaves.
    repeated: Vec<String>,
}

/// Sorted, distinct `numbers` as runs, such as `0–14` or `0–3, 6`.
fn ranges(numbers: &[u32]) -> String {
    let mut runs: Vec<String> = Vec::new();
    let mut start = 0;
    for i in 1..=numbers.len() {
        if i == numbers.len() || numbers[i] != numbers[i - 1] + 1 {
            let (first, last) = (numbers[start], numbers[i - 1]);
            runs.push(if first == last {
                first.to_string()
            } else {
                format!("{first}–{last}")
            });
            start = i;
        }
    }
    runs.join(", ")
}

/// Ports, by the word and colour numbered lines share; one apiece for any
/// other port.
type PortGroup<'p> = (Option<(String, String)>, Vec<&'p Port>);

/// `ports` as entries, in order of first appearance, numbered lines
/// gathered under their shared word.
fn port_entries(ports: &[Port]) -> Vec<PortEntry<'_>> {
    let numbered = |port: &Port| {
        let (word, number) = port.name.rsplit_once(' ')?;
        let number: u32 = number.parse().ok()?;
        (port.bits.len() == 1).then(|| (word.to_owned(), number))
    };
    let mut entries: Vec<PortGroup> = Vec::new();
    for port in ports {
        let key = numbered(port).map(|(word, _)| (word, port.block.clone()));
        match entries.iter_mut().find(|(k, _)| key.is_some() && *k == key) {
            Some((_, group)) => group.push(port),
            None => entries.push((key, vec![port])),
        }
    }
    entries
        .into_iter()
        .map(|(key, ports)| {
            let (name, repeated) = match (&key, ports.as_slice()) {
                (Some((word, _)), [_, _, ..]) => {
                    let mut numbers: Vec<u32> = ports
                        .iter()
                        .filter_map(|p| numbered(p).map(|(_, n)| n))
                        .collect();
                    numbers.sort_unstable();
                    let mut repeated: Vec<String> = numbers
                        .windows(2)
                        .filter(|pair| pair[0] == pair[1])
                        .map(|pair| format!("{word} {}", pair[0]))
                        .collect();
                    repeated.dedup();
                    numbers.dedup();
                    (format!("{word} {}", ranges(&numbers)), repeated)
                }
                _ => (ports[0].name.clone(), Vec::new()),
            };
            PortEntry {
                name,
                ports,
                repeated,
            }
        })
        .collect()
}

/// What an input takes beyond its power, such as a strength, and lines
/// signed twice.
fn notes(entry: &PortEntry, input: bool) -> String {
    let port = entry.ports[0];
    let mut said: Vec<String> = entry
        .repeated
        .iter()
        .map(|name| format!("{name} signed twice"))
        .collect();
    if input && !port.signal_strength {
        let strength =
            entry.ports.iter().map(|p| p.strength).max().unwrap_or(1);
        if strength > 1 {
            said.push(format!("strength ≥ {strength}"));
        }
    }
    said.join(" · ")
}

/// Each broken convention, once however often it is broken, with where in
/// the file each break is.
pub(super) fn violation_list(violations: &[Violation]) -> Markup {
    let mut grouped: Vec<(&str, Vec<[i32; 3]>)> = Vec::new();
    for violation in violations {
        let at = violation.pos.into_iter();
        match grouped.iter_mut().find(|(m, _)| *m == violation.message) {
            Some((_, places)) => places.extend(at),
            None => grouped.push((&violation.message, at.collect())),
        }
    }
    html! {
        ul class="flex flex-col gap-3" {
            @for (message, places) in &grouped {
                li {
                    (message)
                    @if !places.is_empty() {
                        span class="block text-meta text-muted tabular-nums" {
                            @if places.len() > 1 { (places.len()) " places: " } @else { "At " }
                            @for (i, [x, y, z]) in places.iter().enumerate() {
                                @if i > 0 { "; " }
                                (x) ", " (y) ", " (z)
                            }
                        }
                    }
                }
            }
        }
    }
}
