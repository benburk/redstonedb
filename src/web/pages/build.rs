//! A build's page: a verdict under its title, its key facts and specs
//! beside its viewer, each marked where the simulator or the file checked
//! it, then its simulation, inputs and outputs, stacking, and versions.

use maud::{Markup, PreEscaped, html};

use crate::{
    import,
    interface::Interface,
    models::{
        schematic::{self, Flag, Measure, Schematic, StackDirection},
        simulation::Status,
        user::User,
        version::Version,
    },
    simulate::{self, Report, Stacking, Timing},
};

use super::{
    Analysis, PAGE_TITLE, SECTION_TITLE, Waiting, base, beside_viewer,
    cases_pending, delete_dialog, download_dialog, field, grouped,
    interface_section, name_badges, person_badge, plural, search_link,
    similar_section, simulation_section, size, stack_guide, stacking_found,
    version, violations_section,
};

/// A seal with a check, Heroicons' `check-badge`: what the simulator or
/// the file checked, kept apart from a flag's plain check.
const VERIFIED: &str = r#"<svg viewBox="0 0 20 20" fill="currentColor" aria-hidden="true" class="inline size-3.5 align-[-2px]"><path fill-rule="evenodd" d="M16.403 12.652a3 3 0 0 0 0-5.304 3 3 0 0 0-3.75-3.751 3 3 0 0 0-5.305 0 3 3 0 0 0-3.751 3.75 3 3 0 0 0 0 5.305 3 3 0 0 0 3.75 3.751 3 3 0 0 0 5.305 0 3 3 0 0 0 3.751-3.75Zm-2.546-4.46a.75.75 0 0 0-1.214-.883l-3.483 4.79-1.88-1.88a.75.75 0 1 0-1.06 1.061l2.5 2.5a.75.75 0 0 0 1.137-.089l4-5.5Z" clip-rule="evenodd"/></svg>"#;

/// One schematic's page, with an edit button for a mod.
pub fn schematic_page(
    viewer: Option<&User>,
    s: &Schematic,
    flags: &[Flag],
    similar: &[(Schematic, u32)],
    analysis: &Result<Analysis, String>,
    simulation: &Status,
    versions: &[Version],
) -> Markup {
    let report = match simulation {
        Status::Done(report) | Status::Pending(Some(report)) => Some(report),
        Status::Pending(None) | Status::Untested => None,
    };
    let simulating = matches!(simulation, Status::Pending(None));
    let name = s.name();
    let path = format!("/schematics/{}", s.id);
    base(
        viewer,
        &name,
        &path,
        &html! {
            div class="mb-6 flex flex-wrap items-start justify-between gap-4" {
                div class="flex flex-wrap items-center gap-x-4 gap-y-2" {
                    h1 class=(PAGE_TITLE) { (name) }
                    (build_banner(s, report, simulating))
                }
                div class="flex gap-2" {
                    @if viewer.is_some_and(User::is_admin) {
                        button type="button" commandfor="delete" command="show-modal" class="btn btn-danger" { "Delete" }
                        (delete_dialog(s))
                    }
                    @if viewer.is_some_and(|v| v.may_edit(s)) {
                        a href={ (path) "/versions/new" } class="btn btn-quiet" { "New version" }
                        a href={ (path) "/edit" } class="btn btn-quiet" { "Edit" }
                    }
                    button type="button" commandfor="download" command="show-modal" class="btn btn-primary" { "Download" }
                    (download_dialog(s, viewer.is_some()))
                }
            }
            (beside_viewer(&format!("{path}/download?preview=yes"), &build_info(s, flags, similar, analysis, report)))
            @let poll = format!("{path}/simulation");
            @match simulation {
                Status::Done(report) | Status::Pending(Some(report)) => {
                    (simulation_section(report, Waiting::Polled(&poll)))
                }
                Status::Pending(None) => (cases_pending(&poll)),
                Status::Untested => {}
            }
            @if let Ok(analysis) = analysis && s.conventional {
                (violations_section(analysis))
            }
            @if let Ok(analysis) = analysis {
                (stack_guide::stacking_section(s, &analysis.stacking, stacking_found(s, report)))
            }
            (version::versions_section(s, versions))
        },
    )
}

