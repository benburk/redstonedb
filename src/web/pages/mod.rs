//! Every page's markup. This file holds the pieces several pages share,
//! such as facts, badges, and the 3D viewer; each page, or part of one, is
//! in its own module below.

use maud::{Markup, PreEscaped, html};

use crate::{
    category::{Category, Section},
    models::{schematic::Schematic, tag::Tag},
};

const PAGE_TITLE: &str = "text-page font-bold";
const SECTION_TITLE: &str = "text-section font-semibold";
const TABLE_HEAD: &str = "border-b border-line py-2 pr-4 text-left text-meta font-semibold text-muted";
const TABLE_CELL: &str = "border-b border-line py-2.5 pr-4 align-top";
/// One link of a segmented control, such as Cards and List; the chosen one
/// carries `aria-current`.
const SWITCH: &str = "rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink hover:no-underline aria-[current=page]:bg-raised aria-[current=page]:font-semibold aria-[current=page]:text-ink aria-[current=page]:shadow-sm";
/// One choice of a segmented control in a form: a label around a hidden
/// radio, the chosen one marked as `SWITCH` marks its link.
const CHOICE: &str = "cursor-pointer rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink has-checked:bg-raised has-checked:font-semibold has-checked:text-ink has-checked:shadow-sm has-focus-visible:outline-2 has-focus-visible:outline-brand";
/// The bar holding a segmented control's links.
const SWITCHES: &str =
    "flex flex-wrap items-center gap-1 rounded-control bg-sidebar p-1";

mod about;
mod activity;
mod build;
mod checks;
mod description;
mod details_page;
mod diff;
mod form;
mod icons;
mod interface;
mod layout;
mod leaderboard;
mod listing;
mod player;
mod schemati;
mod settings;
mod similar;
mod simulation;
mod stack_guide;
mod trace;
mod upload;
mod users;
mod version;

pub use about::{about, credits, recognized};
pub use activity::{ActivityLog, activity, activity_href};
pub use build::{
    BuildPage, ColoursShown, FileRead, build_banner, build_info,
    favorite_button, schematic_page,
};
pub use checks::{Known, Subject, form_checks};
pub use description::mentions;
pub use details_page::{
    DetailsPage, Duplicate, File, Reply, counted, details_page,
    failing_or_untested, reason_items,
};
pub use diff::diff_page;
pub use form::Choices;
pub use layout::{Preview, forbidden, not_found};
use layout::{base, base_previewed};
pub use leaderboard::{LeaderboardPage, PeoplePage, leaderboard, people};
pub use listing::{
    Facet, HomeQuery, Index, Listing, Sort, SortField, Tile, Viewing,
    card_star, home, home_index, home_results, schematics,
};
pub use player::{PlayerPage, player};
pub use schemati::{Connection, Offered, own_file_field, schemati_sent};
pub use settings::{
    Presets, block_theme, block_theme_page, custom_colours, palette_fields,
    settings,
};
pub use similar::{Found, similar_dialog, similar_page, similar_results};
pub use simulation::{simulation_results, timing_section};
pub use trace::glitch_trace;
pub use upload::{
    analysis_steps, analyzing, checking_version, upload, version_steps,
};
pub use users::{ProfileBuilds, ProfileList, login, profile, user_row, users};
/// The few facts a card has room for, such as `8 ticks · 11×19×18 ·
/// 3762 blocks · embizone`.
fn facts(s: &Schematic) -> Markup {
    let measures: Vec<String> =
        [s.latency.map(ticks), Some(size(s)), Some(volume(s))]
            .into_iter()
            .flatten()
            .collect();
    html! {
        (measures.join(" · "))
        @if !s.authors.is_empty() {
            " · " (name_links(&s.authors))
        }
    }
}

/// The home page searching for `term`, such as a tag or an author.
fn search_href(term: &str) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("q", term)
        .finish();
    format!("/?{query}")
}

