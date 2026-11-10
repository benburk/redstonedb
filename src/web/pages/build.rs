//! A build's page: a verdict under its title, its key facts and specs
//! beside its viewer, each marked where the simulator or the file checked
//! it, then its simulation, inputs and outputs, stacking, and versions.

use maud::{Markup, html};

use crate::{
    category::{self, Measure},
    import::{self, Analysis},
    models::{
        schematic::{Logic, Schematic, StackDirection},
        tag,
        user::User,
        version::Version,
    },
    recognize::{fault, interface::Interface},
    simulate::{Report, Timing},
    web::schematics::Colours,
};

use super::{
    CHOICE, PAGE_TITLE, SECTION_TITLE, SWITCH, SWITCHES, base_previewed,
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
    specs are as its uploader gave them, untested.";

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
    pub simulation: &'a Option<Report>,
    /// Whether storing it is still simulating it in the background, so
    /// what is stored is yet to come.
    pub simulating: bool,
    pub versions: &'a [Version],
    pub favorited: bool,
    pub colours: ColoursShown,
    /// The listing the viewer came to it from, such as a search, which
    /// its breadcrumbs link back to and deleting it returns to.
    pub from: Option<&'a str>,
}

/// The colours a build's page shows it in, and which others it offers:
/// its author's, when kept, and the viewer's theme, when they have one
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
        simulating,
        versions,
        favorited,
        colours,
        from,
    } = *page;
    let report = simulation.as_ref();
    let name = s.name();
    let path = format!("/schematics/{}", s.id);
    base_previewed(
        viewer,
        &name,
        &path,
        &html! {
            (crumbs(s, from))
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
                    @if viewer.is_some() {
                        button type="button" commandfor="download" command="show-modal" class="btn btn-primary flex-1 sm:flex-none" { "Download" }
                        (download_dialog(s, colours))
                    } @else {
                        button type="button" aria-disabled="true" title=(super::SIGN_IN_TO_DOWNLOAD) class="btn btn-primary flex-1 sm:flex-none" { "Download" }
                    }
                    @if viewer.is_some() {
                        (favorite_button(&path, favorited, s.favorites))
                    } @else if s.favorites > 0 {
                        @let label = super::favourites_label(s.favorites);
                        span class="flex h-10 items-center gap-1.5 px-2 text-muted tabular-nums" title=(label) aria-label=(label) {
                            (icon(icons::STAR, "size-5")) (s.favorites)
                        }
                    }
                    (actions_menu(s, viewer, &path, from))
                }
            }
            (beside_viewer_under(
                &format!("{path}/download?preview=yes&colours={}", colours.chosen.name()),
                &colours_switch(&path, colours, from),
                &build_info(s, similar, file, report),
                false,
            ))
            // Unverified or off the list, a build the simulator can't run
            // says why under its reasons already.
            @let skipped_off_list = crate::models::simulation::stands_in(s).is_some()
                && matches!(report, Some(Report::Skipped { .. }));
            @if simulating {
                (cases_pending(&format!("{path}/simulation")))
            } @else if let Some(report) = report.filter(|_| !skipped_off_list) {
                (simulation_section(report, Waiting::Stored))
            }
            @if let Ok(file) = file {
                (stack_guide::stacking_section(s, &file.stacking, stacking_found(s), viewer.is_some()))
            }
            (version::versions_section(s, versions, viewer, viewer.is_some_and(|user| user.may_restore(s))))
        },
        Some(&preview(s)),
    )
}

/// Why build `s`, unverified or filed off the list as one of category
/// `meant`, isn't one, one reason to a line: why it was found unverified,
/// then each fault the check by function finds on its file, every one that
/// gives cases wrong said once, as the simulation shows which.
fn not_recognized(
    s: &Schematic,
    meant: &str,
    file: &Result<FileRead, String>,
) -> Markup {
    let label = meant.replace('_', " ");
    // Only a kind that takes digits may take a signal strength.
    let strength = file.as_ref().is_ok_and(|file| file.strength_inputs)
        && !category::Kind::named(meant)
            .is_some_and(category::Kind::digit_inputs);
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
    // Every reason that gives cases wrong as one, as the simulation below
    // shows which, rather than a case or two from each test that found it.
    if let Some(first) = reasons.iter().position(|r| fault::wrong_outputs(r)) {
        reasons.retain(|reason| !fault::wrong_outputs(reason));
        reasons.insert(first, crate::details::WRONG_OUTPUTS.to_owned());
    }
    if reasons.is_empty() {
        reasons.push(format!("Fails {label} test or conventions"));
    }
    html! {
        section id="unverified" aria-labelledby="unrecognized-heading" class="scroll-mt-4" {
            h2 id="unrecognized-heading" class={ (SECTION_TITLE) " mb-2" } {
                (if s.untested() { "Why it's untested" } else { "Why it's failing" })
            }
            ul class="flex list-disc flex-col gap-3 pl-5" {
                @for reason in &reasons {
                    (super::reason_item(reason))
                }
            }
        }
    }
}

