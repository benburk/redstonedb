//! A Minecraft player's page.

use maud::{Markup, html};

use crate::models::{schematic::Schematic, user::User};

use super::{
    PAGE_TITLE, SECTION_TITLE, base,
    listing::{Viewing, cards},
};

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
    /// The builds whose descriptions name them with `@name`, but don't
    /// credit them.
    pub mentioned: &'a [Schematic],
    /// First places they hold on the leaderboards.
    pub records: usize,
    pub viewing: &'a Viewing,
}

pub fn player(viewer: Option<&User>, page: &PlayerPage) -> Markup {
    let PlayerPage {
        name,
        uuid,
        account,
        built,
        contributed,
        mentioned,
        records,
        viewing,
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
            // One only named in descriptions has no credits to list.
            @if !built.is_empty() || !contributed.is_empty() || mentioned.is_empty() {
                (credits(built, contributed, &html! {
                    p class="text-muted" { "No builds credit " (name) " as builder yet." }
                }, viewing))
            }
            @if !mentioned.is_empty() {
                section aria-labelledby="mentioned-heading" class="mb-10" {
                    h2 id="mentioned-heading" class={ (SECTION_TITLE) " mb-3" } { "Mentioned in" }
                    (cards(mentioned, &[], viewing))
                }
            }
        },
    )
}

/// The builds crediting a player, as builder and as contributor, as cards
/// under their headings, as the viewer sees them; `unbuilt`
/// stands for the first when it is empty.
pub(super) fn credits(
    built: &[Schematic],
    contributed: &[Schematic],
    unbuilt: &Markup,
    viewing: &Viewing,
) -> Markup {
    html! {
        section aria-labelledby="built-heading" class="mb-10" {
            h2 id="built-heading" class={ (SECTION_TITLE) " mb-3" } { "Built" }
            @if built.is_empty() { (unbuilt) } @else { (cards(built, &[], viewing)) }
        }
        @if !contributed.is_empty() {
            section aria-labelledby="contributed-heading" class="mb-10" {
                h2 id="contributed-heading" class={ (SECTION_TITLE) " mb-3" } { "Contributed to" }
                (cards(contributed, &[], viewing))
            }
        }
    }
}
