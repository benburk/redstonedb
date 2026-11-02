//! Stored builds: their details, loaded together with everything that
//! hangs off them, and the writes that change them. Their files live in
//! their versions; see `models::version`.

mod load;
mod stacking;
mod write;

use super::{
    catalogue::{self, Flag, Measure},
    tag::{self, Tag},
};

pub use load::{
    all, all_people, get, get_many, get_visible, ids_with_prefix,
    valid_player_name, visible,
};
pub use stacking::{Faces, StackDirection, Widened};
pub use write::{
    Measured, New, count_download, delete, favorited, favorites, insert,
    set_details, set_favorite, set_flags, set_import_details, set_measured,
    set_public, set_stacking, set_widened,
};

named_values! {
    /// The way a build's bits run.
    Orientation {
        Horizontal = "horz", "Horizontal";
        Vertical = "vert", "Vertical";
        Diagonal = "diag", "Diagonal";
    }
}

/// A build's bits run across unless it says otherwise.
impl Default for Orientation {
    fn default() -> Self {
        Orientation::Horizontal
    }
}

named_values! {
    /// How a build's numbers are written on its lines.
    Encoding {
        /// A bit per line, on levers.
        Binary = "bin", "Binary";
        /// A digit per line, as a signal strength.
        Hex = "hex", "Hex";
    }
}

named_values! {
    /// Whether a build remembers anything.
    Logic {
        /// Once its inputs stop changing, its outputs depend only on them.
        Stateless = "stateless", "Stateless";
        /// It remembers.
        Stateful = "stateful", "Stateful";
    }
}
/// A stored schematic's metadata. Its file stays in the database until
/// [`super::version::current_file`] fetches it.
#[derive(Debug, Clone)]
pub struct Schematic {
    pub id: String,
    pub category: String,
    /// The standard category's label, such as `RAM`; `None` for a category
    /// off the list.
    pub category_label: Option<String>,
    pub width: Option<i64>,
    pub orientation: Orientation,
    /// `None` for a category off the list, such as a piston door.
    pub encoding: Option<Encoding>,
    pub latency: Option<i64>,
    /// Whether every output bit settles on the same tick.
    pub synchronized: Option<bool>,
    /// The build's own logic, else its category's usual logic.
    pub logic: Option<Logic>,
    /// The build's own logic, where it says so; `None` follows its
    /// category's.
    pub own_logic: Option<Logic>,
    /// The fewest ticks from one input to the next that still gives right
    /// outputs; for a serial build, from one bit to the next.
    pub throughput: Option<i64>,
    /// For a threshold, the fewest inputs on that turn it on.
    pub at_least: Option<i64>,
    /// For a monostable, the ticks its pulse lasts.
    pub pulse: Option<i64>,
    /// The numbers its category's builds are named and ranked by, in
    /// order; none for a category off the list.
    pub measures: Vec<Measure>,
    /// Other words its category is searched by, such as `connect4`.
    pub aliases: Vec<String>,
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
    /// width, the build's own width; see `Schematic::widened_to`.
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
    /// The design within its category, such as `carry_lookahead`, or
    /// `other`.
    pub design: String,
    /// The design's usual short name, such as `CLA`.
    pub design_abbreviation: Option<String>,
    /// How people read the design when that is not just its name's words,
    /// such as `LFSR`.
    pub design_given_label: Option<String>,
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
    pub builders: Vec<String>,
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
    /// Whether anyone may see it; a private build only its uploader and
    /// admins see.
    pub public: bool,
    /// Why a build of a category the site recognizes builds of is not
    /// recognized as one, as `details::unrecognized` says; `None` for one
    /// that is, or any other build. Such a build is kept as built.
    pub unverified: Option<String>,
    /// Sorted by kind in glossary order (restriction, other),
    /// then name, after `fails_verification` if it is not recognized as its
    /// category or simulating its file got some cases wrong, see
    /// `models::simulation`; `torchless` when its file has
    /// no torches, and `alpha` when it works from Alpha.
    pub tags: Vec<Tag>,
}

/// The farthest apart copies stack for a tag to say how far, as
/// `4-wide_stackable`: a bus's spacing, not a whole build's length.
const MOST_PITCH: i64 = 4;

