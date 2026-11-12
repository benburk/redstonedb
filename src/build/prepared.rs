//! A file made ready to store, and what is read from it rather than
//! typed.

use crate::{
    schem::{self, Blocks, Size, turn::Turn},
    works_from::{self, WorksFrom},
};
use anyhow::Result;

/// A file made ready to store.
pub struct Prepared {
    /// The file as it is worked on, carrying its roles, if any; see
    /// [`Prepared::stored`] for the file as it is kept.
    pub data: Vec<u8>,
    pub blocks: Blocks,
    /// Whether the build follows the conventions, so is measured without
    /// its interface layers.
    pub conventional: bool,
    /// Whether the file keeps where its uploader's `//paste` put it.
    pub keeps_paste: bool,
    pub size: Size,
    /// The hash of the file as it is kept, without its roles.
    pub sha256: String,
    /// What is read from it, such as its torches.
    pub facts: FileFacts,
    /// The cauldrons and cakes swapped for barrels, by kind, with how many.
    pub swapped: Vec<(String, usize)>,
    /// How it was turned and mirrored to lie the standard way.
    pub turn: Turn,
}

impl Prepared {
    /// The file as it is kept: without the roles it carries, which are
    /// kept beside it; see [`Prepared::roles`].
    pub fn stored(&self) -> Result<Vec<u8>> {
        schem::lines::without(&self.data)
    }

    /// The roles its file carries, as they are kept with its version, if
    /// any.
    pub fn roles(&self) -> Option<&schem::lines::Roles> {
        self.blocks.roles.as_ref()
    }
}

/// What is read from a build's file rather than typed: its torches and the
/// oldest release it works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub torches: i64,
    pub works_from: WorksFrom,
}

impl FileFacts {
    pub fn of(blocks: &Blocks) -> Self {
        FileFacts {
            torches: torches(blocks),
            works_from: works_from::of(blocks),
        }
    }
}

/// How many redstone torches `blocks` has, standing or on a wall.
fn torches(blocks: &Blocks) -> i64 {
    let count = blocks
        .iter()
        .filter(|(_, state)| {
            matches!(
                schem::id(state),
                "minecraft:redstone_torch" | "minecraft:redstone_wall_torch"
            )
        })
        .count();
    i64::try_from(count).unwrap_or(i64::MAX)
}
