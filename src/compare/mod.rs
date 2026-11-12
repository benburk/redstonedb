//! How alike two files are: block for block, as a new version is held
//! against the one before; by what changed, for a version's diff; and by
//! the redstone components that line up, for the builds a build is like.

pub mod blocks;
pub mod diff;
pub mod similarity;
