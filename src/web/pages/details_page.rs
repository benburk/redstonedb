//! The page for a build's file and details, one for every way they change:
//! a new upload, an edit of a stored build, and a stored build's next
//! version, whose file saves with any change to its details. A panel of
//! checks says what is being checked and what holds up publishing; the
//! simulation runs in the page as the category and width are chosen.

use maud::{Markup, html};

use crate::{
    models::{
        schematic::{Schematic, Tag},
        user::User,
    },
    simulate::{Guess, Report},
    version::{Comparison, MAX_CHANGED, MIN_SAME_PERCENT},
};

use super::{
    super::details::StackRow, Analysis, Choices, Fields, PAGE_TITLE,
    SECTION_TITLE, Waiting, base, beside_viewer, fields, grouped,
    interface_section, simulation_section, simulation_summary, spinner,
    tag_picker, timing_summary, violation_list,
};

/// The file a details page shows.
pub enum File<'a> {
    /// A new upload's, carried in the form until it is published.
    Upload {
        /// The prepared file, base64.
        data: &'a str,
        /// The build less its interface layers.
        size: [u16; 3],
        /// The id of a stored build with the same file, if any.
        duplicate: Option<&'a str>,
        /// Whether the file keeps where its uploader's `//paste` put it.
        keep_paste: bool,
        unconventional: bool,
        /// What it seems to be, which the category and design start as.
        guess: Option<&'a Guess>,
        /// The furnaces, cauldrons, and cakes swapped for barrels, by kind,
        /// with how many.
        swapped: &'a [(String, usize)],
        /// How many hoppers it has, kept unless the uploader asks to swap
        /// them.
        hoppers: usize,
    },
    /// A stored build's, as it is, with its stored simulation result.
    Stored {
        s: &'a Schematic,
        simulation: Option<&'a Report>,
        /// Whether the form for choosing a new version starts open.
        choosing: bool,
    },
    /// A stored build's next version, carried in the form until it is
    /// saved.
    Version {
        s: &'a Schematic,
        data: &'a str,
        comparison: Comparison,
        /// Why it cannot replace the build; empty when it can.
        refusals: &'a [String],
        /// The number it would have.
        version: i64,
        note: &'a str,
    },
}

/// What a details page shows.
pub struct DetailsPage<'a> {
    pub file: File<'a>,
    /// What the file shown, a new one if there is one, says about itself.
    pub analysis: &'a Analysis,
    pub fields: &'a Fields<'a>,
    pub chosen_tags: &'a [String],
    pub choices: &'a Choices,
    /// Every tag, to suggest.
    pub tags: &'a [Tag],
    /// Whether the person, an admin, may name a new tag.
    pub may_create_tags: bool,
    /// The width and orientation an adder reads from the file; see
    /// `fields`.
    pub derived: crate::import::Derived,
    /// Why the last attempt to publish or save was refused, if it was.
    pub message: &'a str,
}

/// A stored build's details as its form starts with them.
pub struct Stored {
    width: String,
    latency: String,
    throughput: String,
    bytes: String,
    flags: Vec<String>,
    pub tags: Vec<String>,
    stack_rows: Vec<StackRow>,
}

impl Stored {
    pub fn of(s: &Schematic) -> Self {
        let number =
            |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
        Stored {
            width: number(s.width),
            latency: number(s.latency),
            throughput: number(s.throughput),
            bytes: number(s.bytes),
            flags: s.flags.iter().map(|flag| flag.name.clone()).collect(),
            tags: s
                .tags
                .iter()
                .filter(|tag| !tag.is_derived())
                .map(|tag| tag.name.clone())
                .collect(),
            // A build that does not stack yet starts with a likely direction.
            stack_rows: if s.stacking.is_empty() {
                vec![StackRow::suggested(&s.orientation, s.size)]
            } else {
                s.stacking.iter().map(StackRow::of).collect()
            },
        }
    }

