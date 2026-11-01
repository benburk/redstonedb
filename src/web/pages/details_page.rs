//! The page for a build's file and details, one for every way they change:
//! a new upload, an edit of a stored build, and a stored build's next
//! version, whose file saves with any change to its details. A panel of
//! checks says what is being checked and what holds up publishing; the
//! simulation runs in the page as the category and width are chosen.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use maud::{Markup, html};

use crate::{
    import::Analysis,
    models::{schematic::Schematic, user::User},
    simulate::{Guess, Report},
    version::{Comparison, MAX_CHANGED, MIN_SAME_PERCENT},
};

use super::{
    super::form::Form,
    Choices, PAGE_TITLE, SECTION_TITLE, base, beside_viewer,
    form::{fields, tag_picker},
    grouped,
    interface::{Inversions, interface_section},
    simulation::{
        Waiting, simulation_results, simulation_section, simulation_summary,
        timing_summary,
    },
    spinner,
    upload::file_fields,
};

/// The file a details page shows.
pub enum File<'a> {
    /// A new upload's, carried in the form until it is published.
    Upload {
        /// The prepared file, base64.
        data: &'a str,
        /// The file as its builder made it, base64, for the colours they
        /// gave it; empty when not had.
        original: &'a str,
        /// The build less its interface layers.
        size: [u16; 3],
        /// The id of a stored build with the same file, if any.
        duplicate: Option<&'a str>,
        /// The stored build most like it, with how alike, if any is
        /// similar.
        similar: Option<(&'a Schematic, u32)>,
        /// Whether the blocks read for it break the conventions, so are
        /// not read by them.
        unconventional: bool,
        /// What it seems to be, which the category and design start as.
        guess: Option<&'a Guess>,
        /// The furnaces, cauldrons, and cakes swapped for barrels, by kind,
        /// with how many.
        swapped: &'a [(String, usize)],
        /// How many hoppers it has, kept unless the uploader asks to swap
        /// them.
        hoppers: usize,
        /// What the check by function recognized it as, with where its
        /// inputs and outputs are, if it did.
        recognized: Option<&'a str>,
        /// What only its builder can put right, by that check.
        problems: &'a [String],
    },
    /// A stored build's, as it is, with its stored simulation result.
    Stored {
        s: &'a Schematic,
        simulation: Option<&'a Report>,
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
    pub form: &'a Form,
    pub choices: &'a Choices,
    /// The width and orientation an adder reads from the file; see
    /// `fields`.
    pub derived: crate::import::Derived,
    /// The tags its file gives it, such as torchless, shown in the tag
    /// field but not to be removed.
    pub fixed: &'a [crate::models::tag::Tag],
    /// Why the last attempt to publish or save was refused, if it was.
    pub message: &'a str,
    /// Whether the fields for choosing another file start open, with why
    /// the last file chosen could not be read, if it could not.
    pub choosing: Option<&'a str>,
    /// The viewer's Schemati folder, its files to choose from.
    pub schemati: &'a super::Connection<'a>,
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
        // A file not yet stored is shown from a data address, as the
        // viewer should be given it.
        File::Upload { data, .. } | File::Version { data, .. } => {
            let shown = STANDARD
                .decode(data)
                .ok()
                .and_then(|file| crate::standardize::for_viewer(&file).ok())
                .map_or_else(
                    || (*data).to_owned(),
                    |file| STANDARD.encode(file),
                );
            format!("data:application/octet-stream;base64,{shown}")
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
                (file_chooser(page))
                (details_form(user, page))
            }, true))
            div class="mt-12 max-w-calm" { (interface_section(page.analysis, None, None, &Inversions { category: "", flags: &[], found: &[] })) }
            // Redrawn as the category and width call for a test.
            @let simulate = simulate_url(page);
            @let timing = super::super::simulation::timing_url(&simulate);
            (simulation_results(&simulate, &html! {
                @if let File::Stored { simulation: Some(report), .. } = &page.file {
                    (simulation_section(report, Waiting::Posted(&timing)))
                }
            }))
            script defer src=(crate::web::FORM_JS.url()) {}
        },
    )
}

