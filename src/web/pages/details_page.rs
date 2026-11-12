//! The page for a build's file and details, one for every way they change:
//! a new upload, an edit of a stored build, and a stored build's next
//! version, whose file saves with any change to its details. A panel of
//! checks says what is being checked and what holds up publishing; the
//! simulation runs in the page as the category and width are chosen.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use maud::{Markup, html};

use crate::{
    import::Analysis,
    models::{schematic::Build, user::User},
    recognize::fault::wrong_outputs,
    simulate::Report,
    version::{Comparison, MAX_CHANGED, MIN_SAME_PERCENT},
};

use super::{
    super::form::Form,
    Choices, Offered, PAGE_TITLE, SECTION_TITLE, base, beside_viewer,
    form::{fields, tag_picker},
    grouped,
    interface::{Inversions, interface_section},
    simulation::simulation_results,
    upload::file_fields,
};

/// A stored build an upload's file is already the file of.
#[derive(Clone, Copy)]
pub enum Duplicate<'a> {
    /// One on the site, by its id.
    Shown(&'a str),
    /// One deleted, which only an admin can restore.
    Deleted,
}

/// The file a details page shows.
pub enum File<'a> {
    /// A new upload's, carried in the form until it is published.
    Upload {
        /// The prepared file, base64.
        data: &'a str,
        /// The file as the site will keep it, recognized, its signs,
        /// trapdoors, and unused blocks out, in the site's theme, base64,
        /// which the viewer shows; `None` for a build the site can't read.
        /// Its checks' File stage says what came out.
        shown: Option<&'a str>,
        /// The file as its author made it, base64, for the colours they
        /// gave it; empty when not had.
        original: &'a str,
        /// The stored build with the same file, if any.
        duplicate: Option<Duplicate<'a>>,
        /// The stored build most like it, with how alike, if any is
        /// similar.
        similar: Option<(&'a Build, u32)>,
        /// Whether the blocks read for it break the conventions, so are
        /// not read by them.
        unconventional: bool,
        /// How many hoppers it has, kept unless the uploader asks to swap
        /// them.
        hoppers: usize,
        /// What the check by function recognized it as, with where its
        /// inputs and outputs are, if it did.
        recognized: Option<&'a str>,
        /// What only its author can put right, by that check.
        problems: &'a [String],
    },
    /// A stored build's, as it is, with its stored simulation result.
    Stored {
        s: &'a Build,
        simulation: Option<&'a Report>,
    },
    /// A stored build's next version, carried in the form until it is
    /// saved.
    Version {
        s: &'a Build,
        data: &'a str,
        comparison: Comparison,
        /// Why it cannot replace the build; empty when it can.
        refusals: &'a [String],
        /// What to warn of though it may, as changing much of it.
        warnings: &'a [String],
        /// The number it would have.
        version: i64,
        note: &'a str,
    },
}

/// What a details page says back to the last thing sent from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reply<'a> {
    /// Nothing: the page as first opened.
    #[default]
    Fresh,
    /// Publishing or saving was refused, and why.
    NotSaved(&'a str),
    /// The fields for choosing another file start open, saying why the last
    /// file chosen could not be read, if it could not.
    Choosing(Option<&'a str>),
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
    pub reply: Reply<'a>,
    /// How the viewer's Schemati session stands, as the site keeps it,
    /// for the slot their files load into.
    pub schemati: Option<&'a crate::models::user::SchematiLink>,
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
        File::Upload {
            shown: Some(shown), ..
        } => format!("data:application/octet-stream;base64,{shown}"),
        File::Upload { data, .. } | File::Version { data, .. } => {
            let shown = STANDARD
                .decode(data)
                .ok()
                .and_then(|file| {
                    crate::schem::standardize::for_viewer(&file).ok()
                })
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
                @if let Reply::NotSaved(message) = page.reply {
                    p id="refusal" role="alert" tabindex="-1" autofocus
                        class="rounded-control border border-danger p-3 text-danger" {
                        strong { "Not saved. " } (message)
                    }
                }
                (checks(page))
                (file_chooser(page))
                (details_form(user, page))
            }, true))
            div class="mt-12 max-w-calm" { (interface_section(page.analysis, None, None, &Inversions { category: "", flags: &[], found: &[] })) }
            // Its checks, as a build's page shows them, redrawn as the
            // category and width change: asked for once the page loads.
            @let simulate = simulate_url(page);
            (simulation_results(&simulate, &html! {}))
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

/// Whether a check passed, failed, or waits on something.
#[derive(Clone, Copy)]
enum State {
    Pass,
    /// Worth a look, but holding nothing up.
    Warn,
    Fail,
    Wait,
}

