//! Full page templates.
//!
//! These return complete HTML documents wrapped in the base layout.
//! For fixi partial updates, use components directly.

use maud::{Markup, PreEscaped, html};

use crate::{APP_NAME, models::schematic::Schematic};

use super::components::{
    SessionDisplay, base, email_form, login_form, password_form, signup_form,
    username_form,
};

pub fn schematics(username: &str, schematics: &[Schematic]) -> Markup {
    base(
        username,
        APP_NAME,
        "/",
        &html! {
            h1 { (APP_NAME) }
            @if schematics.is_empty() {
                p { "No schematics yet. Import one with " code { "just rsdb add" } "." }
            } @else {
                div class="overflow-auto" {
                    table class="striped" {
                        thead {
                            tr {
                                th scope="col" { "Name" }
                                th scope="col" { "Orientation" }
                                th scope="col" { "Encoding" }
                                th scope="col" { "Timing" }
                                th scope="col" { "Latency" }
                                th scope="col" { "Size" }
                                th scope="col" { "Author" }
                                th scope="col" { "Built" }
                            }
                        }
                        tbody {
                            @for s in schematics {
                                tr {
                                    th scope="row" {
                                        a href={ "/schematics/" (s.id) } { (s.title()) }
                                    }
                                    td { (s.orientation) }
                                    td { (s.encoding) }
                                    td { (timing(s)) }
                                    td { (latency(s)) }
                                    td { (size(s)) }
                                    td { (s.author.as_deref().unwrap_or("")) }
                                    td { (s.built_on.as_deref().unwrap_or("")) }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

pub fn schematic(username: &str, s: &Schematic) -> Markup {
    let title = s.title();
    let path = format!("/schematics/{}", s.id);
    let file = format!("{path}/download");
    base(
        username,
        &title,
        &path,
        &html! {
            h1 { (title) }
            (viewer(&file))
            p {
                a href=(file) download=(s.filename()) role="button" { "Download" }
            }
            table {
                tbody {
                    (field("Function", &s.function.replace('_', " ")))
                    (field("Width", &s.width.map(|w| format!("{w} bits")).unwrap_or_default()))
                    (field("Orientation", &s.orientation))
                    (field("Encoding", &s.encoding))
                    (field("Timing", &timing(s)))
                    (field("Latency", &latency(s)))
                    (field("Size", &size(s)))
                    (field("Author", s.author.as_deref().unwrap_or("unknown")))
                    (field("Built", s.built_on.as_deref().unwrap_or("unknown")))
                    (field("Tags", &s.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(" ")))
                    (field("File", &s.filename()))
                    (field("Imported from", &s.source_name))
                }
            }
        },
    )
}

fn field(name: &str, value: &str) -> Markup {
    html! {
        tr {
            th scope="row" { (name) }
            td { (value) }
        }
    }
}

fn timing(s: &Schematic) -> String {
    s.tags
        .iter()
        .filter(|t| t.category == "timing")
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(" ")
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
        div style="height: 70vh; position: relative" {
            canvas id="viewer" data-src=(file)
                style="position: absolute; inset: 0; width: 100%; height: 100%" {}
        }
        p id="viewer-status" { small { "Loading viewer…" } }
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
            h1 { "About" }
            p { "Hello World" }
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
            h1 { "Settings" }
            section {
                (username_form("", "", false))
            }
            section {
                (email_form(email.unwrap_or(""), "", false))
            }
            section {
                details {
                    summary { "Change password" }
                    (password_form("", "", false, false))
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
            h1 { "Hello, " (username) "!" }
            h2 id="active-sessions-heading" tabindex="-1" { "Active Sessions" }
            @if !sessions.is_empty() {
                table {
                    thead {
                        tr {
                            th scope="col" { "Device/Browser" }
                            th scope="col" { "IP Address" }
                            th scope="col" { "Created" }
                            th scope="col" { "Expires" }
                            th scope="col" { "Actions" }
                        }
                    }
                    tbody {
                        @for session in sessions {
                            tr id={ "session-" (session.id) }
                                data-theme=[session.is_current.then_some("primary")] {
                                td { (session.user_agent) }
                                td { (session.ip_address) }
                                td { (session.created_at) }
                                td { (session.expires_at) }
                                td {
                                    @if session.is_current {
                                        small { "Current session" }
                                    } @else {
                                        button type="button"
                                            fx-action={"/sessions/" (session.id)}
                                            fx-method="delete"
                                            fx-target={"#session-" (session.id)}
                                            ext-fx-confirm="Are you sure you want to revoke this session?"
                                            data-focus-after-remove="#active-sessions-heading"
                                            data-theme="outline"
                                        {
                                            "Revoke"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } @else {
                p { "No active sessions found." }
            }
        },
    )
}
