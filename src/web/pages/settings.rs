//! The settings page: the viewer's Minecraft name and Schemati session.

use maud::{Markup, html};

use crate::{
    models::user::User,
    palette::{self, Palette},
};

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
    chosen: &Palette,
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
                (palette_section(chosen))
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
/// The blocks recognized builds are shown and downloaded in, as `chosen`.
fn palette_section(chosen: &Palette) -> Markup {
    html! {
        section aria-labelledby="palette-heading" class="mt-12" {
            h2 id="palette-heading" class={ (SECTION_TITLE) " mb-1" } { "Palette" }
            p class="mb-4 max-w-prose text-muted" {
                "The blocks recognized builds are shown and downloaded in."
            }
            (palette_form(chosen, "/settings#palette-heading", "Save"))
        }
    }
}

/// The palette's form, as `chosen`, going to `next` once saved: presets to
/// start from, then each block and colour as a button that opens its
/// picker.
pub(super) fn palette_form(chosen: &Palette, next: &str, save: &str) -> Markup {
    let plain = |groups: Vec<(&'static str, Vec<String>)>| -> Vec<(&'static str, Vec<(String, String)>)> {
        groups
            .into_iter()
            .map(|(group, blocks)| {
                (group, blocks.into_iter().map(|b| (b.clone(), b)).collect())
            })
            .collect()
    };
    let mut marks = plain(palette::marks());
    marks.push((
        "None",
        vec![(palette::NONE.to_owned(), palette::NONE.to_owned())],
    ));
    let switch = "cursor-pointer rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink has-checked:bg-raised has-checked:font-semibold has-checked:text-ink has-checked:shadow-sm has-focus-visible:outline-2 has-focus-visible:outline-brand";
    html! {
        form action="/settings/palette" method="post" class="flex max-w-form flex-col gap-4" {
            input type="hidden" name="next" value=(next);
            div {
                button type="button" popovertarget="presets" class="btn btn-quiet" { "Start from a preset ▾" }
                div id="presets" popover
                    class="m-auto rounded-overlay border border-line bg-raised p-2 text-ink shadow-lg backdrop:bg-black/20" {
                    ul class="flex flex-col" {
                        @for (key, name, preset) in palette::presets() {
                            li { (preset_choice("preset", key, name, &preset)) }
                        }
                        li { (preset_choice("reset", "yes", "The site's own", &Palette::default())) }
                    }
                }
            }
            fieldset class="flex flex-wrap items-center gap-3" {
                legend class="sr-only" { "Blocks that may be either kind" }
                span aria-hidden="true" { "Either kind:" }
                div class={ (SWITCHES) " inline-flex" } {
                    @for (value, label) in [("solid", "Solid"), ("transparent", "Transparent")] {
                        label class=(switch) {
                            input type="radio" name="prefer" value=(value)
                                checked[(value == "solid") == chosen.prefer_solid] class="sr-only";
                            (label)
                        }
                    }
                }
            }
            div class="grid grid-cols-[auto_1fr] items-center gap-x-4 gap-y-2" {
                (picker("solid", "Solid", &plain(palette::solid()), &chosen.solid, &chosen.solid))
                (picker("transparent", "Transparent", &plain(palette::transparent()), &chosen.transparent, &chosen.transparent))
                @for (key, label, _) in palette::ROLES {
                    @let mark = chosen.mark(key).unwrap_or_else(|| palette::NONE.to_owned());
                    (picker(key, label, &marks, &mark, &mark))
                }
            }
            div { button type="submit" class="btn btn-primary" { (save) } }
        }
    }
}

/// A preset in the presets menu: a button sending `name` as `value`, with
/// whose it is and its blocks, solid, transparent, and the kind blocks that
/// may be either take, at the end.
fn preset_choice(
    name: &str,
    value: &str,
    label: &str,
    preset: &Palette,
) -> Markup {
    let either = if preset.prefer_solid {
        "solid"
    } else {
        "glass"
    };
    html! {
        button type="submit" name=(name) value=(value)
            class="flex w-full items-center justify-between gap-6 rounded-control px-3 py-2 text-left hover:bg-selection" {
            span class="font-semibold" { (label) }
            span class="flex items-center gap-1 text-meta text-muted" {
                @for block in [&preset.solid, &preset.transparent] {
                    span title=(palette::label(block)) class="inline-flex size-6" { (maud::PreEscaped(palette::icon(block))) }
                }
                span class="ml-1" { (either) " by default" }
            }
        }
    }
}

