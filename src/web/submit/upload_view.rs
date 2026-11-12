//! The upload page: choosing a file, from the uploader's Schemati folder
//! first, else from their computer or a link; and the file fields every
//! page that takes a file shares.

use maud::{Markup, html};

use crate::{
    account::{SchematiLink, User},
    build::Build,
    web::{
        href,
        schemati::view::{self, Offered},
        submit::analysis::{self, Step},
        ui::{
            PAGE_TITLE, SECTION_TITLE,
            icons::{self, LINK, UPLOAD, icon},
            layout::base,
        },
    },
};

/// The ways to choose a file, the same wherever one is chosen: from the
/// viewer's Schemati folder, from their computer, or from a Schemati link,
/// each its own section, a quiet "or" between them, the Schemati files
/// loading once the page is in, as `schemati` says how the viewer's
/// session stands. `offered` says where they are, which names the form they
/// belong to when they stand outside it.
pub(crate) fn file_fields(
    offered: Offered,
    schemati: Option<&SchematiLink>,
) -> Markup {
    let form = offered.form();
    html! {
        section aria-labelledby="from-schemati" {
            h2 id="from-schemati" class={ (SECTION_TITLE) " mb-2" } { "From your Schemati" }
            (view::own_files_slot(schemati, offered))
        }
        (or())
        section aria-labelledby="from-computer" {
            h2 id="from-computer" class={ (SECTION_TITLE) " mb-2" } { "From your computer" }
            // The field covers the whole zone, so a file dropped on it is
            // taken as one chosen.
            label class="relative flex min-h-row-rich cursor-pointer items-center gap-3 rounded-control border border-dashed border-boundary px-4 py-5 text-muted hover:bg-selection hover:text-ink has-focus-visible:outline-2 has-focus-visible:outline-brand" {
                (icon(UPLOAD, "size-5"))
                span { "Choose a " code { ".schem" } " file, or drop one here" }
                input type="file" name="file" accept=".schem" form=[form]
                    class="absolute inset-0 cursor-pointer opacity-0";
            }
        }
        (or())
        section aria-labelledby="from-link" {
            h2 id="from-link" class={ (SECTION_TITLE) " mb-2" } { "From a link" }
            div class="flex items-center gap-2" {
                span class="text-muted" { (icon(LINK, "size-5")) }
                input type="url" name="schemati" inputmode="url" autocomplete="off" form=[form]
                    aria-label="Schemati link from //download"
                    placeholder="Paste a link from //download"
                    class="inline-input";
                // Elsewhere the form's own button goes on with the file.
                @if form.is_none() {
                    button type="submit" class="btn btn-primary min-h-8 px-3" { "Preview" }
                }
            }
        }
    }
}

/// The quiet word between two ways of choosing a file.
fn or() -> Markup {
    html! {
        div class="flex items-center gap-3 text-meta text-muted" aria-hidden="true" {
            span class="h-px flex-1 bg-line" {}
            "or"
            span class="h-px flex-1 bg-line" {}
        }
    }
}

/// The upload page's first step: choosing a file, which goes on to its
/// preview as soon as one is picked, with the guide to read first a link
/// away.
pub fn upload(
    user: &User,
    message: &str,
    schemati: Option<&SchematiLink>,
) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-form" {
                div class="mb-6 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1" {
                    h1 class=(PAGE_TITLE) { "Upload a build" }
                    a href="/about/recognized" { "How builds are recognized" }
                }
                @if !message.is_empty() {
                    p role="alert" class="mb-4 text-danger" { (message) }
                }
                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    data-submit-on-pick data-analyzing class="flex flex-col gap-5" {
                    (file_fields(Offered::Upload, schemati))
                }
                // Shown in place of the form as soon as a file is sent.
                div data-uploading hidden role="status" aria-live="polite" {
                    (analysis_steps(0, None))
                }
            }
        },
    )
}

/// The page of an upload being analyzed, `step` showing how far it has
/// got, asking after it until it is done and then loading again as the
/// preview.
pub fn analyzing(user: &User, id: &str, step: &Step) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-form" {
                div class="mb-6 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1" {
                    h1 class=(PAGE_TITLE) { "Upload a build" }
                    a href="/about/recognized" { "How builds are recognized" }
                }
                div fx-action={ "/upload/" (id) "/progress" } fx-trigger="poll" data-every="250"
                    fx-swap="innerHTML" role="status" aria-live="polite" {
                    (analysis_steps(step.place(), Some(step)))
                }
            }
        },
    )
}

/// The page for checking a file as build `s`'s next version, check
/// `id`, how far it has got, until it is done and the edit page shows it.
pub fn checking_version(user: &User, s: &Build, id: &str) -> Markup {
    base(
        Some(user),
        &format!("Edit {}", s.name()),
        &format!("/schematics/{}/edit", s.id),
        &html! {
            div class="max-w-form" {
                h1 class={ (PAGE_TITLE) " mb-6" } { "Upload a new version" }
                // A step on the way to the build's page, as its edit page is.
                div fx-action={ (href::build(&s.id)) "/versions/check/" (id) "/progress" }
                    fx-trigger="poll" data-every="250" fx-swap="innerHTML" data-step-page
                    role="status" aria-live="polite" {
                    (version_steps())
                }
            }
        },
    )
}

/// The steps of checking a new version: the file uploaded, and the check
/// under way.
pub fn version_steps() -> Markup {
    const STEPS: [&str; 2] =
        ["Uploading the file", "Checking it against this version"];
    html! {
        ol class="flex flex-col" {
            @for (i, label) in STEPS.iter().enumerate() {
                li class="flex min-h-row-rich items-start gap-3 py-1.5" {
                    span class="flex h-6 w-5 shrink-0 items-center justify-center" {
                        @if i == 0 {
                            (icon(icons::CHECK, "size-5 text-brand"))
                        } @else {
                            span class="size-4 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
                        }
                    }
                    span class="leading-6" { (label) }
                }
            }
        }
    }
}

/// The steps of analyzing an upload, those before `place` done and the
/// one at it under way; while it is probed as each category, `step` says
/// which, and how many it has tried.
pub fn analysis_steps(place: usize, step: Option<&Step>) -> Markup {
    const STEPS: [&str; 4] = [
        "Uploading the file",
        "Finding its inputs and outputs",
        "Testing it as each kind of build",
        "Preparing the preview",
    ];
    html! {
        ol class="flex flex-col" {
            @for (i, label) in STEPS.iter().enumerate() {
                li class="flex min-h-row-rich items-start gap-3 py-1.5" {
                    span class="flex h-6 w-5 shrink-0 items-center justify-center" {
                        @if i < place {
                            (icon(icons::CHECK, "size-5 text-brand"))
                        } @else if i == place {
                            span class="size-4 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
                        } @else {
                            span class="size-4 rounded-full border-2 border-line" {}
                        }
                    }
                    div class="flex min-w-0 flex-1 flex-col gap-1.5" {
                        span class={ "leading-6 " (if i > place { "text-muted" } else { "" }) } { (label) }
                        @if i == place && let Some(Step::Testing { label, tried }) = step {
                            span class="text-meta text-muted" {
                                "Trying " (label) " · " ((tried + 1)) " of " (analysis::kinds())
                            }
                            span class="block h-1 w-full max-w-xs overflow-hidden rounded-full bg-line" {
                                span class="block h-full rounded-full bg-brand transition-[width] duration-200 ease-calm motion-reduce:transition-none"
                                    style={ "width: " (((tried + 1) * 100 / analysis::kinds())) "%" } {}
                            }
                        }
                    }
                }
            }
        }
    }
}
