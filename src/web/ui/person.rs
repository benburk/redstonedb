//! A person on a page: their name, their Minecraft head, and the link to
//! their page.

use crate::web::href;
use maud::{Markup, html};

/// In-game names, each a link to its player's page, which lists what
/// they built and helped with. The links sit above a card's own link.
pub(crate) fn name_links(names: &[String]) -> Markup {
    html! {
        @for (i, name) in names.iter().enumerate() {
            @if i > 0 { ", " }
            a href=(href::player(name)) class="relative z-10" { (name) }
        }
    }
}

/// A person as a badge: their Minecraft head, blank until it loads or when
/// there is none, and their name, linking to `href`.
pub(crate) fn person_badge(name: &str, href: &str) -> Markup {
    html! {
        a href=(href)
            class="relative z-10 inline-flex items-center gap-1.5 rounded-full border border-line bg-raised py-0.5 pr-2.5 pl-1 text-meta text-ink hover:bg-selection hover:no-underline" {
            img src={ "/players/" (name) "/head.png" } alt="" width="18" height="18" loading="lazy"
                class="size-[18px] rounded-[4px] bg-line [image-rendering:pixelated]";
            span { (name) }
        }
    }
}

/// A person as their Minecraft head, blank until it loads or when there is
/// none, and their name beside it, linking to `href`: a badge's parts with
/// no outline, for a list of people such as a leaderboard.
pub(crate) fn person_link(name: &str, href: &str) -> Markup {
    html! {
        a href=(href) class="inline-flex items-center gap-2" {
            img src={ "/players/" (name) "/head.png" } alt="" width="16" height="16" loading="lazy"
                class="size-4 shrink-0 rounded-[3px] bg-line [image-rendering:pixelated]";
            span { (name) }
        }
    }
}

/// Each of `names` as a badge linking to their player page.
pub(crate) fn name_badges(names: &[String]) -> Markup {
    html! {
        span class="flex flex-wrap gap-1.5" {
            @for name in names {
                (person_badge(name, &href::player(name)))
            }
        }
    }
}
