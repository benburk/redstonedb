//! The markup several pages share, such as the page around each one,
//! facts, badges, and the 3D viewer.

pub(crate) mod cards;
pub(crate) mod description;
pub(crate) mod dialog;
pub(crate) mod editable;
pub(crate) mod hover;
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

/// What a pill or a mark says of what it marks, which its colours show;
/// in markup, the class that colours it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Good,
    /// Worth a look, but nothing wrong.
    Warn,
    Bad,
    /// Neither good nor bad.
    Plain,
}

impl Tone {
    pub(crate) const fn class(self) -> &'static str {
        match self {
            Tone::Good => "tone-good",
            Tone::Warn => "tone-warn",
            Tone::Bad => "tone-bad",
            Tone::Plain => "tone-plain",
        }
    }
}

impl maud::Render for Tone {
    fn render_to(&self, buffer: &mut String) {
        buffer.push_str(self.class());
    }
}

/// A small spinning ring, for something under way; still where motion is
/// reduced.
pub fn spinner() -> Markup {
    html! {
        span aria-hidden="true"
            class="inline-block size-4 shrink-0 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
    }
}

/// A line saying something is under way, with a spinner.
pub(crate) fn waiting(text: &str) -> Markup {
    html! {
        p class="flex items-center gap-2 text-muted" { (spinner()) (text) }
    }
}

/// The top of a page that is a list to manage: its title, a line on what
/// it lists, and `message`, why something asked of it was refused, when
/// there is one.
pub(crate) fn page_header(title: &str, about: &str, message: &str) -> Markup {
    html! {
        h1 class={ (PAGE_TITLE) " mb-2" } { (title) }
        p class="mb-6 max-w-prose text-muted" { (about) }
        @if !message.is_empty() {
            p role="alert" class="mb-6 text-danger" { (message) }
        }
    }
}

/// One link of a segmented control: what it says, where it goes, whether
/// it is the one chosen, and how many it leads to, when it says.
pub(crate) struct Segment<'a> {
    pub label: &'a str,
    pub href: String,
    pub chosen: bool,
    pub count: Option<usize>,
}

/// A segmented control: links side by side in a bar, the chosen one
/// marked, such as a list's filters.
pub(crate) fn segmented(segments: &[Segment]) -> Markup {
    html! {
        div class={ (SWITCHES) " w-fit" } {
            @for segment in segments {
                a href=(segment.href) class=(SWITCH) aria-current=[segment.chosen.then_some("page")] {
                    (segment.label)
                    @if let Some(count) = segment.count {
                        span class="ml-1.5 text-meta text-muted tabular-nums" { (count) }
                    }
                }
            }
        }
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

/// Why a download is greyed out for someone signed out.
const SIGN_IN_TO_DOWNLOAD: &str = "Sign in to download";

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

/// A download's button for someone signed out: greyed out, saying why.
pub(crate) fn locked_download(label: &str, class: &str) -> Markup {
    html! {
        button type="button" aria-disabled="true" title=(SIGN_IN_TO_DOWNLOAD) class=(class) { (label) }
    }
}

pub(crate) fn field(name: &str, value: impl maud::Render) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}
