//! The frame every page shares, its sidebar, and the pages with nothing
//! else in them: not found and not allowed.

use maud::{DOCTYPE, Markup, html};

use crate::{APP_NAME, models::user::User};

use super::{
    PAGE_TITLE,
    icons::{self, icon},
};

/// What the site is, for search results and link previews.
const DESCRIPTION: &str = "Minecraft redstone schematics, such as adders and \
    multipliers, each with its latency, size, and builders, ranked and ready \
    to download.";

/// A block of redstone, turned on its corner, as in the header.
const ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Crect x='3' y='3' width='10' height='10' rx='1.5' transform='rotate(45 8 8)' fill='%23c4302b'/%3E%3C/svg%3E";

/// A page around `content`, with the sidebar for `viewer`, the account
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
                // Not deferred: the chosen theme must apply before the page
                // is drawn.
                script src=(crate::web::THEME_JS.url()) {}
                // fixi first, so the app's policies hear its every event.
                script defer src=(crate::web::FIXI_JS.url()) {}
                script defer src=(crate::web::APP_JS.url()) {}
                script defer src=(crate::web::SIDEBAR_JS.url()) {}
            }
            body class="min-h-screen desktop:flex" {
                a href="#main"
                    class="sr-only rounded-control bg-raised px-3 py-2 focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-10" {
                    "Skip to content"
                }
                // A phone has no room for the sidebar beside the page, so
                // a bar holds the button that opens it, where the sidebar's
                // own button closes it, and the brand, its name short.
                header class="flex min-h-14 items-center gap-3 border-b border-line bg-sidebar px-4 py-2 desktop:hidden" {
                    button type="button" popovertarget="sidebar" aria-label="Open the menu"
                        class="btn btn-quiet min-h-9 px-2" {
                        (icon(icons::MENU, "size-5"))
                    }
                    (brand(current_path, "RSDB"))
                }
                (sidebar(viewer, current_path))
                div class="min-w-0 flex-1" {
                    main id="main" class="mx-auto w-full max-w-wide px-4 py-8 desktop:px-8" {
                        (content)
                    }
                }
                // The one place a script says what happened, outside every
                // swap, so it is announced and stays put.
                p id="notice" role="status"
                    class="fixed right-4 bottom-4 max-w-sm rounded-control bg-ink px-3 py-2 text-canvas shadow-overlay empty:hidden" {}
            }
        }
    }
}

/// The site's mark and `name`, its own or a short one, linking home; a
/// screen reader hears its own name either way.
fn brand(current_path: &str, name: &str) -> Markup {
    html! {
        a href="/" aria-current=[(current_path == "/").then_some("page")] aria-label=(APP_NAME)
            class="flex items-center gap-2.5 font-bold whitespace-nowrap text-ink hover:no-underline" {
            // The brand mark: a block of redstone.
            span aria-hidden="true" class="size-3.5 shrink-0 rotate-45 rounded-[3px] bg-redstone" {}
            (name)
        }
    }
}

/// One link in the sidebar, marked as the page shown when it is.
const SIDEBAR_LINK: &str = "group flex w-full items-center gap-2.5 rounded-control px-2.5 py-1.5 text-nav text-ink hover:bg-selection hover:no-underline aria-[current=page]:bg-selection aria-[current=page]:font-semibold";

/// A sidebar link's icon: muted, as dark as its words once it is hovered or
/// is the page shown.
const SIDEBAR_ICON: &str =
    "size-4 text-muted group-hover:text-ink group-aria-[current=page]:text-ink";

/// A heading over a group of the sidebar's links.
const SIDEBAR_HEADING: &str =
    "mt-5 mb-1 px-2.5 text-meta font-semibold text-muted";

/// The sidebar, the same on every page: under the brand, the viewer's
/// profile, from which their settings are reached, or logging in; then
/// the site's pages and the pages most visits don't need; `current_path`
/// is marked as the page shown. Beside the page on a wide screen, staying
/// in view, its pages scrolling between its brand and its foot; on a
/// phone, a panel the header's button opens over the page.
fn sidebar(viewer: Option<&User>, current_path: &str) -> Markup {
    let current = |path: &str| (current_path == path).then_some("page");
    let link = |href: &str, shape: &str, label: &str, chosen: Option<&str>| {
        html! {
            li {
                a href=(href) aria-current=[chosen] class=(SIDEBAR_LINK) {
                    (icon(shape, SIDEBAR_ICON))
                    span { (label) }
                }
            }
        }
    };
    html! {
        // On a phone, a panel held to the screen's top and bottom rather
        // than sized to it, so it never changes height as the browser's
        // bars come and go, and its scrolling stays its own.
        nav id="sidebar" popover aria-label="Site"
            class="inset-y-0 left-0 m-0 h-auto max-h-none w-72 overscroll-contain max-w-[85vw] flex-col border-r border-line bg-sidebar text-ink shadow-overlay backdrop:bg-black/30 open:flex desktop:sticky desktop:top-0 desktop:flex desktop:h-screen desktop:w-60 desktop:max-w-none desktop:shrink-0 desktop:shadow-none" {
            // On a phone, its top matches the bar's, its button where the
            // bar's was.
            div class="flex min-h-14 shrink-0 items-center gap-3 px-4 desktop:min-h-0 desktop:px-5.5 desktop:pt-4 desktop:pb-4" {
                button type="button" popovertarget="sidebar" popovertargetaction="hide"
                    aria-label="Close the menu" class="btn btn-quiet min-h-9 px-2 desktop:hidden" {
                    (icon(icons::CLOSE, "size-5"))
                }
                (brand(current_path, APP_NAME))
            }
            div class="shrink-0 px-3 pb-3" {
                ul {
                    @match viewer {
                        None => (link("/sessions/new", icons::LOG_IN, "Log in", current("/sessions/new"))),
                        Some(user) => {
                            @let profile = format!("/users/{}", user.username);
                            // Settings are the profile's, so it stays marked there.
                            @let chosen = (current_path == profile || current_path == "/settings").then_some("page");
                            li {
                                a href=(profile) aria-current=[chosen] class=(SIDEBAR_LINK) {
                                    @match user.picture(16) {
                                        Some(picture) => (picture),
                                        None => (icon(icons::USER, SIDEBAR_ICON)),
                                    }
                                    span class="truncate" { (user.name()) }
                                }
                            }
                        }
                    }
                }
            }
            // The site's pages scroll between the brand and the account,
            // which stay put.
            div data-sidebar class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pb-4 [scrollbar-gutter:stable]" {
                ul class="flex flex-col gap-0.5" {
                    (link("/", icons::HOME, "Home", current("/")))
                    (link("/builds", icons::CLOCK, "Recent builds", current("/builds")))
                    (link("/leaderboards", icons::TROPHY, "Leaderboards", current_path.starts_with("/leaderboards").then_some("page")))
                    @if viewer.is_some() {
                        (link("/upload", icons::UPLOAD, "Upload", current("/upload")))
                    }
                }
                h2 class=(SIDEBAR_HEADING) { "About" }
                ul class="flex flex-col gap-0.5" {
                    (link("/about", icons::INFO, "About the site", current("/about")))
                    (link("/about/categories", icons::LAYERS, "How builds are sorted", current("/about/categories")))
                    (link("/about/simulation", icons::CHECK, "What the site checks", current("/about/simulation")))
                    (link("/about/conventions", icons::BOOK, "Building conventions", current("/about/conventions")))
                    @if viewer.is_some_and(User::is_admin) {
                        (link("/users", icons::USERS, "Users", current("/users")))
                    }
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
