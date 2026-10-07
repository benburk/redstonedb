use maud::{DOCTYPE, Markup, html};

use crate::{APP_NAME, models::user::User};

/// A page around `content`, with the header for `viewer`, the account
/// viewing it, if any.
pub fn base(
    viewer: Option<&User>,
    page_title: &str,
    current_path: &str,
    content: &Markup,
) -> Markup {
    let document_title = if page_title == APP_NAME {
        APP_NAME.to_owned()
    } else {
        format!("{page_title} · {APP_NAME}")
    };
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (document_title) }
                link rel="stylesheet" href="/app.css";
                script defer src="/fixi.js" {}
                script defer src="/fixi-config.js" {}
            }
            body class="flex min-h-screen flex-col" {
                a href="#main"
                    class="sr-only rounded-control bg-raised px-3 py-2 focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-10" {
                    "Skip to content"
                }
                (navbar(viewer, current_path))
                main id="main" class="mx-auto w-full max-w-wide flex-1 px-4 py-8 desktop:px-5" {
                    (content)
                }
                // fixi-config.js writes failures here; no swap replaces it.
                div id="notice" role="status"
                    class="fixed bottom-4 left-1/2 -translate-x-1/2 rounded-control bg-ink px-4 py-2 text-canvas shadow-overlay empty:hidden" {}
            }
        }
    }
}

pub fn navbar(viewer: Option<&User>, current_path: &str) -> Markup {
    let current = |path: &str| (current_path == path).then_some("page");
    let link = "whitespace-nowrap rounded-control px-2 py-1.5 desktop:px-2.5 text-muted hover:bg-selection hover:text-ink hover:no-underline aria-[current=page]:font-semibold aria-[current=page]:text-ink";
    html! {
        header class="border-b border-line bg-sidebar" {
            nav aria-label="Primary"
                class="mx-auto flex min-h-14 max-w-wide flex-wrap items-center gap-x-1 px-4 py-2 desktop:px-5" {
                a href="/" aria-current=[current("/")]
                    class="mr-auto flex items-center gap-2.5 font-bold whitespace-nowrap text-ink hover:no-underline" {
                    // The brand mark: a block of redstone.
                    span aria-hidden="true" class="size-3.5 rotate-45 rounded-[3px] bg-redstone" {}
                    (APP_NAME)
                }
                a href="/about" aria-current=[current("/about")] class=(link) { "About" }
                @match viewer {
                    None => {
                        a href="/sessions/new" aria-current=[current("/sessions/new")] class=(link) { "Log in" }
                    }
                    Some(user) => {
                        @let profile_path = format!("/users/{}", user.username);
                        a href="/upload" aria-current=[current("/upload")] class=(link) { "Upload" }
                        @if user.is_admin() {
                            a href="/users" aria-current=[current("/users")] class=(link) { "Users" }
                        }
                        a href=(profile_path) aria-current=[current(&profile_path)]
                            class={ (link) " flex items-center gap-2" } {
                            @if let Some(avatar) = user.avatar_url() {
                                img src=(avatar) alt="" width="20" height="20" class="size-5 rounded-full";
                            }
                            (user.name())
                        }
                        a href="/" fx-action="/sessions" fx-method="delete" fx-swap="none" class=(link) {
                            "Log out"
                        }
                    }
                }
            }
        }
    }
}
