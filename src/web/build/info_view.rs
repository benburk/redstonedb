//! What a build's page says of it beside its viewer: its key facts, its
//! specs, each marked where the simulator or the file checked it, and its
//! credits.

use maud::{Markup, html};

use crate::{
    build::{Build, Status, tag},
    category::{Logic, Measure},
    simulate::Report,
    stack::StackDirection,
    web::{
        build::{
            detail_view::{FileRead, similar_section, verdict_tags},
            interface_view::{Inversions, interface_section},
            sources::{Source, Sources, stacking_found},
        },
        href,
        ui::{
            SECTION_TITLE, Tone, description, field,
            hover::{hover_note, hover_words},
            icons::{self, icon},
            person::{name_badges, person_badge},
            summary::{self, search_link, size},
            text::{self, long_date, plural},
        },
    },
};

/// What a build page's "Verified specs" means, on hover.
const VERIFIED_NOTE: &str = "Each spec here, and each key fact above, comes from simulating \
    the build or reading its file, not from what its uploader typed. Each new version is measured again.";

const UNVERIFIED_NOTE: &str = "The site couldn't verify this build as its category, so its \
    specs are as its uploader gave them, untested.";

/// `value`, with a check beside it when the simulator or the file checked
/// `value`, marked when the site didn't check it on a page where it
/// checked others, and with why when the simulation didn't bear it out.
/// A checked spec needs no mark: the page's heading says what is.
fn sourced(
    value: impl maud::Render,
    source: Source,
    sources: &Sources,
) -> Markup {
    html! {
        (value)
        @match source {
            Source::Simulated | Source::File => {},
            Source::Given => @if sources.some_checked() {
                " " span class="text-meta text-muted" { "untested" }
            },
            Source::Unconfirmed(why) => {
                " " span class="text-meta text-danger" { (why) }
            },
        }
    }
}

/// A build's tags, facts, and the builds like it, beside its viewer: a
/// live panel, redrawn when its simulation comes in, since that checks and
/// measures some of its facts.
pub fn build_info(
    s: &Build,
    similar: &[(Build, u32)],
    file: &Result<FileRead, String>,
    report: Option<&Report>,
) -> Markup {
    let interface = file.as_ref().ok().map(|file| &file.analysis.interface);
    let sources = Sources::of(s, interface, report);
    let flag_names: Vec<String> =
        s.flags.iter().map(|flag| flag.name.to_owned()).collect();
    html! {
        div id="build-info" fx-action={ (href::build(&s.id)) "?panel=info" } fx-trigger="redraw"
            class="flex flex-col gap-8" {
            (facts(s, &sources))
            // Whether it works leads its tags, then its design, orientation,
            // and flags, but not one another implies, as 4-wide stackable
            // beside 2-wide stackable.
            @let every = s.every_tag();
            @let tags: Vec<_> = every.iter().filter(|tag| !tag.is_unverified() && !tag::implied(&tag.name, &every)).collect();
            ul aria-label="Tags" class="flex flex-wrap gap-1.5" {
                (verdict_tags(s, report))
                @for tag in tags {
                    li { (summary::badge(tag, s)) }
                }
            }
            (specs(s, &sources, report))
            (credits(s))
            @match file {
                // Kept as built and never tested, its lines were never read.
                _ if !s.file.conventional
                    && !matches!(s.status, Status::Passing | Status::Failing) =>
                {
                    p class="text-meta text-muted" {
                        "Kept as built, so its inputs and outputs are not read."
                    }
                }
                Ok(file) => (interface_section(&file.analysis, report.and_then(Report::timing), report.and_then(Report::strength), &Inversions {
                    category: &s.category,
                    flags: &flag_names,
                    found: report.map_or(&[], Report::inverted),
                })),
                Err(err) => {
                    p class="text-meta text-muted" { "Could not read its inputs and outputs: " (err) }
                }
            }
            @if let Ok(file) = file && !file.block_counts.is_empty() {
                (block_counts(&file.block_counts))
            }
            @if let Some(description) = &s.description {
                section aria-labelledby="description-heading" {
                    h2 id="description-heading" class={ (SECTION_TITLE) " mb-2" } { "Description" }
                    div class="rich max-w-prose leading-prose break-words" { (description::render(description)) }
                }
            }
            @if !similar.is_empty() {
                (similar_section(similar))
            }
        }
    }
}

