//! The settings page: a row for each setting, its value beside its name,
//! edited in place.

use maud::{Markup, html};

use crate::{
    models::user::User,
    palette::{self, Palette},
};

use super::{
    Connection, PAGE_TITLE, SWITCHES, base,
    icons::{HELP, PENCIL, icon},
};

/// `user`'s settings, with their Schemati session as `connection` says it
/// stands; `minecraft` and `schemati`, why a change to either could not be
/// made, when one could not, which opens that row for editing.
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
                h1 class={ (PAGE_TITLE) " mb-6" } { "Settings" }
                dl class="divide-y divide-line" {
                    (minecraft_row(user, minecraft))
                    (schemati_row(connection, schemati))
                    (block_theme_row(chosen))
                    (appearance_row())
                }
            }
            (session_help())
        },
    )
}

/// One setting: its name, and its value with what changes it, on the page
/// itself, a line between it and the next.
fn row(name: &Markup, value: &Markup) -> Markup {
    html! {
        div class="flex min-h-row-rich flex-col gap-1 py-2 sm:flex-row sm:items-center sm:gap-6" {
            dt class="flex items-center gap-1 sm:w-48 sm:shrink-0" { (name) }
            dd class="min-w-0 flex-1" { (value) }
        }
    }
}

/// The look of a bare icon button at a row's end.
const ICON_BUTTON: &str = "inline-flex size-8 shrink-0 items-center justify-center rounded-control text-muted hover:bg-selection hover:text-ink";

/// A value shown with its edit button at the row's end, and the form that
/// takes its place, open already when `open`, as when its last try was
/// refused, saying `message`.
fn editable(
    what: &str,
    shown: &Markup,
    form: &Markup,
    open: bool,
    message: &str,
) -> Markup {
    html! {
        div data-editable {
            div data-view hidden[open] class="flex items-center gap-2" {
                div class="flex min-w-0 flex-1 items-center gap-2" { (shown) }
                button type="button" data-edit aria-label={ "Edit " (what) } title="Edit" class=(ICON_BUTTON) {
                    (icon(PENCIL, "size-4"))
                }
            }
            div data-editor hidden[!open] {
                (form)
                @if !message.is_empty() {
                    p role="alert" class="mt-1 text-meta text-danger" { (message) }
                }
            }
        }
    }
}

/// The viewer's Minecraft name, confirmed or not, edited in place.
fn minecraft_row(user: &User, message: &str) -> Markup {
    let name = user.minecraft_name.as_deref().unwrap_or_default();
    let shown = html! {
        @if name.is_empty() {
            span class="text-muted" { "Not set" }
        } @else {
            span { (name) }
            @if user.minecraft_verified {
                span title="Confirmed by an admin" class="inline-flex text-brand" {
                    (icon(super::icons::CHECK, "size-4"))
                    span class="sr-only" { "Confirmed" }
                }
            }
        }
    };
    let form = html! {
        form action="/settings/minecraft" method="post" class="flex items-center gap-2" {
            input type="hidden" name="next" value="/settings";
            input name="minecraft_name" value=(name) aria-label="Minecraft username"
                pattern="[A-Za-z0-9_]{1,16}" maxlength="16" autocomplete="off"
                aria-invalid=[(!message.is_empty()).then_some("true")]
                class="inline-input";
            button type="submit" class="btn btn-primary min-h-8 px-3" { "Save" }
            button type="button" data-cancel class="btn btn-quiet min-h-8 px-3" { "Cancel" }
        }
    };
    row(
        &html! { "Minecraft username" },
        &editable(
            "Minecraft username",
            &shown,
            &form,
            !message.is_empty(),
            message,
        ),
    )
}

