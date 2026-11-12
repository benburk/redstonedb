//! Stored builds, loaded together with everything that hangs off them, and
//! the writes that change them. What a build is lives in [`crate::build`];
//! its files live in its versions.

mod load;
mod write;

pub use load::{
    all, all_people, get, get_many, get_many_with_deleted, ids_with_prefix,
    visible,
};
pub use write::{
    DownloadKind, New, Restorable, count_download, delete, favorite_count,
    favorited, favorites, insert, log_download, purge, restore, set_details,
    set_favorite, set_flags, set_import_details, set_public, set_specs,
    set_stacking, set_widened, undelete,
};
