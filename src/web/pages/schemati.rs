//! The page for the viewer's own Schemati folder, and the parts of the
//! settings page that connect it.

use maud::{Markup, html};

use serde::Deserialize;

use crate::{
    external::schemati::Folder,
    models::user::{SchematiLink, User},
};

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

/// Where the viewer's Schemati files are offered, which the slot they
/// load into names in asking for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Offered {
    /// The upload page's form.
    Upload,
    /// A details page's fields for choosing another file, which stand
    /// outside its form.
    Details,
    /// The dialog for finding similar builds.
    Similar,
}

impl Offered {
    /// The form the files belong to when they stand outside it.
    pub(super) fn form(self) -> Option<&'static str> {
        match self {
            Offered::Details => Some("details-form"),
            Offered::Upload | Offered::Similar => None,
        }
    }

    /// Where the files are asked for.
    fn url(self) -> &'static str {
        match self {
            Offered::Upload => "/schemati/picker?for=upload",
            Offered::Details => "/schemati/picker?for=details",
            Offered::Similar => "/schemati/picker?for=similar",
        }
    }
}

/// The id of the slot the viewer's Schemati files load into.
const SLOT: &str = "schemati-files";

/// Where the viewer's Schemati files go, decided from what the site keeps
/// of their session, so the page asks Schemati nothing: a note saying how
/// to connect it, or that it expired, or while it seems to work, a slot
/// that asks for the files once the page is in, as `own_file_field` shows
/// them. Without scripts, the slot says where else to find them.
pub(super) fn own_files_slot(
    link: Option<&SchematiLink>,
    offered: Offered,
) -> Markup {
    match link {
        None => own_file_field(&Connection::Unset, offered),
        Some(link) if link.expired => {
            own_file_field(&Connection::Forgotten, offered)
        }
        Some(_) => html! {
            div id=(SLOT) fx-action=(offered.url()) fx-trigger="load" role="status" {
                p class="text-muted" { "Loading your Schemati files…" }
                noscript {
                    p class="text-muted" {
                        a href="/schemati" { "See your Schemati files" } " on their own page."
                    }
                }
            }
        },
    }
}

/// The viewer's Schemati files to choose one from, the newest chosen
/// already when Schemati says which it is, or the only one; or why there
/// are none to choose, with a button to ask again when Schemati could not
/// be asked. They belong to the form `offered` names when they stand
/// outside it.
pub fn own_file_field(connection: &Connection, offered: Offered) -> Markup {
    let note = "text-muted";
    let form = offered.form();
    html! {
        @match connection {
            Connection::Open(folder) if !folder.files.is_empty() => {
                fieldset {
                    legend class="sr-only" { "Your Schemati files" }
                    div class="flex max-h-80 flex-col overflow-y-auto" {
                        @for (index, file) in folder.files.iter().enumerate() {
                            label class="flex min-h-row cursor-pointer items-center gap-3 rounded-control px-2 hover:bg-selection has-checked:bg-selection has-focus-visible:outline-2 has-focus-visible:outline-brand" {
                                input type="radio" name="schemati_file" value=(file.name) form=[form]
                                    checked[index == 0 && (folder.dated() || folder.files.len() == 1)]
                                    class="sr-only";
                                span class="text-muted" { (super::icons::icon(super::icons::FILE, "size-4")) }
                                span class="min-w-0 flex-1 truncate" { (file.name) }
                                @if let Some(saved) = file.modified {
                                    span class="shrink-0 text-meta text-muted tabular-nums" { (ago(saved)) }
                                }
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
            // Asking again takes this one's place.
            Connection::Unreachable(why) => div id=(SLOT) class="flex flex-wrap items-center gap-3" {
                p class=(note) { "Schemati: " (why) }
                button type="button" fx-action=(offered.url()) fx-target={ "#" (SLOT) }
                    class="btn btn-quiet" { "Retry" }
            },
        }
    }
}

/// How long ago `millis`, milliseconds since 1970, was, such as `5 min
/// ago`, or its date once a month has passed.
pub(super) fn ago(millis: u64) -> String {
    let now = time::OffsetDateTime::now_utc();
    let then = i64::try_from(millis / 1000)
        .ok()
        .and_then(|secs| time::OffsetDateTime::from_unix_timestamp(secs).ok());
    let Some(then) = then else {
        return String::new();
    };
    let minutes = (now - then).whole_minutes().max(0);
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        1440..2880 => "yesterday".to_owned(),
        2880..43200 => format!("{} days ago", minutes / 1440),
        _ => then.date().to_string(),
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
                        @for (index, file) in folder.files.iter().enumerate() {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                    span class="block font-mono break-all" { (file.name) }
                                    @if let Some(saved) = file.modified {
                                        span class="block text-meta text-muted" { "Saved " (ago(saved)) }
                                    }
                                }
                                td class=(TABLE_CELL) { (file_actions(index, &file.name)) }
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
        dialog id=(rename) aria-labelledby={ (rename) "-heading" } class="dialog w-full max-w-form" {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external::schemati::File;

    /// Each file shows when it was saved, newest first and chosen.
    #[test]
    fn the_picker_shows_when_each_file_was_saved() {
        let now = u64::try_from(
            time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
        )
        .unwrap();
        let folder = Folder {
            player: "embizone".to_owned(),
            files: vec![
                File {
                    name: "new.schem".to_owned(),
                    modified: Some(now - 5 * 60_000),
                },
                File {
                    name: "old.schem".to_owned(),
                    modified: Some(now - 3 * 86_400_000),
                },
            ],
        };
        let page = own_file_field(&Connection::Open(&folder), Offered::Upload)
            .into_string();
        assert!(page.contains("5 min ago"), "{page}");
        assert!(page.contains("3 days ago"), "{page}");
        assert!(page.contains(r#"value="new.schem" checked"#), "{page}");
    }

    /// The slot for the viewer's files is decided from what the site
    /// keeps of their session: how to connect it, that it expired, or,
    /// while it seems to work, the files to load once the page is in.
    #[test]
    fn the_slot_asks_for_the_files_only_while_the_session_works() {
        let unset = own_files_slot(None, Offered::Upload).into_string();
        assert!(unset.contains("Connect your Schemati"), "{unset}");
        assert!(!unset.contains("fx-action"), "{unset}");

        let mut link = SchematiLink {
            player: Some("embizone".to_owned()),
            checked: Some(0),
            expired: true,
        };
        let expired =
            own_files_slot(Some(&link), Offered::Upload).into_string();
        assert!(expired.contains("session expired"), "{expired}");
        assert!(expired.contains(r#"href="/settings""#), "{expired}");
        assert!(!expired.contains("fx-action"), "{expired}");

        link.expired = false;
        let loading =
            own_files_slot(Some(&link), Offered::Details).into_string();
        assert!(
            loading.contains(r#"fx-action="/schemati/picker?for=details""#),
            "{loading}"
        );
        assert!(loading.contains(r#"fx-trigger="load""#), "{loading}");
        assert!(loading.contains("Loading your Schemati files"), "{loading}");
        assert!(loading.contains(r#"href="/schemati""#), "{loading}");
    }
}
