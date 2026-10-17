//! Stored builds: their details, loaded together with everything that
//! hangs off them, and the writes that change them. Their files live in
//! their versions; see `models::version`.

use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

use super::{
    catalogue::{self, Flag, Measure},
    tag::{self, Tag},
    version,
};
use crate::{details::Ready, import::Prepared};

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

/// One direction a stackable build repeats along. Each copy sits `west`,
/// `up`, and `north` blocks from the one before, since copies count up from
/// the south-east-bottom corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackDirection {
    pub west: i64,
    pub up: i64,
    pub north: i64,
    /// The most copies that work; `None` for no limit.
    pub max_copies: Option<i64>,
}

impl StackDirection {
    fn parts(&self) -> impl Iterator<Item = (i64, &'static str)> {
        [(self.west, "west"), (self.up, "up"), (self.north, "north")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
    }

    /// The way copies go, such as `west` or `west-up`. It names the
    /// direction in a download link.
    pub fn name(&self) -> String {
        self.parts()
            .map(|(_, way)| way)
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Such as `2 west per copy, up to 7 copies`.
    pub fn describe(&self) -> String {
        let steps: Vec<String> =
            self.parts().map(|(n, way)| format!("{n} {way}")).collect();
        let limit = match self.max_copies {
            Some(n) => format!("up to {n} copies"),
            None => "any number of copies".to_owned(),
        };
        format!("{} per copy, {limit}", steps.join(" and "))
    }

    /// The way the bits of a build run whose copies this way add bits: west
    /// alone is horizontal, up alone vertical, and west and up diagonal.
    /// `None` for any other way.
    pub fn orientation(&self) -> Option<Orientation> {
        match (self.west > 0, self.up > 0, self.north > 0) {
            (true, false, false) => Some(Orientation::Horizontal),
            (false, true, false) => Some(Orientation::Vertical),
            (true, true, false) => Some(Orientation::Diagonal),
            _ => None,
        }
    }

    /// Whether copies this way add bits, as along the way the build's bits
    /// already run, rather than add parallel copies such as more registers.
    pub fn adds_bits(&self, orientation: Orientation) -> bool {
        self.orientation() == Some(orientation)
    }
}

impl std::str::FromStr for StackDirection {
    type Err = String;

    /// Parses `west=2`, `north=4,max=3`, or `west=1,up=1`.
    fn from_str(text: &str) -> Result<Self, String> {
        let mut direction = StackDirection {
            west: 0,
            up: 0,
            north: 0,
            max_copies: None,
        };
        for pair in text.split(',') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("expected key=value, got `{pair}`"))?;
            let value: i64 = value
                .trim()
                .parse()
                .map_err(|_| format!("`{value}` is not a whole number"))?;
            match key.trim() {
                "west" => direction.west = value,
                "up" => direction.up = value,
                "north" => direction.north = value,
                "max" => direction.max_copies = Some(value),
                other => {
                    return Err(format!(
                        "unknown key `{other}`; use west, up, north, or max"
                    ));
                }
            }
        }
        if direction.west + direction.up + direction.north <= 0 {
            return Err("give a step west, up, or north".to_owned());
        }
        Ok(direction)
    }
}

/// A stored schematic's metadata. Its file stays in the database until
/// [`version::current_file`] fetches it.
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
    /// Whether the build follows the building conventions, so the lint and
    /// the interface reader apply to it.
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
    /// Sorted by kind in glossary order (restriction, other),
    /// then name, after `fails_simulation` if simulating its file got some
    /// cases wrong, see `models::simulation`; `torchless` when its file has
    /// no torches, and `alpha` when it works from Alpha.
    pub tags: Vec<Tag>,
}

