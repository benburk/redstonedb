//! A person's page: their picture and name, their account if they have
//! one, the records they hold, and their builds, one list at a time.

use maud::{Markup, html};

use crate::{
    account::User,
    build::Build,
    leaderboard::Record,
    web::{
        href,
        leaderboard::people_view,
        people::{person::Person, users_view::role_badge},
        ui::{
            PAGE_TITLE, SECTION_TITLE, Segment,
            cards::{Viewing, cards},
            icons::{self, icon},
            layout::base,
            person, segmented, text,
        },
    },
};

/// What a person's page shows.
pub struct PersonPage<'a> {
    pub who: &'a Person<'a>,
    /// Their UUID, for their head, when Mojang knows them.
    pub uuid: Option<&'a str>,
    /// The records builds crediting them hold.
    pub records: &'a [&'a Record<'a>],
    /// Each list the page can show, in order.
    pub lists: &'a [List],
    /// Which of `lists` is shown.
    pub shown: usize,
    pub viewing: &'a Viewing,
}

/// One list of a person's builds: its name in the address, its label, and
/// its builds.
pub struct List {
    pub name: &'static str,
    pub label: &'static str,
    pub builds: Vec<Build>,
}

pub fn person(viewer: Option<&User>, page: &PersonPage) -> Markup {
    let PersonPage {
        who,
        uuid,
        records,
        lists,
        shown,
        viewing,
    } = *page;
    let account = who.account();
    let own = viewer
        .zip(account)
        .is_some_and(|(viewer, account)| viewer.id == account.id);
    let path = who.path();
    let tabs: Vec<Segment> = lists
        .iter()
        .enumerate()
        .map(|(i, list)| Segment {
            label: list.label,
            href: if i == 0 {
                path.clone()
            } else {
                format!("{path}?show={}", list.name)
            },
            chosen: i == shown,
            count: Some(list.builds.len()),
        })
        .collect();
    let list = &lists[shown];
    base(
        viewer,
        who.name(),
        &path,
        &html! {
            div class="mb-8 flex items-center gap-4" {
                @if let Some(picture) = person::face(uuid, account.and_then(User::avatar_url), 64) { (picture) }
                div class="min-w-0 flex-1" {
                    h1 class=(PAGE_TITLE) { (who.name()) }
                    p class="flex flex-wrap items-center gap-x-2 text-muted" {
                        // Each its own element, so the gap parts them.
                        @match account {
                            Some(account) => {
                                span { "@" (account.username) }
                                (role_badge(account.role))
                            }
                            None => span { "Not signed up" },
                        }
                        @if !records.is_empty() {
                            span aria-hidden="true" { "·" }
                            a href="#records-heading" { (text::count(records.len(), "record")) }
                        }
                    }
                    // A name an admin has yet to confirm has a page of its
                    // own, the player's.
                    @if let Person::Account(account) = who && let Some(name) = &account.minecraft_name {
                        p {
                            "Minecraft: "
                            a href=(href::player(name)) { (name) }
                            span class="text-meta text-muted" { " · not yet confirmed" }
                        }
                    }
                }
                @if own {
                    a href="/settings" class="btn btn-quiet shrink-0 gap-2 self-start hover:no-underline" {
                        (icon(icons::SETTINGS, "size-4"))
                        "Settings"
                    }
                }
            }
            @if !records.is_empty() {
                section aria-labelledby="records-heading" class="mb-10" {
                    h2 id="records-heading" class={ (SECTION_TITLE) " mb-3" } { "Records" }
                    (people_view::record_table(records, who.credited_as()))
                }
            }
            nav aria-label="Builds" class="mb-5" { (segmented(&tabs)) }
            @if list.builds.is_empty() {
                p class="text-muted" { (nothing(who, list.name, own)) }
            } @else {
                (cards(&list.builds, &[], viewing))
            }
        },
    )
}

/// What an empty list named `list` says in place of its builds, on one's
/// `own` page or another's.
fn nothing(who: &Person, list: &str, own: bool) -> Markup {
    html! {
        @match (list, who.credited_as()) {
            ("all" | "built" | "contributed", None) if own => {
                "Give your Minecraft name in " a href="/settings" { "Settings" } " to list the builds credited to it."
            }
            ("built" | "contributed", None) => { "No Minecraft name given, so their builds can't be listed." }
            ("built", Some(name)) => { "No builds credit " (name) " as author yet." }
            ("favourites", _) => { "Star a build on its page to keep it here." }
            _ => { "None yet." }
        }
    }
}