/// The viewer's Schemati session: whose folder it opens, or that there is
/// none, edited in place, with how to find one behind its help button.
fn schemati_row(connection: &Connection, message: &str) -> Markup {
    let shown = html! {
        @match connection {
            Connection::Unset => span class="text-muted" { "Not connected" },
            Connection::Forgotten => span class="text-danger" { "Expired" },
            Connection::Unreachable(why) => span class="text-danger" { (why) },
            Connection::Open(folder) => span { "Connected as " (folder.player) },
        }
    };
    let form = html! {
        form action="/schemati/session" method="post" class="flex items-center gap-2" {
            input type="hidden" name="next" value="/settings";
            input name="session" required autocomplete="off" spellcheck="false"
                aria-label="Schemati session ID" placeholder="e2565926ee94c42943f6a68b88e1fe07"
                class="inline-input font-mono";
            button type="submit" class="btn btn-primary min-h-8 px-3" { "Save" }
            button type="button" data-cancel class="btn btn-quiet min-h-8 px-3" { "Cancel" }
        }
    };
    row(
        &html! {
            "Schemati session ID"
            button type="button" commandfor="session-help" command="show-modal"
                aria-label="How to find your session ID" title="How to find it"
                class="inline-flex size-6 items-center justify-center rounded-control text-muted hover:bg-selection hover:text-ink" {
                (icon(HELP, "size-4"))
            }
        },
        &editable(
            "Schemati session ID",
            &shown,
            &form,
            !message.is_empty(),
            message,
        ),
    )
}

/// How to find a Schemati session ID, in a dialog its help button opens.
fn session_help() -> Markup {
    html! {
        dialog id="session-help" aria-labelledby="session-help-title"
            class="m-auto max-w-prose rounded-overlay border border-line bg-raised p-5 text-ink shadow-lg backdrop:bg-black/40" {
            div class="mb-3 flex items-center justify-between gap-4" {
                h2 id="session-help-title" class="font-semibold" { "Schemati session ID" }
                button type="button" commandfor="session-help" command="close" class="btn btn-quiet" { "Close" }
            }
            ol class="flex list-decimal flex-col gap-1 pl-5" {
                li { "Log in at " a href="https://schemati.openredstone.org/login" target="_blank" rel="noopener" { "schemati.openredstone.org" } "." }
                li { "Press F12, open Storage (Firefox) or Application (Chrome), then Cookies." }
                li { "Copy " code { "sessionId" } "." }
            }
        }
    }
}

/// The viewer's block theme, by its blocks, and the dialog that changes it.
fn block_theme_row(chosen: &Palette) -> Markup {
    let name = palette::name(chosen);
    row(
        &html! { "Block theme" },
        &html! {
            div class="flex items-center gap-2" {
                div class="flex min-w-0 flex-1 items-center gap-2" {
                    @for block in [&chosen.solid, &chosen.transparent] {
                        span title=(palette::label(block)) class="inline-flex size-6" { (maud::PreEscaped(palette::icon(block))) }
                    }
                    span { (name) }
                }
                button type="button" commandfor="block-theme" command="show-modal"
                    aria-label="Edit block theme" title="Edit" class=(ICON_BUTTON) {
                    (icon(PENCIL, "size-4"))
                }
            }
            dialog id="block-theme" aria-labelledby="block-theme-title"
                class="m-auto max-w-calm rounded-overlay border border-line bg-raised p-5 text-ink shadow-lg backdrop:bg-black/40" {
                div class="mb-4 flex items-center justify-between gap-4" {
                    h2 id="block-theme-title" class="font-semibold" { "Block theme" }
                    // Its choices save as they change; done shows them.
                    a href="/settings" class="btn btn-quiet" { "Done" }
                }
                (palette_form(chosen, "/settings", ""))
            }
        },
    )
}

/// The choice of a light or dark site, or the system's, which a script
/// applies on every page.
fn appearance_row() -> Markup {
    let choice = "cursor-pointer rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink has-checked:bg-raised has-checked:font-semibold has-checked:text-ink has-checked:shadow-sm has-focus-visible:outline-2 has-focus-visible:outline-brand";
    row(
        &html! { "Appearance" },
        &html! {
            fieldset {
                legend class="sr-only" { "Appearance" }
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
        },
    )
}

/// The palette's form, as `chosen`: presets to start from, then each block
/// and colour as a button that opens its picker. It saves itself as each
/// changes; `save`, if not empty, names a button going on to `next`.
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
        form action="/settings/palette" method="post" data-autosave="Block theme saved"
            class="flex max-w-form flex-col gap-4" {
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
            // It saves itself as it changes; this goes on from it.
            @if !save.is_empty() {
                div { a href=(next) class="btn btn-primary" { (save) } }
            }
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
                        h1 class={ (PAGE_TITLE) " mb-2" } { "Block theme" }
                        p class="mb-4 text-muted" { "The blocks builds are shown and downloaded in." }
                        (palette_form(chosen, "/", "Finish"))
                    }
                }
            }
        },
    )
}