/// How many of each kind of block a build has, most first, folded away
/// until opened.
pub(crate) fn block_counts(counts: &[(&str, usize)]) -> Markup {
    html! {
        details class="group" {
            summary class={ (SECTION_TITLE) " flex w-fit cursor-pointer list-none items-center gap-1.5 [&::-webkit-details-marker]:hidden" } {
                "Block distribution"
                (icon(icons::CHEVRON_RIGHT, "size-4 text-muted transition-transform duration-150 ease-calm group-open:rotate-90"))
            }
            dl class="mt-2 grid max-w-md grid-cols-2 gap-x-8" {
                @for (kind, count) in counts {
                    div class="flex min-h-row items-center justify-between gap-3" {
                        dt class="whitespace-nowrap text-muted" { (kind) }
                        dd class="tabular-nums" { (count) }
                    }
                }
            }
        }
    }
}

/// The numbers a build is compared by, large: its measures, such as
/// width, then latency, volume, size, and the release it works from.
pub(crate) fn facts(s: &Build, sources: &Sources) -> Markup {
    let fact = |label: &str, value: Markup| {
        html! {
            div class="flex flex-col" {
                dt class="text-meta text-muted" { (label) }
                dd class="text-section font-semibold tabular-nums" { (value) }
            }
        }
    };
    let unknown =
        || html! { span class="font-normal text-muted" { "unknown" } };
    html! {
        section aria-label="Key facts" {
            dl class="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3" {
                @for &measure in s.measures() {
                    @let source = match measure {
                        Measure::Width => sources.width,
                        Measure::Pulse => sources.pulse,
                        Measure::AtLeast => Source::Given,
                    };
                    (fact(measure.label(), s.specs.of(measure).map_or_else(unknown, |n| {
                        sourced(search_link(&measure.words(n), &measure.term(n)), source, sources)
                    })))
                }
                (fact("Latency", s.specs.latency.map_or_else(unknown, |t| {
                    sourced(text::ticks(t), sources.timing, sources)
                })))
                @match s.per_copy() {
                    // A copy's own shape, beside what it adds.
                    Some((x, y, z)) => (fact("Blocks per copy", html! {
                        (s.volume())
                        span class="block text-meta font-normal text-muted" { (y) "×" (x) "×" (z) " H×W×D" }
                    })),
                    None => (fact("Blocks", html! { (s.volume()) })),
                }
                (fact("Size, H×W×D", html! { (size(s)) }))
                @if let Some(from) = &s.file.works_from {
                    // Why that release, on hover: the newest part it uses.
                    @let why = match &s.file.works_from_by {
                        Some(by) => format!("Needs {from} for its {by}"),
                        None => format!("Uses only what redstone has had since {from}"),
                    };
                    (fact("Works from", sourced(hover_note(
                        "works-from-note",
                        &search_link(from, &format!("version:{}", from.to_lowercase())),
                        &why,
                    ), Source::File, sources)))
                }
            }
        }
    }
}

/// One way build `s` stacks, short: for copies that add bits, their step
/// and pitch class, such as `16 up, pitch 2, any number`; for a
/// register's copies across its bits, as more words, such as `4 west as
/// words, up to 3`; and for a stacking its author gave, its step, such
/// as `2 wide west, up to 7` or `2 west and 2 up, any number`.
fn short_stacking(s: &Build, direction: &StackDirection) -> String {
    let steps: Vec<(i64, &str)> = direction.parts().collect();
    let step = match (steps.as_slice(), direction.pitch) {
        ([(n, way)], Some(pitch)) => format!("{n} {way}, pitch {pitch}"),
        ([(n, way)], None)
            if stacking_found(s)
                && !direction.adds_bits(s.specs.orientation) =>
        {
            format!("{n} {way} as words")
        }
        ([(n, way)], None) => format!("{n} wide {way}"),
        _ => {
            let each: Vec<String> =
                steps.iter().map(|(n, way)| format!("{n} {way}")).collect();
            each.join(" and ")
        }
    };
    match direction.max_copies {
        Some(n) => format!("{step}, up to {n}"),
        None => format!("{step}, any number"),
    }
}

