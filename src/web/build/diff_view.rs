//! The page comparing two versions of a build: one viewer swapping the
//! older, with what the newer takes out in red, for the newer, with what it
//! adds in green, a changed component's block yellow on both; what changed,
//! in blocks and in what each version measured; and a choice of two other
//! versions.

use maud::{Markup, html};

use crate::{
    account::User,
    build::Build,
    compare::diff::Counts,
    store::versions::{Measures, Version},
    web::{
        href,
        ui::{
            PAGE_TITLE, SECTION_TITLE, SWITCHES, layout::base, locked_download,
            text::plural, viewer::viewer_3d_sides,
        },
    },
};

/// One button of the viewer's two sides; the side shown is pressed.
const SIDE: &str = "cursor-pointer rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink disabled:cursor-wait aria-pressed:bg-raised aria-pressed:font-semibold aria-pressed:text-ink aria-pressed:shadow-sm";

/// The diff of build `s`'s versions `from` and `to`, older first, of its
/// `versions`, newest first, which changed as `counts` says.
pub fn diff_page(
    viewer: Option<&User>,
    s: &Build,
    versions: &[Version],
    (from, to): (i64, i64),
    counts: Counts,
) -> Markup {
    let path = href::build(&s.id);
    let between = format!("from={from}&to={to}");
    let file = |side: &str| format!("{path}/diff/{side}?{between}");
    let preview = |side: &str| format!("{}&preview=yes", file(side));
    let measures = |number: i64| {
        versions
            .iter()
            .find(|v| v.version == number)
            .and_then(|v| v.measures.as_ref())
    };
    let name = s.name();
    let title = format!("{name}: version {from} to {to}");
    base(
        viewer,
        &title,
        &format!("{path}/diff"),
        &html! {
            p class="mb-1 text-meta" { a href=(path) { "← " (name) } }
            div class="mb-6 flex flex-wrap items-center justify-between gap-3" {
                h1 class=(PAGE_TITLE) { "Version " (from) " to " (to) }
                (steps(&path, versions, (from, to)))
            }
            div class="grid grid-cols-[minmax(0,1fr)] items-start gap-8 desktop:grid-cols-[var(--container-form)_minmax(0,1fr)]" {
                div class="desktop:sticky desktop:top-4 desktop:order-last" {
                    div class="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1" {
                        div class=(SWITCHES) role="group" aria-label="Version shown" {
                            button type="button" class=(SIDE) data-side="before" aria-pressed="false" disabled { "Before, v" (from) }
                            button type="button" class=(SIDE) data-side="after" aria-pressed="true" disabled { "After, v" (to) }
                        }
                    }
                    (viewer_3d_sides(&preview("after"), &preview("before")))
                }
                div class="flex flex-col gap-8" {
                    section aria-labelledby="changed-heading" {
                        h2 id="changed-heading" class={ (SECTION_TITLE) " mb-3" } { "What changed" }
                        p { (summary(counts)) }
                        @if let Some(lines) = measured(measures(from), measures(to)) {
                            ul class="mt-2 flex flex-col gap-1" {
                                @for line in lines { li { (line) } }
                            }
                        }
                    }
                    (legend())
                    section aria-labelledby="files-heading" {
                        h2 id="files-heading" class={ (SECTION_TITLE) " mb-3" } { "Files" }
                        p class="mb-3 text-muted" {
                            "Each side works as its version does, only its building blocks coloured. Paste it in game to walk round the change."
                        }
                        div class="flex flex-wrap gap-2" {
                            @for (side, label) in [("before", format!("Before, v{from}")), ("after", format!("After, v{to}"))] {
                                @if viewer.is_some() {
                                    a href=(file(side)) class="btn btn-quiet" { "Download " (label) }
                                } @else {
                                    (locked_download(&format!("Download {label}"), "btn btn-quiet"))
                                }
                            }
                        }
                    }
                    (picker(&path, versions, (from, to)))
                }
            }
        },
    )
}

/// Links to the diffs a step back and a step on, so the versions can be
/// followed one change at a time: from comparing `from` with `to`, back
/// to the version before `from` with `from`, and on to `to` with the one
/// after; each only where that version is one of `versions`.
fn steps(path: &str, versions: &[Version], (from, to): (i64, i64)) -> Markup {
    let has = |number: i64| versions.iter().any(|v| v.version == number);
    let link =
        |older: i64, newer: i64| format!("{path}/diff?from={older}&to={newer}");
    html! {
        nav aria-label="Other changes" class="flex items-center gap-2" {
            @if has(from - 1) {
                a href=(link(from - 1, from)) class="btn btn-quiet" rel="prev" { "← v" (from - 1) " to v" (from) }
            }
            @if has(to + 1) {
                a href=(link(to, to + 1)) class="btn btn-quiet" rel="next" { "v" (to) " to v" (to + 1) " →" }
            }
        }
    }
}

