//! Full page templates.
//!
//! These return complete HTML documents wrapped in the base layout.
//! For fixi partial updates, use components directly.

use maud::{Markup, PreEscaped, html};

use crate::{
    APP_NAME,
    models::schematic::{Schematic, Tag},
};

use super::components::{
    SessionDisplay, base, email_form, login_form, password_form, signup_form,
    username_form,
};

const PAGE_TITLE: &str = "text-page font-bold";
const SECTION_TITLE: &str = "text-section font-semibold";
const TABLE_HEAD: &str = "border-b border-line py-2 pr-4 text-left text-meta font-semibold text-muted";
const TABLE_CELL: &str = "border-b border-line py-2.5 pr-4 align-top";

pub fn schematics(username: &str, schematics: &[Schematic]) -> Markup {
    // Columns a phone has no room for; the name and its tags come first.
    let secondary = "hidden desktop:table-cell";
    base(
        username,
        APP_NAME,
        "/",
        &html! {
            div class="mb-6 flex items-baseline justify-between gap-4" {
                h1 class=(PAGE_TITLE) { "Schematics" }
                p class="text-muted tabular-nums" {
                    (schematics.len()) " " (if schematics.len() == 1 { "build" } else { "builds" })
                }
            }
            @if schematics.is_empty() {
                p class="text-muted" { "No schematics yet. Import one with " code { "just rsdb add" } "." }
            } @else {
                div class="overflow-x-auto" {
                    table class="w-full border-collapse" {
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Name" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Orientation" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Encoding" }
                                th scope="col" class=(TABLE_HEAD) { "Latency" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Size" }
                                th scope="col" class=(TABLE_HEAD) { "Author" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Built" }
                            }
                        }
                        tbody {
                            @for s in schematics {
                                tr class="hover:bg-selection/50" {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                        a href={ "/schematics/" (s.id) } class="font-semibold" { (s.name()) }
                                        @if !s.tags.is_empty() {
                                            ul class="mt-1.5 flex flex-wrap gap-1.5" aria-label="Tags" {
                                                @for tag in &s.tags {
                                                    li { (badge(tag)) }
                                                }
                                            }
                                        }
                                    }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.orientation_label()) }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.encoding_label()) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (latency(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (size(s)) }
                                    td class=(TABLE_CELL) { (s.author.as_deref().unwrap_or("")) }
                                    td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap " (secondary) } {
                                        (s.built_on.as_deref().unwrap_or(""))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

/// One schematic's page. `available` holds the tags that may still be
/// attached, and is `None` for a visitor who may not edit tags.
pub fn schematic(
    username: &str,
    s: &Schematic,
    available: Option<&[Tag]>,
) -> Markup {
    let name = s.name();
    let path = format!("/schematics/{}", s.id);
    base(
        username,
        &name,
        &path,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { (name) }
            div class="grid gap-8 desktop:grid-cols-[minmax(0,1fr)_var(--spacing-aside)]" {
                (viewer(&format!("{path}/download")))
                (schematic_info(s, available, ""))
            }
        },
    )
}

/// Everything on a schematic's page except its title and viewer. Tag edits
/// redraw it whole, since a tag can change the filename shown here too.
pub fn schematic_info(
    s: &Schematic,
    available: Option<&[Tag]>,
    message: &str,
) -> Markup {
    let path = format!("/schematics/{}", s.id);
    let file = s.filename();
    html! {
        div id="info" class="flex flex-col gap-8" {
            section aria-labelledby="tags-heading" {
                h2 id="tags-heading" tabindex="-1" class={ (SECTION_TITLE) " mb-3" } { "Tags" }
                @if s.tags.is_empty() {
                    p class="text-muted" { "No tags yet." }
                } @else {
                    ul class="flex flex-wrap gap-1.5" {
                        @for tag in &s.tags {
                            li {
                                @if available.is_some() {
                                    (removable_badge(&path, tag))
                                } @else {
                                    (badge(tag))
                                }
                            }
                        }
                    }
                }
                @match available {
                    Some(available) if !available.is_empty() => {
                        form action={ (path) "/tags" } method="post"
                            fx-action={ (path) "/tags" } fx-method="post" fx-target="#info"
                            class="mt-3 flex gap-2" {
                            label for="add-tag" class="sr-only" { "Tag to add" }
                            select id="add-tag" name="tag" class="input min-h-9 flex-1"
                                aria-invalid=[(!message.is_empty()).then_some("true")]
                                aria-describedby=[(!message.is_empty()).then_some("tag-message")] {
                                @for category in ["restriction", "timing", "other"] {
                                    @let tags: Vec<&Tag> = available.iter().filter(|t| t.category == category).collect();
                                    @if !tags.is_empty() {
                                        optgroup label=(category_label(category)) {
                                            @for tag in tags {
                                                option value=(tag.name) { (tag.label()) }
                                            }
                                        }
                                    }
                                }
                            }
                            button type="submit" class="btn btn-quiet" { "Add" }
                        }
                    }
                    Some(_) => {}
                    None => {
                        p class="mt-3 text-meta text-muted" {
                            a href="/sessions/new" { "Log in" } " to add or remove tags."
                        }
                    }
                }
                @if !message.is_empty() {
                    p id="tag-message" role="alert" class="message mt-2" { (message) }
                }
            }
            section aria-labelledby="file-heading" class="flex flex-col gap-2" {
                h2 id="file-heading" class="sr-only" { "File" }
                a href={ (path) "/download" } download=(file) class="btn btn-primary" { "Download" }
                p class="text-meta break-all text-muted" { code { (file) } }
            }
            section aria-labelledby="details-heading" {
                h2 id="details-heading" class={ (SECTION_TITLE) " mb-3" } { "Details" }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-2" {
                    (field("Function", &s.function.replace('_', " ")))
                    (field("Width", &s.width.map(|w| format!("{w} bits")).unwrap_or_default()))
                    (field("Orientation", s.orientation_label()))
                    (field("Encoding", s.encoding_label()))
                    (field("Latency", &latency(s)))
                    (field("Size", &size(s)))
                    (field("Author", s.author.as_deref().unwrap_or("unknown")))
                    (field("Built", s.built_on.as_deref().unwrap_or("unknown")))
                    (field("Imported from", &s.source_name))
                }
            }
        }
    }
}

fn badge(tag: &Tag) -> Markup {
    html! {
        span class={ "badge badge-" (tag.category) } title=(category_label(&tag.category)) {
            (tag.label())
        }
    }
}

fn removable_badge(path: &str, tag: &Tag) -> Markup {
    html! {
        span class={ "badge badge-" (tag.category) " pr-1" } title=(category_label(&tag.category)) {
            (tag.label())
            button type="button" id={ "remove-tag-" (tag.name) }
                aria-label={ "Remove " (tag.label()) }
                fx-action={ (path) "/tags/" (tag.name) } fx-method="delete" fx-target="#info"
                class="flex size-4.5 cursor-pointer items-center justify-center rounded-full opacity-70 hover:bg-current/15 hover:opacity-100" {
                svg aria-hidden="true" viewBox="0 0 12 12" class="size-2.5" {
                    path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" {}
                }
            }
        }
    }
}

fn category_label(category: &str) -> &str {
    match category {
        "restriction" => "Restriction",
        "timing" => "Timing",
        _ => "Other",
    }
}

fn field(name: &str, value: &str) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}

fn latency(s: &Schematic) -> String {
    s.latency.map(|t| format!("{t} ticks")).unwrap_or_default()
}

fn size(s: &Schematic) -> String {
    let (x, y, z) = s.size;
    format!("{x}×{y}×{z}")
}

/// A 3D view of the schematic, drawn by schematic-renderer with the vanilla
/// resource pack it publishes alongside its own build.
fn viewer(file: &str) -> Markup {
    html! {
        script type="importmap" { (PreEscaped(VIEWER_IMPORTS)) }
        // The renderer sizes its canvas to the parent, so the parent's height
        // must be fixed or the two grow each other without bound.
        div class="relative h-[60vh] overflow-hidden rounded-overlay border border-line bg-sidebar desktop:h-[calc(100vh-12rem)]" {
            canvas id="viewer" data-src=(file) aria-label="3D view of the schematic"
                class="absolute inset-0 size-full" {}
            p id="viewer-status" class="absolute inset-0 flex items-center justify-center text-muted" {
                "Loading viewer…"
            }
        }
        script type="module" { (PreEscaped(VIEWER_SCRIPT)) }
    }
}

const VIEWER_IMPORTS: &str = r#"{ "imports": {
  "three": "https://cdn.jsdelivr.net/npm/three@0.184.0/build/three.module.js",
  "nucleation": "https://cdn.jsdelivr.net/npm/nucleation@0.2.18/nucleation.js"
} }"#;

const VIEWER_SCRIPT: &str = r#"
const CDN = "https://cdn.jsdelivr.net/npm";
const { SchematicRenderer } = await import(`${CDN}/schematic-renderer@1.6.1/dist/schematic-renderer.es.js`);
const canvas = document.getElementById("viewer");
const status = document.getElementById("viewer-status");
const say = (text) => { status.textContent = text; };
new SchematicRenderer(canvas, {}, {
  vanilla: () => fetch(`${CDN}/schematic-renderer@1.6.1/dist/pack.zip`).then((r) => r.blob()),
}, {
  enableDragAndDrop: false,
  callbacks: {
    onRendererInitialized: async (renderer) => {
      try {
        await renderer.schematicManager.loadSchematicFromURL(canvas.dataset.src, "schematic");
        renderer.cameraManager.focusOnSchematics();
        status.hidden = true;
      } catch (error) {
        say(`Could not load the schematic: ${error.message}`);
      }
    },
  },
});
"#;

pub fn about(username: &str) -> Markup {
    base(
        username,
        "About",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "About" }
                p class="mb-4" {
                    "A collection of Minecraft redstone schematics. Each build is stored with its facts, "
                    "so it can be found, compared, and downloaded under a name that describes it."
                }
                h2 class={ (SECTION_TITLE) " mt-8 mb-3" } { "Tags" }
                p class="mb-3" { "A tag's colour gives its kind." }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2" {
                    dt { span class="badge badge-restriction" { "Restriction" } }
                    dd { "Components the build does without." }
                    dt { span class="badge badge-timing" { "Timing" } }
                    dd { "How the outputs settle, when it differs from what the function implies." }
                    dt { span class="badge badge-other" { "Other" } }
                    dd { "Anything else worth searching by, such as the design." }
                }
            }
        },
    )
}