/// Beside a build's title, what its simulation says, as a badge: passes,
/// fails, still running, or not simulated, with the counts or the reason
/// in a popup on hover or focus. A live panel, redrawn when its
/// simulation comes in.
pub fn build_banner(
    s: &Schematic,
    report: Option<&Report>,
    simulating: bool,
) -> Markup {
    let (label, tone, note) = match report {
        Some(Report::Ran(run)) if run.wrong > 0 => (
            "Fails simulation",
            "bg-danger/12 text-danger",
            format!(
                "{} of {} cases of {} wrong; see which below.",
                grouped(run.wrong),
                grouped(run.cases),
                run.pattern
            ),
        ),
        Some(Report::Ran(run)) => (
            "Passes simulation",
            "bg-success/15 text-success",
            format!(
                "All {} cases of {} right{}.",
                grouped(run.cases),
                run.pattern,
                if run.timing.is_none() {
                    "; timing to follow"
                } else {
                    ""
                }
            ),
        ),
        Some(Report::Broken { reason }) => (
            "Fails simulation",
            "bg-danger/12 text-danger",
            reason.clone(),
        ),
        Some(Report::Skipped { reason }) => {
            ("Not simulated", "bg-muted/12 text-muted", reason.clone())
        }
        None if simulating => (
            "Simulating…",
            "bg-muted/12 text-muted",
            "Its cases are running; this updates when they finish.".to_owned(),
        ),
        None => (
            "Not simulated",
            "bg-muted/12 text-muted",
            "The simulator has no test for this category yet.".to_owned(),
        ),
    };
    let pill = html! {
        span class={ "inline-block rounded-full px-2.5 text-meta leading-6 font-semibold " (tone) } { (label) }
    };
    html! {
        div id="build-banner" fx-action={ "/schematics/" (s.id) "?panel=banner" } fx-trigger="redraw" {
            (hover_note("verdict-note", &pill, &note))
        }
    }
}

