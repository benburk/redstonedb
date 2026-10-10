//! Uploading a new version of a build, and its list of versions.

use maud::{Markup, html};

use crate::{
    models::{schematic::Schematic, user::User, version::Version},
    simulate::Report,
    version::{Comparison, MAX_CHANGED, MIN_SAME_PERCENT},
};

use super::{
    PAGE_TITLE, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, base, beside_viewer,
    grouped,
};

/// The page for choosing a build's new version.
pub fn new_version(user: &User, s: &Schematic, message: &str) -> Markup {
    let path = format!("/schematics/{}", s.id);
    base(
        Some(user),
        &format!("New version of {}", s.name()),
        &path,
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-2" } { "New version of " (s.name()) }
                p class="mb-6 max-w-prose text-muted" {
                    "For fixing a build: the new file replaces this one, keeping its details, credits, and downloads, "
                    "and the old one stays listed under it. It must be as wide and deep as the build, though it may be taller; "
                    "have the same inputs and outputs; and keep at least " (MIN_SAME_PERCENT) "% of its blocks, or change at most "
                    (MAX_CHANGED) ". Anything more is a build of its own: "
                    a href="/upload" { "upload it" } " instead."
                }
                form action={ (path) "/versions/preview" } method="post" enctype="multipart/form-data"
                    class="mb-10 flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
                    label class="field" {
                        span { "Schematic file" }
                        input type="file" name="file" accept=".schem" class="input py-1.5";
                    }
                    label class="field" {
                        span { "Or a Schemati link" }
                        input type="url" name="schemati" inputmode="url" autocomplete="off"
                            placeholder="https://schemati.openredstone.org/download?file=…" class="input";
                    }
                    label class="flex items-start gap-2" {
                        input type="checkbox" name="standardize" value="yes" checked class="mt-1 size-4 appearance-auto";
                        span { "Standardize it, as when uploading." }
                    }
                    @if !message.is_empty() {
                        p role="alert" class="text-danger" { (message) }
                    }
                    div class="flex items-center gap-4" {
                        button type="submit" class="btn btn-primary" { "Preview" }
                        a href=(path) { "Cancel" }
                    }
                }
            }
        },
    )
}

/// What a new version's preview shows.
pub struct VersionPreview<'a> {
    pub s: &'a Schematic,
    /// The prepared file, base64, carried to publishing.
    pub data: &'a str,
    pub comparison: Comparison,
    /// Why it cannot be published; empty when it can.
    pub refusals: &'a [String],
    /// The number it would have.
    pub version: i64,
    /// The current version's simulation result, to compare.
    pub current: Option<&'a Report>,
    pub note: &'a str,
}

/// A new version, checked against the build, with its note to fill in.
pub fn version_preview(user: &User, p: &VersionPreview) -> Markup {
    let s = p.s;
    let path = format!("/schematics/{}", s.id);
    let permille = p.comparison.same_permille();
    base(
        Some(user),
        &format!("New version of {}", s.name()),
        &path,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { "Version " (p.version) " of " (s.name()) }
            (beside_viewer(&format!("data:application/octet-stream;base64,{}", p.data), &html! {
                div class="flex flex-col gap-3" {
                    p {
                        "It changes " (blocks(p.comparison.changed)) " and keeps "
                        (grouped(p.comparison.same as u64)) ", " (permille / 10) "." (permille % 10) "% of them."
                    }
                    @if let Some(report) = p.current {
                        p class="text-muted" { "Version " (p.version - 1) "'s simulation: " (summary(report)) }
                    }
                    a href={ (path) "/versions/new" } { "Choose a different file" }
                }
                @if !p.refusals.is_empty() {
                    section aria-labelledby="refusals-heading" role="alert" class="flex flex-col gap-2" {
                        h2 id="refusals-heading" class={ (SECTION_TITLE) " text-danger" } { "It can't replace this build" }
                        ul class="flex list-disc flex-col gap-1.5 pl-5" {
                            @for refusal in p.refusals { li { (refusal) } }
                        }
                    }
                }
                form action={ (path) "/versions" } method="post" data-simulate="/upload/simulation" class="flex flex-col gap-4" {
                    input type="hidden" name="data" value=(p.data);
                    // For the simulation the build's category and width call for.
                    input type="hidden" name="category" value=(s.category);
                    input type="hidden" name="width" value=(s.width.map(|w| w.to_string()).unwrap_or_default());
                    @if !s.conventional {
                        input type="hidden" name="unconventional" value="yes";
                    }
                    label class="field" {
                        span { "What it fixes " span class="font-normal" { "(optional)" } }
                        textarea name="note" rows="3" maxlength="500" class="input py-2 leading-prose" { (p.note) }
                    }
                    div class="flex items-center gap-4" {
                        button type="submit" class="btn btn-primary" disabled[!p.refusals.is_empty()] {
                            "Publish version " (p.version)
                        }
                        a href=(path) { "Cancel" }
                    }
                }
            }))
            // Filled by form.js with the new version's simulation.
            div id="simulation-results" {}
            script defer src=(super::super::form_js_url()) {}
        },
    )
}

/// A count of blocks, such as `1 block` or `1,024 blocks`.
fn blocks(count: usize) -> String {
    match count {
        1 => "1 block".to_owned(),
        n => format!("{} blocks", grouped(n as u64)),
    }
}

/// A simulation result in a few words, such as `all 131,072 cases right`.
fn summary(report: &Report) -> String {
    match report {
        Report::Ran(run) if run.wrong == 0 => {
            let timing = match &run.timing {
                Some(timing) if timing.synced => ", synced",
                Some(_) => ", not synced",
                None => "",
            };
            format!("all {} cases right{timing}.", grouped(run.cases))
        }
        Report::Ran(run) => format!(
            "{} of {} cases wrong.",
            grouped(run.wrong),
            grouped(run.cases)
        ),
        Report::Broken { reason } | Report::Skipped { reason } => {
            reason.clone()
        }
    }
}

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
