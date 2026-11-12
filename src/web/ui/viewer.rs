//! The 3D view of a file, drawn by `static/viewer.js`, and the layout of a
//! page that has one.

use maud::{Markup, PreEscaped, html};

use crate::web::assets::VIEWER_JS;

/// The layout of a build's page and of an upload's preview: `side`, its
/// facts or form, on the left, and the 3D view of the file at `file` on
/// the right, staying in sight while the side scrolls. On a phone the view
/// comes first, or last with `side_first`, as for a form to fill in.
pub(crate) fn beside_viewer(
    file: &str,
    side: &Markup,
    side_first: bool,
) -> Markup {
    beside_viewer_under(file, &html! {}, side, side_first)
}

/// As [`beside_viewer`], with `above` over the viewer, such as a choice of
/// the colours it shows.
pub(crate) fn beside_viewer_under(
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
pub(crate) const VIEWER_IMPORTS: &str = r#"{ "imports": {
  "three": "https://cdn.jsdelivr.net/npm/three@0.184.0/build/three.module.js",
  "nucleation": "https://cdn.jsdelivr.net/npm/nucleation@0.2.18/nucleation.js"
} }"#;

/// A 3D view of the schematic at `file`, an address the browser can fetch,
/// drawn by `static/viewer.js`.
pub(crate) fn viewer_3d(file: &str) -> Markup {
    viewer_canvas(file, None)
}

/// A 3D view of the two sides of a diff, `after` shown first and `before`
/// in its place when a `data-side` button asks, the camera kept.
pub(crate) fn viewer_3d_sides(after: &str, before: &str) -> Markup {
    viewer_canvas(after, Some(before))
}

/// The viewer's canvas, drawing `file`, and `before` too, hidden, if given.
pub(crate) fn viewer_canvas(file: &str, before: Option<&str>) -> Markup {
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
        script type="module" src=(VIEWER_JS.url()) {}
    }
}
