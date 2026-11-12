//! A build's list of versions: what each changed, comparing each with the
//! one before, and restoring an older one.

use maud::{Markup, html};

use crate::{
    build::Build,
    models::{user::User, version::Version},
};

use super::{SECTION_TITLE, diff};

/// A build's versions, newest first, each with what it changed from the
/// one before and a link comparing the two; each older one downloadable
/// for someone signed in, and restorable by `viewer` if `may_restore`.
/// Shown once it has more than one.
pub fn versions_section(
    s: &Build,
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
    // A compact button, beside each version.
    let action = "btn btn-quiet min-h-8 px-3 text-meta";
    html! {
        @if versions.len() > 1 {
            section aria-labelledby="versions-heading" class="mt-12 max-w-calm" {
                h2 id="versions-heading" class={ (SECTION_TITLE) " mb-3" } { "Versions" }
                ol class="flex flex-col" {
                    @for version in versions {
                        @let number = version.version;
                        li class="flex flex-wrap items-start justify-between gap-x-4 gap-y-2 border-b border-line py-3" {
                            div class="flex min-w-0 flex-1 basis-64 flex-col gap-1" {
                                div class="flex flex-wrap items-center gap-x-2 gap-y-1" {
                                    // Every number a pill of one width, so the
                                    // lines align; the current one filled.
                                    @if number == current {
                                        span class="min-w-9 rounded-control bg-brand px-1.5 text-center font-semibold text-white tabular-nums" title="Current version" {
                                            "v" (number) span class="sr-only" { ", current" }
                                        }
                                    } @else {
                                        span class="min-w-9 rounded-control bg-sidebar px-1.5 text-center font-semibold tabular-nums" { "v" (number) }
                                    }
                                    span class="text-meta text-muted tabular-nums" {
                                        (version.added_at.get(..10).unwrap_or(&version.added_at))
                                    }
                                    @if version.by_site {
                                        span class="badge badge-found" title="Made by the site itself" { "RedstoneDB" }
                                    } @else if let (Some(by), Some(username)) = (&version.added_by, &version.added_by_username) {
                                        (super::person_badge(by, &format!("/users/{username}")))
                                    }
                                }
                                @match &version.note {
                                    Some(note) => p class="line-clamp-2" title=(note) { (note) },
                                    None if number == 1 => p class="text-muted" { "First upload" },
                                    None => {},
                                }
                                @if let Some(counts) = version.changes {
                                    p class="text-meta text-muted" {
                                        (diff::summary(counts))
                                        @if let Some(lines) = diff::measured(measured(number - 1), measured(number)) {
                                            @for line in lines { " · " (line) }
                                        }
                                    }
                                }
                            }
                            div class="flex flex-wrap items-center gap-2" {
                                @if version.changes.is_some() {
                                    a href={ (path) "/diff?to=" (number) } class=(action) { "View diff" }
                                }
                                @if number != current {
                                    @if viewer.is_some() {
                                        a href={ (path) "/versions/" (number) "/download" } class=(action) { "Download" }
                                    } @else {
                                        button type="button" aria-disabled="true" title=(super::SIGN_IN_TO_DOWNLOAD) class=(action) { "Download" }
                                    }
                                    @if may_restore {
                                        form action={ (path) "/versions/" (number) "/restore" } method="post" class="contents" {
                                            button type="submit" class=(action) { "Restore" }
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
