//! A build's page: a verdict under its title, its key facts and specs
//! beside its viewer, each marked where the simulator or the file checked
//! it, then its simulation, inputs and outputs, stacking, and versions.

use maud::{Markup, html};

use crate::{
    category,
    import::{self, Analysis},
    interface::Interface,
    models::{
        catalogue::{Flag, Measure},
        schematic::{Logic, Schematic, StackDirection},
        simulation::Status,
        tag,
        user::User,
        version::Version,
    },
    simulate::{Report, Stacking, Test, Timing},
};

use super::{
    PAGE_TITLE, SECTION_TITLE, base, beside_viewer, field, grouped,
    interface::{interface_section, violations_section},
    name_badges, percent, person_badge, plural, search_link,
    simulation::{Waiting, cases_pending, simulation_section},
    size, stack_guide, version,
};

/// What a build page's "Verified specs" means, on hover.
const VERIFIED_NOTE: &str = "Each spec here, and each key fact above, comes from simulating \
    the build or reading its file, not from what its uploader typed. Each new version is measured again.";

/// What a stored build's file says about it.
pub struct FileRead {
    pub analysis: Analysis,
    /// For a build off the list as an adder or a register, which the check
    /// by function recognizes, each fault that keeps it from being one.
    pub unrecognized: Vec<String>,
    /// Whether it takes a signal strength as an input, which keeps it
    /// from being recognized.
    pub strength_inputs: bool,
    /// How its stacking looks, each way it stacks.
    pub stacking: Vec<crate::stack::Review>,
}

/// One schematic's page, with an edit button for a mod.
pub fn schematic_page(
    viewer: Option<&User>,
    s: &Schematic,
    flags: &[Flag],
    similar: &[(Schematic, u32)],
    file: &Result<FileRead, String>,
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
                    @if viewer.is_some_and(User::can_delete) {
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
            (beside_viewer(&format!("{path}/download?preview=yes"), &build_info(s, flags, similar, file, report)))
            @if let Some(meant) = crate::models::simulation::meant_by(&s.category) {
                (not_recognized(meant, file))
            }
            @let poll = format!("{path}/simulation");
            @match simulation {
                Status::Done(report) | Status::Pending(Some(report)) => {
                    (simulation_section(report, waiting(s, &poll)))
                }
                Status::Pending(None) => (cases_pending(&poll)),
                Status::Untested => {}
            }
            @if let Ok(file) = file {
                (violations_section(&file.analysis))
                (stack_guide::stacking_section(s, &file.stacking, stacking_found(s, report)))
            }
            (version::versions_section(s, versions))
        },
    )
}

/// Why a build filed off the list as one of category `meant` isn't one:
/// each fault the check by function finds, one to a line.
fn not_recognized(meant: &str, file: &Result<FileRead, String>) -> Markup {
    let label = meant.replace('_', " ");
    let strength = file.as_ref().is_ok_and(|file| file.strength_inputs);
    let faults = file
        .as_ref()
        .map(|file| file.unrecognized.as_slice())
        .unwrap_or_default();
    html! {
        section aria-labelledby="unrecognized-heading" class="mt-12 max-w-prose" {
            h2 id="unrecognized-heading" class={ (SECTION_TITLE) " mb-2" } { "Failed verification" }
            ul class="flex list-disc flex-col gap-1.5 pl-5" {
                @if strength {
                    li { "Signal-strength inputs" }
                }
                @for fault in faults {
                    li { (fault) }
                }
                @if !strength && faults.is_empty() {
                    li { "Fails " (label) " test or conventions" }
                }
            }
        }
    }
}

/// How build `s`'s page waits on its timing, asked of `poll`: never, for
/// one off the list, which is only tested.
pub fn waiting<'a>(s: &Schematic, poll: &'a str) -> Waiting<'a> {
    if crate::models::simulation::meant_by(&s.category).is_some() {
        Waiting::Untimed
    } else {
        Waiting::Polled(poll)
    }
}