    pub fn fields<'a>(&'a self, s: &'a Schematic) -> Fields<'a> {
        Fields {
            category: &s.category,
            width: &self.width,
            orientation: &s.orientation,
            encoding: s.encoding.as_deref().unwrap_or_default(),
            latency: &self.latency,
            synchronized: s.synchronized,
            builders: &s.builders,
            contributors: &s.contributors,
            built_on: s.built_on.as_deref().unwrap_or(""),
            description: s.description.as_deref().unwrap_or(""),
            given_name: s.given_name.as_deref().unwrap_or(""),
            logic: s.own_logic.as_deref().unwrap_or_default(),
            throughput: &self.throughput,
            bytes: &self.bytes,
            design: &s.design,
            flags: &self.flags,
            stackable: !s.stacking.is_empty(),
            stack_rows: &self.stack_rows,
        }
    }
}

pub fn details_page(user: &User, page: &DetailsPage) -> Markup {
    let (title, heading, nav) = match &page.file {
        File::Upload { .. } => (
            "Upload".to_owned(),
            "Check and describe it".to_owned(),
            "/upload".to_owned(),
        ),
        File::Stored { s, .. } => (
            format!("Edit {}", s.name()),
            format!("Edit {}", s.name()),
            format!("/schematics/{}", s.id),
        ),
        File::Version { s, version, .. } => (
            format!("Version {version} of {}", s.name()),
            format!("Version {version} of {}", s.name()),
            format!("/schematics/{}", s.id),
        ),
    };
    let viewer = match &page.file {
        // A file not yet stored is shown from a data address.
        File::Upload { data, .. } | File::Version { data, .. } => {
            format!("data:application/octet-stream;base64,{data}")
        }
        File::Stored { s, .. } => {
            format!("/schematics/{}/download?preview=yes", s.id)
        }
    };
    base(
        Some(user),
        &title,
        &nav,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { (heading) }
            (beside_viewer(&viewer, &html! {
                // A refused publish or save comes back here, so say why first.
                @if !page.message.is_empty() {
                    p id="refusal" role="alert" tabindex="-1" autofocus
                        class="rounded-control border border-danger p-3 text-danger" {
                        strong { "Not saved. " } (page.message)
                    }
                }
                (checks(page))
                @if let File::Stored { s, choosing, .. } = &page.file {
                    (version_chooser(s, *choosing))
                }
                (details_form(page))
            }))
            (interface_section(page.analysis, false))
            // Filled by form.js as the category and width call for a test.
            div id="simulation-results" {
                @if let File::Stored { simulation: Some(report), .. } = &page.file {
                    (simulation_section(report, Waiting::Fetched))
                }
            }
            script defer src=(super::super::form_js_url()) {}
        },
    )
}

/// Whether a check passed, failed, is under way, or waits on something.
#[derive(Clone, Copy)]
enum State {
    Pass,
    Fail,
    Run,
    Wait,
}

/// One line of the checks panel; `form.js` changes the simulation's and
/// the timing's as they run, by their `name`.
fn check(name: &str, state: State, text: &Markup, hidden: bool) -> Markup {
    let (key, class) = match state {
        State::Pass => ("pass", ""),
        State::Fail => ("fail", "text-danger"),
        State::Run => ("run", "text-muted"),
        State::Wait => ("wait", "text-muted"),
    };
    html! {
        li data-check=(name) data-state=(key) hidden[hidden]
            class={ "flex items-start gap-2 " (class) } {
            span data-icon aria-hidden="true" class="mt-0.5 inline-flex size-4 shrink-0 items-center justify-center" {
                @match state {
                    State::Pass => "✓",
                    State::Fail => "✗",
                    State::Run => (spinner()),
                    State::Wait => "○",
                }
            }
            span data-text { (text) }
        }
    }
}

