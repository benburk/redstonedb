//! The page for the viewer's own Schemati folder, and the parts of the
//! settings page that connect it.

use maud::{Markup, html};

use crate::{models::user::User, schemati::Folder};

use super::{PAGE_TITLE, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, base};

/// How the viewer's Schemati folder stands.
pub enum Connection<'a> {
    /// They have given no session.
    Unset,
    /// Schemati no longer knows the session they gave.
    Forgotten,
    /// Schemati could not be asked; why.
    Unreachable(&'a str),
    Open(&'a Folder),
}

/// The viewer's Schemati folder, or the form for connecting it. `done`
/// says what was just done; `refused`, why it could not be.
pub fn schemati(
    user: &User,
    connection: &Connection,
    done: &str,
    refused: &str,
) -> Markup {
    base(
        Some(user),
        "Schemati",
        "/schemati",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-2" } { "Your Schemati" }
                p class="mb-6 max-w-prose text-muted" {
                    "What you " code { "//save" } " on Open Redstone. Files added here load in game with " code { "//load <file>" } "."
                }
                (schemati_folder(connection, done, refused))
            }
        },
    )
}

/// The part of the Schemati page its forms change, which fixi redraws:
/// what was just `done` or `refused`, and the folder, or the form for
/// connecting it.
pub fn schemati_folder(
    connection: &Connection,
    done: &str,
    refused: &str,
) -> Markup {
    html! {
        div id="schemati-folder" {
            @if !done.is_empty() {
                p class="mb-6 rounded-control border border-line bg-raised p-3" { (done) }
            }
            @if !refused.is_empty() {
                p class="mb-6 text-danger" { (refused) }
            }
            @match connection {
                Connection::Unset => p {
                    a href="/settings" { "Connect your Schemati" } " in Settings to see its files here."
                },
                Connection::Forgotten => p {
                    "Schemati has forgotten your session. " a href="/settings" { "Give it a new one" } " in Settings."
                },
                Connection::Unreachable(why) => p class="text-danger" { (why) },
                Connection::Open(folder) => (open(folder)),
            }
        }
    }
}

/// The viewer's Schemati files to choose one from, the newest chosen
/// already when Schemati says which it is, or the only one; or why there
/// are none to choose. `form` names the form they belong to when they stand
/// outside it.
pub(super) fn own_file_field(
    connection: &Connection,
    form: Option<&str>,
) -> Markup {
    let note = "text-muted";
    html! {
        @match connection {
            Connection::Open(folder) if !folder.files.is_empty() => {
                fieldset {
                    legend class="sr-only" { "Your Schemati files" }
                    div class="flex max-h-80 flex-col overflow-y-auto" {
                        @for (index, name) in folder.files.iter().enumerate() {
                            label class="flex min-h-row cursor-pointer items-center gap-3 rounded-control px-2 hover:bg-selection has-checked:bg-selection has-focus-visible:outline-2 has-focus-visible:outline-brand" {
                                input type="radio" name="schemati_file" value=(name) form=[form]
                                    checked[index == 0 && (folder.dated || folder.files.len() == 1)]
                                    class="sr-only";
                                span class="text-muted" { (super::icons::icon(super::icons::FILE, "size-4")) }
                                span class="min-w-0 flex-1 truncate" { (name) }
                            }
                        }
                    }
                }
            }
            Connection::Open(_) => p class=(note) {
                "Your Schemati folder is empty. " code { "//save <name>" } " in game adds to it."
            },
            Connection::Unset => p class=(note) {
                a href="/settings" { "Connect your Schemati" } " to pick from your files."
            },
            Connection::Forgotten => p class=(note) {
                "Your Schemati session expired. " a href="/settings" { "Give a new one" } "."
            },
            Connection::Unreachable(why) => p class=(note) { "Schemati: " (why) },
        }
    }
}

/// How sending a build to the viewer's Schemati went, for the build's
/// download dialog to show: the sentence saying it was saved, or why not
/// with a link to the Schemati page, where a session is given.
pub fn schemati_sent(sent: &Result<String, String>) -> Markup {
    html! {
        @match sent {
            Ok(done) => p { (done) },
            Err(why) => p class="text-danger" {
                (why) " " a href="/settings" { "Check it in Settings" }
            },
        }
    }
}

/// The open folder: its files, each with what can be done to it, and the
/// form for adding files.
fn open(folder: &Folder) -> Markup {
    html! {
        p class="mb-6" {
            "Connected as " strong { (folder.player) } ". "
            a href="/settings" { "Change in Settings" }
        }
        form action="/schemati/files" method="post" enctype="multipart/form-data"
            fx-action="/schemati/files" fx-method="post" fx-target="#schemati-folder"
            class="mb-8 flex flex-wrap items-end gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
            label class="field flex-1" {
                span { "Add files" }
                input type="file" name="file" multiple required accept=".schem,.schematic" class="input";
            }
            button type="submit" class="btn btn-primary" { "Add to Schemati" }
        }
        @if folder.files.is_empty() {
            p class="text-muted" {
                "Your Schemati folder is empty. In game, " code { "//save <name>" } " adds to it."
            }
        } @else {
            div class="relative overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "File" }
                            th scope="col" class=(TABLE_HEAD) { span class="sr-only" { "Actions" } }
                        }
                    }
                    tbody {
                        @for (index, name) in folder.files.iter().enumerate() {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal font-mono break-all" } { (name) }
                                td class=(TABLE_CELL) { (file_actions(index, name)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What can be done to the file `name`, the `index`th listed: download it,
/// publish it, rename it in a dialog, or delete it once asked.
fn file_actions(index: usize, name: &str) -> Markup {
    let path = format!("/schemati/files/{name}");
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, "schem"));
    let rename = format!("rename-{index}");
    let dialog = "m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30";
    html! {
        div class="flex flex-wrap justify-end gap-2" {
            a href=(path) download=(name) class="btn btn-quiet" { "Download" }
            // Publishing previews it as the upload page does a chosen file.
            form action="/upload/preview" method="post" enctype="multipart/form-data" class="contents" {
                input type="hidden" name="schemati_file" value=(name);
                button type="submit" class="btn btn-quiet" { "Publish" }
            }
            button type="button" commandfor=(rename) command="show-modal" class="btn btn-quiet" { "Rename" }
            form action={ (path) "/delete" } method="post"
                fx-action={ (path) "/delete" } fx-method="post" fx-target="#schemati-folder" class="contents" {
                button type="submit" data-confirm={ "Delete " (name) " from your Schemati folder for good?" }
                    class="btn btn-quiet" { "Delete" }
            }
        }
        dialog id=(rename) aria-labelledby={ (rename) "-heading" } class=(dialog) {
            form action={ (path) "/rename" } method="post"
                fx-action={ (path) "/rename" } fx-method="post" fx-target="#schemati-folder"
                class="flex flex-col gap-4" {
                h2 id={ (rename) "-heading" } class=(SECTION_TITLE) { "Rename " (name) }
                label class="field" {
                    span { "New name" }
                    div class="flex items-center gap-1" {
                        input name="new_name" value=(stem) required
                            pattern="[A-Za-z0-9_.]+" autocomplete="off" class="input font-mono";
                        span class="font-mono text-muted" { "." (extension) }
                    }
                    span class="text-meta font-normal" { "Letters, digits, underscores, and dots." }
                }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Rename" }
                    button type="button" commandfor=(rename) command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}