/// Beside a build's title, what its simulation says, as a badge, unless it
/// passes: fails, still running, or not simulated, with the counts or the
/// reason in a popup on hover or focus. A live panel, redrawn when its
/// simulation comes in.
pub fn build_banner(
    s: &Schematic,
    report: Option<&Report>,
    simulating: bool,
) -> Markup {
    let verdict = match report {
        // A pass needs no badge: it is what puts a build on the list.
        Some(Report::Ran(run)) if run.wrong == 0 => None,
        Some(Report::Ran(run)) => Some((
            "Fails simulation",
            "bg-danger/12 text-danger",
            format!(
                "{} of {} cases of {} wrong; see which below.",
                grouped(run.wrong),
                grouped(run.cases),
                run.pattern
            ),
        )),
        Some(Report::Broken { reason }) => Some((
            "Fails simulation",
            "bg-danger/12 text-danger",
            reason.clone(),
        )),
        Some(Report::Skipped { reason }) => {
            Some(("Not simulated", "bg-muted/12 text-muted", reason.clone()))
        }
        None if simulating => Some((
            "Simulating…",
            "bg-muted/12 text-muted",
            "Its cases are running; this updates when they finish.".to_owned(),
        )),
        None => Some((
            "Not simulated",
            "bg-muted/12 text-muted",
            "The simulator has no test for this category yet.".to_owned(),
        )),
    };
    // Kept when empty, as the page redraws it when a simulation finishes.
    html! {
        div id="build-banner" fx-action={ "/schematics/" (s.id) "?panel=banner" } fx-trigger="redraw" {
            @if let Some((label, tone, note)) = verdict {
                @let pill = html! {
                    span class={ "inline-block rounded-full px-2.5 text-meta leading-6 font-semibold " (tone) } { (label) }
                };
                (hover_note("verdict-note", &pill, &note))
            }
        }
    }
}

