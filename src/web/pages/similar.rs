//! Finding the stored builds most like a file, without uploading it: a
//! dialog off the home page's search, and the builds it finds in place of
//! the home page's tiles.

use maud::{Markup, html};

use crate::models::{schematic::Schematic, user::User};

use super::{Connection, PAGE_TITLE, SECTION_TITLE, facts, schemati};

/// What a search found: the file's name and the builds most like it, with
/// how alike, the most alike first.
pub struct Found<'a> {
    pub file: &'a str,
    pub builds: &'a [(Schematic, u32)],
}

/// The dialog to choose a file in, opening as it arrives, with `viewer`'s
/// Schemati files to choose from when they have a `connection`. It sends
/// to the home page's results, and stays open until they come.
pub fn similar_dialog(connection: Option<&Connection>) -> Markup {
    html! {
        dialog id="similar" data-show aria-labelledby="similar-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action="/similar" method="post" enctype="multipart/form-data" data-wait
                fx-action="/similar" fx-method="post" fx-target="#listing"
                class="group flex flex-col gap-4" {
                div {
                    h2 id="similar-heading" class={ (SECTION_TITLE) " mb-1" } { "Find similar builds" }
                    p class="text-meta text-muted" {
                        "Which builds share the most redstone with a file, turned or mirrored as need be. Nothing is checked or kept."
                    }
                }
                label class="field" {
                    span { "Schematic file" }
                    input type="file" name="file" accept=".schem" class="input py-1.5";
                }
                label class="field" {
                    span { "Or a Schemati link" }
                    input type="url" name="schemati" inputmode="url" autocomplete="off"
                        placeholder="https://schemati.openredstone.org/download?file=…" class="input";
                }
                @if let Some(connection) = connection {
                    (schemati::own_file_field(connection, None))
                }
                div class="flex flex-wrap items-center gap-3" {
                    button type="submit" class="btn btn-primary" { "Search" }
                    button type="button" commandfor="similar" command="close" class="btn btn-quiet" { "Cancel" }
                    // Shown while the search runs, as its button is busy.
                    span class="hidden text-meta text-muted group-has-[[aria-busy=true]]:inline" {
                        "Comparing with every build…"
                    }
                }
            }
        }
    }
}

/// What a search found, or `message`, why it could not run: the part of
/// the home page below its search.
pub fn similar_results(found: Option<&Found>, message: &str) -> Markup {
    html! {
        div id="listing" {
            div class="mb-4 flex flex-wrap items-baseline gap-x-4 gap-y-1" {
                h1 class={ (PAGE_TITLE) " mr-auto break-words" } {
                    @if let Some(found) = found { "Most like " (found.file) } @else { "Find similar builds" }
                }
                a href="/" { "Clear" }
            }
            @if !message.is_empty() {
                p class="text-danger" { (message) }
            }
            @if let Some(found) = found {
                (results(found))
            }
        }
    }
}

/// The builds most like the file, each with a thumbnail, its facts, and
/// how alike, as a share of their redstone that lines up.
fn results(found: &Found) -> Markup {
    html! {
        @if found.builds.is_empty() {
            p class="text-muted" { "No build shares any redstone with it." }
        } @else {
            p class="mb-4 text-meta text-muted" { "Share of their redstone that lines up." }
            ol class="flex flex-col" {
                @for (s, permille) in found.builds {
                    li class="group relative flex items-center gap-4 border-b border-line py-3" {
                        img src={ "/schematics/" (s.id) "/thumbnail.png?v=" (&s.sha256[..12.min(s.sha256.len())]) }
                            alt="" loading="lazy" width="96" height="72"
                            class="aspect-[4/3] w-24 shrink-0 rounded-control bg-sidebar object-contain";
                        div class="flex min-w-0 flex-1 flex-col gap-0.5" {
                            a href={ "/schematics/" (s.id) }
                                class="font-semibold text-ink group-hover:text-brand hover:no-underline after:absolute after:inset-0" { (s.name()) }
                            p class="text-meta text-muted tabular-nums" { (s.category_label()) " · " (facts(s)) }
                        }
                        span class="text-section font-semibold tabular-nums" { (super::percent(*permille)) }
                    }
                }
            }
        }
    }
}

/// The home page showing what a search for similar builds `found`, or
/// `message`, why it could not run.
pub fn similar_page(
    viewer: Option<&User>,
    found: Option<&Found>,
    message: &str,
) -> Markup {
    super::listing::home_page(
        viewer,
        "/",
        None,
        &similar_results(found, message),
    )
}