/// What a build is and how it behaves, beyond its key facts.
pub(crate) fn specs(
    s: &Build,
    sources: &Sources,
    report: Option<&Report>,
) -> Markup {
    // An adder's design, among its tags, has groups as its run found.
    let group = match report {
        Some(Report::Ran(run)) => run.design.as_ref().and_then(|d| d.group),
        _ => None,
    };
    html! {
        section aria-labelledby="specs-heading" {
            h2 id="specs-heading" class={ (SECTION_TITLE) " mb-2" } {
                @if sources.verified {
                    (hover_words("specs-note", "Verified specs", VERIFIED_NOTE))
                } @else if s.unverified.is_some() {
                    (hover_words("specs-note", "Untested specs", UNVERIFIED_NOTE))
                } @else {
                    "Specs"
                }
            }
            dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                // A category off the list has no list of its own to link to.
                (field("Category", sourced(html! {
                    @if s.kind().is_some() {
                        a href=(href::category(&s.category)) { (s.category_label()) }
                    } @else {
                        (s.category_label())
                    }
                }, sources.category, sources)))
                @if let Some(group) = group {
                    (field("Groups", html! { (group) " bits" }))
                }
                @if let Some(t) = s.specs.throughput {
                    (field("Throughput", sourced(search_link(&Measure::Pulse.words(t), &format!("throughput:{t}")), sources.throughput, sources)))
                }
                @if let Some(synced) = s.specs.synchronized {
                    (field("Synced", sourced(flag(synced, Some(if synced { tag::SYNCED } else { tag::UNSYNCED })), sources.timing, sources)))
                }
                @if let Some(logic) = s.logic() {
                    (field("Stateless", sourced(flag(logic == Logic::Stateless, Some(logic.name())), sources.logic, sources)))
                }
                @if s.pipelined() {
                    (field("Pipelined", sourced(flag(true, Some("pipelined")), sources.throughput, sources)))
                }
                // Whether it stacks is among its tags; how, here, after
                // the yes-or-no specs, as it says more.
                @if !s.stacking.is_empty() {
                    (field("Stacks", sourced(html! {
                        a href="#stacking" { (s.stacking.iter().map(|d| short_stacking(s, d)).collect::<Vec<_>>().join(" · ")) }
                    }, sources.stacking, sources)))
                }
                // Its copies stacked to each wider width, as they compete
                // on that width's leaderboard.
                @let timed: Vec<String> = s.widened.iter().filter_map(|w| {
                    w.latency.map(|t| format!("{}-bit in {t} ticks", w.width))
                }).collect();
                @if !timed.is_empty() {
                    (field("Stacked", sourced(html! { (timed.join(" · ")) }, sources.timing, sources)))
                }
            }
        }
    }
}

/// A yes or no as a small pill: green-tinted for yes, neutral for no,
/// since a build without a trait is not wrong. A yes links to the builds
/// that share it when `term` searches them.
pub(crate) fn flag(on: bool, term: Option<&str>) -> Markup {
    let pill = "inline-block min-w-10 rounded-full px-2 text-center text-meta leading-[1.375rem] font-semibold";
    let (word, color) = if on {
        ("Yes", Tone::Good)
    } else {
        ("No", Tone::Plain)
    };
    html! {
        @match term {
            Some(term) => a href=(href::search(term)) title="Find builds that share it"
                class={ (pill) " " (color) " hover:no-underline" } { (word) },
            None => span class={ (pill) " " (color) } { (word) },
        }
    }
}

/// Who built it and when, and its file's history, folded away.
pub(crate) fn credits(s: &Build) -> Markup {
    html! {
        section aria-labelledby="credits-heading" {
            h2 id="credits-heading" class={ (SECTION_TITLE) " mb-2" } { "Credits" }
            dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                (field(&plural(s.credits.authors.len(), "Author"), if s.credits.authors.is_empty() {
                    html! { span class="text-muted" { "unknown" } }
                } else {
                    name_badges(&s.credits.authors)
                }))
                @if !s.credits.contributors.is_empty() {
                    (field(&plural(s.credits.contributors.len(), "Contributor"), name_badges(&s.credits.contributors)))
                }
                @if let Some(built) = &s.built_on {
                    (field("Built", long_date(built)))
                }
                // An upload names its account, by its Minecraft name if it
                // has one; an import names whoever ran it.
                @match (&s.entry.uploader, &s.entry.uploader_name) {
                    (Some(username), Some(name)) => {
                        (field("Uploaded by", person_badge(name, &href::user(username))))
                    }
                    _ => @if let Some(name) = &s.entry.imported_by {
                        (field("Imported by", person_badge(name, &href::search(&format!("importer:{name}")))))
                    },
                }
            }
            details class="mt-3" {
                summary class="cursor-pointer text-meta text-muted" { "File" }
                dl class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                    (field("Name", html! { code class="break-all" { (s.filename()) } }))
                    (field("Uploaded", long_date(&s.entry.added_at)))
                    @if let Some(edited) = &s.entry.edited_at {
                        (field("Last edited", long_date(edited)))
                    }
                    (field("Downloads", s.entry.downloads.to_string()))
                    (field("Conventions", if s.file.conventional {
                        html! { "followed" }
                    } else {
                        search_link("not followed", "unconventional")
                    }))
                }
            }
        }
    }
}
