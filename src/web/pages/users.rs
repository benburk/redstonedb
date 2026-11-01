//! Accounts: logging in, the admin's list of users, and an account's
//! profile.

use maud::{Markup, html};

use crate::models::{
    schematic::Schematic,
    user::{Role, User},
};

use super::{
    PAGE_TITLE, SECTION_TITLE, TABLE_HEAD, base,
    icons::{self, icon},
    listing::cards,
    player::credits,
    player_href,
};

/// The builds `viewer` has favourited, the newest favourite first,
/// pictured in their `palette`.
pub fn favorites(
    viewer: &User,
    builds: &[Schematic],
    palette: Option<&str>,
) -> Markup {
    base(
        Some(viewer),
        "Favourites",
        "/favorites",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { "Favourites" }
            @if builds.is_empty() {
                p class="text-muted" { "Star a build on its page to keep it here." }
            } @else {
                (cards(builds, &[], palette))
            }
        },
    )
}

/// The login page. `enabled` is false when the Discord settings are
/// missing; `message` explains a failed login.
pub fn login(enabled: bool, message: &str) -> Markup {
    base(
        None,
        "Log in",
        "/sessions/new",
        &html! {
            section class="mx-auto mt-8 flex max-w-sm flex-col gap-5 rounded-overlay border border-line bg-raised p-6 shadow-sm" {
                h1 class=(PAGE_TITLE) { "Log in" }
                p class="text-muted" {
                    "Log in with your Discord account to upload builds. Your account is made the first time you log in."
                }
                @if !message.is_empty() {
                    p role="alert" class="text-danger" { (message) }
                }
                @if enabled {
                    a href="/auth/discord" class="btn btn-primary" { "Log in with Discord" }
                } @else {
                    p role="alert" class="text-danger" {
                        "Discord login is not set up on this server: DISCORD_CLIENT_ID, DISCORD_CLIENT_SECRET, and DISCORD_REDIRECT_URI must be set."
                    }
                }
            }
        },
    )
}

/// The admin's list of accounts, each with forms to change it; `message`
/// says why a change was refused.
pub fn users(admin: &User, users: &[User], message: &str) -> Markup {
    base(
        Some(admin),
        "Users",
        "/users",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-2" } { "Users" }
            p class="mb-6 max-w-prose text-muted" {
                "Mods edit and delete any build; admins also set roles. A confirmed Minecraft name lets its account edit builds crediting it, so confirm only names you know are theirs."
            }
            @if !message.is_empty() {
                p role="alert" class="mb-6 text-danger" { (message) }
            }
            // Each control in its own column, of a fixed width so a name
            // opened for editing moves nothing; on a phone each account is
            // a block, its name and role over its Minecraft name.
            div class="relative max-w-calm" {
                table class="block w-full border-collapse sm:table sm:table-fixed" {
                    thead class="hidden sm:table-header-group" {
                        tr {
                            th scope="col" class={ (TABLE_HEAD) " w-[35%]" } { "Account" }
                            th scope="col" class=(TABLE_HEAD) { "Minecraft name" }
                            th scope="col" class={ (TABLE_HEAD) " w-32" } { "Role" }
                        }
                    }
                    tbody class="block sm:table-row-group" {
                        @for user in users {
                            (user_row(admin, user))
                        }
                    }
                }
            }
        },
    )
}

/// A cell of the users table, from a phone up; on a phone the row's line
/// stands in for the cells'.
const CELL: &str = "sm:table-cell sm:h-row-rich sm:border-b sm:border-line sm:py-2 sm:pr-4 sm:align-middle";

/// One account's row on the users page, redrawn by fixi when one of its
/// forms changes it. A role saves as soon as it changes; without scripts,
/// its button sends it.
pub fn user_row(admin: &User, user: &User) -> Markup {
    let row = format!("user-{}", user.username);
    let target = format!("#{row}");
    html! {
        tr id=(row) class="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-line py-3 sm:table-row sm:border-0 sm:py-0" {
            th scope="row" class={ (CELL) " min-w-0 flex-1 text-left font-normal" } {
                (account_link(user))
            }
            // On a phone, a line of its own under the account and role.
            td class={ (CELL) " order-last min-w-0 basis-full" } { (minecraft_name(user, &target)) }
            td class=(CELL) {
                @if user.id == admin.id {
                    span class="flex items-center gap-2" {
                        (role_badge(user.role))
                        span class="text-meta text-muted" { "you" }
                    }
                } @else {
                    @let action = format!("/users/{}/role", user.username);
                    form action=(action) method="post"
                        fx-action=(action) fx-method="post" fx-target=(target) fx-trigger="change"
                        class="flex items-center gap-2" {
                        label for={ "role-" (user.username) } class="sr-only" {
                            "Role for " (user.name())
                        }
                        select id={ "role-" (user.username) } name="role" class={ "badge min-h-7 " (role_colour(user.role)) } {
                            @for role in Role::ALL {
                                option value=(role.as_str()) selected[role == user.role] { (role.as_str()) }
                            }
                        }
                        noscript { button type="submit" class="btn btn-quiet min-h-9" { "Set" } }
                    }
                }
            }
        }
    }
}

/// The colour of a badge for `role`: an admin's the brand's, a mod's the
/// tint other tags take, and a user's plain.
fn role_colour(role: Role) -> &'static str {
    match role {
        Role::Admin => "bg-brand text-on-brand",
        Role::Mod => "badge-other",
        Role::User => "bg-muted/12 text-muted",
    }
}

