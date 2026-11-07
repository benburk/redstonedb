//! A build's inputs and outputs as its roles mark them, shown on its page
//! and on the details page.

use maud::{Markup, html};

use crate::{
    category::Flag,
    category::Kind,
    import::Analysis,
    recognize::{
        colour,
        interface::{Bit, Port},
    },
    schem::lines::Role,
    simulate::{InputTiming, OutputTiming, Strength, Timing},
};

use super::{
    SECTION_TITLE, TABLE_CELL, TABLE_HEAD,
    build::hover_note,
    icons::{CHEVRON_RIGHT, icon},
};

/// The lines a flag says are read inverted.
pub(super) enum Inverts {
    /// The inputs of this role.
    Input(Role),
    /// The outputs of this role.
    Output(Role),
    /// Every output.
    Outputs,
}

/// Which lines `flag` on a build of `category` says are read inverted,
/// if it says any are: an adder's carry in, a control line, or the
/// outputs of a register or a monostable, the only builds whose outputs
/// may read inverted. A decoder's outputs always do, so no flag says so.
pub(super) fn inverts(category: &str, flag: &str) -> Option<Inverts> {
    Some(match (Kind::named(category)?, flag) {
        (
            Kind::Adder
            | Kind::HexAdder
            | Kind::Subtractor
            | Kind::InvertedAdder,
            Flag::INVERTED_CARRY_IN,
        ) => Inverts::Input(Role::Control),
        (
            Kind::Adder | Kind::HexAdder | Kind::InvertedAdder,
            Flag::INVERTED_CARRY_OUT,
        ) => Inverts::Output(Role::Status),
        (
            Kind::Register | Kind::Monostable | Kind::DualEdgeMonostable,
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
    flags.iter().map(String::as_str).find(|flag| {
        match inverts(category, flag) {
            Some(Inverts::Input(role)) => input && port.role == Some(role),
            Some(Inverts::Output(role)) => !input && port.role == Some(role),
            Some(Inverts::Outputs) => !input,
            None => false,
        }
    })
}

/// Which of a build's lines read inverted: those its `flags`, of its
/// `category`, say are, and those its test `found` are, by name.
pub(super) struct Inversions<'a> {
    pub category: &'a str,
    pub flags: &'a [String],
    pub found: &'a [String],
}

/// Whether a line reads inverted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Inverted<'f> {
    /// As this flag says, which finds the builds that share it.
    Flagged(&'f str),
    /// As its test found, with no flag to say so.
    Found,
    No,
}

impl Inversions<'_> {
    /// Whether `entry` reads inverted.
    fn of(&self, entry: &PortEntry, input: bool) -> Inverted<'_> {
        let port = entry.ports[0];
        match inverted_by(self.category, self.flags, port, input) {
            Some(flag) => Inverted::Flagged(flag),
            None if entry
                .ports
                .iter()
                .any(|p| self.found.contains(&p.name)) =>
            {
                Inverted::Found
            }
            None => Inverted::No,
        }
    }
}

