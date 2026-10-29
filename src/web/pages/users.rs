//! Accounts: logging in, the admin's list of users, and an account's
//! profile.

use maud::{Markup, html};

use crate::models::{
    schematic::Schematic,
    user::{Role, User},
};

use super::{
    PAGE_TITLE, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, base, listing::cards,
    player::credits, player_href,
};

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
            div class="relative max-w-calm overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Account" }
                            th scope="col" class=(TABLE_HEAD) { "Minecraft name" }
                            th scope="col" class=(TABLE_HEAD) { "Role" }
                        }
                    }
                    tbody {
                        @for user in users {
                            (user_row(admin, user))
                        }
                    }
                }
            }
        },
    )
}

/// One account's row on the users page, redrawn by fixi when one of its
/// forms changes it.
pub fn user_row(admin: &User, user: &User) -> Markup {
    let row = format!("user-{}", user.username);
    let target = format!("#{row}");
    html! {
        tr id=(row) {
            th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                (account_link(user))
            }
            td class=(TABLE_CELL) { (minecraft_cell(user, &target)) }
            td class=(TABLE_CELL) {
                @if user.id == admin.id {
                    span class="text-muted" { "admin (you)" }
                } @else {
                    @let action = format!("/users/{}/role", user.username);
                    form action=(action) method="post"
                        fx-action=(action) fx-method="post" fx-target=(target) class="flex gap-2" {
                        label for={ "role-" (user.username) } class="sr-only" {
                            "Role for " (user.name())
                        }
                        select id={ "role-" (user.username) } name="role" class="input min-h-9 w-auto" {
                            @for role in Role::ALL {
                                option value=(role.as_str()) selected[role == user.role] { (role.as_str()) }
                            }
                        }
                        button type="submit" class="btn btn-quiet" { "Set" }
                    }
                }
            }
        }
    }
}

/// An account's Minecraft name on the users page, with the button that
/// confirms it or withdraws the confirmation.
fn minecraft_cell(user: &User, target: &str) -> Markup {
    let Some(name) = &user.minecraft_name else {
        // None given yet, so the admin may give one.
        let id = format!("minecraft-{}", user.username);
        let action = format!("/users/{}/minecraft/name", user.username);
        return html! {
            form action=(action) method="post"
                fx-action=(action) fx-method="post" fx-target=(target)
                class="flex items-center gap-2" {
                label for=(id) class="sr-only" { "Minecraft name for " (user.name()) }
                input id=(id) name="minecraft_name" required
                    pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                    placeholder="None given" class="input min-h-8 w-40";
                button type="submit" class="btn btn-quiet min-h-8 px-3" { "Set" }
            }
        };
    };
    let (value, action) = if user.minecraft_verified {
        ("no", "Withdraw")
    } else {
        ("yes", "Confirm")
    };
    let verify = format!("/users/{}/minecraft/verify", user.username);
    html! {
        form action=(verify) method="post"
            fx-action=(verify) fx-method="post" fx-target=(target)
            class="flex flex-wrap items-center gap-x-3 gap-y-1" {
            span { (name) }
            span class="text-meta text-muted" {
                (if user.minecraft_verified { "confirmed" } else { "not confirmed" })
            }
            input type="hidden" name="verified" value=(value);
            button type="submit" class="btn btn-quiet min-h-8 px-3"
                aria-label={ (action) " " (name) " as " (user.name()) "'s Minecraft name" } { (action) }
        }
    }
}

/// An account's avatar and name, linking to its page.
fn account_link(user: &User) -> Markup {
    html! {
        a href={ "/users/" (user.username) } class="inline-flex items-center gap-2" {
            @if let Some(picture) = user.picture(24) { (picture) }
            (user.name())
        }
        // The Discord username, which says who it is when its name does not.
        @if user.name() != user.username {
            span class="ml-2 text-meta text-muted" { "@" (user.username) }
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
                    p class="text-muted" {
                        "@" (account.username) " · " (account.role.as_str())
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