/// Where a spec on a build's page comes from, marked beside it, so a
/// reader can tell what the site checked from what its uploader typed.
#[derive(Clone, Copy)]
enum Source {
    /// Measured or confirmed by the simulation.
    Simulated,
    /// Read from the file.
    File,
    /// As typed, unchecked.
    Given,
    /// Typed, and the simulation did not bear it out, as it says.
    Unconfirmed(&'static str),
}

/// Where each spec that can be checked on build `s`'s page comes from.
struct Sources {
    category: Source,
    width: Source,
    orientation: Source,
    encoding: Source,
    /// Latency and whether it is synchronized, measured together.
    timing: Source,
    stacking: Source,
}

impl Sources {
    /// Build `s`'s sources, with its file's `interface` if it could be
    /// read and its simulation's `report` if it has one.
    fn of(
        s: &Schematic,
        interface: Option<&Interface>,
        report: Option<&Report>,
    ) -> Self {
        let passed = report.is_some_and(Report::passed);
        // A conventional build's file sets these over what was typed.
        let derived = interface
            .filter(|_| s.conventional)
            .map(|interface| import::derived(&s.category, interface))
            .unwrap_or_default();
        let checked = |from_file: bool, simulated: bool| {
            if from_file {
                Source::File
            } else if simulated {
                Source::Simulated
            } else {
                Source::Given
            }
        };
        let run = match report {
            Some(Report::Ran(run)) => Some(run),
            _ => None,
        };
        let stacking = if stacking_found(s, report)
            && run.is_some_and(|run| run.stacking.is_some())
        {
            Source::Simulated
        } else if !s.stacking.is_empty()
            && simulate::FINDS_STACKING.contains(&s.category.as_str())
            && run.is_some_and(|run| {
                matches!(run.stacking, Some(Stacking::Not { .. }))
            })
        {
            Source::Unconfirmed("not found by simulation")
        } else {
            Source::Given
        };
        Sources {
            category: if report.is_some_and(Report::failed) {
                Source::Unconfirmed("fails simulation")
            } else {
                checked(false, passed)
            },
            width: checked(
                derived.width == s.width && s.width.is_some(),
                passed,
            ),
            orientation: checked(
                derived.orientation == Some(s.orientation.as_str()),
                false,
            ),
            encoding: checked(
                derived.encoding.is_some()
                    && derived.encoding == s.encoding.as_deref(),
                false,
            ),
            timing: checked(
                false,
                report
                    .and_then(Report::timing)
                    .and_then(Timing::latency)
                    .is_some(),
            ),
            stacking,
        }
    }
}

/// `value`, with a check beside it when the simulator or the file checked
/// it, or why the simulation does not bear it out.
fn sourced(value: impl maud::Render, source: Source) -> Markup {
    let mark = |said: &str| {
        html! {
            span class="ml-1.5 text-brand" title=(said) {
                (PreEscaped(VERIFIED))
                span class="sr-only" { (said) }
            }
        }
    };
    html! {
        (value)
        @match source {
            Source::Simulated => (mark("Auto-verified by the simulator")),
            Source::File => (mark("Auto-verified from the file")),
            Source::Given => {},
            Source::Unconfirmed(why) => {
                " " a href="#simulation" class="text-meta text-danger" { (why) }
            },
        }
    }
}

/// A build's tags, facts, and the builds like it, beside its viewer: a
/// live panel, redrawn when its simulation comes in, since that checks and
/// measures some of its facts.
pub fn build_info(
    s: &Schematic,
    flags: &[Flag],
    similar: &[(Schematic, u32)],
    analysis: &Result<Analysis, String>,
    report: Option<&Report>,
) -> Markup {
    let interface = analysis.as_ref().ok().map(|analysis| &analysis.interface);
    let sources = Sources::of(s, interface, report);
    html! {
        div id="build-info" fx-action={ "/schematics/" (s.id) "?panel=info" } fx-trigger="redraw"
            class="flex flex-col gap-8" {
            (facts(s, &sources))
            // A failing build says so beside its title, not among its tags.
            @let tags: Vec<_> = s.tags.iter().filter(|tag| tag.name != schematic::FAILS_SIMULATION).collect();
            @if !tags.is_empty() {
                ul aria-label="Tags" class="flex flex-wrap gap-1.5" {
                    @for tag in tags {
                        li { (super::badge(tag)) }
                    }
                }
            }
            (specs(s, flags, &sources, report))
            (credits(s))
            @match analysis {
                _ if !s.conventional => {
                    p class="text-meta text-muted" {
                        "Follows its own layout, so its inputs and outputs are not read and it is not linted."
                    }
                }
                Ok(analysis) => (interface_section(analysis)),
                Err(err) => {
                    p class="text-meta text-muted" { "Could not read its inputs and outputs: " (err) }
                }
            }
            @if let Some(description) = &s.description {
                section aria-labelledby="description-heading" {
                    h2 id="description-heading" class={ (SECTION_TITLE) " mb-2" } { "Description" }
                    p class="leading-prose whitespace-pre-line" { (description) }
                }
            }
            @if !similar.is_empty() {
                (similar_section(similar))
            }
        }
    }
}

/// The numbers a build is compared by, large: its measures, such as
/// width, then latency, volume, size, and the release it works from.
fn facts(s: &Schematic, sources: &Sources) -> Markup {
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
                @for &measure in &s.measures {
                    @let source = if measure == Measure::Width { sources.width } else { Source::Given };
                    (fact(measure.heading(), measure.of(s).map_or_else(unknown, |n| {
                        sourced(search_link(&measure.words(n), &measure.term(n)), source)
                    })))
                }
                (fact("Latency", s.latency.map_or_else(unknown, |t| {
                    sourced(format!("{t} ticks"), sources.timing)
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
                @if let Some(from) = &s.works_from {
                    // Why that release, on hover: the newest part it uses.
                    @let why = match &s.works_from_by {
                        Some(by) => format!("Needs {from} for its {by}"),
                        None => format!("Uses only what redstone has had since {from}"),
                    };
                    (fact("Works from", sourced(hover_note(
                        "works-from-note",
                        &search_link(from, &format!("version:{}", from.to_lowercase())),
                        &why,
                    ), Source::File)))
                }
            }
            p class="mt-3 text-meta text-muted" {
                span class="text-brand" { (PreEscaped(VERIFIED)) } " Auto-verified"
            }
        }
    }
}

/// `content` with `note` in a small popup above it while it is hovered or
/// focused, and as its description for a screen reader. `id` names the
/// note, once on the page.
fn hover_note(id: &str, content: &Markup, note: &str) -> Markup {
    html! {
        span tabindex="0" aria-describedby=(id)
            class="group relative inline-block cursor-help rounded-control focus-visible:outline-2 focus-visible:outline-boundary" {
            (content)
            span id=(id) role="tooltip"
                class="pointer-events-none invisible absolute bottom-full left-0 z-20 mb-1.5 w-max max-w-xs rounded-control bg-ink px-2.5 py-1.5 text-meta font-normal text-canvas opacity-0 shadow-overlay transition-opacity duration-150 ease-calm group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100 motion-reduce:transition-none" {
                (note)
            }
        }
    }
}

/// One way a build stacks, short, such as `2 west, up to 7`.
fn short_stacking(direction: &StackDirection) -> String {
    let steps: Vec<String> = [
        (direction.west, "west"),
        (direction.up, "up"),
        (direction.north, "north"),
    ]
    .into_iter()
    .filter(|&(n, _)| n > 0)
    .map(|(n, way)| format!("{n} {way}"))
    .collect();
    match direction.max_copies {
        Some(n) => format!("{}, up to {n}", steps.join(" and ")),
        None => steps.join(" and "),
    }
}

/// What a build is and how it behaves, beyond its key facts.
fn specs(
    s: &Schematic,
    flags: &[Flag],
    sources: &Sources,
    report: Option<&Report>,
) -> Markup {
    html! {
        section aria-labelledby="specs-heading" {
            h2 id="specs-heading" class={ (SECTION_TITLE) " mb-2" } { "Specs" }
            dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                // A category off the list has no list of its own to link to.
                (field("Category", sourced(html! {
                    @if s.category_label.is_some() {
                        a href={ "/?category=" (s.category) } { (s.category_label()) }
                    } @else {
                        (s.category_label())
                    }
                }, sources.category)))
                @if let Some(design) = s.design_label() {
                    (field("Design", search_link(&design, &s.design)))
                }
                (field("Orientation", sourced(search_link(s.orientation_label(), &s.orientation_label().to_lowercase()), sources.orientation)))
                @if let Some(encoding) = s.encoding_label() {
                    (field("Encoding", sourced(search_link(encoding, &format!("encoding:{}", s.encoding.as_deref().unwrap_or_default())), sources.encoding)))
                }
                // A serial build's throughput is among its measures.
                @if !s.has(Measure::Throughput) && let Some(t) = s.throughput {
                    (field("Throughput", search_link(&Measure::Throughput.words(t), &format!("throughput:{t}"))))
                }
                @let stacks = if s.stacking.is_empty() {
                    flag(false, None)
                } else {
                    html! {
                        a href="#stacking" { (s.stacking.iter().map(short_stacking).collect::<Vec<_>>().join(" · ")) }
                    }
                };
                // What stacking copies found, on hover.
                @let found = report.and_then(|report| match report {
                    Report::Ran(run) => run.stacking.as_ref(),
                    _ => None,
                });
                (field("Stacks", sourced(match found {
                    Some(stacking) => hover_note("stacks-note", &stacks, &super::stacking_words(stacking)),
                    None => stacks,
                }, sources.stacking)))
                @if let Some(synced) = s.synchronized {
                    (field("Synced", sourced(flag(synced, Some(if synced { "sync" } else { "unsync" })), sources.timing)))
                }
                @match s.logic.as_deref() {
                    Some("comb") => (field("Stateless", flag(true, Some("stateless")))),
                    Some("seq") => (field("Stateless", flag(false, Some("stateful")))),
                    _ => {}
                }
                @if s.pipelined() {
                    (field("Pipelined", flag(true, Some("pipelined"))))
                }
                // A category such as adder reads its flags from the file.
                @let read = if s.conventional && import::FLAGS_FROM_FILE.contains(&s.category.as_str()) {
                    Source::File
                } else {
                    Source::Given
                };
                @for each in flags {
                    @let has = s.flags.iter().any(|has| has.name == each.name);
                    (field(&each.label, sourced(flag(has, has.then_some(each.name.as_str())), read)))
                }
            }
        }
    }
}

/// A yes or no as a small pill: green-tinted for yes, neutral for no,
/// since a build without a trait is not wrong. A yes links to the builds
/// that share it when `term` searches them.
fn flag(on: bool, term: Option<&str>) -> Markup {
    let pill = "inline-block min-w-10 rounded-full px-2 text-center text-meta leading-[1.375rem] font-semibold";
    let (word, color) = if on {
        ("Yes", "bg-success/15 text-success")
    } else {
        ("No", "bg-muted/12 text-muted")
    };
    html! {
        @match term {
            Some(term) => a href=(super::search_href(term)) title="Find builds that share it"
                class={ (pill) " " (color) " hover:no-underline" } { (word) },
            None => span class={ (pill) " " (color) } { (word) },
        }
    }
}

/// A date such as `2022-07-01`, or a time starting with one, as `July 1,
/// 2022`; anything else as it is.
fn long_date(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = date.get(..10).unwrap_or(date).split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return date.to_owned();
    };
    let month = month
        .parse::<usize>()
        .ok()
        .and_then(|m| MONTHS.get(m.checked_sub(1)?));
    match (month, day.parse::<u32>()) {
        (Some(month), Ok(day)) => format!("{month} {day}, {year}"),
        _ => date.to_owned(),
    }
}

