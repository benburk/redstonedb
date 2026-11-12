//! A person on a page: their name, their Minecraft head, and the link to
//! their page.

use maud::{Markup, html};

use crate::{account::User, web::href};

/// An account's picture at `size` pixels square: its player's head, drawn
/// pixelated, when Mojang knows the player, else the Discord avatar,
/// round, if it has one.
pub(crate) fn picture(user: &User, size: u32) -> Option<Markup> {
    let (src, shape) = match (&user.minecraft_uuid, user.avatar_url()) {
        (Some(uuid), _) => (
            format!("/heads/{uuid}"),
            "rounded-[2px] [image-rendering:pixelated]",
        ),
        (None, Some(avatar)) => (avatar, "rounded-full"),
        (None, None) => return None,
    };
    Some(html! {
        img src=(src) alt="" width=(size) height=(size)
            class=(shape) style={ "width:" (size) "px;height:" (size) "px" };
    })
}

/// A player as a badge in running text, as a description's `@name` shows
/// them: their head, blank until it loads, and their name, linking to
/// their page.
pub(crate) fn mention(name: &str) -> Markup {
    html! {
        a href=(href::player(name)) class="mention" {
            img src=(href::head(name)) alt="" width="16" height="16" loading="lazy";
            (name)
        }
    }
}

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
            img src=(href::head(name)) alt="" width="18" height="18" loading="lazy"
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
            img src=(href::head(name)) alt="" width="16" height="16" loading="lazy"
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
