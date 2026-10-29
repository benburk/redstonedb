//! A build's inputs and outputs as its file marks them, and the building
//! conventions it breaks, shown on its page and on the details page.

use maud::{Markup, html};

use crate::{
    import::Analysis,
    interface::{Bit, Interface, Port},
    lint::Violation,
    simulate::{InputTiming, OutputTiming, Timing},
};

use super::{SECTION_TITLE, TABLE_CELL, TABLE_HEAD};

/// The build's inputs and outputs, read from the file, each a row: a bus
/// by its bits, and a row of numbered lines, such as a memory's `read 0`
/// to `read 15`, as one. With its `timing`, each input says how late it
/// may switch and each output when it changes and settles, in ticks from
/// the first input, t = 0.
pub(super) fn interface_section(
    analysis: &Analysis,
    timing: Option<&Timing>,
) -> Markup {
    let Interface { inputs, outputs } = &analysis.interface;
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
    html! {
        section aria-labelledby="interface-heading" class="flex flex-col gap-6" {
            h2 id="interface-heading" class=(SECTION_TITLE) { "Inputs and outputs" }
            div class="relative overflow-x-auto" {
                table class="w-full border-collapse tabular-nums" {
                    caption class="sr-only" { "Inputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Input" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                            th scope="col" class=(TABLE_HEAD) { "Power" }
                            @if timing.is_some() { th scope="col" class=(TABLE_HEAD) { "Delay" } }
                        }
                    }
                    tbody {
                        @for entry in port_entries(inputs) {
                            tr {
                                (line_cell(&entry, true))
                                td class=(TABLE_CELL) { (bits(&entry)) }
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
                }
            }
            div class="relative overflow-x-auto" {
                table class="w-full border-collapse tabular-nums" {
                    caption class="sr-only" { "Outputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Output" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                            @if timing.is_some() {
                                th scope="col" class=(TABLE_HEAD) { "First change" }
                                th scope="col" class=(TABLE_HEAD) { "Settles" }
                            }
                        }
                    }
                    tbody {
                        @for entry in port_entries(outputs) {
                            tr {
                                (line_cell(&entry, false))
                                td class=(TABLE_CELL) { (bits(&entry)) }
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

/// When an input, the `i`th timed, may switch: the first is t = 0; another
/// by its latest tick, or for a pulse its window.
fn switch_by(i: usize, input: &InputTiming) -> String {
    match (i, input.earliest, input.tick) {
        (0, _, _) => "t = 0".to_owned(),
        (_, Some(earliest), Some(latest)) => {
            format!("{} to {}", ticks(earliest), ticks(latest))
        }
        (_, None, Some(latest)) => format!("≤ {}", ticks(latest)),
        (_, _, None) => "no effect".to_owned(),
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

/// An output's first change and settling tick, as cells.
fn output_ticks(output: &OutputTiming) -> Markup {
    html! {
        td class=(TABLE_CELL) { (output.first_change) "t" }
        td class={ (TABLE_CELL) " font-semibold" } { (output.latency) "t" }
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

/// What an input takes beyond its power, such as a strength or being
/// inverted, as its levers are saved on, and lines signed twice.
fn notes(entry: &PortEntry, input: bool) -> String {
    let port = entry.ports[0];
    let mut said: Vec<String> = entry
        .repeated
        .iter()
        .map(|name| format!("{name} signed twice"))
        .collect();
    if input && !port.signal_strength {
        let bits: Vec<&Bit> =
            entry.ports.iter().flat_map(|p| p.bits.iter()).collect();
        let strength =
            entry.ports.iter().map(|p| p.strength).max().unwrap_or(1);
        if strength > 1 {
            said.push(format!("strength ≥ {strength}"));
        }
        if bits.iter().all(|b| b.inverted) {
            said.push("inverted".to_owned());
        } else if bits.iter().any(|b| b.inverted) {
            said.push("some bits inverted".to_owned());
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
