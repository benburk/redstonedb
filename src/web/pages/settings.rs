//! The settings page: a row for each setting, its value beside its name,
//! edited in place.

use maud::{Markup, html};

use crate::{
    models::user::{SchematiLink, User},
    recognize::palette::{self, Palette},
};

use super::{
    CHOICE, PAGE_TITLE, SWITCHES, base,
    icons::{PENCIL, icon},
};

/// `user`'s settings, with their Schemati session as `schemati_link` says
/// it stands; `minecraft` and `schemati`, why a change to either could not
/// be made, when one could not, which opens that row for editing.
pub fn settings(
    user: &User,
    schemati_link: Option<&SchematiLink>,
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
                    (schemati_row(schemati_link, schemati))
                    (block_theme_row(chosen))
                    (appearance_row())
                }
                form action="/logout" method="post" class="mt-6" {
                    button type="submit" class="btn btn-quiet gap-2" {
                        (icon(super::icons::LOG_OUT, "size-4"))
                        "Log out"
                    }
                }
            }
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

/// A value shown with its edit button at the row's end, after `actions`,
/// anything else done to it, and the form that takes its place, open
/// already when `open`, as when its last try was refused, saying
/// `message`. The edit button says it does `edit`, such as `Edit`.
struct Editable<'a> {
    what: &'a str,
    edit: &'a str,
    shown: Markup,
    actions: Markup,
    form: Markup,
    open: bool,
    message: &'a str,
}

/// The value and its form, as `e` gives them.
fn editable(e: &Editable) -> Markup {
    let Editable {
        what,
        edit,
        shown,
        actions,
        form,
        open,
        message,
    } = e;
    html! {
        div data-editable {
            div data-view hidden[*open] class="flex items-center gap-2" {
                div class="flex min-w-0 flex-1 items-center gap-2" { (shown) }
                (actions)
                button type="button" data-edit aria-label={ (edit) " " (what) } title=(edit) class=(ICON_BUTTON) {
                    (icon(PENCIL, "size-4"))
                }
            }
            div data-editor hidden[!*open] {
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
        &editable(&Editable {
            what: "Minecraft username",
            edit: "Edit",
            shown,
            actions: html! {},
            form,
            open: !message.is_empty(),
            message,
        }),
    )
}

/// The viewer's Schemati session, as `link` keeps what Schemati last said
/// of it, so showing it asks Schemati nothing: whose folder it opens and
/// when that was last seen, that it expired, or that there is none and
/// what one is for. It is replaced in place, with how to find one under
/// its field, and once given may be disconnected.
fn schemati_row(link: Option<&SchematiLink>, message: &str) -> Markup {
    let shown = html! {
        @match link {
            None => span class="text-muted" {
                "Not connected. Connect it to upload builds from your Schemati folder and send builds to it."
            },
            Some(link) if link.expired => span class="text-danger" {
                "Expired. Paste a new session to reconnect."
            },
            Some(link) => {
                span class="min-w-0 truncate" {
                    @match &link.player {
                        Some(player) => { "Connected as " strong { (player) } }
                        None => "Connected",
                    }
                    @if let Some(checked) = link.checked {
                        span class="text-muted" { " · checked " (super::schemati::ago(checked.max(0).unsigned_abs() * 1000)) }
                    }
                }
            }
        }
    };
    // Disconnecting goes back to this page, the session gone.
    let actions = html! {
        @if link.is_some() {
            form action="/schemati/session/delete" method="post" class="contents" {
                button type="submit" class="btn btn-quiet min-h-8 px-3" { "Disconnect" }
            }
        }
    };
    let placeholder = if link.is_some() {
        "Paste a new sessionId"
    } else {
        "Paste your sessionId"
    };
    let form = html! {
        form action="/schemati/session" method="post" class="flex items-center gap-2" {
            input name="session" required autocomplete="off" spellcheck="false"
                aria-label="Schemati session ID" placeholder=(placeholder)
                class="inline-input font-mono";
            button type="submit" class="btn btn-primary min-h-8 px-3" { "Save" }
            button type="button" data-cancel class="btn btn-quiet min-h-8 px-3" { "Cancel" }
        }
        (session_steps())
    };
    row(
        &html! { "Schemati session ID" },
        &editable(&Editable {
            what: "Schemati session ID",
            edit: if link.is_some() { "Replace" } else { "Edit" },
            shown,
            actions,
            form,
            open: !message.is_empty(),
            message,
        }),
    )
}

/// How to find a Schemati session ID, as steps under the field that takes
/// one.
fn session_steps() -> Markup {
    html! {
        ol class="mt-2 flex list-decimal flex-col gap-0.5 pl-5 text-meta text-muted" {
            li { "Log in at " a href="https://schemati.openredstone.org/login" target="_blank" rel="noopener" { "schemati.openredstone.org" } "." }
            li { "Press F12, open Storage (Firefox) or Application (Chrome), then Cookies." }
            li { "Copy " code { "sessionId" } " and paste it here." }
        }
    }
}

/// The viewer's block theme, by its blocks, and a link to its editor:
/// through fixi, the editor's dialog, which opens once it is in, and
/// without it, the editor's page.
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
                a href="/settings/palette" fx-action="/settings/palette" fx-target="#block-theme"
                    aria-label="Edit block theme" title="Edit" class=(ICON_BUTTON) {
                    (icon(PENCIL, "size-4"))
                }
            }
            // Where the editor's dialog comes in.
            div id="block-theme" hidden {}
        },
    )
}

