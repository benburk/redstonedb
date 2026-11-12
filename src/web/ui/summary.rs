//! What a card or a row says of a build: its few facts, its tags, and the
//! searches they link to.

use maud::{Markup, html};

use crate::{
    build::{Build, tag::Tag},
    web::{
        href,
        ui::{icons, person::name_links, text::ticks},
    },
};

/// The few facts a card has room for, such as `8 ticks · 11×19×18 ·
/// 3762 blocks · embizone`.
pub(crate) fn facts(s: &Build) -> Markup {
    let measures: Vec<String> =
        [s.specs.latency.map(ticks), Some(size(s)), Some(volume(s))]
            .into_iter()
            .flatten()
            .collect();
    html! {
        (measures.join(" · "))
        @if !s.credits.authors.is_empty() {
            " · " (name_links(&s.credits.authors))
        }
    }
}

pub(crate) fn latency(s: &Build) -> String {
    s.specs.latency.map(ticks).unwrap_or_default()
}

/// The blocks a build takes as it is compared, said per copy for a
/// stackable build, such as `48 blocks per copy`.
pub(crate) fn volume(s: &Build) -> String {
    let per = if s.per_copy().is_some() {
        " per copy"
    } else {
        ""
    };
    format!("{} blocks{per}", s.volume())
}

/// Height × width × depth, as the build looks from the south.
pub(crate) fn size(s: &Build) -> String {
    let (x, y, z) = s.file.size;
    format!("{y}×{x}×{z}")
}

/// A tag of build `s`, linking to the builds of its category that carry
/// it, as flags of one name differ between categories; a build off the
/// list, to those under Other.
pub(crate) fn badge(tag: &Tag, s: &Build) -> Markup {
    let category = if s.kind().is_some() {
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

/// The mark of a build only its uploader sees.
pub(crate) fn private_badge() -> Markup {
    html! {
        span class="badge badge-author" title="Only you can see it" {
            (icons::icon(icons::LOCK, "size-3")) "Private"
        }
    }
}

/// `text` linking to the builds matching `term`. It sits above a card's
/// own link.
pub(crate) fn search_link(text: &str, term: &str) -> Markup {
    html! {
        a href=(href::search(term)) class="relative z-10" { (text) }
    }
}