/// One choice of the palette: its label, and a button with the chosen
/// block's icon and name that opens a dialog of every choice under its
/// group's name, each a value sent as `name` and the block pictured for
/// it. `app.js` shows a new choice on the button and closes the dialog.
fn picker(
    name: &str,
    label: &str,
    groups: &[(&str, Vec<(String, String)>)],
    current: &str,
    pictured: &str,
) -> Markup {
    let words = palette::label;
    let dialog = format!("pick-{name}");
    html! {
        span id={ (dialog) "-label" } { (label) }
        div {
            button type="button" commandfor=(dialog) command="show-modal"
                aria-labelledby={ (dialog) "-label " (dialog) "-chosen" }
                class="inline-flex items-center gap-2 whitespace-nowrap rounded-control border border-line bg-raised px-2 py-1 hover:border-boundary" {
                span data-picked-icon class="inline-flex size-6" { (maud::PreEscaped(palette::icon(pictured))) }
                span id={ (dialog) "-chosen" } data-picked-name { (words(current)) }
            }
            dialog id=(dialog) data-picker aria-labelledby={ (dialog) "-title" }
                class="m-auto max-w-calm rounded-overlay border border-line bg-raised p-5 text-ink shadow-lg backdrop:bg-black/40" {
                div class="mb-3 flex items-center justify-between gap-4" {
                    h3 id={ (dialog) "-title" } class="font-semibold" { (label) }
                    button type="button" commandfor=(dialog) command="close" class="btn btn-quiet" { "Done" }
                }
                @for (group, choices) in groups {
                    fieldset class="mb-3" {
                        legend class="mb-1 text-meta text-muted" { (group) }
                        div class="flex flex-wrap gap-1" {
                            @for (value, block) in choices {
                                label title=(words(block)) data-name=(words(value))
                                    class="cursor-pointer rounded-control border-2 border-transparent p-0.5 hover:border-boundary has-checked:border-brand has-focus-visible:outline-2 has-focus-visible:outline-brand" {
                                    input type="radio" name=(name) value=(value)
                                        checked[value == current] class="sr-only";
                                    span class="sr-only" { (words(value)) }
                                    span class="inline-flex size-8" { (maud::PreEscaped(palette::icon(block))) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

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

/// The welcome steps a new account sees once: `step` 1, its Minecraft
/// name; 2, its Schemati session; 3, its palette, as `chosen`. `message`
/// says why the last try at a step was refused.
pub fn welcome(
    user: &User,
    step: u8,
    message: &str,
    chosen: &Palette,
) -> Markup {
    let next = |step: u8| format!("/welcome?step={step}");
    let skip =
        |to: &str| html! { a href=(to) class="btn btn-quiet" { "Skip" } };
    base(
        Some(user),
        "Welcome",
        "/welcome",
        &html! {
            div class="max-w-calm" {
                p class="mb-1 text-meta text-muted" { "Step " (step) " of 3" }
                @match step {
                    1 => {
                        h1 class={ (PAGE_TITLE) " mb-2" } { "Your Minecraft name" }
                        p class="mb-4 text-muted" { "Links you to the builds credited to it." }
                        form action="/settings/minecraft" method="post" class="flex max-w-form flex-col gap-3" {
                            input type="hidden" name="next" value=(next(2));
                            input name="minecraft_name" aria-label="Your Minecraft name"
                                value=(user.minecraft_name.as_deref().unwrap_or_default())
                                pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                                placeholder="Your in-game name" class="input";
                            @if !message.is_empty() { p role="alert" class="text-meta text-danger" { (message) } }
                            div class="flex gap-2" {
                                button type="submit" class="btn btn-primary" { "Next" }
                                (skip(&next(2)))
                            }
                        }
                    }
                    2 => {
                        h1 class={ (PAGE_TITLE) " mb-2" } { "Connect Schemati" }
                        p class="mb-4 text-muted" { "Upload builds straight from your Schemati folder, and send builds to it." }
                        ol class="mb-4 flex list-decimal flex-col gap-1 pl-5" {
                            li { "Log in at " a href="https://schemati.openredstone.org/login" target="_blank" rel="noopener" { "schemati.openredstone.org" } "." }
                            li { "Press F12, open Storage (Firefox) or Application (Chrome), then Cookies." }
                            li { "Copy " code { "sessionId" } " and paste it here." }
                        }
                        form action="/schemati/session" method="post" class="flex max-w-form flex-col gap-3" {
                            input type="hidden" name="next" value=(next(3));
                            input name="session" required autocomplete="off" spellcheck="false"
                                aria-label="Schemati session" placeholder="e2565926ee94c42943f6a68b88e1fe07"
                                class="input font-mono";
                            @if !message.is_empty() { p role="alert" class="text-meta text-danger" { (message) } }
                            div class="flex gap-2" {
                                button type="submit" class="btn btn-primary" { "Test and save" }
                                (skip(&next(3)))
                            }
                        }
                    }
                    _ => {
                        h1 class={ (PAGE_TITLE) " mb-2" } { "Your palette" }
                        p class="mb-4 text-muted" { "The blocks builds are shown and downloaded in. Change it any time in Settings." }
                        (palette_form(chosen, "/", "Finish"))
                    }
                }
            }
        },
    )
}