impl Schematic {
    fn tags_in(&self, kind: tag::Kind) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(move |tag| tag.kind == kind)
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
        self.given_name
            .clone()
            .unwrap_or_else(|| self.category_label())
    }

    /// Its category's measures that lead a name, such as `8-bit` and
    /// `32-byte`, or `2-tick`; an unknown one is left out.
    fn measures(&self) -> impl Iterator<Item = String> {
        self.measures
            .iter()
            .filter_map(|m| m.of(self).map(|n| m.lead(n)))
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
            .chain(self.hex())
            .chain(self.kind())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `Hex` for a build that counts in hex, before its design, as in
    /// `32-bit Hex Carry Cancel Adder`; a binary one says nothing.
    fn hex(&self) -> Option<String> {
        (self.encoding == Some(Encoding::Hex))
            .then(|| Encoding::Hex.label().to_owned())
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
            .chain(self.hex())
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
        let mut stem = String::new();
        for c in self.name().chars() {
            if c.is_ascii_alphanumeric() {
                stem.push(c);
            } else if !stem.is_empty() && !stem.ends_with('_') {
                stem.push('_');
            }
        }
        format!("{}.schem", stem.trim_end_matches('_'))
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

/// Every schematic, ordered by category, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    load(db, None).await
}

/// The schematic `id`, if there is one.
pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    Ok(load(db, Some(ids_json(&[id]))).await?.pop())
}

/// The schematics `ids` name, in that order, leaving out any id that names
/// none.
pub async fn get_many(
    db: &SqlitePool,
    ids: &[&str],
) -> Result<Vec<Schematic>, sqlx::Error> {
    let mut found: HashMap<String, Schematic> = load(db, Some(ids_json(ids)))
        .await?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    Ok(ids.iter().filter_map(|&id| found.remove(id)).collect())
}

/// `ids` as a JSON array, as the loader's queries take a list.
fn ids_json(ids: &[&str]) -> String {
    serde_json::to_string(ids).expect("a list of strings is JSON")
}

/// The schematics `only` names, as a JSON array of ids, or every one for
/// `None`, ordered by category, width, then insertion: each row with its
/// tags, flags, stacking, credits, and its category's measures and aliases.
async fn load(
    db: &SqlitePool,
    only: Option<String>,
) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT schematic.id, schematic.category, width,
            orientation as "orientation: Orientation",
            encoding as "encoding: Encoding", latency,
            synchronized as "synchronized: bool", throughput, at_least, pulse,
            bytes, torches, works_from, works_from_by, size_x, size_y, size_z,
            built_on, imported_by, file.sha256 as "sha256!", added_at,
            edited_at, downloads, keeps_paste as "keeps_paste: bool",
            schematic.logic as "own_logic: Logic",
            coalesce(schematic.logic, category.logic) as "logic?: Logic",
            conventional as "conventional: bool", description, given_name,
            design, design.abbreviation as "design_abbreviation?: String",
            design.label as "design_given_label?: String",
            category.label as "category_label?: String",
            user.username as "uploader?: String",
            coalesce(user.minecraft_name, user.username)
                as "uploader_name?: String",
            EXISTS (SELECT 1 FROM simulation
                WHERE simulation.schematic_id = schematic.id
                    AND simulation.sha256 = file.sha256
                    AND simulation.failed)
                as "fails_simulation!: bool"
         FROM schematic
         JOIN current_file AS file ON file.schematic_id = schematic.id
         LEFT JOIN category ON category.name = schematic.category
         LEFT JOIN design ON design.category = schematic.category
            AND design.name = schematic.design
         LEFT JOIN user ON user.id = schematic.uploaded_by
         WHERE ?1 IS NULL OR schematic.id IN (SELECT value FROM json_each(?1))
         ORDER BY schematic.category, width, added_at"#,
        only
    )
    .fetch_all(db)
    .await?;
    let mut tags = tags_by_schematic(db, only.as_deref()).await?;
    let mut flags = flags_by_schematic(db, only.as_deref()).await?;
    let mut stacking = stacking_by_schematic(db, only.as_deref()).await?;
    let mut credits = credits_by_schematic(db, only.as_deref()).await?;
    let extras = catalogue::Extras::load(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (builders, contributors) =
                credits.remove(&row.id).unwrap_or_default();
            Schematic {
                measures: extras.measures(&row.category),
                aliases: extras.aliases(&row.category),
                tags: tag::with_derived(
                    tags.remove(&row.id).unwrap_or_default(),
                    row.fails_simulation,
                    row.torches,
                    row.works_from.as_deref(),
                ),
                flags: flags.remove(&row.id).unwrap_or_default(),
                stacking: stacking.remove(&row.id).unwrap_or_default(),
                builders,
                contributors,
                id: row.id,
                category: row.category,
                category_label: row.category_label,
                width: row.width,
                orientation: row.orientation,
                encoding: row.encoding,
                latency: row.latency,
                synchronized: row.synchronized,
                logic: row.logic,
                own_logic: row.own_logic,
                throughput: row.throughput,
                at_least: row.at_least,
                pulse: row.pulse,
                torches: row.torches,
                works_from: row.works_from,
                works_from_by: row.works_from_by,
                bytes: row.bytes,
                size: (row.size_x, row.size_y, row.size_z),
                built_on: row.built_on,
                imported_by: row.imported_by,
                conventional: row.conventional,
                keeps_paste: row.keeps_paste,
                design: row.design,
                design_abbreviation: row.design_abbreviation,
                design_given_label: row.design_given_label,
                description: row.description,
                given_name: row.given_name,
                uploader: row.uploader,
                uploader_name: row.uploader_name,
                sha256: row.sha256,
                added_at: row.added_at,
                edited_at: row.edited_at,
                downloads: row.downloads,
            }
        })
        .collect())
}