/// Where a spec on a build's page comes from, so a reader can tell what the
/// site checked from what its uploader typed.
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
    /// A monostable's pulse and whether it is inverted, measured with its
    /// timing.
    pulse: Source,
    stacking: Source,
    design: Source,
    throughput: Source,
    /// Whether it holds state.
    logic: Source,
    /// Whether every spec was measured or read from the file, as for a
    /// recognized build of a category whose every detail `evaluate` works
    /// out, once it passes: the page then says so once, not spec by spec.
    verified: bool,
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
            && category::behaviour(&s.category).stacking
                == category::Stacking::Found
            && run.is_some_and(|run| {
                matches!(run.stacking, Some(Stacking::Not { .. }))
            })
        {
            Source::Unconfirmed("not found by simulation")
        } else {
            Source::Given
        };
        let category_source = if report.is_some_and(Report::failed) {
            Source::Unconfirmed("fails simulation")
        } else if crate::models::simulation::meant_by(&s.category).is_some() {
            // Passing or not, it is off the list until recognized.
            Source::Unconfirmed("not recognized")
        } else {
            checked(false, passed)
        };
        if passed
            && matches!(category_source, Source::Simulated)
            && category::behaviour(&s.category).evaluated
        {
            return Sources::every(Source::Simulated);
        }
        Sources {
            category: category_source,
            width: checked(
                derived.width == s.width && s.width.is_some(),
                passed,
            ),
            orientation: checked(
                derived.orientation == Some(s.orientation),
                false,
            ),
            encoding: checked(
                derived.encoding.is_some() && derived.encoding == s.encoding,
                false,
            ),
            timing: checked(
                false,
                report
                    .and_then(Report::timing)
                    .and_then(Timing::latency)
                    .is_some(),
            ),
            pulse: checked(
                false,
                report
                    .and_then(Report::timing)
                    .is_some_and(|timing| timing.pulse.is_some()),
            ),
            stacking,
            design: Source::Given,
            throughput: Source::Given,
            logic: Source::Given,
            verified: false,
        }
    }

    /// Whether the site checked any spec on the page.
    fn some_checked(&self) -> bool {
        [
            self.category,
            self.width,
            self.orientation,
            self.encoding,
            self.timing,
            self.pulse,
            self.stacking,
        ]
        .iter()
        .any(|source| matches!(source, Source::Simulated | Source::File))
    }

    /// Every spec from `source`, all verified.
    fn every(source: Source) -> Self {
        Sources {
            category: source,
            width: source,
            orientation: source,
            encoding: source,
            timing: source,
            pulse: source,
            stacking: source,
            design: source,
            throughput: source,
            logic: source,
            verified: true,
        }
    }
}

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
                " " span class="text-meta text-muted" { "unverified" }
            },
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
    file: &Result<FileRead, String>,
    report: Option<&Report>,
) -> Markup {
    let interface = file.as_ref().ok().map(|file| &file.analysis.interface);
    let sources = Sources::of(s, interface, report);
    html! {
        div id="build-info" fx-action={ "/schematics/" (s.id) "?panel=info" } fx-trigger="redraw"
            class="flex flex-col gap-8" {
            (facts(s, &sources))
            // A failing build says so beside its title, not among its tags.
            @let tags: Vec<_> = s.tags.iter().filter(|tag| tag.name != tag::FAILS_SIMULATION).collect();
            @if !tags.is_empty() {
                ul aria-label="Tags" class="flex flex-wrap gap-1.5" {
                    @for tag in tags {
                        li { (super::badge(tag)) }
                    }
                }
            }
            (specs(s, flags, &sources))
            (credits(s))
            @match file {
                _ if !s.conventional => {
                    p class="text-meta text-muted" {
                        "Doesn't follow the building conventions, so its inputs and outputs are not read and it is not linted."
                    }
                }
                Ok(file) => (interface_section(&file.analysis)),
                Err(err) => {
                    p class="text-meta text-muted" { "Could not read its inputs and outputs: " (err) }
                }
            }
            @if let Some(description) = &s.description {
                section aria-labelledby="description-heading" {
                    h2 id="description-heading" class={ (SECTION_TITLE) " mb-2" } { "Description" }
                    div class="rich max-w-prose leading-prose break-words" { (super::description::render(description)) }
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
                    @let source = match measure {
                        Measure::Width => sources.width,
                        Measure::Pulse => sources.pulse,
                        _ => Source::Given,
                    };
                    (fact(measure.label(), measure.of(s).map_or_else(unknown, |n| {
                        sourced(search_link(&measure.words(n), &measure.term(n)), source, sources)
                    })))
                }
                (fact("Latency", s.latency.map_or_else(unknown, |t| {
                    sourced(super::ticks(t), sources.timing, sources)
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
                    ), Source::File, sources)))
                }
            }
        }
    }
}

/// `content` with `note` in a small popup above it while it is hovered or
/// focused, and as its description for a screen reader. `id` names the
/// note, once on the page. Hidden, it takes no room, and on a phone it
/// shows along the foot of the screen, so one near the edge never widens
/// the page.
fn hover_note(id: &str, content: &Markup, note: &str) -> Markup {
    html! {
        span tabindex="0" aria-describedby=(id)
            class="group relative inline-block cursor-help rounded-control focus-visible:outline-2 focus-visible:outline-boundary" {
            (content)
            span id=(id) role="tooltip"
                class="pointer-events-none absolute bottom-full left-0 z-20 mb-1.5 hidden w-max max-w-xs max-desktop:fixed max-desktop:inset-x-4 max-desktop:top-auto max-desktop:bottom-4 max-desktop:mb-0 max-desktop:w-auto max-desktop:max-w-none rounded-control bg-ink px-2.5 py-1.5 text-meta font-normal text-canvas shadow-overlay transition-[opacity,display] transition-discrete duration-150 ease-calm group-hover:block group-focus-within:block starting:opacity-0 motion-reduce:transition-none" {
                (note)
            }
        }
    }
}