/// What changed in blocks, as `+12 −3 blocks, ~2 components`.
pub fn summary(counts: Counts) -> String {
    if counts.is_empty() {
        return "No blocks changed".to_owned();
    }
    let mut parts = Vec::new();
    // Only the sides that changed, as `+25 blocks` or `+1 −6 blocks`.
    let sides: Vec<String> = [("+", counts.added), ("−", counts.removed)]
        .into_iter()
        .filter(|&(_, n)| n > 0)
        .map(|(sign, n)| format!("{sign}{n}"))
        .collect();
    if !sides.is_empty() {
        let blocks = plural(counts.added + counts.removed, "block");
        parts.push(format!("{} {blocks}", sides.join(" ")));
    }
    if counts.components > 0 {
        parts.push(format!(
            "~{} {}",
            counts.components,
            plural(counts.components, "component")
        ));
    }
    parts.join(", ")
}

/// What changed in what the two versions measured, one line each, such as
/// `Latency 5t → 4t`; `None` when either was not kept.
pub fn measured(
    before: Option<&Measures>,
    after: Option<&Measures>,
) -> Option<Vec<String>> {
    let (before, after) = (before?, after?);
    let mut lines = Vec::new();
    let mut line = |what: &str, old: String, new: String| {
        if old != new {
            lines.push(format!("{what} {old} → {new}"));
        }
    };
    line("Test", verdict(&before.verdict), verdict(&after.verdict));
    let ticks =
        |t: Option<i64>| t.map_or("none".to_owned(), |t| format!("{t}t"));
    line("Latency", ticks(before.latency), ticks(after.latency));
    line(
        "Throughput",
        ticks(before.throughput),
        ticks(after.throughput),
    );
    let size = |[x, y, z]: [i64; 3]| format!("{x}×{y}×{z}");
    line("Size", size(before.size), size(after.size));
    let pitch = |p: Option<i64>| {
        p.map_or("doesn't stack".to_owned(), |p| {
            format!("{p}-wide stackable")
        })
    };
    line("Stacking", pitch(before.pitch), pitch(after.pitch));
    Some(lines)
}

/// A verdict as the page says it.
fn verdict(verdict: &str) -> String {
    match verdict {
        "passes" => "passes",
        "wrong" => "gets cases wrong",
        "broken" => "broken",
        "skipped" => "skipped",
        _ => "untested",
    }
    .to_owned()
}

/// What the colours mean.
fn legend() -> Markup {
    let swatch = |colour: &str| {
        html! { span class={ "inline-block size-3.5 shrink-0 rounded-sm border border-line " (colour) } aria-hidden="true" {} }
    };
    html! {
        section aria-labelledby="legend-heading" {
            h2 id="legend-heading" class={ (SECTION_TITLE) " mb-3" } { "Colours" }
            ul class="flex flex-col gap-1.5" {
                li class="flex items-center gap-2" { (swatch("bg-[#8e2121]")) "Taken out, shown before" }
                li class="flex items-center gap-2" { (swatch("bg-[#5ea918]")) "Added, shown after" }
                li class="flex items-center gap-2" { (swatch("bg-[#f1af15]")) "A component on it changed, shown on both" }
            }
            p class="mt-2 text-muted" {
                "Concrete for a solid block, stained glass for a see-through one. A component that went or came marks the block it's on in red or green."
            }
        }
    }
}

/// A choice of two versions to compare.
fn picker(path: &str, versions: &[Version], (from, to): (i64, i64)) -> Markup {
    let select = |name: &str, label: &str, chosen: i64| {
        html! {
            label class="flex flex-col gap-1" {
                span class="text-meta font-semibold text-muted" { (label) }
                select name=(name) class="input" {
                    @for v in versions.iter().rev() {
                        option value=(v.version) selected[v.version == chosen] { "Version " (v.version) }
                    }
                }
            }
        }
    };
    html! {
        @if versions.len() > 2 {
            section aria-labelledby="compare-heading" {
                h2 id="compare-heading" class={ (SECTION_TITLE) " mb-3" } { "Compare others" }
                form action={ (path) "/diff" } method="get" class="flex flex-wrap items-end gap-3" {
                    (select("from", "From", from))
                    (select("to", "To", to))
                    button type="submit" class="btn btn-quiet" { "Compare" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_says_blocks_and_components() {
        let counts = Counts {
            added: 12,
            removed: 3,
            components: 2,
        };
        assert_eq!(summary(counts), "+12 −3 blocks, ~2 components");
        let taken = Counts {
            added: 0,
            removed: 1,
            components: 0,
        };
        assert_eq!(summary(taken), "−1 block");
        let one = Counts {
            added: 0,
            removed: 0,
            components: 1,
        };
        assert_eq!(summary(one), "~1 component");
        assert_eq!(summary(Counts::default()), "No blocks changed");
    }

    #[test]
    fn only_what_changed_is_measured() {
        let before = Measures {
            verdict: "wrong".to_owned(),
            latency: Some(5),
            throughput: None,
            size: [3, 2, 8],
            pitch: Some(2),
        };
        let after = Measures {
            verdict: "passes".to_owned(),
            latency: Some(4),
            ..before.clone()
        };
        assert_eq!(
            measured(Some(&before), Some(&after)).unwrap(),
            ["Test gets cases wrong → passes", "Latency 5t → 4t"]
        );
        assert_eq!(measured(None, Some(&after)), None);
    }
}
