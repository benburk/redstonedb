//! The moderation log: what happened on the site, newest first, a day at a
//! time.

use std::collections::HashMap;

use maud::{Markup, html};
use uuid::Uuid;

use crate::models::{
    activity::{self, Change, Event, Filter, Kind, Page},
    schematic::Build,
    user::User,
};

use super::{
    PAGE_TITLE, SWITCH, SWITCHES, base,
    build::long_date,
    icons::{self, icon},
};

/// What the log page shows.
pub struct ActivityLog<'a> {
    pub viewer: &'a User,
    pub filter: Filter,
    /// The newest time the page shows, when it is an older page.
    pub before: Option<&'a str>,
    /// Why a revert was refused; empty for none.
    pub message: &'a str,
    pub page: &'a Page,
    /// The accounts its events name, by id.
    pub users: &'a HashMap<Uuid, User>,
    /// The builds its events name, by id.
    pub builds: &'a HashMap<String, Build>,
}

/// The log's address for `filter`, from time `before` if given.
pub fn activity_href(filter: Filter, before: Option<&str>) -> String {
    let mut query = Vec::new();
    if filter != Filter::All {
        query.push(format!("show={}", filter.name()));
    }
    if let Some(before) = before {
        query.push(format!("before={before}"));
    }
    if query.is_empty() {
        "/log".to_owned()
    } else {
        format!("/log?{}", query.join("&"))
    }
}

/// The moderation log, with a button for each kind of event it can show
/// alone.
pub fn activity(log: &ActivityLog) -> Markup {
    // The events in runs of one day each.
    let mut days: Vec<(&str, Vec<&Event>)> = Vec::new();
    for event in &log.page.events {
        let day = event.at.get(..10).unwrap_or(&event.at);
        match days.last_mut() {
            Some((last, events)) if *last == day => events.push(event),
            _ => days.push((day, vec![event])),
        }
    }
    base(
        Some(log.viewer),
        "Moderation log",
        "/log",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-2" } { "Moderation log" }
            p class="mb-6 max-w-prose text-muted" {
                "Accounts made, builds uploaded, details edited, files downloaded, and builds deleted, newest first. Times are UTC."
            }
            @if !log.message.is_empty() {
                p role="alert" class="mb-6 text-danger" { (log.message) }
            }
            nav aria-label="Events" class="mb-6" {
                div class={ (SWITCHES) " w-fit" } {
                    @for filter in Filter::ALL {
                        a href=(activity_href(filter, None)) class=(SWITCH)
                            aria-current=[(filter == log.filter).then_some("page")] {
                            (filter.label())
                        }
                    }
                }
            }
            @if days.is_empty() {
                p class="text-muted" { "Nothing yet." }
            }
            div class="flex max-w-calm flex-col gap-8" {
                @for (day, events) in &days {
                    section {
                        h2 class="mb-2 text-meta font-semibold text-muted" { (long_date(day)) }
                        ol class="flex flex-col" {
                            @for event in events {
                                (row(log, event))
                            }
                        }
                    }
                }
            }
            @if let Some(older) = &log.page.older {
                a href=(activity_href(log.filter, Some(older))) class="btn btn-quiet mt-8" { "Older" }
            }
        },
    )
}

/// The icon and its colours for an event of `kind`.
fn mark(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::SignUp => (icons::USER, "bg-found text-on-found"),
        Kind::Upload | Kind::Version => {
            (icons::UPLOAD, "bg-selection text-brand")
        }
        Kind::Edit => (icons::PENCIL, "bg-restriction text-on-restriction"),
        Kind::Download => (icons::DOWNLOAD, "bg-other text-on-other"),
        Kind::Delete => (icons::TRASH, "bg-danger/10 text-danger"),
        Kind::Restore => (icons::UNDO, "bg-success/10 text-success"),
    }
}

/// One event: who did what to which build, at what time, with what it
/// changed or took under it.
fn row(log: &ActivityLog, event: &Event) -> Markup {
    let (mark_icon, colours) = mark(event.kind);
    let who = event.user.and_then(|id| log.users.get(&id));
    let build = event.schematic.as_deref().map(|id| build_link(log, id));
    let version = event.version.unwrap_or(1);
    let time = event.at.get(11..16).unwrap_or_default();
    html! {
        li class="flex items-start gap-3 border-b border-line py-2.5" {
            span class={ "mt-0.5 inline-flex size-7 shrink-0 items-center justify-center rounded-full " (colours) } {
                (icon(mark_icon, "size-4"))
            }
            div class="min-w-0 flex-1" {
                p {
                    @match event.kind {
                        Kind::SignUp => { (account(who)) " signed up" }
                        Kind::Upload => {
                            @if who.is_some() {
                                (account(who)) " uploaded " (build.unwrap_or_default())
                            } @else {
                                (build.unwrap_or_default()) " was imported"
                            }
                        }
                        Kind::Version => {
                            (account(who))
                            @if let Some(restored) = event.restores {
                                " restored version " (restored) " of "
                            } @else {
                                " added version " (version) " of "
                            }
                            (build.unwrap_or_default())
                        }
                        Kind::Edit => { (account(who)) " edited " (build.unwrap_or_default()) }
                        Kind::Download => { (account(who)) " downloaded " (build.unwrap_or_default()) }
                        Kind::Delete => {
                            (account(who)) " deleted " (build.unwrap_or_default())
                            (restore_button(log, event))
                        }
                        Kind::Restore => { (account(who)) " restored " (build.unwrap_or_default()) }
                    }
                }
                @if let Some(note) = note(event, version) {
                    p class="text-meta text-muted" { (note) }
                }
                @if !event.changes.is_empty() {
                    ul class="mt-1 flex flex-col gap-1 text-meta" {
                        @for change in &event.changes {
                            li { (change_row(log, event, change)) }
                        }
                    }
                }
            }
            time datetime=(event.at) class="shrink-0 pt-0.5 text-meta text-muted tabular-nums" { (time) }
        }
    }
}

