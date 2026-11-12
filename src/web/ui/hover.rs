//! A note shown while what it is about is pointed at or focused.

use maud::{Markup, html};

/// `content` with `note` in a small popup above it while it is hovered or
/// focused, and as its description for a screen reader. `id` names the
/// note, once on the page. Hidden, it takes no room, and on a phone it
/// shows along the foot of the screen, so one near the edge never widens
/// the page.
pub(crate) fn hover_note(id: &str, content: &Markup, note: &str) -> Markup {
    html! {
        span tabindex="0" aria-describedby=(id)
            class="group relative inline-block cursor-help rounded-control focus-visible:outline-2 focus-visible:outline-boundary" {
            (content)
            span id=(id) role="tooltip"
                class="pointer-events-none absolute bottom-full left-0 z-20 mb-1.5 hidden w-max max-w-xs max-desktop:fixed max-desktop:inset-x-4 max-desktop:top-auto max-desktop:bottom-4 max-desktop:mb-0 max-desktop:w-auto max-desktop:max-w-none rounded-control bg-ink px-2.5 py-1.5 text-meta font-normal text-canvas shadow-overlay transition-[opacity,display] transition-discrete duration-150 ease-calm group-hover:block group-focus-within:block starting:opacity-0 motion-reduce:transition-none" {
                (note)
            }
        }
    }
}

/// `words` underlined with dots, to say that pointing at them says more,
/// with `note` on hover or focus, as [`hover_note`] shows it.
pub(crate) fn hover_words(id: &str, words: &str, note: &str) -> Markup {
    let dotted = html! {
        span class="underline decoration-boundary decoration-dotted underline-offset-4" { (words) }
    };
    hover_note(id, &dotted, note)
}