/// One way a build stacks, short, such as `2 wide west, up to 7` or `2
/// west and 2 up, any number`.
fn short_stacking(direction: &StackDirection) -> String {
    let steps: Vec<(i64, &str)> = [
        (direction.west, "west"),
        (direction.up, "up"),
        (direction.north, "north"),
    ]
    .into_iter()
    .filter(|&(n, _)| n > 0)
    .collect();
    let step = if let [(n, way)] = steps.as_slice() {
        format!("{n} wide {way}")
    } else {
        let each: Vec<String> =
            steps.iter().map(|(n, way)| format!("{n} {way}")).collect();
        each.join(" and ")
    };
    match direction.max_copies {
        Some(n) => format!("{step}, up to {n}"),
        None => format!("{step}, any number"),
    }
}

/// What a build is and how it behaves, beyond its key facts.
fn specs(s: &Schematic, flags: &[Flag], sources: &Sources) -> Markup {
    html! {
        section aria-labelledby="specs-heading" {
            h2 id="specs-heading" class={ (SECTION_TITLE) " mb-2" } {
                @if sources.verified {
                    (hover_note("specs-note", &html! {
                        span class="underline decoration-boundary decoration-dotted underline-offset-4" { "Verified specs" }
                    }, VERIFIED_NOTE))
                } @else {
                    "Specs"
                }
            }
            dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-1.5" {
                // A category off the list has no list of its own to link to.
                (field("Category", sourced(html! {
                    @if s.category_label.is_some() {
                        a href={ "/?category=" (s.category) } { (s.category_label()) }
                    } @else {
                        (s.category_label())
                    }
                }, sources.category, sources)))
                @if let Some(design) = s.design_label() {
                    (field("Design", sourced(search_link(&design, &s.design), sources.design, sources)))
                }
                (field("Orientation", sourced(search_link(s.orientation.label(), &s.orientation.label().to_lowercase()), sources.orientation, sources)))
                @if let Some(encoding) = s.encoding {
                    (field("Encoding", sourced(search_link(encoding.label(), &format!("encoding:{}", encoding.name())), sources.encoding, sources)))
                }
                // A serial build's throughput is among its measures.
                @if !s.has(Measure::Throughput) && let Some(t) = s.throughput {
                    (field("Throughput", sourced(search_link(&Measure::Throughput.words(t), &format!("throughput:{t}")), sources.throughput, sources)))
                }
                @let stacks = if s.stacking.is_empty() {
                    flag(false, None)
                } else {
                    html! {
                        a href="#stacking" { (s.stacking.iter().map(short_stacking).collect::<Vec<_>>().join(" · ")) }
                    }
                };
                (field("Stacks", sourced(stacks, sources.stacking, sources)))
                @if let Some(synced) = s.synchronized {
                    (field("Synced", sourced(flag(synced, Some(if synced { "sync" } else { "unsync" })), sources.timing, sources)))
                }
                @if let Some(logic) = s.logic {
                    (field("Stateless", sourced(flag(logic == Logic::Stateless, Some(logic.name())), sources.logic, sources)))
                }
                @if s.pipelined() {
                    (field("Pipelined", sourced(flag(true, Some("pipelined")), sources.throughput, sources)))
                }
                // A category such as adder reads its flags from the file,
                // a monostable's Inverted comes with its pulse, and a
                // comparator's Signed with its cases.
                @let read = if s.conventional && category::behaviour(&s.category).flags_from_file {
                    Source::File
                } else if s.has(Measure::Pulse) {
                    sources.pulse
                } else if category::behaviour(&s.category).test == Some(Test::GreaterThan) {
                    sources.category
                } else {
                    Source::Given
                };
                @for each in flags {
                    @let has = s.flags.iter().any(|has| has.name == each.name);
                    (field(&each.label, sourced(flag(has, has.then_some(each.name.as_str())), read, sources)))
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

/// Whether the simulation found how build `s` stacks, rather than its
/// builder giving it: an adder's always, and a register's when its copies
/// stored and read right.
fn stacking_found(s: &Schematic, report: Option<&Report>) -> bool {
    let found = report.is_some_and(|report| {
        matches!(report, Report::Ran(run) if matches!(
            run.stacking,
            Some(
                Stacking::Stacks { .. }
                    | Stacking::Registers { .. }
                    | Stacking::Wider { .. }
                    | Stacking::Separate { .. }
            )
        ))
    });
    s.conventional
        && match category::behaviour(&s.category).stacking {
            category::Stacking::Inferred => true,
            category::Stacking::Found => found,
            category::Stacking::Given | category::Stacking::Never => false,
        }
}

/// An admin's confirmation before deleting a build.
fn delete_dialog(s: &Schematic) -> Markup {
    html! {
        dialog id="delete" aria-labelledby="delete-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action={ "/schematics/" (s.id) "/delete" } method="post" class="flex flex-col gap-4" {
                h2 id="delete-heading" class=(SECTION_TITLE) { "Delete " (s.name()) "?" }
                p { "The build, its file, tags, and credits are removed for good." }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-danger" { "Delete" }
                    button type="button" commandfor="delete" command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}

/// The choices for downloading a build: copies of a stackable one, and
/// glass corner markers for `WorldEdit`; and, for someone `signed_in`, a
/// button that sends the file so made to their Schemati instead.
fn download_dialog(s: &Schematic, signed_in: bool) -> Markup {
    let path = format!("/schematics/{}", s.id);
    html! {
        dialog id="download" aria-labelledby="download-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action={ (path) "/download" } method="get" class="flex flex-col gap-4" {
                h2 id="download-heading" class=(SECTION_TITLE) { "Download" }
                @if !s.stacking.is_empty() {
                    div class="flex gap-2" {
                        @for direction in &s.stacking {
                            label class="field flex-1" {
                                span { "Copies " (direction.name().replace('-', " and ")) }
                                input type="number" name=(direction.name()) min="1"
                                    max=[direction.max_copies] value="1" required
                                    class="input tabular-nums";
                            }
                        }
                    }
                }
                label class="flex items-start gap-2" {
                    input type="checkbox" name="markers" value="yes" class="mt-1 size-4 appearance-auto";
                    span { "Add a glass block off each corner, for WorldEdit to select the build by" }
                }
                div class="flex flex-wrap items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Download" }
                    @if signed_in {
                        button type="submit" formaction={ (path) "/schemati" } formmethod="post"
                            fx-action={ (path) "/schemati" } fx-method="post" fx-target="#sent" fx-swap="innerHTML"
                            class="btn btn-quiet" {
                            "Send to Schemati"
                        }
                    }
                    button type="button" commandfor="download" command="close" class="btn btn-quiet" { "Cancel" }
                }
                @if signed_in {
                    div id="sent" aria-live="polite" class="text-meta empty:hidden" {}
                }
            }
        }
    }
}

/// The builds of its category a build is similar to, the most alike first.
fn similar_section(similar: &[(Schematic, u32)]) -> Markup {
    html! {
        section aria-labelledby="similar-heading" {
            h2 id="similar-heading" class={ (SECTION_TITLE) " mb-1" } { "Similar builds" }
            p class="mb-3 text-meta text-muted" {
                "Share of their redstone that lines up, turned or mirrored as need be."
            }
            ul class="flex flex-col gap-1.5" {
                @for (other, permille) in similar {
                    li class="flex items-baseline justify-between gap-4" {
                        span {
                            a href={ "/schematics/" (other.id) } { (other.title()) }
                            span class="text-muted" {
                                @if !other.builders.is_empty() { " by " (other.builders.join(", ")) }
                                ", " (size(other))
                            }
                        }
                        span class="tabular-nums text-muted whitespace-nowrap" { (percent(*permille)) }
                    }
                }
            }
        }
    }
}
