//! A stored build: what it is, what was measured of it, who made it, and
//! how it stands. Its file lives in its versions; see `models::version`.
//! Loading and storing builds is `models::schematic`.

mod credits;
mod entry;
mod file;
mod name;
mod specs;
mod stacking;
mod status;
mod tags;
#[cfg(test)]
mod tests;

pub use credits::Credits;
pub use entry::Entry;
pub use file::File;
pub use name::file_stem;
pub use specs::Specs;
pub use stacking::Widened;
pub use status::{Operands, Status};

pub use crate::{
    category::Logic,
    interface::{Encoding, Orientation},
    stack::{Faces, PITCHES, StackDirection, classes},
};
use crate::{
    category::{Design, Flag, Kind, Measure},
    models::tag::Tag,
};

/// A stored build's details. Its file stays in the database until
/// [`crate::models::version::current_file`] fetches it.
#[derive(Debug, Clone)]
pub struct Build {
    pub id: String,
    pub category: String,
    pub specs: Specs,
    pub file: File,
    pub entry: Entry,
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
    /// Its copies stacked into wider builds, at each standard width they
    /// reach; see `Widened`.
    pub widened: Vec<Widened>,
    /// For a leaderboard's entry of copies of a build stacked to a wider
    /// width, the build's own width; see `Build::widened_to`.
    pub stacked_from: Option<i64>,
    /// The flags it has, such as an adder's carry in, in their category's
    /// order.
    pub flags: Vec<Flag>,
    /// Free text about the build, such as how to use it.
    pub description: Option<String>,
    pub credits: Credits,
    /// When it was built, a date such as `2024-02-02`.
    pub built_on: Option<String>,
    /// Why a build of a category the site recognizes builds of is not
    /// recognized as one, as `details::unrecognized` says; `None` for one
    /// that is, or any other build. Such a build is kept as built.
    pub unverified: Option<String>,
    /// How it stands against its category's test, from its current file's
    /// stored result when it has one, so a reason stored before the test
    /// last ran says nothing against it; see [`Status::of`].
    pub status: Status,
    /// How its current file's operands' bits lie, as its kept roles place
    /// them.
    pub operands: Operands,
    /// Whether a bus of its current file's, as its kept roles place it,
    /// isn't laid as the conventions lay one; see `Kept::unaligned`. Such
    /// a build is recognized all the same, but ranks on no board.
    pub unaligned: bool,
    /// Sorted by kind in glossary order (restriction, other),
    /// then name, after `failing` if it is not recognized as its
    /// category or simulating its file got some cases wrong, see
    /// `models::simulation`; `torchless` when its file has
    /// no torches, and `alpha` when it works from Alpha.
    pub tags: Vec<Tag>,
}

impl Build {
    /// Whether it works as far as anyone can tell: passing its category's
    /// test, or of a category with none. Only such a build lists unless
    /// the rest are asked for.
    pub fn verified(&self) -> bool {
        matches!(self.status, Status::Passing | Status::NoTest)
            && !self.tags.iter().any(Tag::is_unverified)
    }

    /// Whether it ranks on its category's boards: working, kept to the
    /// conventions, and with nothing stored against it; see
    /// [`Build::unranked`].
    pub fn ranks(&self) -> bool {
        self.verified() && self.unranked().is_none()
    }

    /// Why a working build ranks on no board, though it lists: its buses
    /// not laid as the conventions lay them, it kept as built, or a reason
    /// stored against it, such as a convention it bends; `None` when it
    /// ranks, or doesn't work.
    pub fn unranked(&self) -> Option<String> {
        if !self.verified() {
            None
        } else if self.unaligned {
            Some("unaligned buses".to_owned())
        } else if !self.file.conventional {
            Some("kept as built".to_owned())
        } else {
            self.unverified.clone()
        }
    }

    /// Whether it is untested, its test unable to run on it, as its inputs,
    /// outputs, or parts don't fit its category's, rather than failing.
    pub fn untested(&self) -> bool {
        self.status == Status::NotTested
    }

    /// Its category, when that is a standard one; `None` for one off the
    /// list.
    pub fn kind(&self) -> Option<Kind> {
        Kind::named(&self.category)
    }

    /// How its copies stacked share its buses, as its category and the way
    /// its bits run say.
    pub fn buses(&self) -> crate::stack::Buses {
        self.buses_running(self.specs.orientation)
    }

    /// [`Build::buses`], were its bits to run `orientation`.
    pub fn buses_running(
        &self,
        orientation: Orientation,
    ) -> crate::stack::Buses {
        let copies = crate::category::Behaviour::of(self.kind()).copies;
        crate::stack::Buses::of(copies, orientation)
    }

    /// The numbers its category's builds are named and ranked by, in
    /// order; none for a category off the list.
    pub fn measures(&self) -> &'static [Measure] {
        self.kind().map_or(&[], |kind| kind.facts().measures)
    }

    /// Whether it holds state: as it says itself, else as its category's
    /// builds usually do; `None` where neither says.
    pub fn logic(&self) -> Option<Logic> {
        self.specs
            .logic
            .or_else(|| self.kind().and_then(|kind| kind.facts().logic))
    }

    /// Its design as its category lists it, with its short name and label;
    /// `None` for no design, or one its category doesn't list.
    pub fn listed_design(&self) -> Option<&'static Design> {
        Design::of(&self.category, self.specs.design.as_deref()?)
    }

    /// Whether `evaluate` works out its every measured detail: a
    /// conventional build of a category it does so for, recognized as one.
    pub fn evaluated(&self) -> bool {
        self.file.conventional
            && self.unverified.is_none()
            && crate::category::behaviour(&self.category).evaluated
    }

    /// Whether it is a horizontal build whose operands' bits alternate
    /// along one row, carrying `interleaved_inputs`, as those of one whose
    /// copies tile with A and B split do not.
    pub fn interleaved_inputs(&self) -> bool {
        self.operands == Operands::Interleaved
            && self.specs.orientation == Orientation::Horizontal
    }

    /// Whether its category is named and ranked by `measure`.
    pub fn has(&self, measure: Measure) -> bool {
        self.measures().contains(&measure)
    }

    /// `yes`, `no`, or `unknown`.
    pub fn synchronized_label(&self) -> &str {
        match self.specs.synchronized {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        }
    }

    /// Whether a new input goes in before the last one's result comes out:
    /// a stateless, synchronized build whose throughput is below its
    /// latency.
    pub fn pipelined(&self) -> bool {
        self.logic() == Some(Logic::Stateless)
            && self.specs.synchronized == Some(true)
            && matches!((self.specs.throughput, self.specs.latency), (Some(t), Some(l)) if t < l)
    }

    /// Whether listings show it to `viewer`: a public build to anyone, and
    /// a private one to its uploader and admins. Anyone with its link may
    /// open it either way.
    pub fn listed_to(
        &self,
        viewer: Option<&crate::models::user::User>,
    ) -> bool {
        self.entry.public
            || viewer.is_some_and(|user| user.is_admin() || user.uploaded(self))
    }
}
