//! The upload page: choosing a file, from the uploader's Schemati folder
//! first; and the file fields every page that takes a file shares.

use maud::{Markup, html};

use crate::models::user::User;

use super::{Connection, PAGE_TITLE, base, schemati};

/// The fields for choosing a file, the same wherever one is chosen: a file
/// from the viewer's Schemati folder, else, under Other ways, a file of
/// their own or a Schemati link. `form` names the form they belong to when
/// they stand outside it.
pub(super) fn file_fields(form: Option<&str>, schemati: &Connection) -> Markup {
    html! {
        (schemati::own_file_field(schemati, form))
        details class="group" {
            summary class="cursor-pointer text-muted hover:text-ink" { "Other ways" }
            div class="mt-3 flex flex-col gap-4" {
                label class="field" {
                    span { "Schematic file" }
                    input type="file" name="file" accept=".schem" form=[form] class="input";
                }
                label class="field" {
                    span { "Schemati link from " code { "//download" } }
                    input type="url" name="schemati" inputmode="url" autocomplete="off" form=[form]
                        placeholder="https://schemati.openredstone.org/download?file=…" class="input";
                }
            }
        }
    }
}

/// The upload page's first step: choosing a file, with the guide to read
/// first a link away.
pub fn upload(user: &User, message: &str, connection: &Connection) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-calm" {
                div class="mb-6 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1" {
                    h1 class=(PAGE_TITLE) { "Upload a build" }
                    a href="/about/uploading" { "Before you upload" }
                }
                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    class="flex max-w-form flex-col gap-5" {
                    (file_fields(None, connection))
                    @if !message.is_empty() {
                        p role="alert" class="text-danger" { (message) }
                    }
                    div { button type="submit" class="btn btn-primary" { "Preview" } }
                }
            }
        },
    )
}