/// The stored tags of the builds `only` names, as `load` takes it, by id,
/// by kind then name.
async fn tags_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Tag>>, sqlx::Error> {
    let mut tags: HashMap<String, Vec<Tag>> = HashMap::new();
    for row in sqlx::query!(
        r#"SELECT schematic_tag.schematic_id, tag.name, tag.token,
            tag.kind AS "kind: tag::Kind"
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         WHERE ?1 IS NULL
            OR schematic_tag.schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY tag.kind = 'other', tag.kind, tag.name"#,
        only
    )
    .fetch_all(db)
    .await?
    {
        tags.entry(row.schematic_id).or_default().push(Tag {
            name: row.name,
            token: row.token,
            kind: row.kind,
        });
    }
    Ok(tags)
}

/// The flags of the builds `only` names, by id, in their category's order.
async fn flags_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<Flag>>, sqlx::Error> {
    let mut flags: HashMap<String, Vec<Flag>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_flag.schematic_id, flag.category, flag.name,
            flag.label
         FROM schematic_flag
         JOIN schematic ON schematic.id = schematic_flag.schematic_id
         JOIN flag ON flag.category = schematic.category
            AND flag.name = schematic_flag.name
         WHERE ?1 IS NULL
            OR schematic_flag.schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY flag.position",
        only
    )
    .fetch_all(db)
    .await?
    {
        flags.entry(row.schematic_id).or_default().push(Flag {
            category: row.category,
            name: row.name,
            label: row.label,
        });
    }
    Ok(flags)
}

/// The stacking directions of the builds `only` names, by id, the most
/// westward first.
async fn stacking_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<StackDirection>>, sqlx::Error> {
    let mut stacking: HashMap<String, Vec<StackDirection>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, west, up, north, max_copies
         FROM stack_direction
         WHERE ?1 IS NULL
            OR schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY west DESC, up DESC, north DESC",
        only
    )
    .fetch_all(db)
    .await?
    {
        stacking
            .entry(row.schematic_id)
            .or_default()
            .push(StackDirection {
                west: row.west,
                up: row.up,
                north: row.north,
                max_copies: row.max_copies,
            });
    }
    Ok(stacking)
}

/// The builders and contributors of the builds `only` names, by id, each
/// in the order given.
async fn credits_by_schematic(
    db: &SqlitePool,
    only: Option<&str>,
) -> Result<HashMap<String, (Vec<String>, Vec<String>)>, sqlx::Error> {
    let mut credits: HashMap<String, (Vec<String>, Vec<String>)> =
        HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, role, name FROM credit
         WHERE ?1 IS NULL
            OR schematic_id IN (SELECT value FROM json_each(?1))
         ORDER BY schematic_id, position",
        only
    )
    .fetch_all(db)
    .await?
    {
        let (builders, contributors) =
            credits.entry(row.schematic_id).or_default();
        if row.role == Credit::Builder.as_str() {
            builders.push(row.name);
        } else {
            contributors.push(row.name);
        }
    }
    Ok(credits)
}

