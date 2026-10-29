//! The upload page: choosing a file, from the uploader's Schemati folder
//! first, else from their computer or a link; and the file fields every
//! page that takes a file shares.

use maud::{Markup, html};

use crate::models::user::User;

use super::{
    Connection, PAGE_TITLE, SECTION_TITLE, base,
    icons::{LINK, UPLOAD, icon},
    schemati,
};

/// The ways to choose a file, the same wherever one is chosen: from the
/// viewer's Schemati folder, from their computer, or from a Schemati link,
/// each its own section, a quiet "or" between them. `form` names the form
/// they belong to when they stand outside it.
pub(super) fn file_fields(form: Option<&str>, schemati: &Connection) -> Markup {
    html! {
        section aria-labelledby="from-schemati" {
            h2 id="from-schemati" class={ (SECTION_TITLE) " mb-2" } { "From your Schemati" }
            (schemati::own_file_field(schemati, form))
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
pub fn upload(user: &User, message: &str, connection: &Connection) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-form" {
                div class="mb-6 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1" {
                    h1 class=(PAGE_TITLE) { "Upload a build" }
                    a href="/about/uploading" { "Before you upload" }
                }
                @if !message.is_empty() {
                    p role="alert" class="mb-4 text-danger" { (message) }
                }
                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    data-submit-on-pick class="flex flex-col gap-5" {
                    (file_fields(None, connection))
                }
            }
        },
    )
}
