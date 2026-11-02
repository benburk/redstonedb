//! A build's page: a verdict under its title, its key facts and specs
//! beside its viewer, each marked where the simulator or the file checked
//! it, then its simulation, inputs and outputs, stacking, and versions.

use maud::{Markup, html};

use crate::{
    import::{self, Analysis},
    models::{
        catalogue::Measure,
        schematic::{Logic, Schematic, StackDirection},
        simulation::Status,
        user::User,
        version::Version,
    },
    recognize::{category, interface::Interface},
    simulate::{Report, Timing},
    web::schematics::Colours,
};

use super::{
    CHOICE, PAGE_TITLE, SECTION_TITLE, SWITCH, SWITCHES, base,
    beside_viewer_under, field, grouped,
    icons::{self, icon},
    interface::{Inversions, interface_section},
    name_badges, percent, person_badge, plural, search_link,
    simulation::{Waiting, cases_pending, simulation_section},
    size, stack_guide, version,
};

/// What a build page's "Verified specs" means, on hover.
const VERIFIED_NOTE: &str = "Each spec here, and each key fact above, comes from simulating \
    the build or reading its file, not from what its uploader typed. Each new version is measured again.";

const UNVERIFIED_NOTE: &str = "The site couldn't verify this build as its category, so its \
    specs are as its uploader gave them, unchecked.";

/// What a stored build's file says about it.
pub struct FileRead {
    pub analysis: Analysis,
    /// How many of each kind of block it has, most first.
    pub block_counts: Vec<(&'static str, usize)>,
    /// For a build off the list as an adder or a register, which the check
    /// by function recognizes, each fault that keeps it from being one.
    pub unrecognized: Vec<String>,
    /// Whether it takes a signal strength as an input, which keeps it
    /// from being recognized.
    pub strength_inputs: bool,
    /// How its stacking looks, each way it stacks.
    pub stacking: Vec<crate::schem::stack::Review>,
}

/// What a build's page shows: the build, the builds like it, its file as read,
/// its simulation, its versions, and whether the viewer has favourited it.
pub struct BuildPage<'a> {
    pub s: &'a Schematic,
    pub similar: &'a [(Schematic, u32)],
    pub file: &'a Result<FileRead, String>,
    pub simulation: &'a Status,
    pub versions: &'a [Version],
    pub favorited: bool,
    pub colours: ColoursShown,
}

/// The colours a build's page shows it in, and which others it offers:
/// its builder's, when kept, and the viewer's theme, when they have one
/// and the build can take it.
#[derive(Clone, Copy)]
pub struct ColoursShown {
    pub chosen: Colours,
    pub original: bool,
    pub theme: bool,
    /// Whether the build can take a theme at all, as one the site knows
    /// the needs of: else it is shown and downloaded only in its own.
    pub themeable: bool,
}

/// One schematic's page, with an edit button for a mod.
pub fn schematic_page(viewer: Option<&User>, page: &BuildPage) -> Markup {
    let BuildPage {
        s,
        similar,
        file,
        simulation,
        versions,
        favorited,
        colours,
    } = *page;
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
                    @if !s.public {
                        (super::private_badge())
                    }
                    (build_banner(s, report, simulating))
                }
                // On a phone, under the title, the download as wide as the
                // page with the menu at its end; beside the title once
                // there is room.
                div class="ml-auto flex w-full items-center gap-2 sm:w-auto" {
                    button type="button" commandfor="download" command="show-modal" class="btn btn-primary flex-1 sm:flex-none" { "Download" }
                    (download_dialog(s, viewer.is_some(), colours))
                    @if viewer.is_some() {
                        (favorite_button(&path, favorited))
                    }
                    (actions_menu(s, viewer, &path))
                }
            }
            (beside_viewer_under(
                &format!("{path}/download?preview=yes&colours={}", colours.chosen.name()),
                &colours_switch(&path, colours),
                &build_info(s, similar, file, report),
                false,
            ))
            @let poll = format!("{path}/simulation");
            // Unverified or off the list, a build the simulator can't run
            // says why under its reasons already.
            @let skipped_off_list = crate::models::simulation::stands_in(s).is_some()
                && matches!(report, Some(Report::Skipped { .. }));
            @match simulation {
                Status::Done(_) | Status::Pending(Some(_)) if skipped_off_list => {}
                Status::Done(report) | Status::Pending(Some(report)) => {
                    (simulation_section(report, waiting(s, &poll)))
                }
                Status::Pending(None) => (cases_pending(&poll)),
                Status::Untested => {}
            }
            @if let Ok(file) = file {
                (stack_guide::stacking_section(s, &file.stacking, stacking_found(s)))
            }
            (version::versions_section(s, versions))
        },
    )
}