/// `name` as a file's name, less `.schem`: an underscore for each run of
/// anything but letters and digits, none at either end, as Schemati takes
/// no other characters. Empty when `name` has no letter or digit.
pub fn file_stem(name: &str) -> String {
    let name = name.strip_suffix(".schem").unwrap_or(name);
    let mut stem = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            stem.push(c);
        } else if !stem.is_empty() && !stem.ends_with('_') {
            stem.push('_');
        }
    }
    stem.trim_end_matches('_').to_owned()
}

impl Schematic {
    /// Whether it is verified as its category: recognized as one, if its
    /// category is one the site recognizes, and passing its test, if it
    /// has one. Only a verified build ranks, and lists unless the unverified
    /// are asked for.
    pub fn verified(&self) -> bool {
        self.unverified.is_none() && !self.tags.iter().any(Tag::is_unverified)
    }

    /// Whether it is unverified only because its inputs, outputs, or
    /// parts don't fit its category's, rather than for wrong outputs.
    pub fn non_standard(&self) -> bool {
        self.tags.iter().any(|t| t.kind == tag::Kind::NonStandard)
    }

    /// Whether `evaluate` works out its every measured detail: a
    /// conventional build of a category it does so for, recognized as one.
    pub fn evaluated(&self) -> bool {
        self.conventional
            && self.unverified.is_none()
            && crate::recognize::category::behaviour(&self.category).evaluated
    }