/// The block theme's editor, as `chosen`, in a dialog that opens once it
/// is in.
pub fn block_theme(chosen: &Palette) -> Markup {
    html! {
        dialog id="block-theme" data-show aria-labelledby="block-theme-title"
            class="dialog max-w-calm" {
            div class="mb-4 flex items-center justify-between gap-4" {
                h2 id="block-theme-title" class="font-semibold" { "Block theme" }
                // Its choices save as they change; done shows them.
                a href="/settings" class="btn btn-quiet" { "Done" }
            }
            (palette_form(chosen))
        }
    }
}

/// The block theme's editor as a page of its own, for a browser without
/// scripts, which saves with its button and goes back to the settings;
/// with them it saves as it changes, as the dialog does.
pub fn block_theme_page(user: &User, chosen: &Palette) -> Markup {
    base(
        Some(user),
        "Block theme",
        "/settings/palette",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-2" } { "Block theme" }
                p class="mb-4 text-muted" { "The blocks builds are shown and downloaded in." }
                (palette_form(chosen))
                button type="submit" form="palette-form" class="btn btn-primary mt-4" { "Save" }
            }
        },
    )
}

/// The choice of a light or dark site, or the system's, which a script
/// applies on every page.
fn appearance_row() -> Markup {
    row(
        &html! { "Appearance" },
        &html! {
            fieldset {
                legend class="sr-only" { "Appearance" }
                div class={ (SWITCHES) " inline-flex" } {
                    @for (value, label) in [("system", "System"), ("light", "Light"), ("dark", "Dark")] {
                        label class=(CHOICE) {
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

/// The palette's form, as `chosen`: its fields, as [`palette_fields`]
/// gives them, its presets saved as chosen. Through fixi it saves itself
/// as each changes, the answer the fields as saved.
fn palette_form(chosen: &Palette) -> Markup {
    html! {
        form id="palette-form" action="/settings/palette" method="post"
            fx-action="/settings/palette" fx-method="post" fx-trigger="change" fx-target="#palette-fields"
            class="flex max-w-form flex-col gap-4" {
            (palette_fields(chosen, Presets::Saved))
        }
    }
}

/// What choosing a preset does: saves it as the viewer's block theme, the
/// answer the fields as it saved them, or only shows it, in the download's
/// custom colours, the answer that editor redrawn.
#[derive(Clone, Copy)]
pub enum Presets {
    Saved,
    Shown,
}

/// The download's custom colours: the block theme's editor, as `start`,
/// in a dialog that opens once it is in, for the download's form to send
/// and nothing to keep; for someone `signed_in`, a pointer to keeping a
/// theme in Settings.
pub fn custom_colours(start: &Palette, signed_in: bool) -> Markup {
    html! {
        dialog id="custom-colours" data-show aria-labelledby="custom-colours-title"
            class="dialog max-w-calm" {
            div class="mb-2 flex items-center justify-between gap-4" {
                h3 id="custom-colours-title" class="font-semibold" { "Custom colours" }
                button type="button" commandfor="custom-colours" command="close" class="btn btn-quiet" { "Done" }
            }
            p class="mb-4 text-meta text-muted" {
                "For this download only."
                @if signed_in {
                    " To see every build in them, set your "
                    a href="/settings" { "block theme" } "."
                }
            }
            (palette_fields(start, Presets::Shown))
        }
    }
}

/// A palette's fields, as `chosen`, for whichever form holds them: presets
/// to start from, each doing as `presets` says, then each block and colour
/// as a button that opens its picker.
pub fn palette_fields(chosen: &Palette, presets: Presets) -> Markup {
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
    html! {
        div id="palette-fields" class="flex flex-col gap-4" {
            div {
                button type="button" popovertarget="presets" class="btn btn-quiet" { "Start from a preset ▾" }
                div id="presets" popover
                    class="m-auto rounded-overlay border border-line bg-raised p-2 text-ink shadow-lg backdrop:bg-black/20" {
                    ul class="flex flex-col" {
                        @for (key, name, preset) in palette::presets() {
                            li { (preset_choice(presets, ("preset", key), name, &preset)) }
                        }
                        li { (preset_choice(presets, ("reset", "yes"), "Default", &Palette::default())) }
                    }
                }
            }
            fieldset class="flex flex-wrap items-center gap-3" {
                legend class="sr-only" { "Blocks that work solid or transparent" }
                span aria-hidden="true" { "Where either works:" }
                div class={ (SWITCHES) " inline-flex" } {
                    @for (value, label) in [("solid", "Solid"), ("transparent", "Transparent")] {
                        label class=(CHOICE) {
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
                @for (name, label, block) in [("hard", "Hard-powered", &chosen.hard), ("soft", "Soft-powered", &chosen.soft)] {
                    @let current = block.clone().unwrap_or_else(|| palette::NONE.to_owned());
                    (picker(name, label, &same_as_solid(palette::solid()), &current, &current))
                }
                @for (key, label, _) in palette::ROLES {
                    @let mark = chosen.mark(key).unwrap_or_else(|| palette::NONE.to_owned());
                    (picker(key, label, &marks, &mark, &mark))
                }
            }
        }
    }
}

/// The solid blocks under their groups, with first a choice of none, which
/// keeps the theme's solid block, for blocks carrying power.
fn same_as_solid(
    groups: Vec<(&'static str, Vec<String>)>,
) -> Vec<(&'static str, Vec<(String, String)>)> {
    std::iter::once((
        "Same as solid",
        vec![(palette::NONE.to_owned(), palette::NONE.to_owned())],
    ))
    .chain(groups.into_iter().map(|(group, blocks)| {
        (group, blocks.into_iter().map(|b| (b.clone(), b)).collect())
    }))
    .collect()
}

/// A preset in the presets menu: a button sending `name` as `value`, with
/// whose it is and its blocks, solid, transparent, and the kind blocks that
/// may be either take, at the end. Saved, it sends the form, and through
/// fixi the answer is the fields; shown, it asks for the custom colours'
/// editor as that preset makes it.
fn preset_choice(
    presets: Presets,
    (name, value): (&str, &str),
    label: &str,
    preset: &Palette,
) -> Markup {
    let either = if preset.prefer_solid {
        "solid"
    } else {
        "glass"
    };
    html! {
        @let class = "flex w-full items-center justify-between gap-6 rounded-control px-3 py-2 text-left hover:bg-selection";
        @let inside = html! {
            span class="font-semibold" { (label) }
            span class="flex items-center gap-1 text-meta text-muted" {
                @for block in [&preset.solid, &preset.transparent] {
                    span title=(palette::label(block)) class="inline-flex size-6" { (maud::PreEscaped(palette::icon(block))) }
                }
                span class="ml-1" { (either) " by default" }
            }
        };
        @match presets {
            Presets::Saved => {
                button type="submit" name=(name) value=(value) class=(class)
                    fx-action="/settings/palette" fx-method="post" fx-target="#palette-fields" { (inside) }
            }
            Presets::Shown => {
                button type="button" class=(class)
                    fx-action={ "/palette/custom?" (name) "=" (value) } fx-target="#custom-colours" { (inside) }
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
                class="dialog max-w-calm" {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn link(player: Option<&str>, checked: Option<i64>) -> SchematiLink {
        SchematiLink {
            player: player.map(str::to_owned),
            checked,
            expired: false,
        }
    }

    fn now() -> i64 {
        time::OffsetDateTime::now_utc().unix_timestamp()
    }

    /// The row shows what the database keeps, with no folder to read.
    #[test]
    fn the_schemati_row_shows_the_session_as_kept() {
        let none = schemati_row(None, "").into_string();
        assert!(none.contains("Not connected."), "{none}");
        assert!(!none.contains("Disconnect"), "{none}");

        let known =
            schemati_row(Some(&link(Some("embizone"), Some(now() - 300))), "")
                .into_string();
        assert!(
            known.contains("Connected as <strong>embizone</strong>"),
            "{known}"
        );
        assert!(known.contains("checked 5 min ago"), "{known}");
        assert!(known.contains(r#"action="/schemati/session/delete""#));
        assert!(known.contains("Disconnect"), "{known}");
        assert!(known.contains(r#"aria-label="Replace Schemati session ID""#));

        // A session given before players were kept: connected, no one named.
        let unconfirmed =
            schemati_row(Some(&link(None, None)), "").into_string();
        assert!(unconfirmed.contains("Connected"), "{unconfirmed}");
        assert!(!unconfirmed.contains("Connected as"), "{unconfirmed}");
        assert!(!unconfirmed.contains("checked"), "{unconfirmed}");
        assert!(unconfirmed.contains("Disconnect"), "{unconfirmed}");

        let expired = SchematiLink {
            expired: true,
            ..link(Some("embizone"), Some(now()))
        };
        let expired = schemati_row(Some(&expired), "").into_string();
        assert!(expired.contains(r#"class="text-danger""#), "{expired}");
        assert!(
            expired.contains("Expired. Paste a new session"),
            "{expired}"
        );
        assert!(!expired.contains("Connected"), "{expired}");
        assert!(expired.contains("Disconnect"), "{expired}");
    }
}