/// Why build `s`, unverified or filed off the list as one of category
/// `meant`, isn't one, one reason to a line: why it was found unverified,
/// then each fault the check by function finds on its file.
fn not_recognized(
    s: &Schematic,
    meant: &str,
    file: &Result<FileRead, String>,
) -> Markup {
    let label = meant.replace('_', " ");
    let strength = file.as_ref().is_ok_and(|file| file.strength_inputs);
    let faults = file
        .as_ref()
        .map(|file| file.unrecognized.as_slice())
        .unwrap_or_default();
    let mut reasons: Vec<String> = s.unverified.iter().cloned().collect();
    let strength = strength.then(|| crate::details::WITHOUT_LEVERS.to_owned());
    // Each said once, whichever says it first, however it is put.
    let bare = |reason: &str| {
        let reason = reason.trim();
        reason
            .strip_prefix("Test couldn't run: ")
            .unwrap_or(reason)
            .trim_end_matches('.')
            .to_owned()
    };
    for reason in strength.iter().chain(faults) {
        if !reasons.iter().any(|said| bare(said) == bare(reason)) {
            reasons.push(bare(reason));
        }
    }
    if reasons.is_empty() {
        reasons.push(format!("Fails {label} test or conventions"));
    }
    html! {
        section id="unverified" aria-labelledby="unrecognized-heading" class="scroll-mt-4" {
            h2 id="unrecognized-heading" class={ (SECTION_TITLE) " mb-2" } {
                (if s.non_standard() { "Non-standard" } else { "Failed verification" })
            }
            ul class="flex list-disc flex-col gap-3 pl-5" {
                @for reason in &reasons {
                    (super::reason_item(reason))
                }
            }
        }
    }
}

/// How build `s`'s page waits on its timing, asked of `poll`: never, for
/// one unverified or off the list, which is only tested.
pub fn waiting<'a>(s: &Schematic, poll: &'a str) -> Waiting<'a> {
    if crate::models::simulation::stands_in(s).is_some() {
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
        // An unverified build's reasons are a section of their own.
        _ if s.unverified.is_some() => None,
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
            @if s.unverified.is_some() {
                @let (label, tone) = if s.non_standard() {
                    ("Non-standard", "bg-restriction text-on-restriction")
                } else {
                    ("Fails verification", "bg-danger/12 text-danger")
                };
                a href="#unverified"
                    class={ "inline-block rounded-full px-2.5 text-meta leading-6 font-semibold hover:no-underline " (tone) } {
                    (label)
                }
            }
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
    /// Latency and whether it is synchronized, measured together.
    timing: Source,
    /// A monostable's pulse and whether it is inverted, measured with its
    /// timing.
    pulse: Source,
    stacking: Source,
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
        let stacking = if stacking_found(s)
            && run.is_some_and(|run| run.stacking.is_some())
        {
            Source::Simulated
        } else {
            Source::Given
        };
        let category_source = if report.is_some_and(Report::failed) {
            Source::Unconfirmed("fails simulation")
        } else if crate::models::simulation::stands_in(s).is_some() {
            // Passing or not, it is unverified until recognized.
            Source::Unconfirmed("not recognized")
        } else {
            checked(false, passed)
        };
        if passed
            && matches!(category_source, Source::Simulated)
            && s.evaluated()
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
            timing: source,
            pulse: source,
            stacking: source,
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
                " " span class="text-meta text-danger" { (why) }
            },
        }
    }
}