/// What an event says under it: what an edit changed, a version's note,
/// or what a download took.
fn note(event: &Event, version: i64) -> Option<String> {
    match event.kind {
        Kind::SignUp
        | Kind::Upload
        | Kind::Edit
        | Kind::Delete
        | Kind::Restore => None,
        Kind::Version => event.detail.clone(),
        Kind::Download => Some(match event.detail.as_deref() {
            Some("part") => format!("A stacking part, version {version}"),
            Some("version") => format!("Earlier version {version}"),
            Some("schemati") => format!("Sent to Schemati, version {version}"),
            Some("diff") => format!("Side of a diff, version {version}"),
            _ => format!("Version {version}"),
        }),
    }
}

/// Whether `value` reads on one line beside its detail's name.
fn short(value: Option<&str>) -> bool {
    value.is_none_or(|v| v.chars().count() <= 80 && !v.contains('\n'))
}

/// A detail's value before or after an edit, or none.
fn value(value: Option<&str>, colours: &str) -> Markup {
    html! {
        @match value {
            Some(v) => span class={ "rounded-[3px] px-1 " (colours) } { (v) },
            None => span class="text-muted italic" { "none" },
        }
    }
}

/// A button putting back `change` of edit `event`, when its build still
/// has the detail as the edit left it; nothing otherwise.
fn revert_button(log: &ActivityLog, event: &Event, change: &Change) -> Markup {
    let build = event.schematic.as_ref().and_then(|id| log.builds.get(id));
    let (Some(edit), Some(s)) = (event.edit, build) else {
        return html! {};
    };
    if s.entry.deleted || !activity::revertible(s, change) {
        return html! {};
    }
    let action = format!("/log/edits/{edit}/revert");
    html! {
        form action=(action) method="post" class="inline" {
            input type="hidden" name="detail" value=(change.detail);
            input type="hidden" name="show" value=(log.filter.name());
            @if let Some(before) = log.before {
                input type="hidden" name="before" value=(before);
            }
            button type="submit" class="ml-1 rounded-control px-1.5 text-brand hover:bg-selection"
                title="Put this back as it was before this edit" {
                "Revert"
            }
        }
    }
}

/// A button restoring the build deleted in `event`, while it is still
/// deleted; nothing otherwise.
fn restore_button(log: &ActivityLog, event: &Event) -> Markup {
    let build = event.schematic.as_ref().and_then(|id| log.builds.get(id));
    let Some(s) = build.filter(|s| s.entry.deleted) else {
        return html! {};
    };
    let action = format!("/log/deletions/{}/restore", s.id);
    html! {
        form action=(action) method="post" class="inline" {
            input type="hidden" name="show" value=(log.filter.name());
            @if let Some(before) = log.before {
                input type="hidden" name="before" value=(before);
            }
            button type="submit" class="ml-1 rounded-control px-1.5 text-brand hover:bg-selection"
                title="Put this build back on the site" {
                "Restore"
            }
        }
    }
}

/// One detail an edit changed: on one line, before and after, when both
/// are short; else folded, each in full, as a description is. One that can
/// be put back has a button to.
fn change_row(log: &ActivityLog, event: &Event, change: &Change) -> Markup {
    let (from, to) = (change.from.as_deref(), change.to.as_deref());
    let (removed, added) = (
        "bg-danger/10 text-danger line-through",
        "bg-success/10 text-success",
    );
    html! {
        @if short(from) && short(to) {
            span class="text-muted" { (change.detail) ": " }
            (value(from, removed)) " → " (value(to, added))
            (revert_button(log, event, change))
        } @else {
            details {
                summary class="cursor-pointer text-muted" {
                    (change.detail) " changed"
                    (revert_button(log, event, change))
                }
                div class="mt-1 grid gap-2 sm:grid-cols-2" {
                    @for (label, text, colours) in [("Before", from, "border-danger/40"), ("After", to, "border-success/40")] {
                        div class={ "rounded-control border p-2 " (colours) } {
                            p class="mb-1 font-semibold text-muted" { (label) }
                            @match text {
                                Some(text) => p class="whitespace-pre-wrap break-words text-ink" { (text) },
                                None => p class="text-muted italic" { "none" },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An account, linking to its page; for none, a deleted one.
fn account(user: Option<&User>) -> Markup {
    html! {
        @if let Some(user) = user {
            a href={ "/users/" (user.username) } class="font-medium" { (user.name()) }
        } @else {
            span class="text-muted" { "A deleted account" }
        }
    }
}

/// Build `id`, by its title and linking to its page; a deleted one by its
/// title alone, and one removed for good as such.
fn build_link(log: &ActivityLog, id: &str) -> Markup {
    match log.builds.get(id) {
        Some(s) if s.entry.deleted => html! {
            span class="text-muted" { (s.title()) " (deleted)" }
        },
        Some(s) => html! {
            a href={ "/schematics/" (s.id) } { (s.title()) }
        },
        None => html! { span class="text-muted" { "a removed build" } },
    }
}
