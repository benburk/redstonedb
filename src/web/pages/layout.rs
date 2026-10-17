//! The frame every page shares, its header, and the pages with nothing
//! else in them: not found and not allowed.

use maud::{DOCTYPE, Markup, html};

use crate::{APP_NAME, models::user::User};

use super::PAGE_TITLE;

/// What the site is, for search results and link previews.
const DESCRIPTION: &str = "Minecraft redstone schematics, such as adders and \
    multipliers, each with its latency, size, and builders, ranked and ready \
    to download.";

/// A block of redstone, turned on its corner, as in the header.
const ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Crect x='3' y='3' width='10' height='10' rx='1.5' transform='rotate(45 8 8)' fill='%23c4302b'/%3E%3C/svg%3E";

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
                meta name="description" content=(DESCRIPTION);
                // The brand mark, inline, so there is no icon file to fetch.
                link rel="icon" href=(ICON);
                link rel="stylesheet" href=(crate::web::CSS.url());
                // fixi first, so the app's policies hear its every event.
                script defer src=(crate::web::FIXI_JS.url()) {}
                script defer src=(crate::web::APP_JS.url()) {}
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
                (footer(viewer))
                // The one place a script says what happened, outside every
                // swap, so it is announced and stays put.
                p id="notice" role="status"
                    class="fixed right-4 bottom-4 max-w-sm rounded-control bg-ink px-3 py-2 text-canvas shadow-overlay empty:hidden" {}
            }
        }
    }
}

/// The site's header: its name, its pages, and the viewer's account or a
/// way to log in, `current_path` marked as the page shown.
fn navbar(viewer: Option<&User>, current_path: &str) -> Markup {
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
                a href="/leaderboards"
                    aria-current=[current_path.starts_with("/leaderboards").then_some("page")]
                    class=(link) { "Leaderboards" }
                a href="/similar" aria-current=[current("/similar")] class=(link) { "Find similar" }
                a href="/about" aria-current=[current("/about")] class=(link) { "About" }
                @match viewer {
                    None => {
                        a href="/sessions/new" aria-current=[current("/sessions/new")] class=(link) { "Log in" }
                    }
                    Some(user) => {
                        @let profile_path = format!("/users/{}", user.username);
                        a href="/upload" aria-current=[current("/upload")] class=(link) { "Upload" }
                        a href="/schemati" aria-current=[current("/schemati")] class=(link) { "Schemati" }
                        @if user.is_admin() {
                            a href="/users" aria-current=[current("/users")] class=(link) { "Users" }
                        }
                        a href=(profile_path) aria-current=[current(&profile_path)]
                            class={ (link) " flex items-center gap-2" } {
                            @if let Some(picture) = user.picture(20) { (picture) }
                            (user.name())
                        }
                        form action="/logout" method="post" class="contents" {
                            button type="submit" class={ (link) " cursor-pointer" } { "Log out" }
                        }
                    }
                }
            }
        }
    }
}

/// The pages most visits don't need, at the foot of every page.
fn footer(viewer: Option<&User>) -> Markup {
    html! {
        footer class="mt-12 border-t border-line" {
            nav aria-label="More"
                class="mx-auto flex max-w-wide flex-wrap gap-x-5 gap-y-2 px-4 py-6 text-meta desktop:px-5 *:text-muted *:hover:text-ink" {
                a href="/about" { "About" }
                a href="/about/categories" { "How builds are sorted" }
                a href="/about/simulation" { "What the site checks" }
                a href="/about/conventions" { "Building conventions" }
                a href="/builds" { "All builds" }
                @if viewer.is_some_and(User::is_admin) {
                    a href="/users" { "Users" }
                }
            }
        }
    }
}

/// The page for an address that names nothing.
pub fn not_found(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "Not found",
        "",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "Not found" }
                p class="mb-4" {
                    "Nothing is here. The build or person may have been removed, or the address mistyped."
                }
                p { a href="/" { "See every build" } }
            }
        },
    )
}

/// The page for a signed-in account that may not do what it asked.
pub fn forbidden(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "Not allowed",
        "",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "Not allowed" }
                p class="mb-4" {
                    "Your account doesn't have permission to do that."
                }
                p { a href="/" { "See every build" } }
            }
        },
    )
}