/// `text` linking to the builds matching `term`. It sits above a card's
/// own link.
fn search_link(text: &str, term: &str) -> Markup {
    html! {
        a href=(search_href(term)) class="relative z-10" { (text) }
    }
}

/// In-game names, each a link to its player's page, which lists what
/// they built and helped with. The links sit above a card's own link.
fn name_links(names: &[String]) -> Markup {
    html! {
        @for (i, name) in names.iter().enumerate() {
            @if i > 0 { ", " }
            a href=(player_href(name)) class="relative z-10" { (name) }
        }
    }
}

/// A person as a badge: their Minecraft head, blank until it loads or when
/// there is none, and their name, linking to `href`.
fn person_badge(name: &str, href: &str) -> Markup {
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
fn person_link(name: &str, href: &str) -> Markup {
    html! {
        a href=(href) class="inline-flex items-center gap-2" {
            img src={ "/players/" (name) "/head.png" } alt="" width="16" height="16" loading="lazy"
                class="size-4 shrink-0 rounded-[3px] bg-line [image-rendering:pixelated]";
            span { (name) }
        }
    }
}

/// Each of `names` as a badge linking to their player page.
fn name_badges(names: &[String]) -> Markup {
    html! {
        span class="flex flex-wrap gap-1.5" {
            @for name in names {
                (person_badge(name, &player_href(name)))
            }
        }
    }
}

/// The page of the Minecraft player `name`.
fn player_href(name: &str) -> String {
    format!("/players/{name}")
}

/// A small spinning ring, for something under way; still where motion is
/// reduced.
pub fn spinner() -> Markup {
    html! {
        span aria-hidden="true"
            class="inline-block size-4 shrink-0 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none" {}
    }
}

/// How many people favourited a build, said in words.
fn favourites_label(count: i64) -> String {
    match count {
        1 => "Favourited by 1 person".to_owned(),
        n => format!("Favourited by {n} people"),
    }
}

/// `n` with its thousands separated by commas, such as `131,072`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Tenths of a percent as a whole percent, such as `98%`.
fn percent(permille: u32) -> String {
    format!("{}%", permille / 10)
}

/// The standard `categories` as the menu lists them: each section, by
/// its key and heading, with its categories, then any in none under Other.
/// A section with no categories is left out.
fn sections(categories: &[Category]) -> Vec<(&str, &str, Vec<&Category>)> {
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

/// `Author` for one name, else `Authors`.
fn plural(singular: &str, names: &[String]) -> String {
    if names.len() == 1 {
        singular.to_owned()
    } else {
        format!("{singular}s")
    }
}

/// A tag of build `s`, linking to the builds of its category that carry
/// it, as flags of one name differ between categories; a build off the
/// list, to those under Other.
fn badge(tag: &Tag, s: &Schematic) -> Markup {
    let category = if s.category_label.is_some() {
        s.category.as_str()
    } else {
        "other"
    };
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("category", category)
        .append_pair("tag", &tag.name)
        .finish();
    html! {
        a href={ "/?" (query) }
            title=(tag.about())
            class={ "badge " (tag.class()) " relative z-10 hover:no-underline hover:brightness-95" } {
            (tag.label())
        }
    }
}

/// One reason a build isn't verified, as a list item: what is wrong, then
/// how to fix it, with a link to the rule it breaks when the Build
/// Analysis page pictures it.
fn reason_item(reason: &str) -> Markup {
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

/// The mark of a build only its uploader sees.
fn private_badge() -> Markup {
    html! {
        span class="badge badge-author" title="Only you can see it" {
            (icons::icon(icons::LOCK, "size-3")) "Private"
        }
    }
}

/// Why a download is greyed out for someone signed out.
const SIGN_IN_TO_DOWNLOAD: &str = "Sign in to download";

/// A link to download a file at `href`, for someone signed in; greyed out,
/// saying why, for anyone else.
fn download_link(href: &str, text: &str, signed_in: bool) -> Markup {
    html! {
        @if signed_in {
            a href=(href) { (text) }
        } @else {
            span class="cursor-not-allowed text-muted" title=(SIGN_IN_TO_DOWNLOAD) { (text) }
        }
    }
}

fn field(name: &str, value: impl maud::Render) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}

fn latency(s: &Schematic) -> String {
    s.latency.map(ticks).unwrap_or_default()
}

/// `n` game ticks, such as `1 tick` or `3 ticks`.
fn ticks(n: i64) -> String {
    if n == 1 {
        "1 tick".to_owned()
    } else {
        format!("{n} ticks")
    }
}

/// The blocks a build takes as it is compared, said per copy for a
/// stackable build, such as `48 blocks per copy`.
fn volume(s: &Schematic) -> String {
    let per = if s.per_copy().is_some() {
        " per copy"
    } else {
        ""
    };
    format!("{} blocks{per}", s.volume())
}

/// Height × width × depth, as the build looks from the south.
fn size(s: &Schematic) -> String {
    let (x, y, z) = s.size;
    format!("{y}×{x}×{z}")
}

/// The layout of a build's page and of an upload's preview: `side`, its
/// facts or form, on the left, and the 3D view of the file at `file` on
/// the right, staying in sight while the side scrolls. On a phone the view
/// comes first, or last with `side_first`, as for a form to fill in.
fn beside_viewer(file: &str, side: &Markup, side_first: bool) -> Markup {
    beside_viewer_under(file, &html! {}, side, side_first)
}

/// As [`beside_viewer`], with `above` over the viewer, such as a choice of
/// the colours it shows.
fn beside_viewer_under(
    file: &str,
    above: &Markup,
    side: &Markup,
    side_first: bool,
) -> Markup {
    let viewer = if side_first {
        "order-last desktop:sticky desktop:top-4"
    } else {
        "desktop:sticky desktop:top-4 desktop:order-last"
    };
    html! {
        // One column on a phone, no wider than the screen, so a wide table
        // scrolls in its own box rather than widening the page.
        div class="grid grid-cols-[minmax(0,1fr)] items-start gap-8 desktop:grid-cols-[var(--container-form)_minmax(0,1fr)]" {
            div class=(viewer) {
                (above)
                (viewer_3d(file))
            }
            div class="flex flex-col gap-8" { (side) }
        }
    }
}

/// Where `static/viewer.js` finds the renderer's own imports.
const VIEWER_IMPORTS: &str = r#"{ "imports": {
  "three": "https://cdn.jsdelivr.net/npm/three@0.184.0/build/three.module.js",
  "nucleation": "https://cdn.jsdelivr.net/npm/nucleation@0.2.18/nucleation.js"
} }"#;