/// A build's tags, facts, and the builds like it, beside its viewer: a
/// live panel, redrawn when its simulation comes in, since that checks and
/// measures some of its facts.
pub fn build_info(
    s: &Schematic,
    similar: &[(Schematic, u32)],
    file: &Result<FileRead, String>,
    report: Option<&Report>,
) -> Markup {
    let interface = file.as_ref().ok().map(|file| &file.analysis.interface);
    let sources = Sources::of(s, interface, report);
    let flag_names: Vec<String> =
        s.flags.iter().map(|flag| flag.name.clone()).collect();
    html! {
        div id="build-info" fx-action={ "/schematics/" (s.id) "?panel=info" } fx-trigger="redraw"
            class="flex flex-col gap-8" {
            (facts(s, &sources))
            // Why it isn't verified comes first, as it qualifies the rest.
            @if let Some(meant) = crate::models::simulation::stands_in(s) {
                (not_recognized(s, meant, file))
            }
            // A failing build says so beside its title, not among its tags;
            // its design, orientation, and flags are among them.
            @let tags: Vec<_> = s.every_tag().into_iter().filter(|tag| !tag.is_unverified()).collect();
            @if !tags.is_empty() {
                ul aria-label="Tags" class="flex flex-wrap gap-1.5" {
                    @for tag in &tags {
                        li { (super::badge(tag, s)) }
                    }
                }
            }
            (specs(s, &sources, report))
            (credits(s))
            @match file {
                _ if !s.conventional => {
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
                    div class="rich max-w-prose leading-prose break-words" { (super::description::render(description)) }
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
fn block_counts(counts: &[(&str, usize)]) -> Markup {
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
pub(super) fn hover_note(id: &str, content: &Markup, note: &str) -> Markup {
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
fn specs(s: &Schematic, sources: &Sources, report: Option<&Report>) -> Markup {
    // An adder's design, among its tags, has groups as its run found.
    let group = match report {
        Some(Report::Ran(run)) => run.design.as_ref().and_then(|d| d.group),
        _ => None,
    };
    html! {
        section aria-labelledby="specs-heading" {
            h2 id="specs-heading" class={ (SECTION_TITLE) " mb-2" } {
                @if sources.verified {
                    (hover_note("specs-note", &html! {
                        span class="underline decoration-boundary decoration-dotted underline-offset-4" { "Verified specs" }
                    }, VERIFIED_NOTE))
                } @else if s.unverified.is_some() {
                    (hover_note("specs-note", &html! {
                        span class="underline decoration-boundary decoration-dotted underline-offset-4" { "Unverified specs" }
                    }, UNVERIFIED_NOTE))
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
                @if let Some(group) = group {
                    (field("Groups", html! { (group) " bits" }))
                }
                // A serial build's throughput is among its measures.
                @if !s.has(Measure::Throughput) && let Some(t) = s.throughput {
                    (field("Throughput", sourced(search_link(&Measure::Throughput.words(t), &format!("throughput:{t}")), sources.throughput, sources)))
                }
                @if let Some(synced) = s.synchronized {
                    (field("Synced", sourced(flag(synced, Some(if synced { "synced" } else { "unsynced" })), sources.timing, sources)))
                }
                // Whether it stacks is among its tags; how, here.
                @if !s.stacking.is_empty() {
                    (field("Stacks", sourced(html! {
                        a href="#stacking" { (s.stacking.iter().map(short_stacking).collect::<Vec<_>>().join(" · ")) }
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
                @if let Some(logic) = s.logic {
                    (field("Stateless", sourced(flag(logic == Logic::Stateless, Some(logic.name())), sources.logic, sources)))
                }
                @if s.pipelined() {
                    (field("Pipelined", sourced(flag(true, Some("pipelined")), sources.throughput, sources)))
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

/// Whether the simulation decides how build `s` stacks, rather than its
/// builder giving it: a conventional build of a category whose stacking
/// it infers.
fn stacking_found(s: &Schematic) -> bool {
    s.conventional
        && category::behaviour(&s.category).stacking
            == category::Stacking::Inferred
}

/// What else the viewer may do with build `s`, at `path`, under one button:
/// edit it, upload a new version, or delete it. Nothing for a viewer who
/// may do none of them.
/// A choice of the colours the build at `path` shows in: the site's own,
/// its builder's when kept, and the viewer's theme when they have one;
/// nothing when the site's are the only ones.
fn colours_switch(path: &str, shown: ColoursShown) -> Markup {
    let options = [
        (Colours::Default, "Default", true),
        (Colours::Original, "Original", shown.original),
        (Colours::Theme, "Your theme", shown.theme),
    ];
    html! {
        @if shown.original || shown.theme {
            nav aria-label="Colours" class="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1" {
                span class="text-meta font-semibold text-muted" aria-hidden="true" { "Colours" }
                div class=(SWITCHES) {
                    @for (colours, label, offered) in options {
                        @if offered {
                            a href={ (path) "?colours=" (colours.name()) } class=(SWITCH)
                                aria-current=[(colours == shown.chosen).then_some("page")] { (label) }
                        }
                    }
                }
            }
        }
    }
}

/// A star that favourites the build at `path`, filled once it is, and
/// pressed again takes it back. Through fixi the answer is the star alone,
/// so the rest of the page, its viewer too, stays as it is.
pub fn favorite_button(path: &str, favorited: bool) -> Markup {
    let label = if favorited {
        "Remove from favourites"
    } else {
        "Add to favourites"
    };
    html! {
        form action={ (path) "/favorite" } method="post" class="contents"
            fx-action={ (path) "/favorite" } fx-method="post" {
            @if !favorited {
                input type="hidden" name="on" value="yes";
            }
            button id="favorite" type="submit" aria-label=(label) title=(label) aria-pressed=(favorited)
                class="btn btn-quiet flex size-10 cursor-pointer items-center justify-center p-0" {
                (icon(icons::STAR, if favorited { "size-5 fill-current text-brand" } else { "size-5" }))
            }
        }
    }
}

fn actions_menu(s: &Schematic, viewer: Option<&User>, path: &str) -> Markup {
    let edits = viewer.is_some_and(|v| v.may_edit(s));
    let deletes = viewer.is_some_and(User::can_delete);
    let item = "flex min-h-9 w-full items-center gap-2.5 rounded-control px-2.5 text-left hover:bg-selection hover:no-underline";
    html! {
        @if edits || deletes {
            details class="relative" data-menu {
                summary aria-label="More actions" title="More actions"
                    class="btn btn-quiet flex size-10 cursor-pointer list-none items-center justify-center p-0 [&::-webkit-details-marker]:hidden" {
                    (icon(icons::MORE, "size-5"))
                }
                div class="absolute top-full right-0 z-20 mt-1 flex w-max min-w-44 flex-col rounded-overlay border border-line bg-raised p-1 shadow-overlay" {
                    @if edits {
                        a href={ (path) "/edit" } class={ (item) " text-ink" } { (icon(icons::PENCIL, "size-4 text-muted")) "Edit details" }
                        a href={ (path) "/versions/new" } class={ (item) " text-ink" } { (icon(icons::UPLOAD, "size-4 text-muted")) "Upload a new version" }
                    }
                    @if deletes {
                        button type="button" commandfor="delete" command="show-modal"
                            class={ (item) " cursor-pointer text-danger" } {
                            (icon(icons::TRASH, "size-4")) "Delete"
                        }
                    }
                }
            }
            @if deletes {
                (delete_dialog(s))
            }
        }
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

/// The choices for downloading a build: the file's name; its colours, the
/// page's own first, or a palette made for this download alone, in an
/// editor custom asks the server for, with the fields so far, so choosing
/// it again keeps them; copies of a stackable one, or one copy with glass
/// at two corners of the box `//rstack` stacks, for `WorldEdit` to select
/// it by; and, for someone `signed_in`, a button that sends the file so
/// made to their Schemati instead.
fn download_dialog(
    s: &Schematic,
    signed_in: bool,
    colours: ColoursShown,
) -> Markup {
    let path = format!("/schematics/{}", s.id);
    let filename = s.filename();
    let stem = filename.trim_end_matches(".schem");
    let options = [
        (Colours::Default, "Default", true),
        (Colours::Original, "Original", colours.original),
        (Colours::Theme, "Your theme", colours.theme),
        (Colours::Custom, "Custom", colours.themeable),
    ];
    html! {
        dialog id="download" aria-labelledby="download-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action={ (path) "/download" } method="get" class="flex flex-col gap-4" {
                h2 id="download-heading" class=(SECTION_TITLE) { "Download" }
                label class="field" {
                    span { "File name" }
                    div class="flex items-center gap-2" {
                        input name="name" value=(stem) required maxlength="100"
                            autocomplete="off" spellcheck="false" data-caret-end class="input flex-1";
                        span aria-hidden="true" class="text-muted" { ".schem" }
                    }
                }
                // Default alone is no choice, as for a build that takes no
                // theme and keeps no other colours.
                @if colours.original || colours.themeable {
                fieldset class="field" {
                    legend class="mb-1.5 text-meta font-semibold text-muted" { "Colours" }
                    // The page's colours first; custom asks for its editor.
                    div class={ (SWITCHES) " self-start" } {
                        @for (choice, label, offered) in options {
                            @if offered {
                                label class=(CHOICE) {
                                    @if choice == Colours::Custom {
                                        input type="radio" name="colours" value=(choice.name()) class="sr-only"
                                            fx-action="/palette/custom" fx-trigger="click" fx-target="#custom-colours";
                                    } @else {
                                        input type="radio" name="colours" value=(choice.name())
                                            checked[choice == colours.chosen] class="sr-only";
                                    }
                                    (label)
                                }
                            }
                        }
                    }
                    @if colours.original && !s.stacking.is_empty() {
                        span class="message text-meta text-muted" {
                            "Original colours come on one copy; copies stacked come in Default."
                        }
                    }
                }
                }
                // Where custom colours' editor comes in.
                div id="custom-colours" hidden {}
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
                // One box marked, on one copy, as the server takes them.
                @if !s.stacking.is_empty() {
                    fieldset class="flex flex-col gap-1.5" {
                        legend class="mb-1.5 text-meta font-semibold text-muted" {
                            "Glass at the corners of the box to //rstack, to select and then break; one copy only"
                        }
                        label class="flex items-center gap-2" {
                            input type="radio" name="markers" value="" checked class="size-4 appearance-auto";
                            span { "None" }
                        }
                        @for direction in &s.stacking {
                            label class="flex items-center gap-2" {
                                input type="radio" name="markers" value=(direction.name()) class="size-4 appearance-auto";
                                span { "To stack " (direction.name().replace('-', " and ")) }
                            }
                        }
                    }
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
