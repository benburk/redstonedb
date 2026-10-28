//! A Minecraft player's page.

use maud::{Markup, html};

use crate::models::{schematic::Schematic, user::User};

use super::{PAGE_TITLE, SECTION_TITLE, base, listing::cards};

/// What a player's page shows.
pub struct PlayerPage<'a> {
    /// As the site credits it.
    pub name: &'a str,
    /// Their UUID, for their head, when Mojang knows them.
    pub uuid: Option<&'a str>,
    /// The account whose confirmed Minecraft name this is, if they have
    /// signed up.
    pub account: Option<&'a User>,
    pub built: &'a [Schematic],
    pub contributed: &'a [Schematic],
    /// First places they hold on the leaderboards.
    pub records: usize,
    /// The viewer's palette's key, for thumbnails, if they chose one.
    pub palette: Option<&'a str>,
}

pub fn player(viewer: Option<&User>, page: &PlayerPage) -> Markup {
    let PlayerPage {
        name,
        uuid,
        account,
        built,
        contributed,
        records,
        palette: _,
    } = *page;
    base(
        viewer,
        name,
        &format!("/players/{name}"),
        &html! {
            div class="mb-8 flex items-center gap-4" {
                @if let Some(uuid) = uuid {
                    img src={ "/heads/" (uuid) } alt="" width="64" height="64"
                        class="size-16 rounded-[2px] [image-rendering:pixelated]";
                }
                div {
                    h1 class=(PAGE_TITLE) { (name) }
                    p class="text-muted" {
                        @match account {
                            Some(account) => {
                                "Signed up as "
                                a href={ "/users/" (account.username) } { "@" (account.username) }
                            }
                            None => { "Not signed up" }
                        }
                        @if records > 0 {
                            " · "
                            a href="/leaderboards" {
                                (records) (if records == 1 { " record" } else { " records" })
                            }
                        }
                    }
                }
            }
            (credits(built, contributed, &html! {
                p class="text-muted" { "No builds credit " (name) " as builder yet." }
            }, page.palette))
        },
    )
}

/// The builds crediting a player, as builder and as contributor, as cards
/// under their headings, pictured in the viewer's `palette`; `unbuilt`
/// stands for the first when it is empty.
pub(super) fn credits(
    built: &[Schematic],
    contributed: &[Schematic],
    unbuilt: &Markup,
    palette: Option<&str>,
) -> Markup {
    html! {
        section aria-labelledby="built-heading" class="mb-10" {
            h2 id="built-heading" class={ (SECTION_TITLE) " mb-3" } { "Built" }
            @if built.is_empty() { (unbuilt) } @else { (cards(built, &[], palette)) }
        }
        @if !contributed.is_empty() {
            section aria-labelledby="contributed-heading" class="mb-10" {
                h2 id="contributed-heading" class={ (SECTION_TITLE) " mb-3" } { "Contributed to" }
                (cards(contributed, &[], palette))
            }
        }
    }
}
