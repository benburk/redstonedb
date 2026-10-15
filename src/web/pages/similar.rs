//! Finding the stored builds most like a file, without uploading it.

use maud::{Markup, html};

use crate::models::{schematic::Schematic, user::User};

use super::{Connection, PAGE_TITLE, SECTION_TITLE, base, facts, schemati};

/// What a search found: the file's name and the builds most like it, with
/// how alike, the most alike first.
pub struct Found<'a> {
    pub file: &'a str,
    pub builds: &'a [(Schematic, u32)],
}

/// The search form, for `viewer` if logged in, with their Schemati files
/// to choose from; what it `found`, if it ran; and `message`, why it could
/// not.
pub fn similar_search(
    viewer: Option<&User>,
    connection: Option<&Connection>,
    found: Option<&Found>,
    message: &str,
) -> Markup {
    base(
        viewer,
        "Find similar",
        "/similar",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-2" } { "Find similar builds" }
                p class="mb-6 max-w-prose text-muted" {
                    "Which builds share the most redstone with a file, turned or mirrored as need be. Nothing is checked or kept."
                }
                form action="/similar" method="post" enctype="multipart/form-data"
                    fx-action="/similar" fx-method="post" fx-target="#similar-results"
                    class="group mb-10 flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
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
                    div class="flex items-center gap-3" {
                        button type="submit" class="btn btn-primary" { "Search" }
                        // Shown while the search runs, as its button is busy.
                        span class="hidden text-meta text-muted group-has-[[aria-busy=true]]:inline" {
                            "Comparing with every build…"
                        }
                    }
                }
                (similar_results(found, message))
            }
        },
    )
}

/// What a search found, or why it could not run: the part of the page a
/// search through fixi replaces.
pub fn similar_results(found: Option<&Found>, message: &str) -> Markup {
    html! {
        div id="similar-results" {
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
        section aria-labelledby="results-heading" {
            h2 id="results-heading" class={ (SECTION_TITLE) " mb-1" } { "Most like " (found.file) }
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
}
