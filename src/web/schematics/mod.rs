//! A stored build's routes: the listing, its page, its file and thumbnail,
//! editing its details, and deleting it. A download takes an account, and is
//! counted and logged, unless it is the 3D viewer's `?preview=yes` fetch,
//! which anyone may make, or a HEAD request.

mod detail;
mod download;
mod edit;
mod list;

pub use detail::{detail, favorite, thumbnail};
pub use download::{Colours, download};
pub(super) use download::{Download, export};
pub(super) use edit::{EditView, editable, show_edit};
pub use edit::{check_progress, checked, delete, edit, save};
pub use list::{home, list};