/// One line of the checks panel.
fn check(name: &str, state: State, text: &Markup) -> Markup {
    let (key, class) = match state {
        State::Pass => ("pass", ""),
        State::Warn => ("warn", ""),
        State::Fail => ("fail", "text-danger"),
        State::Wait => ("wait", "text-muted"),
    };
    html! {
        li data-check=(name) data-state=(key)
            class={ "flex items-start gap-2 " (class) } {
            span data-icon aria-hidden="true" class="mt-0.5 inline-flex size-4 shrink-0 items-center justify-center" {
                @match state {
                    State::Pass => "✓",
                    State::Warn => "!",
                    State::Fail => "✗",
                    State::Wait => "○",
                }
            }
            span data-text { (text) }
        }
    }
}

/// For an upload, its hoppers, with an offer to swap them for barrels;
/// what it swapped already its checks' File stage says.
fn swap_checks(page: &DetailsPage) -> Markup {
    html! {
        @if let File::Upload { hoppers, .. } = &page.file {
            @if *hoppers > 0 {
                (check("hoppers", State::Wait, &html! {
                    @if *hoppers == 1 { "Has a hopper, kept as it is: the simulator reads its signal strength but moves no items. " }
                    @else { "Has " (hoppers) " hoppers, kept as they are: the simulator reads their signal strength but moves no items. " }
                    (swap_button("hopper", if *hoppers == 1 { "Swap it for a barrel" } else { "Swap them for barrels" }))
                }))
            }
        }
    }
}

/// Blocks by kind with how many, as words, such as `a water cauldron and
/// 2 cakes`.
pub fn counted(kinds: &[(String, usize)]) -> String {
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

/// Why publishing is held up, if it is, decided here rather than as the
/// page runs, so the button is disabled from the start. Broken conventions
/// hold nothing up: they only keep a build from being recognized.
fn blocked(page: &DetailsPage) -> Option<&'static str> {
    match &page.file {
        File::Upload {
            duplicate: Some(_), ..
        } => Some("This file is already on the site."),
        File::Version { refusals, .. } if !refusals.is_empty() => {
            Some("This file can't replace the build.")
        }
        _ => None,
    }
}

/// The checks the file and its details go through that it has something
/// to say about, such as a duplicate; none when it has nothing. Whether
/// the build is verified is its category's badge, why not and the
/// simulation's result are under the form, and what the simulation
/// measures fills the details' fields.
fn checks(page: &DetailsPage) -> Markup {
    let lines = html! {
        @if let File::Upload { duplicate: Some(duplicate), .. } = &page.file {
            (check("duplicate", State::Fail, &html! {
                @match duplicate {
                    Duplicate::Shown(id) => {
                        "This exact file is already on the site: " a href={ "/schematics/" (id) } { "see it here" } "."
                    }
                    Duplicate::Deleted => {
                        "This exact file is a deleted build's. An admin can restore it."
                    }
                }
            }))
        }
        @if let File::Upload { duplicate: None, similar: Some((other, permille)), .. } = &page.file {
            (check("similar", State::Warn, &html! {
                "Nearly the same as "
                a href={ "/schematics/" (other.id) } { (other.title()) }
                @if !other.authors.is_empty() { " by " (other.authors.join(", ")) }
                ": " (permille / 10) "% of their redstone lines up. If it is that build fixed, "
                a href={ "/schematics/" (other.id) "/versions/new" } { "upload it as a new version" }
                " instead; if it builds on it, say so in the description."
            }))
        }
        @if let File::Version { comparison, refusals, warnings, .. } = &page.file {
            @let permille = comparison.same_permille();
            @let same = html! {
                "Changes " (grouped(comparison.changed as u64)) " of "
                (grouped((comparison.same + comparison.changed) as u64)) " blocks, keeping "
                (permille / 10) "." (permille % 10) "%. "
                a href="#file-chooser" data-open-chooser { "Choose a different file" }
            };
            @let (state, said) = if !refusals.is_empty() {
                (State::Fail, *refusals)
            } else if !warnings.is_empty() {
                (State::Warn, *warnings)
            } else {
                (State::Pass, &[][..])
            };
            (check("same", state, &html! {
                (same)
                @if !said.is_empty() {
                    ul class="mt-1 flex list-disc flex-col gap-1 pl-5" {
                        @for line in said { li { (line) } }
                    }
                }
            }))
        }
        (swap_checks(page))
    };
    html! {
        @if !lines.0.is_empty() {
            section aria-labelledby="checks-heading"
                class="flex flex-col gap-2 rounded-overlay border border-line bg-raised p-4 shadow-sm" {
                h2 id="checks-heading" class="font-semibold" { "Checks" }
                ul class="flex flex-col gap-1.5" { (lines) }
            }
        }
    }
}