/// Who built it and when, and its file's history, folded away.
fn credits(s: &Schematic) -> Markup {
    html! {
        section aria-labelledby="credits-heading" {
            h2 id="credits-heading" class={ (SECTION_TITLE) " mb-2" } { "Credits" }
            dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                (field(&plural("Builder", &s.builders), if s.builders.is_empty() {
                    html! { span class="text-muted" { "unknown" } }
                } else {
                    name_badges(&s.builders)
                }))
                @if !s.contributors.is_empty() {
                    (field(&plural("Contributor", &s.contributors), name_badges(&s.contributors)))
                }
                @if let Some(built) = &s.built_on {
                    (field("Built", long_date(built)))
                }
                // An upload names its account, by its Minecraft name if it
                // has one; an import names whoever ran it.
                @match (&s.uploader, &s.uploader_name) {
                    (Some(username), Some(name)) => {
                        (field("Uploaded by", person_badge(name, &format!("/users/{username}"))))
                    }
                    _ => @if let Some(name) = &s.imported_by {
                        (field("Imported by", person_badge(name, &super::search_href(&format!("importer:{name}")))))
                    },
                }
            }
            details class="mt-3" {
                summary class="cursor-pointer text-meta text-muted" { "File" }
                dl class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                    (field("Name", html! { code class="break-all" { (s.filename()) } }))
                    (field("Uploaded", long_date(&s.added_at)))
                    @if let Some(edited) = &s.edited_at {
                        (field("Last edited", long_date(edited)))
                    }
                    (field("Downloads", s.downloads.to_string()))
                    (field("Conventions", if s.conventional {
                        html! { "followed" }
                    } else {
                        search_link("not followed", "unconventional")
                    }))
                }
            }
        }
    }
}