/// A 3D view of the schematic at `file`, an address the browser can fetch,
/// drawn by `static/viewer.js`.
fn viewer_3d(file: &str) -> Markup {
    viewer_canvas(file, None)
}

/// A 3D view of the two sides of a diff, `after` shown first and `before`
/// in its place when a `data-side` button asks, the camera kept.
fn viewer_3d_sides(after: &str, before: &str) -> Markup {
    viewer_canvas(after, Some(before))
}

/// The viewer's canvas, drawing `file`, and `before` too, hidden, if given.
fn viewer_canvas(file: &str, before: Option<&str>) -> Markup {
    html! {
        script type="importmap" { (PreEscaped(VIEWER_IMPORTS)) }
        // The renderer sizes its canvas to the parent, so the parent's height
        // must be fixed or the two grow each other without bound.
        div class="relative h-[60vh] overflow-hidden rounded-overlay border border-line bg-sidebar desktop:h-[calc(100vh-12rem)]" {
            canvas id="viewer" data-src=(file) data-before=[before] aria-label="3D view of the schematic"
                class="absolute inset-0 size-full" {}
            p id="viewer-status" class="absolute inset-0 flex items-center justify-center text-muted" {
                "Loading viewer…"
            }
        }
        script type="module" src=(crate::web::VIEWER_JS.url()) {}
    }
}
