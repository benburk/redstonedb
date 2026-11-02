//! A stored build's routes: the listing, its page, its file and thumbnail,
//! editing its details, and deleting it. A download is counted unless it is
//! the 3D viewer's `?preview=yes` fetch or a HEAD request.

mod detail;
mod download;
mod edit;
mod list;

pub use detail::{detail, favorite, thumbnail};
pub use download::{Colours, download};
pub(super) use download::{Download, export};
pub use edit::{delete, edit, save};
pub(super) use edit::{editable, show_edit};
pub use list::{home, list};