/// Where the details form asks for its simulation result.
fn simulate_url(page: &DetailsPage) -> String {
    match &page.file {
        File::Stored { s, .. } => format!("/schematics/{}/simulation", s.id),
        File::Upload { .. } | File::Version { .. } => {
            "/upload/simulation".to_owned()
        }
    }
}

/// Whether a check passed, failed, is under way, or waits on something.
#[derive(Clone, Copy)]
enum State {
    Pass,
    /// Worth a look, but holding nothing up.
    Warn,
    Fail,
    Run,
    Wait,
}

/// One line of the checks panel; `form.js` changes the simulation's and
/// the timing's as they run, by their `name`.
fn check(name: &str, state: State, text: &Markup, hidden: bool) -> Markup {
    let (key, class) = match state {
        State::Pass => ("pass", ""),
        State::Warn => ("warn", ""),
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
                    State::Warn => "!",
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
        // A recognized build's check already says what it is.
        @if let File::Upload { guess: Some(guess), recognized: None, .. } = &page.file {
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
pub fn article(word: &str) -> String {
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
/// its kind, for `form.js`, and why. Broken conventions hold nothing up:
/// they only keep a build from being recognized.
fn blocked(page: &DetailsPage) -> Option<(&'static str, &'static str)> {
    match &page.file {
        File::Upload {
            duplicate: Some(_), ..
        } => Some(("duplicate", "This file is already on the site.")),
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
                        a href="#file-chooser" data-open-chooser { "Choose a different one" }
                    }, false))
                }
                @if let File::Upload { duplicate: Some(id), .. } = &page.file {
                    (check("duplicate", State::Fail, &html! {
                        "This exact file is already on the site: " a href={ "/schematics/" (id) } { "see it here" } "."
                    }, false))
                }
                @if let File::Upload { duplicate: None, similar: Some((other, permille)), .. } = &page.file {
                    (check("similar", State::Warn, &html! {
                        "Nearly the same as "
                        a href={ "/schematics/" (other.id) } { (other.title()) }
                        @if !other.builders.is_empty() { " by " (other.builders.join(", ")) }
                        ": " (permille / 10) "% of their redstone lines up. If it is that build fixed, "
                        a href={ "/schematics/" (other.id) "/versions/new" } { "upload it as a new version" }
                        " instead; if it builds on it, say so in the description."
                    }, false))
                }
                @if let File::Version { comparison, refusals, .. } = &page.file {
                    @let permille = comparison.same_permille();
                    @let same = html! {
                        "Changes " (grouped(comparison.changed as u64)) " of "
                        (grouped((comparison.same + comparison.changed) as u64)) " blocks, keeping "
                        (permille / 10) "." (permille % 10) "%. "
                        a href="#file-chooser" data-open-chooser { "Choose a different file" }
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
                @if let File::Upload { recognized, problems, .. } = &page.file {
                    (recognition_check(*recognized, problems))
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
                // Whether it goes up as its category or off the list,
                // which `form.js` shows from the simulation's result.
                (check("filing", State::Wait, &html! {}, true))
            }
        }
    }
}

/// What the check by function made of an upload: recognized, with its
/// inputs and outputs found and coloured in, or what only its builder can
/// put right.
fn recognition_check(recognized: Option<&str>, problems: &[String]) -> Markup {
    html! {
        @if let Some(found) = recognized {
            (check("recognized", State::Pass, &html! {
                "Recognized as " (found) ". Its inputs and outputs are coloured in for it."
            }, false))
        } @else {
            (check("recognized", State::Warn, &html! {
                "Not recognized, so it goes up as built, off the list, until it's fixed:"
                ul class="mt-1 flex list-disc flex-col gap-1 pl-5 text-ink" {
                    @for problem in problems { li { (problem) } }
                }
            }, false))
        }
    }
}

/// The fields for choosing another file, the same on every details page:
/// a new upload's in its place, and a stored build's next version, or
/// another in place of the one waiting. They belong to the details form,
/// so the details filled in so far come back with the file. Folded away
/// unless `page.choosing`, which says why the last file chosen could not
/// be read, if it could not.
fn file_chooser(page: &DetailsPage) -> Markup {
    let (summary, button) = match &page.file {
        File::Upload { .. } => ("Choose a different file", "Check it instead"),
        File::Stored { .. } => ("Upload a new version", "Check it"),
        File::Version { .. } => (
            "Choose a different file for this version",
            "Check it instead",
        ),
    };
    html! {
        details id="file-chooser" open[page.choosing.is_some()]
            class="rounded-overlay border border-line bg-raised p-4 shadow-sm" {
            summary class="cursor-pointer font-semibold" { (summary) }
            div class="mt-2 flex flex-col gap-3" {
                p class="text-meta text-muted" {
                    @match &page.file {
                        File::Upload { .. } => {
                            "It takes this one's place. The details you have filled in below stay."
                        }
                        _ => {
                            "For fixing the build: the new file replaces this one, keeping its details, credits, and downloads, "
                            "and the old one stays listed under it. It must keep at least " (MIN_SAME_PERCENT) "% of its blocks, "
                            "or change at most " (MAX_CHANGED) "; within that, its inputs and outputs may move. "
                            "The details you have changed below stay."
                        }
                    }
                }
                (file_fields(Some("details-form"), page.schemati))
                @if let Some(message) = page.choosing.filter(|message| !message.is_empty()) {
                    p role="alert" class="text-danger" { (message) }
                }
                div {
                    button type="submit" form="details-form" name="action" value="file" formnovalidate
                        class="btn btn-quiet" { (button) }
                }
            }
        }
    }
}

/// The details, the file a new upload or version carries, and the button
/// that publishes or saves them, disabled while something holds it up.
fn details_form(user: &User, page: &DetailsPage) -> Markup {
    let simulate = simulate_url(page);
    let (action, label, cancel) = match &page.file {
        File::Upload { .. } => {
            ("/upload".to_owned(), "Publish".to_owned(), None)
        }
        File::Stored { s, .. } => (
            format!("/schematics/{}/edit", s.id),
            "Save".to_owned(),
            Some(format!("/schematics/{}", s.id)),
        ),
        File::Version { s, version, .. } => (
            format!("/schematics/{}/edit", s.id),
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
        form id="details-form" action=(action) method="post" enctype="multipart/form-data"
            data-simulate=(simulate)
            data-simulated[stored_result] class="flex flex-col gap-4" {
            h2 class=(SECTION_TITLE) { "Details" }
            @match &page.file {
                // Carried as its builder made it, so tested standardized.
                File::Upload { data, original, .. } => {
                    input type="hidden" name="data" value=(data);
                    input type="hidden" name="original" value=(original);
                    input type="hidden" name="as_built" value="yes";
                }
                File::Version { s, data, note, .. } => {
                    input type="hidden" name="data" value=(data);
                    // An off-list build's is as built, to be tested as the
                    // category it may move up to.
                    @if crate::models::catalogue::Category::find(&page.choices.categories, &s.category).is_none() {
                        input type="hidden" name="as_built" value="yes";
                    }
                    label class="field" {
                        span { "What it fixes " span class="font-normal" { "(optional)" } }
                        textarea name="note" rows="2" maxlength="500" class="input py-2 leading-prose" { (note) }
                    }
                }
                File::Stored { .. } => {}
            }
            (fields(page.form, page.choices, &page.derived))
            // Only an admin may name a tag not yet on the list.
            (tag_picker(&page.choices.tags, &page.form.tags, page.fixed, user.is_admin()))
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