/// Beside a build's title, what its simulation says, as a badge, unless it
/// passes: fails, still simulating, as just after it is stored, or not
/// simulated, with the counts or the reason in a popup on hover or focus.
/// A live panel, redrawn when its simulation comes in.
pub fn build_banner(
    s: &Schematic,
    report: Option<&Report>,
    simulating: bool,
) -> Markup {
    let verdict = match report {
        _ if simulating => Some((
            "Simulating…",
            "bg-muted/12 text-muted",
            "Its cases and timing are running; this updates when they finish."
                .to_owned(),
        )),
        // An unverified build's reasons are a section of their own.
        _ if s.unverified.is_some() => None,
        // A pass needs no badge: it is what puts a build on the list.
        Some(Report::Ran(run)) if run.wrong == 0 => None,
        Some(Report::Ran(run)) => Some((
            "Failing",
            "bg-danger/12 text-danger",
            format!(
                "{} of {} cases of {} wrong; see which below.",
                grouped(run.wrong),
                grouped(run.cases),
                run.pattern
            ),
        )),
        Some(Report::Broken { reason }) => {
            Some(("Failing", "bg-danger/12 text-danger", reason.clone()))
        }
        Some(Report::Skipped { reason }) => {
            Some(("Untested", "bg-muted/12 text-muted", reason.clone()))
        }
        None => Some((
            "Untested",
            "bg-muted/12 text-muted",
            "No simulation is stored for it: the simulator has no test for \
             its category and width."
                .to_owned(),
        )),
    };
    // Kept when empty, as the page redraws it when a simulation finishes.
    html! {
        div id="build-banner" fx-action={ "/schematics/" (s.id) "?panel=banner" } fx-trigger="redraw" {
            @if s.unverified.is_some() {
                @let label = if s.untested() { "Untested" } else { "Failing" };
                @let tone = "bg-danger/12 text-danger";
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
            Source::Unconfirmed("failing")
        } else if crate::models::simulation::stands_in(s).is_some() {
            // Passing or not, it is untested as its category until
            // recognized.
            Source::Unconfirmed("untested")
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
    s: &Schematic,
    similar: &[(Schematic, u32)],
    file: &Result<FileRead, String>,
    report: Option<&Report>,
) -> Markup {
    let interface = file.as_ref().ok().map(|file| &file.analysis.interface);
    let sources = Sources::of(s, interface, report);
    let flag_names: Vec<String> =
        s.flags.iter().map(|flag| flag.name.to_owned()).collect();
    html! {
        div id="build-info" fx-action={ "/schematics/" (s.id) "?panel=info" } fx-trigger="redraw"
            class="flex flex-col gap-8" {
            (facts(s, &sources))
            // Why it isn't verified comes first, as it qualifies the rest.
            @if let Some(meant) = crate::models::simulation::stands_in(s) {
                (not_recognized(s, meant, file))
            }
            // A failing build says so beside its title, not among its tags;
            // its design, orientation, and flags are among them, but not one
            // another implies, as 4-wide stackable beside 2-wide stackable.
            @let every = s.every_tag();
            @let tags: Vec<_> = every.iter().filter(|tag| !tag.is_unverified() && !tag::implied(&tag.name, &every)).collect();
            @if !tags.is_empty() {
                ul aria-label="Tags" class="flex flex-wrap gap-1.5" {
                    @for tag in tags {
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

/// One way build `s` stacks, short: for copies that add bits, their step
/// and pitch class, such as `16 up, pitch 2, any number`; for a
/// register's copies across its bits, as more words, such as `4 west as
/// words, up to 3`; and for a stacking its author gave, its step, such
/// as `2 wide west, up to 7` or `2 west and 2 up, any number`.
fn short_stacking(s: &Schematic, direction: &StackDirection) -> String {
    let steps: Vec<(i64, &str)> = direction.parts().collect();
    let step = match (steps.as_slice(), direction.pitch) {
        ([(n, way)], Some(pitch)) => format!("{n} {way}, pitch {pitch}"),
        ([(n, way)], None)
            if stacking_found(s) && !direction.adds_bits(s.orientation) =>
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
                        span class="underline decoration-boundary decoration-dotted underline-offset-4" { "Untested specs" }
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
                    (field("Synced", sourced(flag(synced, Some(if synced { tag::SYNCED } else { tag::UNSYNCED })), sources.timing, sources)))
                }
                @if let Some(logic) = s.logic {
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
pub(super) fn long_date(date: &str) -> String {
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
                (field(&plural("Author", &s.authors), if s.authors.is_empty() {
                    html! { span class="text-muted" { "unknown" } }
                } else {
                    name_badges(&s.authors)
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
/// author giving it: a conventional build of a category whose stacking
/// it infers.
fn stacking_found(s: &Schematic) -> bool {
    s.conventional
        && category::behaviour(&s.category).stacking
            == category::Stacking::Inferred
}

/// Where build `s` sits, a trail from home through its category's builds,
/// after a link back to `from`, the listing the viewer came from, when it
/// is somewhere else, and a search of every build, so a build found leads
/// back to the rest.
fn crumbs(s: &Schematic, from: Option<&str>) -> Markup {
    // A category off the list is among the other builds.
    let (category, plural) = match s.kind() {
        Some(kind) if s.category_label.is_some() => {
            (format!("/?category={}", s.category), kind.plural())
        }
        _ => ("/?category=other".to_owned(), "Other builds".to_owned()),
    };
    let back = from.filter(|&from| from != "/" && from != category);
    html! {
        div class="mb-4 flex flex-wrap items-center justify-between gap-x-6 gap-y-3" {
            // The way back first, pointing back, apart from the trail.
            nav aria-label="Breadcrumb" class="flex min-w-0 items-center gap-3 text-meta text-muted" {
                @if let Some(back) = back {
                    a href=(back) data-back class="flex min-w-0 items-center gap-1.5 text-muted hover:text-ink" {
                        (icon(icons::ARROW_LEFT, "size-3.5 shrink-0"))
                        span class="truncate" { (crate::web::back::label(back)) }
                    }
                    span aria-hidden="true" class="h-4 shrink-0 border-l border-boundary" {}
                }
                ol class="flex shrink-0 items-center gap-1" {
                    li { a href="/" class="text-muted hover:text-ink" { "Home" } }
                    li aria-hidden="true" class="flex" { (icon(icons::CHEVRON_RIGHT, "size-3.5")) }
                    li { a href=(category) class="text-muted hover:text-ink" { (plural) } }
                }
            }
            form action="/" method="get" role="search" class="w-full sm:w-72" {
                label for="search" class="sr-only" { "Search builds" }
                input id="search" type="search" name="q" placeholder="Search builds"
                    autocomplete="off" class="input";
            }
        }
    }
}

/// A choice of the colours the build at `path` shows in: the site's own,
/// its author's when kept, and the viewer's theme when they have one;
/// nothing when the site's are the only ones. Each keeps `from`, where the
/// viewer came from, as the page it links to has the build's as its
/// referrer.
fn colours_switch(
    path: &str,
    shown: ColoursShown,
    from: Option<&str>,
) -> Markup {
    let from = from.map_or(String::new(), |from| {
        format!(
            "&from={}",
            form_urlencoded::byte_serialize(from.as_bytes())
                .collect::<String>()
        )
    });
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
                            a href={ (path) "?colours=" (colours.name()) (from) } class=(SWITCH)
                                aria-current=[(colours == shown.chosen).then_some("page")] { (label) }
                        }
                    }
                }
            }
        }
    }
}

/// A star that favourites the build at `path`, filled once it is, and
/// pressed again takes it back, beside how many people have favourited it,
/// `count`, once anyone has. Through fixi the answer is the star alone, so
/// the rest of the page, its viewer too, stays as it is.
pub fn favorite_button(path: &str, favorited: bool, count: i64) -> Markup {
    let label = if favorited {
        "Remove from favourites"
    } else {
        "Add to favourites"
    };
    let title = if count > 0 {
        format!("{label} · {}", super::favourites_label(count))
    } else {
        label.to_owned()
    };
    html! {
        form action={ (path) "/favorite" } method="post" class="contents"
            fx-action={ (path) "/favorite" } fx-method="post" {
            @if !favorited {
                input type="hidden" name="on" value="yes";
            }
            button id="favorite" type="submit" aria-label=(title) title=(title) aria-pressed=(favorited)
                class={ "btn btn-quiet flex h-10 min-w-10 cursor-pointer items-center justify-center gap-1.5 tabular-nums "
                    (if count > 0 { "px-2.5" } else { "p-0" }) } {
                (icon(icons::STAR, if favorited { "size-5 fill-current text-brand" } else { "size-5" }))
                @if count > 0 { span { (count) } }
            }
        }
    }
}

/// What else the viewer may do with build `s`, at `path`, under one button:
/// edit it, upload a new version, or delete it, going back to `from`, the
/// listing the viewer came from. Nothing for a viewer who may do none of
/// them.
fn actions_menu(
    s: &Schematic,
    viewer: Option<&User>,
    path: &str,
    from: Option<&str>,
) -> Markup {
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
                        button type="button" commandfor="merge" command="show-modal"
                            class={ (item) " cursor-pointer text-ink" } {
                            (icon(icons::MERGE, "size-4 text-muted")) "Merge into another build"
                        }
                        button type="button" commandfor="delete" command="show-modal"
                            class={ (item) " cursor-pointer text-danger" } {
                            (icon(icons::TRASH, "size-4")) "Delete"
                        }
                    }
                }
            }
            @if deletes {
                (merge_dialog(s))
                (delete_dialog(s, from))
            }
        }
    }
}

/// A mod's form for merging a build into another, as a fix uploaded as a
/// build of its own is: its file becomes that build's next version, if it
/// may, and it is deleted.
fn merge_dialog(s: &Schematic) -> Markup {
    html! {
        dialog id="merge" aria-labelledby="merge-heading"
            class="dialog w-full max-w-form" {
            form action={ "/schematics/" (s.id) "/merge" } method="post" data-step class="flex flex-col gap-4" {
                h2 id="merge-heading" class=(SECTION_TITLE) { "Merge " (s.name()) " into another build" }
                p { "Its file becomes the other build's next version, if it keeps 90% of that build's blocks, and this build is deleted." }
                label class="flex flex-col gap-1" {
                    span { "Build to merge into" }
                    input type="text" name="into" required placeholder="Its link or id" class="input";
                }
                label class="flex flex-col gap-1" {
                    span { "Note " span class="text-muted" { "(optional)" } }
                    input type="text" name="note" placeholder={ "Merged from " (s.name()) } class="input";
                }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Merge" }
                    button type="button" commandfor="merge" command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}

/// An admin's confirmation before deleting a build, which then goes back
/// to `from`, the listing the viewer came from, else home.
fn delete_dialog(s: &Schematic, from: Option<&str>) -> Markup {
    html! {
        dialog id="delete" aria-labelledby="delete-heading"
            class="dialog w-full max-w-form" {
            form action={ "/schematics/" (s.id) "/delete" } method="post" data-step class="flex flex-col gap-4" {
                @if let Some(from) = from {
                    input type="hidden" name="next" value=(from);
                }
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
/// it by; and a button that sends the file so made to the viewer's
/// Schemati instead. Only someone signed in is shown it.
fn download_dialog(s: &Schematic, colours: ColoursShown) -> Markup {
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
            class="dialog w-full max-w-form" {
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
                    button type="submit" formaction={ (path) "/schemati" } formmethod="post"
                        fx-action={ (path) "/schemati" } fx-method="post" fx-target="#sent" fx-swap="innerHTML"
                        class="btn btn-quiet" {
                        "Send to Schemati"
                    }
                    button type="button" commandfor="download" command="close" class="btn btn-quiet" { "Cancel" }
                }
                div id="sent" aria-live="polite" class="text-meta empty:hidden" {}
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
                                @if !other.authors.is_empty() { " by " (other.authors.join(", ")) }
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

/// How a link to build `s`'s page shows where it is posted, as in Discord:
/// its name, authors, size, and blocks as the title; its latency, how it
/// stacks, the oldest release it works in, and whether it is verified as
/// the line under it; and its picture.
fn preview(s: &Schematic) -> super::Preview {
    let by = if s.authors.is_empty() {
        String::new()
    } else {
        format!(" by {}", s.authors.join(", "))
    };
    let title = format!(
        "{}{by} · {} H×W×D, {} blocks",
        s.name(),
        super::size(s),
        s.volume()
    );
    let mut facts = vec![s.category_label()];
    if let Some(t) = s.latency {
        facts.push(format!("{t} {}", if t == 1 { "tick" } else { "ticks" }));
    }
    if let Some(pitch) = s.stacking.iter().filter_map(|d| d.pitch).min() {
        facts.push(format!("{pitch}-wide stackable"));
    }
    if let Some(from) = &s.works_from {
        facts.push(format!("works from {from}"));
    }
    facts.push(
        if s.verified() {
            "verified"
        } else if s.untested() {
            "untested"
        } else {
            "failing"
        }
        .to_owned(),
    );
    super::Preview {
        title,
        description: facts.join(" · "),
        image: Some(super::listing::thumbnail_src(s, None)),
    }
}
