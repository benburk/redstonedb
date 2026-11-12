//! The parts dialogs share. A dialog is a `<dialog class="dialog">`, which
//! a button opens by its id.

use maud::{Markup, html};

/// A button that closes dialog `id`, saying `label`, such as Cancel.
pub(crate) fn close(id: &str, label: &str) -> Markup {
    html! {
        button type="button" commandfor=(id) command="close" class="btn btn-quiet" { (label) }
    }
}

/// The top of a dialog whose choices save as they are made: its title,
/// which names the dialog as `{id}-title`, and `way_out` beside it.
pub(crate) fn header(id: &str, title: &str, way_out: &Markup) -> Markup {
    html! {
        div class="mb-3 flex items-center justify-between gap-4" {
            h2 id={ (id) "-title" } class="font-semibold" { (title) }
            (way_out)
        }
    }
}