/// What a person did for a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Credit {
    Builder,
    /// Helped, such as with the design or testing, without building it.
    Contributor,
}

impl Credit {
    fn as_str(self) -> &'static str {
        match self {
            Credit::Builder => "builder",
            Credit::Contributor => "contributor",
        }
    }
}

/// Whether a name can be credited: a Minecraft name of letters, digits,
/// and underscores.
pub fn valid_player_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Everyone credited on a stored build, by name.
pub async fn all_people<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT DISTINCT name FROM credit ORDER BY name")
        .fetch_all(db)
        .await
}

/// Ids starting with `prefix`; at most two, enough to detect ambiguity.
pub async fn ids_with_prefix<'e, E: SqliteExecutor<'e>>(
    db: E,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM schematic WHERE id GLOB ? || '*' LIMIT 2",
        prefix
    )
    .fetch_all(db)
    .await
}

/// A new build: its details as checked, and its file as prepared.
#[derive(Clone, Copy)]
pub struct New<'a> {
    pub ready: &'a Ready,
    pub file: &'a Prepared,
    pub uploaded_by: Option<uuid::Uuid>,
    /// The in-game name of whoever imported it with `rsdb`.
    pub imported_by: Option<&'a str>,
}

/// Stores a new build, with its file as its first version, and returns its
/// generated id.
pub async fn insert(
    db: &SqlitePool,
    new: &New<'_>,
) -> Result<String, sqlx::Error> {
    let (details, file) = (&new.ready.details, new.file);
    let (size, facts) = (&file.size, &file.facts);
    let (x, y, z) = (size.x, size.y, size.z);
    let mut tx = db.begin().await?;
    let id = sqlx::query_scalar!(
        r#"INSERT INTO schematic (category, width, orientation, encoding,
            latency, synchronized, size_x, size_y, size_z, built_on,
            imported_by, conventional, design, description, given_name,
            logic, throughput, at_least, pulse, bytes, torches, works_from,
            works_from_by, keeps_paste, uploaded_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            ?, ?, ?)
         RETURNING id as "id!""#,
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        x,
        y,
        z,
        details.built_on,
        new.imported_by,
        file.conventional,
        details.design,
        details.description,
        details.given_name,
        details.logic,
        details.throughput,
        details.at_least,
        details.pulse,
        details.bytes,
        facts.torches,
        facts.works_from.release,
        facts.works_from.because,
        file.keeps_paste,
        new.uploaded_by,
    )
    .fetch_one(&mut *tx)
    .await?;
    version::first(&mut tx, &id, file, new.uploaded_by).await?;
    set_lists(&mut tx, &id, new.ready).await?;
    tx.commit().await?;
    Ok(id)
}

/// Writes checked details over build `id`'s, with its credits, tags, and
/// flags, and its stacking unless the simulation decides it. The build
/// counts as edited.
pub async fn set_details(
    db: &SqlitePool,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    let mut tx = db.begin().await?;
    sqlx::query!(
        "UPDATE schematic SET category = ?, width = ?, orientation = ?,
            encoding = ?, latency = ?, synchronized = ?, design = ?,
            built_on = ?, description = ?, given_name = ?, logic = ?,
            throughput = ?, at_least = ?, pulse = ?, bytes = ?,
            edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        details.design,
        details.built_on,
        details.description,
        details.given_name,
        details.logic,
        details.throughput,
        details.at_least,
        details.pulse,
        details.bytes,
        id,
    )
    .execute(&mut *tx)
    .await?;
    set_lists(&mut tx, id, ready).await?;
    tx.commit().await
}

/// Replaces build `id`'s credits, tags, and flags with `ready`'s, and its
/// stacking unless the simulation decides it.
async fn set_lists(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    if !ready.stacking_inferred {
        set_stacking(tx, id, &details.stacking).await?;
    }
    set_flags(tx, id, &details.flags).await?;
    let credits = [
        (Credit::Builder, &details.builders),
        (Credit::Contributor, &details.contributors),
    ];
    for (credit, names) in credits {
        set_credits(tx, id, credit, names).await?;
    }
    tag::set(tx, id, &details.tags).await
}