/// For an upload, the furnaces, cauldrons, and cakes swapped for barrels,
/// and its hoppers, with an offer to swap them too.
fn swap_checks(page: &DetailsPage) -> Markup {
    html! {
        @if let File::Upload { guess: Some(guess), .. } = &page.file {
            (check("guess", State::Pass, &html! {
                @if guess.wrong == 0 { "Looks like " } @else { "Looks most like " }
                (article(&guess.category.replace('_', " "))) ": " (guess.pattern) ", right in "
                (grouped(guess.cases - guess.wrong)) " of " (grouped(guess.cases))
                " quick cases, so its category starts as that. Change it if it's something else."
            }, false))
        }
        @if let File::Upload { swapped, .. } = &page.file {
            @if !swapped.is_empty() {
                (check("swapped", State::Pass, &html! {
                    "Swapped " (counted(swapped)) " for "
                    @if matches!(swapped, [(_, 1)]) { "a barrel" } @else { "barrels" }
                    " giving the same signal strength."
                }, false))
            }
        }
        @if let File::Upload { hoppers, .. } = &page.file {
            @if *hoppers > 0 {
                (check("hoppers", State::Wait, &html! {
                    @if *hoppers == 1 { "Has a hopper, kept as it is: the simulator reads its signal strength but moves no items. " }
                    @else { "Has " (hoppers) " hoppers, kept as they are: the simulator reads their signal strength but moves no items. " }
                    (swap_button("hopper", if *hoppers == 1 { "Swap it for a barrel" } else { "Swap them for barrels" }))
                }, false))
            }
        }
    }
}

/// `word` after `a` or `an`, such as `an adder`.
fn article(word: &str) -> String {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => format!("an {word}"),
        _ => format!("a {word}"),
    }
}

/// Blocks by kind with how many, as words, such as `a water cauldron and
/// 2 cakes`.
fn counted(kinds: &[(String, usize)]) -> String {
    let each: Vec<String> = kinds
        .iter()
        .map(|(name, count)| match count {
            1 => format!("a {name}"),
            n => format!("{n} {name}s"),
        })
        .collect();
    match each.as_slice() {
        [.., last] if each.len() > 1 => {
            format!("{} and {last}", each[..each.len() - 1].join(", "))
        }
        _ => each.concat(),
    }
}

/// A button that sends the details form to swap the file's hoppers, as
/// `what` says, for barrels giving the same signal strength, and shows it
/// again rather than publishing.
fn swap_button(what: &str, label: &str) -> Markup {
    html! {
        button type="submit" form="details-form" name="swap" value=(what) formnovalidate
            class="cursor-pointer font-medium text-brand underline hover:no-underline" {
            (label)
        }
        " giving the same signal strength."
    }
}

/// What holds up publishing, decided here rather than as the page runs:
/// its kind, for `form.js`, and why. Broken conventions hold it up only
/// until “follows its own layout” is ticked.
fn blocked(page: &DetailsPage) -> Option<(&'static str, &'static str)> {
    let violations = !page.analysis.violations.is_empty();
    match &page.file {
        File::Upload {
            duplicate: Some(_), ..
        } => Some(("duplicate", "This file is already on the site.")),
        File::Upload {
            unconventional: false,
            ..
        } if violations => {
            Some(("conventions", "Fix the building conventions first."))
        }
        File::Version { refusals, .. } if !refusals.is_empty() => {
            Some(("version", "This file can't replace the build."))
        }
        _ => None,
    }
}