/// `role` as a badge in its colour.
pub(super) fn role_badge(role: Role) -> Markup {
    html! { span class={ "badge " (role_colour(role)) } { (role.as_str()) } }
}

/// An account's Minecraft name on the users page, green with a check once
/// confirmed, or None, with a pencil that swaps it for a form to change it
/// in and confirm it, ticked already for a name confirmed or none yet.
fn minecraft_name(user: &User, target: &str) -> Markup {
    let name = user.minecraft_name.as_deref().unwrap_or_default();
    let confirmed = user.minecraft_verified && !name.is_empty();
    let shown = html! {
        @if name.is_empty() {
            span class="text-muted" { "None" }
        } @else if confirmed {
            span class="flex min-w-0 items-center gap-1 text-success" title="Confirmed" {
                span class="truncate" { (name) }
                (icon(icons::CHECK, "size-4 shrink-0"))
                span class="sr-only" { ", confirmed" }
            }
        } @else {
            span class="block truncate" title="Not confirmed" {
                (name)
                span class="sr-only" { ", not confirmed" }
            }
        }
    };
    let action = format!("/users/{}/minecraft/name", user.username);
    let form = html! {
        form action=(action) method="post"
            fx-action=(action) fx-method="post" fx-target=(target)
            class="flex flex-wrap items-center gap-x-2 gap-y-1" {
            input name="minecraft_name" value=(name) required
                aria-label={ "Minecraft name for " (user.name()) }
                pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                class="inline-input";
            label class="flex shrink-0 items-center gap-1.5 text-meta text-muted" {
                input type="checkbox" name="verified" value="yes"
                    checked[confirmed || name.is_empty()] class="size-4 appearance-auto";
                "Confirmed"
            }
            button type="submit" class="btn btn-primary min-h-8 px-3" { "Save" }
            button type="button" data-cancel class="btn btn-quiet min-h-8 px-3" { "Cancel" }
        }
    };
    // The pencil right after the name, swapping the two for the form, as
    // Settings does.
    html! {
        div data-editable {
            div data-view class="flex min-w-0 items-center gap-1" {
                (shown)
                button type="button" data-edit title="Edit"
                    aria-label={ "Edit " (user.name()) "'s Minecraft name" }
                    class="inline-flex size-8 shrink-0 items-center justify-center rounded-control text-muted hover:bg-selection hover:text-ink" {
                    (icon(icons::PENCIL, "size-4"))
                }
            }
            div data-editor hidden { (form) }
        }
    }
}

/// An account's avatar and name, linking to its page, with its Discord
/// username under it when the name does not say who it is.
fn account_link(user: &User) -> Markup {
    html! {
        div class="flex items-center gap-2.5" {
            @if let Some(picture) = user.picture(28) { (picture) }
            div class="flex min-w-0 flex-col" {
                a href={ "/users/" (user.username) } class="truncate" { (user.name()) }
                @if user.name() != user.username {
                    span class="truncate text-meta text-muted" { "@" (user.username) }
                }
            }
        }
    }
}

/// An account's page: who they are and what they uploaded.
/// The builds a profile lists.
pub struct ProfileBuilds {
    /// Credited to the account's Minecraft name as builder.
    pub built: Vec<Schematic>,
    /// Credited to it as contributor.
    pub contributed: Vec<Schematic>,
    /// Uploaded by the account through the site.
    pub uploaded: Vec<Schematic>,
    /// The viewer's palette's key, for thumbnails, if they chose one.
    pub palette: Option<String>,
}

/// An account's page: the builds it built, helped with, and uploaded. Its
/// owner can set their Minecraft name here; `message` says why a name was
/// refused.
pub fn profile(
    viewer: Option<&User>,
    account: &User,
    builds: &ProfileBuilds,
) -> Markup {
    let path = format!("/users/{}", account.username);
    let own = viewer.is_some_and(|viewer| viewer.id == account.id);
    base(
        viewer,
        account.name(),
        &path,
        &html! {
            div class="mb-8 flex items-center gap-4" {
                @if let Some(picture) = account.picture(64) { (picture) }
                div class="min-w-0 flex-1" {
                    h1 class=(PAGE_TITLE) { (account.name()) }
                    p class="flex items-center gap-2 text-muted" {
                        "@" (account.username) (role_badge(account.role))
                    }
                    @if let Some(name) = &account.minecraft_name {
                        p {
                            "Minecraft: "
                            a href=(player_href(name)) { (name) }
                            @if !account.minecraft_verified {
                                span class="text-meta text-muted" { " · not yet confirmed" }
                            }
                        }
                    }
                }
                @if own {
                    a href="/settings" class="btn btn-quiet shrink-0 gap-2 self-start hover:no-underline" {
                        (super::icons::icon(super::icons::SETTINGS, "size-4"))
                        "Settings"
                    }
                }
            }
            (credits(&builds.built, &builds.contributed, &html! {
                p class="text-muted" {
                    @match &account.minecraft_name {
                        None if own => {
                            "Give your Minecraft name in " a href="/settings" { "Settings" } " to list the builds credited to it."
                        }
                        None => { "No Minecraft name given, so their builds can't be listed." }
                        Some(name) => { "No builds credit " (name) " as builder yet." }
                    }
                }
            }, builds.palette.as_deref()))
            section aria-labelledby="uploads-heading" {
                h2 id="uploads-heading" class={ (SECTION_TITLE) " mb-3" } { "Uploads" }
                @if builds.uploaded.is_empty() {
                    p class="text-muted" { "No uploads yet." }
                } @else {
                    (cards(&builds.uploaded, &[], builds.palette.as_deref()))
                }
            }
        },
    )
}
