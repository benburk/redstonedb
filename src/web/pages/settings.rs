//! The settings page: the viewer's Minecraft name and Schemati session.

use maud::{Markup, html};

use crate::models::user::User;

use super::{
    Connection, PAGE_TITLE, SECTION_TITLE, SWITCHES, base,
    schemati::{disconnect, session_form},
};

/// `user`'s settings, with their Schemati session as `connection` says it
/// stands; `minecraft` and `schemati`, why a change to either section
/// could not be made, when one could not.
pub fn settings(
    user: &User,
    connection: &Connection,
    minecraft: &str,
    schemati: &str,
) -> Markup {
    base(
        Some(user),
        "Settings",
        "/settings",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-8" } { "Settings" }
                (minecraft_section(user, minecraft))
                (schemati_section(connection, schemati))
                (theme_section())
            }
        },
    )
}

/// The form for the viewer's Minecraft name, saying whether an admin has
/// confirmed it, or `message`, why it could not be saved.
fn minecraft_section(user: &User, message: &str) -> Markup {
    html! {
        section aria-labelledby="minecraft-heading" class="mb-12" {
            h2 id="minecraft-heading" class={ (SECTION_TITLE) " mb-1" } { "Minecraft name" }
            p class="mb-4 max-w-prose text-muted" {
                "Links you to the builds credited to it. Once an admin confirms it, you can edit them."
            }
            form action="/settings/minecraft" method="post" class="flex max-w-form flex-col gap-2" {
                label for="minecraft-name" class="sr-only" { "Your Minecraft name" }
                div class="flex gap-2" {
                    input id="minecraft-name" name="minecraft_name"
                        value=(user.minecraft_name.as_deref().unwrap_or_default())
                        pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                        placeholder="Your in-game name"
                        aria-invalid=[(!message.is_empty()).then_some("true")]
                        aria-describedby=[(!message.is_empty()).then_some("minecraft-error")]
                        class="input flex-1";
                    button type="submit" class="btn btn-quiet" { "Save" }
                }
                @if !message.is_empty() {
                    p id="minecraft-error" role="alert" class="text-meta text-danger" { (message) }
                } @else if user.minecraft_name.is_some() {
                    p class="text-meta text-muted" {
                        @if user.minecraft_verified { "Confirmed by an admin." } @else { "Not yet confirmed by an admin." }
                    }
                }
            }
        }
    }
}

/// The viewer's Schemati session: who it opens the folder of, with a link
/// to their files and the button that forgets it, or the form for giving
/// one; and `message`, why one given was refused.
fn schemati_section(connection: &Connection, message: &str) -> Markup {
    html! {
        section aria-labelledby="schemati-heading" {
            h2 id="schemati-heading" class={ (SECTION_TITLE) " mb-1" } { "Schemati" }
            p class="mb-4 max-w-prose text-muted" {
                "Upload and compare what you " code { "//save" } " on Open Redstone, and send builds there to "
                code { "//load" } "."
            }
            @if !message.is_empty() {
                p role="alert" class="mb-4 text-danger" { (message) }
            }
            @match connection {
                Connection::Unset => (session_form(false)),
                Connection::Forgotten => (session_form(true)),
                Connection::Unreachable(why) => {
                    p class="mb-4 text-danger" { (why) }
                    (disconnect())
                }
                Connection::Open(folder) => {
                    div class="flex flex-wrap items-center gap-x-4 gap-y-2" {
                        p {
                            "Connected as " strong { (folder.player) } ". "
                            a href="/schemati" { "Your files" }
                        }
                        (disconnect())
                    }
                }
            }
        }
    }
}

/// The choice of a light or dark theme, or the system's, which a script
/// keeps in this browser and applies on every page.
fn theme_section() -> Markup {
    let choice = "cursor-pointer rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink has-checked:bg-raised has-checked:font-semibold has-checked:text-ink has-checked:shadow-sm has-focus-visible:outline-2 has-focus-visible:outline-brand";
    html! {
        section aria-labelledby="theme-heading" class="mt-12" {
            h2 id="theme-heading" class={ (SECTION_TITLE) " mb-1" } { "Theme" }
            p class="mb-4 max-w-prose text-muted" { "Kept in this browser." }
            fieldset {
                legend class="sr-only" { "Theme" }
                div class={ (SWITCHES) " inline-flex" } {
                    @for (value, label) in [("system", "System"), ("light", "Light"), ("dark", "Dark")] {
                        label class=(choice) {
                            input type="radio" name="theme" value=(value)
                                checked[value == "system"] class="sr-only";
                            (label)
                        }
                    }
                }
            }
        }
    }
}