/// The checks the file and its details go through, each with how it
/// stands: those decided here, then the simulation and its timing, which
/// `form.js` runs and updates.
fn checks(page: &DetailsPage) -> Markup {
    let violations = &page.analysis.violations;
    let unconventional = match &page.file {
        File::Upload { unconventional, .. } => *unconventional,
        File::Stored { s, .. } | File::Version { s, .. } => !s.conventional,
    };
    let stored = match &page.file {
        File::Stored { simulation, .. } => *simulation,
        _ => None,
    };
    html! {
        section aria-labelledby="checks-heading"
            class="flex flex-col gap-2 rounded-overlay border border-line bg-raised p-4 shadow-sm" {
            h2 id="checks-heading" class="font-semibold" { "Checks" }
            ul class="flex flex-col gap-1.5" {
                @if let File::Upload { size: [x, y, z], .. } = &page.file {
                    (check("file", State::Pass, &html! {
                        "Read the file: " span class="tabular-nums" { (y) "×" (x) "×" (z) " (HWD)" } ". "
                        a href="/upload" { "Choose a different one" }
                    }, false))
                }
                @if let File::Upload { duplicate: Some(id), .. } = &page.file {
                    (check("duplicate", State::Fail, &html! {
                        "This exact file is already on the site: " a href={ "/schematics/" (id) } { "see it here" } "."
                    }, false))
                }
                @if let File::Version { s, comparison, refusals, .. } = &page.file {
                    @let permille = comparison.same_permille();
                    @let same = html! {
                        "Changes " (grouped(comparison.changed as u64)) " of "
                        (grouped((comparison.same + comparison.changed) as u64)) " blocks, keeping "
                        (permille / 10) "." (permille % 10) "%. "
                        a href={ "/schematics/" (s.id) "/versions/new" } { "Choose a different file" }
                    };
                    @if refusals.is_empty() {
                        (check("same", State::Pass, &same, false))
                    } @else {
                        (check("same", State::Fail, &html! {
                            (same)
                            ul class="mt-1 flex list-disc flex-col gap-1 pl-5" {
                                @for refusal in *refusals { li { (refusal) } }
                            }
                        }, false))
                    }
                }
                @if unconventional {
                    (check("conventions", State::Wait, &html! {
                        "Follows its own layout, so the building conventions are not checked."
                    }, false))
                } @else if violations.is_empty() {
                    (check("conventions", State::Pass, &html! { "Follows the building conventions." }, false))
                } @else {
                    (check("conventions", State::Fail, &html! {
                        @match violations.len() {
                            1 => "Breaks a building convention: ",
                            n => { "Breaks " (n) " building conventions: " }
                        }
                        "choose the file again with Standardize ticked, fix them in-game, or tick "
                        "“follows its own layout” below if it is a display or similar."
                        details class="mt-1 text-ink" {
                            summary class="cursor-pointer text-danger" {
                                "Show " @if violations.len() == 1 { "it" } @else { "them" }
                            }
                            div class="mt-1" { (violation_list(violations)) }
                        }
                    }, false))
                }
                (swap_checks(page))
                @match stored {
                    Some(report) => {
                        @let failed = report.failed();
                        (check("simulation", if failed { State::Fail } else { State::Pass },
                            &html! { "Simulation: " (simulation_summary(report)) }, false))
                        @match report.timing() {
                            Some(timing) => (check("timing",
                                if timing.synced { State::Pass } else { State::Fail },
                                &html! { "Timing: " (timing_summary(timing)) }, false)),
                            None => (check("timing", State::Run, &html! { "Measuring the timing…" }, !report.passed())),
                        }
                    }
                    None => {
                        (check("simulation", State::Wait, &html! { "Choose a category to simulate it." }, false))
                        (check("timing", State::Wait, &html! {}, true))
                    }
                }
            }
        }
    }
}

