//! A build's list of versions: what each changed, comparing each with the
//! one before, and restoring an older one.

use maud::{Markup, html};

use crate::models::{schematic::Schematic, user::User, version::Version};

use super::{SECTION_TITLE, TABLE_CELL, TABLE_HEAD, diff};

/// A build's versions, newest first, each with what it changed from the
/// one before and a link comparing the two; each older one downloadable
/// for someone signed in, and restorable by `viewer` if `may_restore`.
/// Shown once it has more than one.
pub fn versions_section(
    s: &Schematic,
    versions: &[Version],
    viewer: Option<&User>,
    may_restore: bool,
) -> Markup {
    let current = versions.first().map_or(1, |v| v.version);
    let path = format!("/schematics/{}", s.id);
    let measured = |number: i64| {
        versions
            .iter()
            .find(|v| v.version == number)
            .and_then(|v| v.measures.as_ref())
    };
    html! {
        @if versions.len() > 1 {
            section aria-labelledby="versions-heading" class="mt-12 max-w-calm" {
                h2 id="versions-heading" class={ (SECTION_TITLE) " mb-3" } { "Versions" }
                // Relative, so the hidden caption and heading, placed
                // absolutely, scroll with the table rather than widening
                // the page.
                div class="relative overflow-x-auto" {
                    table class="w-full border-collapse" {
                        caption class="sr-only" { "Versions" }
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Version" }
                                th scope="col" class=(TABLE_HEAD) { "Date" }
                                th scope="col" class=(TABLE_HEAD) { "By" }
                                th scope="col" class=(TABLE_HEAD) { "What it fixed" }
                                th scope="col" class=(TABLE_HEAD) { "Changes" }
                                th scope="col" class=(TABLE_HEAD) { span class="sr-only" { "Actions" } }
                            }
                        }
                        tbody {
                            @for version in versions {
                                @let number = version.version;
                                tr {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal tabular-nums" } {
                                        (number)
                                        @if number == current { span class="text-muted" { ", current" } }
                                    }
                                    td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap" } {
                                        (version.added_at.get(..10).unwrap_or(&version.added_at))
                                    }
                                    td class=(TABLE_CELL) {
                                        @if version.by_site {
                                            span class="badge badge-found" title="Made by the site itself" { "RedstoneDB" }
                                        } @else {
                                            (version.added_by.as_deref().unwrap_or(""))
                                        }
                                    }
                                    td class=(TABLE_CELL) {
                                        @match &version.note {
                                            Some(note) => (note),
                                            None if number == 1 => span class="text-muted" { "First upload" },
                                            None => "",
                                        }
                                    }
                                    td class={ (TABLE_CELL) " text-meta" } {
                                        @if let Some(counts) = version.changes {
                                            a href={ (path) "/diff?to=" (number) } class="whitespace-nowrap" { (diff::summary(counts)) }
                                            @if let Some(lines) = diff::measured(measured(number - 1), measured(number)) {
                                                @for line in lines { div class="text-muted" { (line) } }
                                            }
                                        }
                                    }
                                    td class={ (TABLE_CELL) " whitespace-nowrap" } {
                                        @if number != current {
                                            div class="flex items-center gap-3" {
                                                (super::download_link(&format!("{path}/versions/{number}/download"), "Download", viewer.is_some()))
                                                @if may_restore {
                                                    form action={ (path) "/versions/" (number) "/restore" } method="post" class="contents" {
                                                        button type="submit" class="cursor-pointer text-brand hover:underline" { "Restore" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
