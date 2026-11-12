//! The markup several pages share, such as the page around each one,
//! facts, badges, and the 3D viewer.

pub(crate) mod description;
pub(crate) mod icons;
pub(crate) mod layout;
pub(crate) mod person;
pub(crate) mod summary;
pub(crate) mod text;
pub(crate) mod viewer;

use maud::{Markup, html};

use crate::category::{Category, Section};

pub(crate) const PAGE_TITLE: &str = "text-page font-bold";
pub(crate) const SECTION_TITLE: &str = "text-section font-semibold";
pub(crate) const TABLE_HEAD: &str = "border-b border-line py-2 pr-4 text-left text-meta font-semibold text-muted";
pub(crate) const TABLE_CELL: &str =
    "border-b border-line py-2.5 pr-4 align-top";
/// One link of a segmented control, such as Cards and List; the chosen one
/// carries `aria-current`.
pub(crate) const SWITCH: &str = "rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink hover:no-underline aria-[current=page]:bg-raised aria-[current=page]:font-semibold aria-[current=page]:text-ink aria-[current=page]:shadow-sm";
/// One choice of a segmented control in a form: a label around a hidden
/// radio, the chosen one marked as `SWITCH` marks its link.
pub(crate) const CHOICE: &str = "cursor-pointer rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink has-checked:bg-raised has-checked:font-semibold has-checked:text-ink has-checked:shadow-sm has-focus-visible:outline-2 has-focus-visible:outline-brand";
/// The bar holding a segmented control's links.
pub(crate) const SWITCHES: &str =
    "flex flex-wrap items-center gap-1 rounded-control bg-sidebar p-1";

/// A small spinning ring, for something under way; still where motion is
/// reduced.
pub fn spinner() -> Markup {
    html! {
        span aria-hidden="true"
            class="inline-block size-4 shrink-0 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
    }
}

/// The standard `categories` as the menu lists them: each section, by
/// its key and heading, with its categories, then any in none under Other.
/// A section with no categories is left out.
pub(crate) fn sections(
    categories: &[Category],
) -> Vec<(&str, &str, Vec<&Category>)> {
    let of = |section: Option<Section>| -> Vec<&Category> {
        categories.iter().filter(|c| c.section == section).collect()
    };
    Section::ALL
        .iter()
        .map(|&section| (section.name(), section.label(), of(Some(section))))
        .chain([("other", "Other", of(None))])
        .filter(|(_, _, members)| !members.is_empty())
        .collect()
}

/// One reason a build isn't verified, as a list item: what is wrong, then
/// how to fix it, with a link to the rule it breaks when the Build
/// Analysis page pictures it.
pub(crate) fn reason_item(reason: &str) -> Markup {
    let advice = crate::recognize::fault::advice(reason);
    html! {
        li {
            span class="block" { (reason) }
            @if let Some(advice) = advice {
                span class="block text-muted" {
                    "Fix: " (advice.fix)
                    @if let Some(rule) = advice.rule {
                        " " a href={ "/about/recognized#" (rule) } { "See the rule" }
                    }
                }
            }
        }
    }
}

/// Why a download is greyed out for someone signed out.
pub(crate) const SIGN_IN_TO_DOWNLOAD: &str = "Sign in to download";

/// A link to download a file at `href`, for someone signed in; greyed out,
/// saying why, for anyone else.
pub(crate) fn download_link(href: &str, text: &str, signed_in: bool) -> Markup {
    html! {
        @if signed_in {
            a href=(href) { (text) }
        } @else {
            span class="cursor-not-allowed text-muted" title=(SIGN_IN_TO_DOWNLOAD) { (text) }
        }
    }
}

pub(crate) fn field(name: &str, value: impl maud::Render) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}