/// The form for choosing a stored build's next version, folded away
/// unless `open`; it comes back to this page with the new file in it.
fn version_chooser(s: &Schematic, open: bool) -> Markup {
    html! {
        details id="new-version" open[open]
            class="rounded-overlay border border-line bg-raised p-4 shadow-sm" {
            summary class="cursor-pointer font-semibold" { "Upload a new version" }
            p class="mt-2 mb-3 text-meta text-muted" {
                "For fixing the build: the new file replaces this one, keeping its details, credits, and downloads, "
                "and the old one stays listed under it. It must be as wide and deep, though it may be taller; "
                "have the same inputs and outputs; and keep at least " (MIN_SAME_PERCENT) "% of its blocks, "
                "or change at most " (MAX_CHANGED) ". Changes to the details below are kept only once it is chosen."
            }
            form action={ "/schematics/" (s.id) "/versions/preview" } method="post"
                enctype="multipart/form-data" class="flex flex-col gap-3" {
                label class="field" {
                    span { "Schematic file" }
                    input type="file" name="file" accept=".schem" class="input py-1.5";
                }
                label class="field" {
                    span { "Or a Schemati link" }
                    input type="url" name="schemati" inputmode="url" autocomplete="off"
                        placeholder="https://schemati.openredstone.org/download?file=…" class="input";
                }
                label class="flex items-start gap-2" {
                    input type="checkbox" name="standardize" value="yes" checked class="mt-1 size-4 appearance-auto";
                    span { "Standardize it, as when uploading." }
                }
                div { button type="submit" class="btn btn-quiet" { "Check it" } }
            }
        }
    }
}

/// The details, the file a new upload or version carries, and the button
/// that publishes or saves them, disabled while something holds it up.
fn details_form(page: &DetailsPage) -> Markup {
    let (action, simulate, label, cancel) = match &page.file {
        File::Upload { .. } => (
            "/upload".to_owned(),
            "/upload/simulation".to_owned(),
            "Publish".to_owned(),
            None,
        ),
        File::Stored { s, .. } => (
            format!("/schematics/{}/edit", s.id),
            format!("/schematics/{}/simulation", s.id),
            "Save".to_owned(),
            Some(format!("/schematics/{}", s.id)),
        ),
        File::Version { s, version, .. } => (
            format!("/schematics/{}/edit", s.id),
            "/upload/simulation".to_owned(),
            format!("Save and publish version {version}"),
            Some(format!("/schematics/{}", s.id)),
        ),
    };
    let stored_result = matches!(
        &page.file,
        File::Stored {
            simulation: Some(_),
            ..
        }
    );
    let blocked = blocked(page);
    html! {
        form id="details-form" action=(action) method="post" data-simulate=(simulate)
            data-simulated[stored_result] class="flex flex-col gap-4" {
            h2 class=(SECTION_TITLE) { "Details" }
            @match &page.file {
                File::Upload { data, keep_paste, .. } => {
                    input type="hidden" name="data" value=(data);
                    @if *keep_paste {
                        input type="hidden" name="keep_paste" value="yes";
                    }
                }
                File::Version { data, note, .. } => {
                    input type="hidden" name="data" value=(data);
                    label class="field" {
                        span { "What it fixes " span class="font-normal" { "(optional)" } }
                        textarea name="note" rows="2" maxlength="500" class="input py-2 leading-prose" { (note) }
                    }
                }
                File::Stored { .. } => {}
            }
            (fields(page.fields, page.choices, &page.derived))
            (tag_picker(page.tags, page.chosen_tags, page.may_create_tags))
            @if let File::Upload { unconventional, .. } = &page.file
                && (!page.analysis.violations.is_empty() || *unconventional)
            {
                label class="flex items-start gap-2" {
                    input type="checkbox" name="unconventional" value="yes" data-unconventional
                        checked[*unconventional] class="mt-1 size-4 appearance-auto";
                    span {
                        "This build follows its own layout, such as a display, so the building conventions do not apply to it."
                    }
                }
            }
            div class="flex flex-wrap items-center gap-x-4 gap-y-2" {
                button type="submit" class="btn btn-primary" data-publish
                    data-blocked=[blocked.map(|(kind, _)| kind)] disabled[blocked.is_some()] { (label) }
                @if let Some(cancel) = cancel {
                    a href=(cancel) { "Cancel" }
                }
                span data-gate-reason role="status" class="text-meta text-muted" {
                    (blocked.map_or("", |(_, why)| why))
                }
            }
        }
    }
}