/// Whether the build is verified as far as is known before the simulation
/// runs, for its category's badge: `pass`, `fail` for one whose test got
/// cases wrong, `warn` for one whose test couldn't run, or empty for not
/// known yet; and why not, each reason with how to fix it, leaving out
/// wrong cases the stored result shows under the form. `form.js` puts what
/// the simulation finds in their place.
fn verdict<'p>(page: &DetailsPage<'p>) -> (&'static str, Vec<&'p str>) {
    match &page.file {
        File::Upload {
            recognized: None,
            problems,
            ..
        } => {
            let problems: Vec<&str> =
                problems.iter().map(String::as_str).collect();
            (failing_or_untested(&problems), problems)
        }
        File::Stored { s, simulation } => {
            let failed = simulation.is_some_and(Report::failed);
            match &s.unverified {
                Some(why) => (
                    failing_or_untested(&[why]),
                    [why.as_str()]
                        .into_iter()
                        .filter(|why| !(failed && wrong_outputs(why)))
                        .collect(),
                ),
                None if failed => ("fail", Vec::new()),
                None if simulation.is_some_and(Report::passed) => {
                    ("pass", Vec::new())
                }
                None => ("", Vec::new()),
            }
        }
        _ => ("", Vec::new()),
    }
}

/// The verdict of a build that isn't verified for `reasons`: `fail` when
/// its test ran and got cases wrong, else `warn`, as its test couldn't
/// run.
pub fn failing_or_untested(reasons: &[&str]) -> &'static str {
    if reasons.iter().any(|reason| wrong_outputs(reason)) {
        "fail"
    } else {
        "warn"
    }
}

/// Why a build isn't verified, each reason with how to fix it, as list
/// items, for the section under the form that says so.
pub fn reason_items(reasons: &[&str]) -> Markup {
    html! { @for reason in reasons { (super::reason_item(reason)) } }
}

/// The fields for choosing another file, the same on every details page:
/// a new upload's in its place, and a stored build's next version, or
/// another in place of the one waiting. They belong to the details form,
/// so the details filled in so far come back with the file. Folded away
/// unless the page replies `Reply::Choosing`.
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
        details id="file-chooser" open[matches!(page.reply, Reply::Choosing(_))]
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
                            "and the old one stays listed under it. A fix most often keeps at least " (MIN_SAME_PERCENT) "% of its blocks, "
                            "or changes at most " (MAX_CHANGED) ", and one that changes more is warned of; its inputs and outputs may move. "
                            "The details you have changed below stay."
                        }
                    }
                }
                (file_fields(Offered::Details, page.schemati))
                @if let Reply::Choosing(Some(message)) = page.reply {
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
    let blocked = blocked(page);
    // An unverified build's specs are typed; form.js shows their fields,
    // and marks an upload so once its checks say it goes up unverified.
    let unverified = matches!(
        &page.file,
        File::Stored { s, .. } | File::Version { s, .. } if s.unverified.is_some()
    );
    html! {
        form id="details-form" action=(action) method="post" enctype="multipart/form-data"
            data-step data-step-page data-simulate=(simulate) data-listed-kinds=(crate::category::Kind::listed())
            data-unverified[unverified]
            data-verdict=(verdict(page).0) class="flex flex-col gap-4" {
            h2 class=(SECTION_TITLE) { "Details" }
            @match &page.file {
                // Carried as its author made it, so tested standardized.
                File::Upload { data, original, .. } => {
                    input type="hidden" name="data" value=(data);
                    input type="hidden" name="original" value=(original);
                    input type="hidden" name="as_built" value="yes";
                }
                File::Version { s, data, note, .. } => {
                    input type="hidden" name="data" value=(data);
                    // An off-list build's is as built, to be tested as the
                    // category it may move up to.
                    @if crate::category::Category::find(&page.choices.categories, &s.category).is_none() {
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
            // Only a mod or admin may name a tag not yet on the list.
            (tag_picker(&page.choices.tags, &page.form.tags, page.fixed, user.can_create_tags()))
            div class="flex flex-wrap items-center gap-x-4 gap-y-2" {
                button type="submit" class="btn btn-primary" data-publish
                    disabled[blocked.is_some()] { (label) }
                @if let Some(cancel) = cancel {
                    a href=(cancel) { "Cancel" }
                }
                span data-gate-reason role="status" class="text-meta text-muted" {
                    (blocked.unwrap_or_default())
                }
            }
        }
    }
}
