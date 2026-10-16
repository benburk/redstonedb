//! A build's inputs and outputs as its file marks them, and the building
//! conventions it breaks, shown on its page and on the details page.

use maud::{Markup, html};

use crate::{
    import::Analysis,
    interface::{Bit, Interface, Port},
    lint::Violation,
};

use super::SECTION_TITLE;

/// The build's inputs and outputs, read from the file, a short list of
/// each: a bus by its bits, and a row of numbered lines, such as a
/// memory's `read 0` to `read 15`, as one entry.
pub(super) fn interface_section(analysis: &Analysis) -> Markup {
    let Interface { inputs, outputs } = &analysis.interface;
    html! {
        section aria-labelledby="interface-heading" {
            h2 id="interface-heading" class={ (SECTION_TITLE) " mb-3" } { "Inputs and outputs" }
            div class="grid gap-6 sm:grid-cols-2" {
                (port_list("Inputs", &port_entries(inputs), true))
                (port_list("Outputs", &port_entries(outputs), false))
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

/// A list of input or output entries under `heading`, each with a swatch
/// of its marking colour, its bits or lines, and, for an input, what it
/// takes beyond any power.
fn port_list(heading: &str, entries: &[PortEntry], inputs: bool) -> Markup {
    html! {
        div {
            h3 class="mb-2 text-meta font-semibold text-muted" { (heading) }
            ul class="flex flex-col gap-1.5" {
                @for entry in entries {
                    @let port = entry.ports[0];
                    @let dye = port.block.strip_suffix(" concrete").unwrap_or(&port.block).replace(' ', "_");
                    @let [r, g, b, _] = crate::thumbnail::dye_color(&dye);
                    li class="flex items-start gap-2" {
                        span aria-hidden="true" class="mt-1.5 inline-block size-3 shrink-0 rounded-[3px] border border-line"
                            style={ "background: rgb(" (r) " " (g) " " (b) ")" } {}
                        span {
                            span class="font-semibold" { (entry.name) }
                            " "
                            span class="text-meta text-muted" { (port_detail(entry, inputs)) }
                        }
                    }
                }
            }
        }
    }
}

/// What an entry carries, such as `8 bits` or `16 lines`, and for an
/// input, the strength its signs ask for and whether it is inverted, as
/// its levers are saved on.
fn port_detail(entry: &PortEntry, inputs: bool) -> String {
    let port = entry.ports[0];
    let mut said = vec![if entry.ports.len() > 1 {
        format!("{} lines", entry.ports.len())
    } else if port.signal_strength {
        format!(
            "{} bits, a hex digit per block's strength",
            port.bits.len() * 4
        )
    } else if port.bits.len() == 1 {
        "1 bit".to_owned()
    } else {
        format!("{} bits", port.bits.len())
    }];
    for name in &entry.repeated {
        said.push(format!("{name} signed more than once"));
    }
    if inputs && !port.signal_strength {
        let bits: Vec<&Bit> =
            entry.ports.iter().flat_map(|p| p.bits.iter()).collect();
        let strength =
            entry.ports.iter().map(|p| p.strength).max().unwrap_or(1);
        if strength > 1 {
            said.push(format!("strength {strength} or more"));
        }
        // An input saved with its lever on is inverted: off means 1.
        if bits.iter().all(|b| b.inverted) {
            said.push("inverted".to_owned());
        } else if bits.iter().any(|b| b.inverted) {
            said.push("some bits inverted".to_owned());
        }
    }
    said.join(" · ")
}
/// Each broken convention with where in the file it is.
pub(super) fn violation_list(violations: &[Violation]) -> Markup {
    html! {
        ul class="flex flex-col gap-1.5" {
            @for violation in violations {
                li class="flex gap-3" {
                    span class="w-24 shrink-0 text-muted tabular-nums" {
                        @if let Some([x, y, z]) = violation.pos { (x) ", " (y) ", " (z) }
                    }
                    span { (violation.message) }
                }
            }
        }
    }
}
