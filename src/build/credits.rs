//! Who a build is credited to.

/// The people a build is credited to, by their in-game names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credits {
    /// Who built it, in the order given; empty when unknown.
    pub authors: Vec<String>,
    /// Who helped without building it, in the order given.
    pub contributors: Vec<String>,
}

impl Credits {
    /// Everyone credited, authors first.
    pub fn everyone(&self) -> impl Iterator<Item = &String> {
        self.authors.iter().chain(&self.contributors)
    }
}