pub fn login_page() -> Markup {
    base("", "Log in", "/sessions/new", &login_form("", ""))
}

pub fn signup_page(
    invite_code: &str,
    invite_issuer: &str,
    invite_message: &str,
) -> Markup {
    base(
        "",
        "Sign up",
        "/users/new",
        &signup_form(invite_code, invite_issuer, invite_message, "", "", ""),
    )
}

pub fn settings(username: &str, email: Option<&str>) -> Markup {
    base(
        username,
        "Settings",
        "/settings",
        &html! {
            div class="mx-auto max-w-md" {
                h1 class={ (PAGE_TITLE) " mb-6" } { "Settings" }
                div class="flex flex-col gap-10" {
                    (username_form("", "", false))
                    (email_form(email.unwrap_or(""), "", false))
                    section {
                        details class="group" {
                            summary class={ (SECTION_TITLE) " cursor-pointer" } { "Change password" }
                            div class="mt-3" { (password_form("", "", false, false)) }
                        }
                    }
                }
            }
        },
    )
}

pub fn profile(username: &str, sessions: &[SessionDisplay]) -> Markup {
    let profile_path = format!("/users/{username}");
    base(
        username,
        "Profile",
        &profile_path,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-8" } { "Hello, " (username) "!" }
            h2 id="active-sessions-heading" tabindex="-1" class={ (SECTION_TITLE) " mb-3" } { "Active sessions" }
            @if !sessions.is_empty() {
                div class="max-w-calm overflow-x-auto" {
                    table class="w-full border-collapse" {
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Device or browser" }
                                th scope="col" class=(TABLE_HEAD) { "IP address" }
                                th scope="col" class=(TABLE_HEAD) { "Created" }
                                th scope="col" class=(TABLE_HEAD) { "Expires" }
                                th scope="col" class=(TABLE_HEAD) { span class="sr-only" { "Actions" } }
                            }
                        }
                        tbody {
                            @for session in sessions {
                                tr id={ "session-" (session.id) } {
                                    td class=(TABLE_CELL) { (session.user_agent) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (session.ip_address) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (session.created_at) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (session.expires_at) }
                                    td class={ (TABLE_CELL) " text-right" } {
                                        @if session.is_current {
                                            span class="text-meta text-muted" { "Current session" }
                                        } @else {
                                            button type="button"
                                                fx-action={"/sessions/" (session.id)}
                                                fx-method="delete"
                                                fx-target={"#session-" (session.id)}
                                                ext-fx-confirm="Are you sure you want to revoke this session?"
                                                data-focus-after-remove="#active-sessions-heading"
                                                class="btn btn-quiet min-h-8 px-3 text-meta"
                                            {
                                                "Revoke"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } @else {
                p class="text-muted" { "No active sessions found." }
            }
        },
    )
}