/// The build's inputs and outputs, read from the file, each a row: a bus
/// by its bits, and a row of numbered lines, such as a memory's `read 0`
/// to `read 15`, as one. With its `timing`, each input says when it
/// switches for the build to stay synced and each output when it changes
/// and settles, in ticks from t = 0, as the inputs the outputs wait
/// longest on switch. When any line reads
/// inverted, each says whether it does.
pub(super) fn interface_section(
    analysis: &Analysis,
    timing: Option<&Timing>,
    strength: Option<&Strength>,
    inversions: &Inversions,
) -> Markup {
    let (inputs, outputs) =
        (&analysis.interface.inputs, &analysis.interface.outputs);
    let inverted = |entry: &PortEntry, input: bool| inversions.of(entry, input);
    let any_inverted = port_entries(inputs)
        .iter()
        .any(|e| inverted(e, true) != Inverted::No)
        || port_entries(outputs)
            .iter()
            .any(|e| inverted(e, false) != Inverted::No);
    let timed_in = |entry: &PortEntry| {
        timing?
            .inputs
            .iter()
            .find(|input| input.name == entry.ports[0].name)
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
                            @if strength.is_some() { (head("in-ss", "Min SS", Some(INPUT_SS_NOTE))) }
                            (head("in-power", "Power", Some(POWER_NOTE)))
                            @if timing.is_some() { (head("in-delay", "Delay", Some(DELAY_NOTE))) }
                        }
                        @for entry in port_entries(inputs) {
                            tr {
                                (line_cell(&entry, true, None))
                                td class=(TABLE_CELL) { (bits(&entry)) }
                                @if any_inverted { (inverted_cell(inverted(&entry, true))) }
                                @if let Some(strength) = strength {
                                    @if entry.ports[0].control() {
                                        td class={ (TABLE_CELL) " text-muted" } { "—" }
                                    } @else {
                                        td class=(TABLE_CELL) { (strength.input) }
                                    }
                                }
                                td class=(TABLE_CELL) { (power(&entry)) }
                                @if timing.is_some() {
                                    td class=(TABLE_CELL) {
                                        @match timed_in(&entry) {
                                            Some(input) => (switch_by(input)),
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
                            @if strength.is_some() { (head("out-ss", "Min SS", Some(OUTPUT_SS_NOTE))) }
                            @if timing.is_some() {
                                (head("out-first", "First change", Some(FIRST_CHANGE_NOTE)))
                                (head("out-settles", "Settles", Some(SETTLES_NOTE)))
                            }
                        }
                        @for entry in port_entries(outputs) {
                            @let rows = bit_rows(&entry, timed_out(&entry), strength);
                            tr {
                                (line_cell(&entry, false, rows.as_ref().map(|rows| rows.ids.as_str())))
                                td class=(TABLE_CELL) { (bits(&entry)) }
                                @if any_inverted { (inverted_cell(inverted(&entry, false))) }
                                @if let Some(strength) = strength {
                                    (output_strength(&entry, strength))
                                }
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
                            @if let Some(rows) = rows {
                                (rows.markup(any_inverted, strength.is_some(), timing.is_some()))
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
const INPUT_SS_NOTE: &str = "Weakest signal strength the data inputs work at, all driven \
    at it together, every case still right";
const OUTPUT_SS_NOTE: &str = "Weakest signal strength arriving at a bit's output \
    repeater or comparator while on, from the dust, block, or comparator behind it, \
    not what the output gives out; a dust output's own. The data inputs at their weakest";
const POWER_NOTE: &str = "Hard: dust touches the input block, so a repeater, comparator, \
    or torch must drive it. Soft: dust may drive it.";
const DELAY_NOTE: &str = "Tick to switch it, from t = 0, when the inputs the \
    outputs wait longest on switch, for the outputs to stay synced";
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

/// A bit row's cell: a table cell, tighter, as the rows are the output's
/// parts.
const BIT_CELL: &str = "border-b border-line py-1.5 pr-4 align-top";

/// An output's bits, each a row hidden under the output's own until its
/// name opens them: each bit's weakest strength while on and its ticks.
struct BitRows<'a> {
    /// The rows' ids, space-separated, for the name to open.
    ids: String,
    strengths: Option<&'a [u8]>,
    timing: Option<&'a OutputTiming>,
}

/// `entry`'s bit rows, for an output of more than one bit whose bits were
/// measured; `None` for any other.
fn bit_rows<'a>(
    entry: &PortEntry,
    timing: Option<&'a OutputTiming>,
    strength: Option<&'a Strength>,
) -> Option<BitRows<'a>> {
    let port = entry.ports[0];
    let strengths =
        strength.and_then(|strength| output_bits_strength(port, strength));
    let timed = timing.filter(|t| t.bits.len() == port.bits.len());
    let wide = entry.ports.len() == 1 && port.bits.len() > 1;
    (wide && (strengths.is_some() || timed.is_some())).then(|| BitRows {
        ids: (0..port.bits.len())
            .map(|bit| format!("bits-{}-{bit}", port.name.replace(' ', "-")))
            .collect::<Vec<_>>()
            .join(" "),
        strengths,
        timing: timed,
    })
}

impl BitRows<'_> {
    fn markup(&self, inverted: bool, strength: bool, timing: bool) -> Markup {
        html! {
            @for (bit, id) in self.ids.split(' ').enumerate() {
                tr id=(id) hidden {
                    th scope="row" class={ (BIT_CELL) " pl-5 text-left font-normal text-muted" } { "bit " (bit) }
                    td class=(BIT_CELL) {}
                    @if inverted { td class=(BIT_CELL) {} }
                    @if strength {
                        @match self.strengths.and_then(|bits| bits.get(bit)).filter(|&&s| s > 0) {
                            Some(s) => td class=(BIT_CELL) { (s) },
                            None => td class={ (BIT_CELL) " text-muted" } { "—" },
                        }
                    }
                    @if timing {
                        @match self.timing.and_then(|t| t.bits.get(bit)) {
                            Some(b) => {
                                td class=(BIT_CELL) { (b.first_change) "t" }
                                td class=(BIT_CELL) { (b.latency) "t" }
                            }
                            None => {
                                td class=(BIT_CELL) {}
                                td class=(BIT_CELL) {}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The weakest strength each bit of output `port` showed while on, as
/// `strength` measured it.
fn output_bits_strength<'a>(
    port: &Port,
    strength: &'a Strength,
) -> Option<&'a [u8]> {
    strength
        .outputs
        .iter()
        .find(|(name, _)| *name == port.name)
        .map(|(_, bits)| bits.as_slice())
}

/// A line's name beside a swatch of the colour the site shows its role in,
/// see [`colour::shown`], with what else it
/// takes, such as a strength or being inverted, beneath. With `opens`, the
/// name is a button showing or hiding the rows those ids name.
fn line_cell(entry: &PortEntry, input: bool, opens: Option<&str>) -> Markup {
    let port = entry.ports[0];
    let dye = port.role.map_or("white", colour::shown);
    let [r, g, b, _] = blockrender::dye_color(dye);
    let notes = notes(entry, input);
    html! {
        th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
            span class="flex items-start gap-2" {
                span aria-hidden="true" class="mt-1.5 inline-block size-3 shrink-0 rounded-[3px] border border-line"
                    style={ "background: rgb(" (r) " " (g) " " (b) ")" } {}
                span {
                    @match opens {
                        Some(ids) => button type="button" data-disclose aria-expanded="false" aria-controls=(ids)
                            class="group inline-flex cursor-pointer items-center gap-1 font-semibold" {
                            (entry.name)
                            (icon(CHEVRON_RIGHT, "size-3.5 text-muted transition-transform duration-150 ease-calm group-aria-expanded:rotate-90"))
                        },
                        None => span class="font-semibold" { (entry.name) },
                    }
                    @if !notes.is_empty() {
                        span class="block text-meta text-muted" { (notes) }
                    }
                }
            }
        }
    }
}

/// Whether a line reads inverted, a yes by a flag finding the builds that
/// share it.
fn inverted_cell(inverted: Inverted) -> Markup {
    html! {
        @match inverted {
            Inverted::Flagged(flag) => td class=(TABLE_CELL) {
                a href=(super::search_href(flag)) title="Find builds that share it" { "Yes" }
            },
            Inverted::Found => td class=(TABLE_CELL) { "Yes" },
            Inverted::No => td class={ (TABLE_CELL) " text-muted" } { "No" },
        }
    }
}

/// The weakest strength any bit of output `entry` carries while on, as
/// `strength` measured it, each bit's in its row; a dash for an output it
/// didn't measure, as a status line.
fn output_strength(entry: &PortEntry, strength: &Strength) -> Markup {
    let weakest = output_bits_strength(entry.ports[0], strength)
        .and_then(|bits| bits.iter().copied().filter(|&s| s > 0).min());
    html! {
        @match weakest {
            Some(weakest) => td class=(TABLE_CELL) { (weakest) },
            None => td class={ (TABLE_CELL) " text-muted" } { "—" },
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

/// When an input switches so its change lands with the slowest's: those
/// an output waits longest on at t = 0, another at its offset from them.
fn switch_by(input: &InputTiming) -> String {
    match input.tick {
        Some(0) => "t = 0".to_owned(),
        Some(tick) => ticks(tick),
        None => "no effect".to_owned(),
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
type PortGroup<'p> = (Option<(String, Option<Role>)>, Vec<&'p Port>);

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
        let key = numbered(port).map(|(word, _)| (word, port.role));
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
