//! Stored builds, loaded together with everything that hangs off them, and
//! the writes that change them. What a build is lives in [`crate::build`];
//! its files live in its versions, see `models::version`.

mod load;
mod write;

// Until the queries move to a store of their own, the build's types are
// still reached through here, as they were.
pub use crate::build::{
    Build, Credits, Encoding, Faces, Logic, Operands, Orientation, PITCHES,
    Specs, StackDirection, Status, Widened, classes, file_stem,
};
pub use load::{
    all, all_people, get, get_many, get_many_with_deleted, ids_with_prefix,
    valid_player_name, visible,
};
pub use write::{
    DownloadKind, New, Restorable, count_download, delete, favorite_count,
    favorited, favorites, insert, log_download, purge, restore, set_details,
    set_favorite, set_flags, set_import_details, set_public, set_specs,
    set_stacking, set_widened, undelete,
};
