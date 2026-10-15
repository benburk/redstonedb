//! The page for the viewer's own Schemati folder.

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
                    "Schemati keeps what you " code { "//save" } " on the Open Redstone Engineers server. "
                    "Here you can download those files, publish them, rename and delete them, and add files, "
                    "such as a build sent from its page here, to load in game with " code { "//load <file>" } "."
                }
                @if !done.is_empty() {
                    p role="status" class="mb-6 rounded-control border border-line bg-raised p-3" { (done) }
                }
                @if !refused.is_empty() {
                    p role="alert" class="mb-6 text-danger" { (refused) }
                }
                @match connection {
                    Connection::Unset => (session_form(false)),
                    Connection::Forgotten => (session_form(true)),
                    Connection::Unreachable(why) => {
                        p role="alert" class="mb-6 text-danger" { (why) }
                        (disconnect())
                    }
                    Connection::Open(folder) => (open(folder)),
                }
            }
        },
    )
}

/// How sending a build to the viewer's Schemati went, for the build's
/// download dialog to show: the sentence saying it was saved, or why not
/// with a link to the Schemati page, where a session is given.
pub fn schemati_sent(sent: &Result<String, String>) -> Markup {
    html! {
        @match sent {
            Ok(done) => p role="status" { (done) },
            Err(why) => p role="alert" class="text-danger" {
                (why) " " a href="/schemati" { "Open your Schemati" }
            },
        }
    }
}

/// The form for giving a Schemati session, with how to find one, saying
/// so first when Schemati has `forgotten` the last.
fn session_form(forgotten: bool) -> Markup {
    html! {
        section aria-labelledby="session-heading"
            class="flex max-w-prose flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
            h2 id="session-heading" class=(SECTION_TITLE) { "Connect your Schemati" }
            @if forgotten {
                p role="alert" class="text-danger" {
                    "Schemati has forgotten the session you gave, as it does after a day or when you log out there. Give it a new one."
                }
            }
            ol class="flex list-decimal flex-col gap-1.5 pl-5 leading-prose" {
                li {
                    "Log in at "
                    a href="https://schemati.openredstone.org/login" target="_blank" rel="noopener" { "schemati.openredstone.org" }
                    " with Discord."
                }
                li {
                    "Open your browser's developer tools with F12, then Storage in Firefox or Application in Chrome, "
                    "then Cookies, then schemati.openredstone.org."
                }
                li { "Copy the value of " code { "sessionId" } " and paste it below." }
            }
            form action="/schemati/session" method="post" class="flex flex-col gap-4" {
                label class="field" {
                    span { "Schemati session" }
                    input name="session" required autocomplete="off" spellcheck="false"
                        placeholder="e2565926ee94c42943f6a68b88e1fe07" class="input font-mono";
                }
                div { button type="submit" class="btn btn-primary" { "Connect" } }
            }
            p class="text-meta text-muted" {
                "The session opens your Schemati folder and nothing else. It is kept until you disconnect or give another."
            }
        }
    }
}

/// The button that forgets the viewer's session.
fn disconnect() -> Markup {
    html! {
        form action="/schemati/session/delete" method="post" {
            button type="submit" class="btn btn-quiet" { "Disconnect" }
        }
    }
}

/// The open folder: its files, each with what can be done to it, and the
/// form for adding files.
fn open(folder: &Folder) -> Markup {
    html! {
        div class="mb-6 flex flex-wrap items-center justify-between gap-4" {
            p {
                "Connected as " strong { (folder.player) } "."
            }
            (disconnect())
        }
        form action="/schemati/files" method="post" enctype="multipart/form-data"
            class="mb-8 flex flex-wrap items-end gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
            label class="field flex-1" {
                span { "Add files" }
                input type="file" name="file" multiple required accept=".schem,.schematic" class="input py-1.5";
            }
            button type="submit" class="btn btn-primary" { "Add to Schemati" }
        }
        @if folder.files.is_empty() {
            p class="text-muted" {
                "Your Schemati folder is empty. In game, " code { "//save <name>" } " adds to it."
            }
        } @else {
            div class="overflow-x-auto" {
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
/// publish it, or rename or delete it, each of those in a dialog.
fn file_actions(index: usize, name: &str) -> Markup {
    let path = format!("/schemati/files/{name}");
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, "schem"));
    let rename = format!("rename-{index}");
    let delete = format!("delete-{index}");
    let dialog = "m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30";
    html! {
        div class="flex flex-wrap justify-end gap-2" {
            a href=(path) download=(name) class="btn btn-quiet" { "Download" }
            // Publishing previews it as the upload page does a chosen file.
            form action="/upload/preview" method="post" enctype="multipart/form-data" class="contents" {
                input type="hidden" name="schemati_file" value=(name);
                input type="hidden" name="standardize" value="yes";
                input type="hidden" name="whiten_gray" value="yes";
                button type="submit" class="btn btn-quiet" { "Publish" }
            }
            button type="button" commandfor=(rename) command="show-modal" class="btn btn-quiet" { "Rename" }
            button type="button" commandfor=(delete) command="show-modal" class="btn btn-quiet" { "Delete" }
        }
        dialog id=(rename) aria-labelledby={ (rename) "-heading" } class=(dialog) {
            form action={ (path) "/rename" } method="post" class="flex flex-col gap-4" {
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
        dialog id=(delete) aria-labelledby={ (delete) "-heading" } class=(dialog) {
            form action={ (path) "/delete" } method="post" class="flex flex-col gap-4" {
                h2 id={ (delete) "-heading" } class=(SECTION_TITLE) { "Delete " (name) "?" }
                p { "It is removed from your Schemati folder for good." }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-danger" { "Delete" }
                    button type="button" commandfor=(delete) command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}