    fn tags_in(&self, kind: tag::Kind) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(move |tag| tag.kind == kind)
    }

    /// Its tags with its design, orientation, and flags among them, as its page
    /// shows them and a list is narrowed by: `fails_verification` first, then
    /// its design, its orientation, its flags in its category's order,
    /// `stackable` when its copies stack, `synced` when its outputs settle
    /// together or `unsynced` when they don't, and its other tags. One the site
    /// works out from a conventional build is found, as its category's test or
    /// file sets it; one a person gives is of kind other.
    pub fn every_tag(&self) -> Vec<Tag> {
        let behaviour = crate::recognize::category::behaviour(&self.category);
        let found = |kind, by_site: bool| {
            if self.conventional && self.unverified.is_none() && by_site {
                kind
            } else {
                tag::Kind::Other
            }
        };
        let design = (self.design != "other").then(|| Tag {
            name: self.design.clone(),
            token: self.design_abbreviation.clone(),
            kind: found(tag::Kind::Design, behaviour.evaluated),
        });
        let orientation = Tag {
            name: self.orientation.label().to_lowercase(),
            token: Some(self.orientation.name().to_owned()),
            kind: found(
                tag::Kind::Orientation,
                behaviour.evaluated
                    || behaviour.reads
                        != crate::recognize::category::Reads::Nothing,
            ),
        };
        // An inverted carry in says only how one line is read, as the
        // inputs table shows.
        let flags = self
            .flags
            .iter()
            .filter(|flag| flag.name != Flag::INVERTED_CARRY_IN)
            .map(|flag| Tag {
                name: flag.name.clone(),
                token: None,
                kind: found(
                    tag::Kind::Feature,
                    behaviour.evaluated || behaviour.flags_from_file,
                ),
            });
        let stackable = (!self.stacking.is_empty()).then(|| Tag {
            name: tag::STACKABLE.to_owned(),
            token: None,
            kind: found(
                tag::Kind::Feature,
                behaviour.stacking
                    == crate::recognize::category::Stacking::Inferred,
            ),
        });
        // How far apart copies stack, where that is a few blocks, the
        // spacing of a bus, as `2-high_stackable`.
        let pitches = self.stacking.iter().filter_map(|way| {
            let pitch = match (way.west, way.up, way.north) {
                (n, 0, 0) if n > 0 => format!("{n}-wide"),
                (0, n, 0) if n > 0 => format!("{n}-high"),
                (0, 0, n) if n > 0 => format!("{n}-deep"),
                _ => return None,
            };
            (way.west + way.up + way.north <= MOST_PITCH).then(|| Tag {
                name: tag::pitch_stackable(&pitch),
                token: None,
                kind: found(
                    tag::Kind::Feature,
                    behaviour.stacking
                        == crate::recognize::category::Stacking::Inferred,
                ),
            })
        });
        // Whether its outputs settle together, once measured, either way.
        let synced = self.synchronized.map(|synced| Tag {
            name: if synced { tag::SYNCED } else { tag::UNSYNCED }.to_owned(),
            token: None,
            kind: found(tag::Kind::Feature, behaviour.evaluated),
        });
        let (failing, rest): (Vec<&Tag>, Vec<&Tag>) =
            self.tags.iter().partition(|tag| tag.is_unverified());
        failing
            .into_iter()
            .cloned()
            .chain(design)
            .chain([orientation])
            .chain(flags)
            .chain(stackable)
            .chain(pitches)
            .chain(synced)
            .chain(rest.into_iter().cloned())
            .collect()
    }

    /// The width in words, such as `8 bits` or `1 bit`.
    pub fn width_label(&self) -> Option<String> {
        self.width
            .map(|w| format!("{w} {}", if w == 1 { "bit" } else { "bits" }))
    }

    /// The category as people read it, such as `RAM` or `Line Drawer`.
    pub fn category_label(&self) -> String {
        self.category_label
            .clone()
            .unwrap_or_else(|| catalogue::words(&self.category))
    }

    /// What the build is: its given name, or else its category.
    fn what(&self) -> String {
        // A subtractor is an adder whose B is inverted, called by what it
        // does.
        let subtracts = self.category == "adder"
            && self.flags.iter().any(|flag| flag.name == "subtract");
        if subtracts {
            return "Subtractor".to_owned();
        }
        self.given_name
            .clone()
            .unwrap_or_else(|| self.category_label())
    }

    /// Its category's measures that lead a name, such as `8-bit` and
    /// `32-byte`, or `2-tick`; an unknown one is left out. A multiplier's
    /// width is its shape, as `8x8=16`, keeping the upper half of its
    /// product, or `8x8=8`, and `8x8+8=16` for a fused multiply-add.
    fn measures(&self) -> impl Iterator<Item = String> {
        let shape = self.product_shape();
        self.measures.iter().filter_map(move |m| match (m, &shape) {
            (Measure::Width, Some(shape)) => Some(shape.clone()),
            _ => m.of(self).map(|n| m.lead(n)),
        })
    }

    /// A multiplier's operands and product, as `8x8=16`; `None` for any
    /// other build, or one of unknown width.
    fn product_shape(&self) -> Option<String> {
        let width = self.width.filter(|_| self.category == "multiplier")?;
        let flagged = |name: &str| self.flags.iter().any(|f| f.name == name);
        let product = if flagged("upper_product") {
            2 * width
        } else {
            width
        };
        let addend = if flagged("fma") {
            format!("+{width}")
        } else {
            String::new()
        };
        Some(format!("{width}x{width}{addend}={product}"))
    }

    /// Whether its category is named and ranked by `measure`.
    pub fn has(&self, measure: Measure) -> bool {
        self.measures.contains(&measure)
    }

    /// The width, design, and category, such as `8-bit Carry Lookahead
    /// Adder`, for views that show the tags beside it. A build with a given
    /// name, such as `8-bit Bus Crossing`, uses it for the category.
    pub fn name(&self) -> String {
        self.measures()
            .chain(self.kind())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The design and what the build is, as a name ends, such as `Carry
    /// Lookahead Adder`.
    fn kind(&self) -> impl Iterator<Item = String> {
        self.design_label().into_iter().chain([self.what()])
    }

    /// A readable name from the most identifying facts, such as
    /// `8-bit alpha Carry Lookahead Adder`.
    pub fn title(&self) -> String {
        self.measures()
            .chain(self.tags_in(tag::Kind::Restriction).map(Tag::label))
            .chain(self.tags_in(tag::Kind::Other).map(Tag::label))
            .chain(self.kind())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The design in words, such as `Carry Lookahead`; `None` for no
    /// design or `other`.
    pub fn design_label(&self) -> Option<String> {
        (self.design != "other").then(|| {
            self.design_given_label
                .clone()
                .unwrap_or_else(|| catalogue::words(&self.design))
        })
    }

    /// The download filename: the build's name with an underscore for each
    /// run of anything but letters and digits, such as
    /// `8_bit_Carry_Lookahead_Adder.schem`, as Schemati, the Open Redstone
    /// Engineers server's schematic store, takes no other characters.
    pub fn filename(&self) -> String {
        format!("{}.schem", file_stem(&self.name()))
    }

    /// The blocks a build takes, as it is compared: for a stackable build,
    /// what each copy adds, since its copies may overlap; else its whole
    /// box.
    pub fn volume(&self) -> i64 {
        let (x, y, z) = self.per_copy().unwrap_or(self.size);
        x * y * z
    }

    /// The number of blocks inside the build's bounding box.
    pub fn box_volume(&self) -> i64 {
        let (x, y, z) = self.size;
        x * y * z
    }

    /// The box a stack grows by with each copy: the build's size, but its
    /// step along each way it stacks, so the parts copies overlap on count
    /// once. `None` for a build that does not stack, or stacks diagonally,
    /// whose stack does not grow by a box.
    pub fn per_copy(&self) -> Option<(i64, i64, i64)> {
        if self.stacking.is_empty() {
            return None;
        }
        let (mut x, mut y, mut z) = self.size;
        for d in &self.stacking {
            match (d.west, d.up, d.north) {
                (west, 0, 0) => x = west,
                (0, up, 0) => y = up,
                (0, 0, north) => z = north,
                _ => return None,
            }
        }
        Some((x, y, z))
    }

    /// This build stacked into a box of inner `size`, with `copies` along
    /// each direction. Copies that add bits widen it, and every copy of a
    /// memory holds its bytes. A stack's latency and timing are not its
    /// slice's, so they are unknown, as is the throughput of any but a serial
    /// build.
    #[must_use]
    pub fn stacked(
        &self,
        copies: &[(StackDirection, usize)],
        size: [i32; 3],
    ) -> Schematic {
        let count = |n: &usize| i64::try_from(*n).unwrap_or(i64::MAX);
        let factor: i64 = copies
            .iter()
            .filter(|(direction, _)| direction.adds_bits(self.orientation))
            .map(|(_, n)| count(n))
            .product();
        let all: i64 = copies.iter().map(|(_, n)| count(n)).product();
        Schematic {
            width: self.width.map(|w| w * factor),
            bytes: self.bytes.map(|b| b.saturating_mul(all)),
            latency: None,
            synchronized: None,
            throughput: self
                .throughput
                .filter(|_| self.has(Measure::Throughput)),
            size: (size[0].into(), size[1].into(), size[2].into()),
            ..self.clone()
        }
    }

    /// Copies of this build stacked into the wider build `w`, as a
    /// leaderboard ranks it among builds of `w`'s width: with the timing
    /// measured so, and the box the copies take, the step its bits run
    /// along counting once a copy. `None` for a build with no way of
    /// stacking that adds bits.
    #[must_use]
    pub fn widened_to(&self, w: &Widened) -> Option<Schematic> {
        let along = *self
            .stacking
            .iter()
            .find(|d| d.adds_bits(self.orientation))?;
        let more = w.copies - 1;
        let (x, y, z) = self.size;
        let size = (
            x + along.west * more,
            y + along.up * more,
            z + along.north * more,
        );
        let stacking = self
            .stacking
            .iter()
            .map(|&d| {
                if d == along {
                    StackDirection {
                        west: d.west * w.copies,
                        up: d.up * w.copies,
                        north: d.north * w.copies,
                        max_copies: None,
                        faces: d.faces,
                    }
                } else {
                    d
                }
            })
            .collect();
        Some(Schematic {
            width: Some(w.width),
            latency: w.latency,
            synchronized: w.synchronized,
            throughput: w.throughput,
            size,
            stacking,
            widened: Vec::new(),
            stacked_from: self.width,
            ..self.clone()
        })
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
        self.logic == Some(Logic::Stateless)
            && self.synchronized == Some(true)
            && matches!((self.throughput, self.latency), (Some(t), Some(l)) if t < l)
    }
}

impl Schematic {
    /// Whether `viewer` may see it: anyone a public build, and its uploader
    /// and admins a private one.
    pub fn visible_to(
        &self,
        viewer: Option<&crate::models::user::User>,
    ) -> bool {
        self.public
            || viewer.is_some_and(|user| user.is_admin() || user.uploaded(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cla() -> Schematic {
        Schematic {
            id: String::new(),
            category: "adder".to_owned(),
            category_label: Some("Adder".to_owned()),
            width: Some(8),
            orientation: Orientation::Horizontal,
            encoding: Some(Encoding::Binary),
            latency: Some(8),
            synchronized: Some(false),
            logic: Some(Logic::Stateless),
            own_logic: None,
            throughput: None,
            at_least: None,
            pulse: None,
            measures: vec![Measure::Width],
            aliases: Vec::new(),
            torches: None,
            works_from: None,
            works_from_by: None,
            bytes: None,
            size: (19, 11, 18),
            stacking: Vec::new(),
            widened: Vec::new(),
            stacked_from: None,
            builders: vec!["embizone".to_owned()],
            contributors: Vec::new(),
            built_on: Some("2014-09-06".to_owned()),
            imported_by: None,
            conventional: true,
            keeps_paste: false,
            design: "carry_lookahead".to_owned(),
            design_abbreviation: None,
            design_given_label: None,
            flags: Vec::new(),
            description: None,
            given_name: None,
            uploader: None,
            uploader_name: None,
            sha256: String::new(),
            added_at: String::new(),
            edited_at: None,
            downloads: 0,
            public: true,
            unverified: None,
            tags: vec![Tag {
                name: "alpha".to_owned(),
                token: None,
                kind: tag::Kind::Restriction,
            }],
        }
    }

    /// Its design, orientation, and flags are among its tags, after failing and
    /// before the rest, coloured as found when the site works them out, and as
    /// given for a build kept as built.
    #[test]
    fn a_build_s_design_orientation_and_flags_are_among_its_tags() {
        let mut s = cla();
        s.flags = vec![Flag {
            category: "adder".to_owned(),
            name: "carry_in".to_owned(),
            label: "Carry in".to_owned(),
        }];
        s.tags.insert(
            0,
            Tag::derived(tag::FAILS_VERIFICATION, tag::Kind::Failing),
        );
        let tags = s.every_tag();
        let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "fails_verification",
                "carry_lookahead",
                "horizontal",
                "carry_in",
                // Its outputs settle apart, as measured.
                "unsynced",
                "alpha"
            ]
        );
        let classes: Vec<&str> = tags.iter().map(Tag::class).collect();
        assert_eq!(
            classes,
            [
                "badge-failing",
                "badge-found",
                "badge-found",
                "badge-found",
                "badge-found",
                "badge-found"
            ]
        );
        s.conventional = false;
        let given: Vec<&str> =
            s.every_tag()[1..4].iter().map(Tag::class).collect();
        assert_eq!(given, ["badge-other", "badge-other", "badge-other"]);
    }

    #[test]
    fn title_reads_width_restrictions_design_then_category() {
        assert_eq!(cla().title(), "8-bit alpha Carry Lookahead Adder");
    }

    #[test]
    fn filename_is_the_name() {
        assert_eq!(cla().filename(), "8_bit_Carry_Lookahead_Adder.schem");
    }

    #[test]
    fn a_name_given_for_a_file_keeps_only_what_schemati_takes() {
        assert_eq!(file_stem("my adder (v2).schem"), "my_adder_v2");
        assert_eq!(file_stem("  __CCA-8__ "), "CCA_8");
        assert_eq!(file_stem("ünïcode"), "n_code");
        assert_eq!(file_stem("../"), "");
    }

    #[test]
    fn a_value_parses_from_its_name_or_label() {
        assert_eq!("horz".parse(), Ok(Orientation::Horizontal));
        assert_eq!(" Vertical ".parse(), Ok(Orientation::Vertical));
        assert_eq!("HEX".parse(), Ok(Encoding::Hex));
        assert_eq!("stateful".parse(), Ok(Logic::Stateful));
        assert!("sideways".parse::<Orientation>().is_err());
    }

    #[test]
    fn only_a_mod_a_confirmed_builder_or_the_uploader_may_edit() {
        use crate::models::user::{Role, User};
        let user = |name: Option<&str>, verified, role| User {
            id: uuid::Uuid::nil(),
            discord_id: String::new(),
            username: "someone".to_owned(),
            display_name: None,
            avatar: None,
            role,
            minecraft_name: name.map(str::to_owned),
            minecraft_verified: verified,
            minecraft_uuid: None,
        };
        let cla = cla();
        assert!(user(Some("EmbiZone"), true, Role::User).may_edit(&cla));
        assert!(!user(Some("embizone"), false, Role::User).may_edit(&cla));
        assert!(!user(Some("alex_you"), true, Role::User).may_edit(&cla));
        assert!(!user(None, false, Role::User).may_edit(&cla));
        assert!(user(None, false, Role::Mod).may_edit(&cla));
        let mut uploaded = cla;
        uploaded.uploader = Some("someone".to_owned());
        assert!(user(None, false, Role::User).may_edit(&uploaded));
    }

    #[test]
    fn an_adder_that_subtracts_is_named_a_subtractor() {
        let mut subtractor = cla();
        subtractor.flags = vec![Flag {
            category: "adder".to_owned(),
            name: "subtract".to_owned(),
            label: String::new(),
        }];
        assert_eq!(subtractor.name(), "8-bit Carry Lookahead Subtractor");
        assert_eq!(cla().name(), "8-bit Carry Lookahead Adder");
    }

    /// Each gate is a category of its own and is called by its label, as an
    /// XNOR gate was once an XOR gate flagged `xnor`, and a flag of that
    /// left on a build calls it nothing else.
    #[test]
    fn a_gate_is_named_by_its_category() {
        let gate = |category: &str, label: &str, flag: Option<&str>| {
            let mut gate = cla();
            gate.category = category.to_owned();
            gate.category_label = Some(label.to_owned());
            gate.flags = flag
                .map(|name| Flag {
                    category: category.to_owned(),
                    name: name.to_owned(),
                    label: String::new(),
                })
                .into_iter()
                .collect();
            gate.what()
        };
        assert_eq!(gate("xor_gate", "XOR Gate", None), "XOR Gate");
        assert_eq!(gate("xor_gate", "XOR Gate", Some("xnor")), "XOR Gate");
        assert_eq!(gate("xnor_gate", "XNOR Gate", None), "XNOR Gate");
        assert_eq!(gate("nand_gate", "NAND Gate", None), "NAND Gate");
    }

    #[test]
    fn a_multiplier_is_named_by_its_operands_and_product() {
        let flag = |name: &str| Flag {
            category: "multiplier".to_owned(),
            name: name.to_owned(),
            label: String::new(),
        };
        let mut multiplier = Schematic {
            category: "multiplier".to_owned(),
            category_label: Some("Multiplier".to_owned()),
            design: "other".to_owned(),
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(multiplier.name(), "8x8=8 Multiplier");
        multiplier.flags = vec![flag("upper_product")];
        assert_eq!(multiplier.name(), "8x8=16 Multiplier");
        assert_eq!(multiplier.filename(), "8x8_16_Multiplier.schem");
        multiplier.flags.push(flag("fma"));
        assert_eq!(multiplier.name(), "8x8+8=16 Multiplier");
    }

    #[test]
    fn a_memory_names_its_capacity_and_stacks_multiply_it() {
        let ram = Schematic {
            category: "ram".to_owned(),
            category_label: Some("RAM".to_owned()),
            design: "other".to_owned(),
            bytes: Some(32),
            measures: vec![Measure::Width, Measure::Bytes],
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(ram.name(), "8-bit 32-byte RAM");
        let north = StackDirection {
            west: 0,
            up: 0,
            north: 3,
            max_copies: None,
            faces: None,
        };
        let stacked = ram.stacked(&[(north, 4)], [19, 11, 72]);
        assert_eq!(stacked.bytes, Some(128));
        assert_eq!(stacked.width, Some(8), "copies north add words, not bits");
    }

    #[test]
    fn a_category_label_keeps_its_symbols_out_of_the_filename() {
        let s = Schematic {
            category: "inc_dec".to_owned(),
            category_label: Some("Inc/Dec".to_owned()),
            design: "other".to_owned(),
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.name(), "8-bit Inc/Dec");
        assert_eq!(s.filename(), "8_bit_Inc_Dec.schem");
    }

    #[test]
    fn a_design_keeps_its_label() {
        let s = Schematic {
            category: "rng".to_owned(),
            category_label: Some("RNG".to_owned()),
            design: "lfsr".to_owned(),
            design_given_label: Some("LFSR".to_owned()),
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.name(), "8-bit LFSR RNG");
    }

    #[test]
    fn a_threshold_names_its_k_after_its_width() {
        let s = Schematic {
            category: "threshold".to_owned(),
            category_label: Some("Threshold".to_owned()),
            design: "other".to_owned(),
            at_least: Some(3),
            measures: vec![Measure::Width, Measure::AtLeast],
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.name(), "8-bit At Least 3 Threshold");
        assert_eq!(s.filename(), "8_bit_At_Least_3_Threshold.schem");
    }

    #[test]
    fn a_serial_build_leads_with_its_throughput_and_others_do_not() {
        let serial = Schematic {
            category: "serial_adder".to_owned(),
            category_label: Some("Serial Adder".to_owned()),
            design: "other".to_owned(),
            width: None,
            throughput: Some(2),
            measures: vec![Measure::Throughput],
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(serial.name(), "2-tick Serial Adder");
        let adder = Schematic {
            throughput: Some(3),
            ..cla()
        };
        assert_eq!(adder.name(), "8-bit Carry Lookahead Adder");
    }

    #[test]
    fn a_synchronized_stateless_build_faster_than_its_latency_is_pipelined() {
        let piped = Schematic {
            synchronized: Some(true),
            throughput: Some(3),
            ..cla()
        };
        assert!(piped.pipelined());
        let unsynced = Schematic {
            synchronized: Some(false),
            ..piped.clone()
        };
        assert!(!unsynced.pipelined(), "pipelining needs synced outputs");
        let stateful = Schematic {
            logic: Some(Logic::Stateful),
            ..piped.clone()
        };
        assert!(!stateful.pipelined());
        let as_slow = Schematic {
            throughput: Some(8),
            ..piped
        };
        assert!(!as_slow.pipelined(), "throughput must beat latency");
    }

    #[test]
    fn a_stack_grows_by_its_step_however_much_copies_overlap() {
        let way = |west, up, north| StackDirection {
            west,
            up,
            north,
            max_copies: None,
            faces: None,
        };
        // Copies 2 up a box 6 high overlap by 4, so each adds 2 high.
        let vertical = Schematic {
            size: (5, 6, 6),
            stacking: vec![way(0, 2, 0)],
            ..cla()
        };
        assert_eq!(vertical.per_copy(), Some((5, 2, 6)));
        let horizontal = Schematic {
            size: (7, 2, 6),
            stacking: vec![way(5, 0, 0)],
            ..cla()
        };
        assert_eq!(horizontal.per_copy(), Some((5, 2, 6)));
        let diagonal = Schematic {
            stacking: vec![way(1, 1, 0)],
            ..cla()
        };
        assert_eq!(diagonal.per_copy(), None);
        assert_eq!(cla().per_copy(), None);
        // Compared by what a copy adds, but its box is still all of it.
        assert_eq!((vertical.volume(), vertical.box_volume()), (60, 180));
        assert_eq!(cla().volume(), cla().box_volume());
    }

    #[test]
    fn a_category_keeps_its_label() {
        let s = Schematic {
            category: "ram".to_owned(),
            category_label: Some("RAM".to_owned()),
            design: "other".to_owned(),
            ..cla()
        };
        assert_eq!(s.name(), "8-bit RAM");
    }

    #[test]
    fn a_category_off_the_list_is_capitalized() {
        let s = Schematic {
            category: "maze_screen".to_owned(),
            category_label: None,
            width: None,
            design: "other".to_owned(),
            ..cla()
        };
        assert_eq!(s.name(), "Maze Screen");
    }

    #[test]
    fn unknown_parts_are_left_out() {
        let s = Schematic {
            width: None,
            design: "other".to_owned(),
            latency: None,
            synchronized: None,
            throughput: None,
            bytes: None,
            builders: Vec::new(),
            built_on: None,
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.title(), "Adder");
        assert_eq!(s.filename(), "Adder.schem");
    }
}
