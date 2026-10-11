//! A build's list of versions.

use maud::{Markup, html};

use crate::models::{schematic::Schematic, version::Version};

use super::{SECTION_TITLE, TABLE_CELL, TABLE_HEAD};

/// A build's versions, newest first, each older one downloadable; shown
/// once it has more than one.
pub fn versions_section(s: &Schematic, versions: &[Version]) -> Markup {
    let current = versions.first().map_or(1, |v| v.version);
    html! {
        @if versions.len() > 1 {
            section aria-labelledby="versions-heading" class="mt-12 max-w-calm" {
                h2 id="versions-heading" class={ (SECTION_TITLE) " mb-3" } { "Versions" }
                div class="overflow-x-auto" {
                    table class="w-full border-collapse" {
                        caption class="sr-only" { "Versions" }
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Version" }
                                th scope="col" class=(TABLE_HEAD) { "Date" }
                                th scope="col" class=(TABLE_HEAD) { "By" }
                                th scope="col" class=(TABLE_HEAD) { "What it fixed" }
                                th scope="col" class=(TABLE_HEAD) { span class="sr-only" { "File" } }
                            }
                        }
                        tbody {
                            @for version in versions {
                                tr {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal tabular-nums" } {
                                        (version.version)
                                        @if version.version == current { span class="text-muted" { ", current" } }
                                    }
                                    td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap" } {
                                        (version.added_at.get(..10).unwrap_or(&version.added_at))
                                    }
                                    td class=(TABLE_CELL) { (version.added_by.as_deref().unwrap_or("")) }
                                    td class=(TABLE_CELL) {
                                        @match &version.note {
                                            Some(note) => (note),
                                            None if version.version == 1 => span class="text-muted" { "First upload" },
                                            None => "",
                                        }
                                    }
                                    td class=(TABLE_CELL) {
                                        @if version.version != current {
                                            a href={ "/schematics/" (s.id) "/versions/" (version.version) "/download" } { "Download" }
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
