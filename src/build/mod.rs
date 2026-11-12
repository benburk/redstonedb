//! A stored build: what it is, what was measured of it, who made it, and
//! how it stands. Its file lives in its versions; see `models::version`.
//! Loading and storing builds is `models::schematic`.

mod facts;
mod measured;
mod name;
mod stacking;
mod status;
mod tags;
#[cfg(test)]
mod tests;

pub use facts::{Encoding, Logic, Orientation};
pub use measured::Measured;
pub use name::file_stem;
pub use stacking::{Faces, PITCHES, StackDirection, Widened, classes};
pub use status::{Operands, Status};

use crate::{
    category::{Design, Flag, Kind, Measure},
    models::tag::Tag,
};

/// A stored build's details. Its file stays in the database until
/// [`crate::models::version::current_file`] fetches it.
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a row of facts, each yes or no on its own"
)]
pub struct Build {
    pub id: String,
    pub category: String,
    pub width: Option<i64>,
    pub orientation: Orientation,
    /// `None` for a category off the list, such as a piston door.
    pub encoding: Option<Encoding>,
    pub latency: Option<i64>,
    /// Whether every output bit settles on the same tick.
    pub synchronized: Option<bool>,
    /// The build's own logic, where it says so; `None` follows its
    /// category's, see [`Build::logic`].
    pub own_logic: Option<Logic>,
    /// The fewest ticks from one input to the next that still gives right
    /// outputs; for a serial build, from one bit to the next.
    pub throughput: Option<i64>,
    /// For a threshold, the fewest inputs on that turn it on.
    pub at_least: Option<i64>,
    /// For a monostable, the ticks its pulse lasts.
    pub pulse: Option<i64>,
    /// How many redstone torches its file has; `None` until counted.
    pub torches: Option<i64>,
    /// The oldest Minecraft release it works in, such as `1.5`; `None`
    /// until its file is read.
    pub works_from: Option<String>,
    /// The component or mechanic that sets `works_from`, such as
    /// `comparator`; `None` for a build of nothing newer than Alpha.
    pub works_from_by: Option<String>,
    /// For a memory, how many bytes it holds.
    pub bytes: Option<i64>,
    /// The build's box less any outer layer holding only its interface.
    pub size: (i64, i64, i64),
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
    /// Its copies stacked into wider builds, at each standard width they
    /// reach; see `Widened`.
    pub widened: Vec<Widened>,
    /// For a leaderboard's entry of copies of a build stacked to a wider
    /// width, the build's own width; see `Build::widened_to`.
    pub stacked_from: Option<i64>,
    /// Whether the build follows the building conventions, so the
    /// interface reader applies to it.
    pub conventional: bool,
    /// Whether its file keeps where its uploader's `//paste` put it, which
    /// a new version or a reread file keeps too.
    pub keeps_paste: bool,
    /// The name the build was given, for a category that takes one, such
    /// as `Bus Crossing` under bussing.
    pub given_name: Option<String>,
    /// The design within its category, such as `carry_lookahead`; `None`
    /// for none.
    pub design: Option<String>,
    /// The flags it has, such as an adder's carry in, in their category's
    /// order.
    pub flags: Vec<Flag>,
    /// Free text about the build, such as how to use it.
    pub description: Option<String>,
    /// The username of the account that uploaded it through the site.
    pub uploader: Option<String>,
    /// That account as people know it: its Minecraft name, else its
    /// username.
    pub uploader_name: Option<String>,
    /// In-game names, in the order given; empty when unknown.
    pub authors: Vec<String>,
    /// In-game names of people who helped without building it, in order.
    pub contributors: Vec<String>,
    pub built_on: Option<String>,
    /// The in-game name of whoever imported the build.
    pub imported_by: Option<String>,
    /// Of its current file, so a URL derived from the file can name its
    /// version, and a simulation result can be matched to it.
    pub sha256: String,
    pub added_at: String,
    /// When a person last changed its details; `None` if never.
    pub edited_at: Option<String>,
    /// How many times it has been downloaded.
    pub downloads: i64,
    /// How many people have favourited it.
    pub favorites: i64,
    /// Whether it is listed for everyone; a private build is listed only
    /// for its uploader and admins, though anyone with its link sees it.
    pub public: bool,
    /// Whether it was deleted, kept for an admin to restore; only the
    /// moderation log loads one.
    pub deleted: bool,
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
        } else if !self.conventional {
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

    /// The numbers its category's builds are named and ranked by, in
    /// order; none for a category off the list.
    pub fn measures(&self) -> &'static [Measure] {
        self.kind().map_or(&[], |kind| kind.facts().measures)
    }

    /// Whether it holds state: as it says itself, else as its category's
    /// builds usually do; `None` where neither says.
    pub fn logic(&self) -> Option<Logic> {
        self.own_logic
            .or_else(|| self.kind().and_then(|kind| kind.facts().logic))
    }

    /// Its design as its category lists it, with its short name and label;
    /// `None` for no design, or one its category doesn't list.
    pub fn listed_design(&self) -> Option<&'static Design> {
        Design::of(&self.category, self.design.as_deref()?)
    }

    /// Whether `evaluate` works out its every measured detail: a
    /// conventional build of a category it does so for, recognized as one.
    pub fn evaluated(&self) -> bool {
        self.conventional
            && self.unverified.is_none()
            && crate::category::behaviour(&self.category).evaluated
    }

    /// Whether it is a horizontal build whose operands' bits alternate
    /// along one row, carrying `interleaved_inputs`, as those of one whose
    /// copies tile with A and B split do not.
    pub fn interleaved_inputs(&self) -> bool {
        self.operands == Operands::Interleaved
            && self.orientation == Orientation::Horizontal
    }

    /// Whether its category is named and ranked by `measure`.
    pub fn has(&self, measure: Measure) -> bool {
        self.measures().contains(&measure)
    }

    /// `yes`, `no`, or `unknown`.
    pub fn synchronized_label(&self) -> &str {
        match self.synchronized {
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
            && self.synchronized == Some(true)
            && matches!((self.throughput, self.latency), (Some(t), Some(l)) if t < l)
    }

    /// Whether listings show it to `viewer`: a public build to anyone, and
    /// a private one to its uploader and admins. Anyone with its link may
    /// open it either way.
    pub fn listed_to(
        &self,
        viewer: Option<&crate::models::user::User>,
    ) -> bool {
        self.public
            || viewer.is_some_and(|user| user.is_admin() || user.uploaded(self))
    }
}