/// Replaces the people a build credits in one role with `names`, in that
/// order. A name given twice, in any case, is credited once.
async fn set_credits(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    credit: Credit,
    names: &[String],
) -> Result<(), sqlx::Error> {
    let role = credit.as_str();
    sqlx::query!(
        "DELETE FROM credit WHERE schematic_id = ? AND role = ?",
        id,
        role
    )
    .execute(&mut *tx)
    .await?;
    for (position, name) in (0_i64..).zip(names) {
        // A name an admin has confirmed for an account is stored as that
        // account spells it.
        sqlx::query!(
            "INSERT OR IGNORE INTO credit (schematic_id, role, name, position)
             VALUES (?, ?, coalesce(
                (SELECT minecraft_name FROM user WHERE minecraft_verified
                    AND minecraft_name = ? COLLATE NOCASE),
                ?), ?)",
            id,
            role,
            name,
            name,
            position
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the directions a build stacks along; none makes it unstackable.
pub async fn set_stacking(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    directions: &[StackDirection],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM stack_direction WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for d in directions {
        sqlx::query!(
            "INSERT INTO stack_direction (schematic_id, west, up, north, max_copies)
             VALUES (?, ?, ?, ?, ?)",
            id,
            d.west,
            d.up,
            d.north,
            d.max_copies,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the flags a build has with `names`.
pub async fn set_flags(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic_flag WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for name in names {
        sqlx::query!(
            "INSERT OR IGNORE INTO schematic_flag (schematic_id, name)
             VALUES (?, ?)",
            id,
            name
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Sets the details worked out from a build's file and its simulation
/// rather than typed: its width, orientation, and encoding as its file
/// shows them, and its latency and whether it is synchronized as the
/// simulation measured them. None of it is a person's edit.
pub async fn set_derived<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    width: Option<i64>,
    orientation: Orientation,
    encoding: Option<Encoding>,
    latency: Option<i64>,
    synchronized: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET width = ?, orientation = ?, encoding = ?,
            latency = ?, synchronized = ?
         WHERE id = ?",
        width,
        orientation,
        encoding,
        latency,
        synchronized,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets a monostable's pulse as the simulation measured it, which is no
/// person's edit either.
pub async fn set_pulse<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    pulse: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE schematic SET pulse = ? WHERE id = ?", pulse, id)
        .execute(db)
        .await?;
    Ok(())
}

/// Sets the fields only `rsdb set` changes once a build is stored: who
/// imported it, and whether it follows the building conventions.
pub async fn set_import_details<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    imported_by: Option<&str>,
    conventional: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET imported_by = ?, conventional = ? WHERE id = ?",
        imported_by,
        conventional,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Counts one more download of a schematic.
pub async fn count_download<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET downloads = downloads + 1 WHERE id = ?",
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Deletes a schematic, with its versions and everything else of it.
pub async fn delete<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic WHERE id = ?", id)
        .execute(db)
        .await?;
    Ok(())
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
            tags: vec![Tag {
                name: "alpha".to_owned(),
                token: None,
                kind: tag::Kind::Restriction,
            }],
        }
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
    fn a_hex_build_says_so_before_its_design() {
        let mut hex = cla();
        hex.width = Some(32);
        hex.encoding = Some(Encoding::Hex);
        assert_eq!(hex.name(), "32-bit Hex Carry Lookahead Adder");
        assert_eq!(hex.title(), "32-bit alpha Hex Carry Lookahead Adder");
        assert_eq!(hex.filename(), "32_bit_Hex_Carry_Lookahead_Adder.schem");
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
    fn copies_add_bits_along_the_way_the_bits_run() {
        let way = |west, up, north| StackDirection {
            west,
            up,
            north,
            max_copies: None,
        };
        assert!(way(2, 0, 0).adds_bits(Orientation::Horizontal));
        assert!(way(0, 2, 0).adds_bits(Orientation::Vertical));
        assert!(way(1, 1, 0).adds_bits(Orientation::Diagonal));
        assert!(!way(0, 0, 3).adds_bits(Orientation::Horizontal));
        assert!(!way(2, 0, 0).adds_bits(Orientation::Vertical));
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
