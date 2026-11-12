//! A value edited in place. Inside a `data-editable` element, a pencil in
//! its `data-view` swaps the view for its `data-editor`, a form, and the
//! form's Cancel swaps them back; `static/app.js` does the swapping.

use maud::{Markup, html};

use crate::web::ui::icons::{PENCIL, icon};

/// The look of a bare icon button, such as the pencil.
pub(crate) const ICON_BUTTON: &str = "inline-flex size-8 shrink-0 items-center justify-center rounded-control text-muted hover:bg-selection hover:text-ink";

/// The pencil that opens the editor, saying `label` to a screen reader
/// and `title` on hover.
pub(crate) fn pencil(label: &str, title: &str) -> Markup {
    html! {
        button type="button" data-edit aria-label=(label) title=(title) class=(ICON_BUTTON) {
            (icon(PENCIL, "size-4"))
        }
    }
}

/// The editor's buttons: Save, and Cancel, which shows the view again.
pub(crate) fn buttons() -> Markup {
    html! {
        button type="submit" class="btn btn-primary min-h-8 px-3" { "Save" }
        button type="button" data-cancel class="btn btn-quiet min-h-8 px-3" { "Cancel" }
    }
}
