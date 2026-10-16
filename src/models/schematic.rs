use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

use crate::import::FileFacts;

/// A tag's name is written in full words joined by underscores, such as
/// `carry_lookahead`.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    /// A short form that commands and search accept.
    pub token: Option<String>,
    /// `restriction`, `other`, or `failing`, which colours its badge.
    pub kind: String,
}

/// The name of the tag simulation puts on a build that gets some cases
/// wrong. It is not stored, so no one can add or remove it.
pub const FAILS_SIMULATION: &str = "fails_simulation";

/// The name of the tag a build whose file has no redstone torches carries.
/// It is not stored, so no one can add or remove it.
pub const TORCHLESS: &str = "torchless";

/// The name of the tag a build carries that works from Alpha, using no
/// redstone component or mechanic added since. It is not stored either.
pub const ALPHA: &str = "alpha";

/// The tags worked out from a build rather than given by a person.
pub const DERIVED: [&str; 3] = [FAILS_SIMULATION, TORCHLESS, ALPHA];

impl Tag {
    fn derived(name: &str, kind: &str) -> Self {
        Tag {
            name: name.to_owned(),
            token: None,
            kind: kind.to_owned(),
        }
    }

    /// Whether the build's file or simulation sets it, rather than a
    /// person.
    pub fn is_derived(&self) -> bool {
        DERIVED.contains(&self.name.as_str())
    }

    /// The name as words, such as `carry lookahead`.
    pub fn label(&self) -> String {
        self.name.replace('_', " ")
    }

    /// How a filename writes this tag.
    pub fn token(&self) -> &str {
        self.token.as_deref().unwrap_or(&self.name)
    }
}

/// The form of a tag name a person types, with spaces for underscores.
pub fn tag_name(input: &str) -> String {
    input.trim().to_lowercase().replace(' ', "_")
}

/// A name's words for people to read, each capitalized, such as
/// `Carry Lookahead` for `carry_lookahead`.
pub fn words(name: &str) -> String {
    name.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a build remembers anything, as people read it: `Stateless` for
/// `comb`, whose outputs, once its inputs stop changing, depend only on
/// them, and `Stateful` for `seq`, which remembers. The stored values keep
/// their old names, combinational and sequential.
pub fn logic_label(logic: &str) -> &str {
    match logic {
        "comb" => "Stateless",
        "seq" => "Stateful",
        other => other,
    }
}

/// The stored value a word names: `comb` for `stateless`, `seq` for
/// `stateful`, and the stored values themselves.
pub fn logic_value(word: &str) -> &str {
    match word {
        "stateless" => "comb",
        "stateful" => "seq",
        other => other,
    }
}

/// A number a category's builds are named and ranked by, as width is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Measure {
    /// Bits, as in `8-bit`.
    Width,
    /// A memory's capacity, as in `32-byte`.
    Bytes,
    /// A serial build's ticks from one bit to the next, as in `2-tick`.
    Throughput,
    /// A threshold's k, the fewest inputs on that turn it on, as in `At
    /// Least 3`.
    AtLeast,
}

impl Measure {
    pub const ALL: [Measure; 4] = [
        Measure::Width,
        Measure::Bytes,
        Measure::Throughput,
        Measure::AtLeast,
    ];

    /// As stored, and as a leaderboard's address and `rsdb` name it.
    pub fn name(self) -> &'static str {
        match self {
            Measure::Width => "width",
            Measure::Bytes => "bytes",
            Measure::Throughput => "throughput",
            Measure::AtLeast => "at_least",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Measure::ALL.into_iter().find(|m| m.name() == name)
    }

    /// Its heading, such as `Capacity`.
    pub fn heading(self) -> &'static str {
        match self {
            Measure::Width => "Width",
            Measure::Bytes => "Capacity",
            Measure::Throughput => "Throughput",
            Measure::AtLeast => "At least",
        }
    }

    /// The build's value of it.
    pub fn of(self, s: &Schematic) -> Option<i64> {
        match self {
            Measure::Width => s.width,
            Measure::Bytes => s.bytes,
            Measure::Throughput => s.throughput,
            Measure::AtLeast => s.at_least,
        }
    }

    /// `n` of it as a name leads with it, such as `8-bit`, `32-byte`,
    /// `2-tick`, or `At Least 3`.
    pub fn lead(self, n: i64) -> String {
        match self {
            Measure::Width => format!("{n}-bit"),
            Measure::Bytes => format!("{n}-byte"),
            Measure::Throughput => format!("{n}-tick"),
            Measure::AtLeast => format!("At Least {n}"),
        }
    }

    /// `n` of it in words, such as `8 bits` or `at least 3 on`.
    pub fn words(self, n: i64) -> String {
        let count = |one: &str, many: &str| {
            format!("{n} {}", if n == 1 { one } else { many })
        };
        match self {
            Measure::Width => count("bit", "bits"),
            Measure::Bytes => count("byte", "bytes"),
            Measure::Throughput => count("tick", "ticks"),
            Measure::AtLeast => format!("at least {n} on"),
        }
    }

    /// The search term that finds `n` of it, such as `8b`.
    pub fn term(self, n: i64) -> String {
        match self {
            Measure::Width => format!("{n}b"),
            Measure::Bytes => format!("{n}-byte"),
            Measure::Throughput => format!("{n}t"),
            Measure::AtLeast => format!("at least {n}"),
        }
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

    /// Whether copies this way add bits, as along the way the build's bits
    /// already run, rather than add parallel copies such as more registers.
    pub fn adds_bits(&self, orientation: &str) -> bool {
        match orientation {
            "horz" => self.west > 0 && self.up == 0 && self.north == 0,
            "vert" => self.up > 0 && self.west == 0 && self.north == 0,
            "diag" => self.west > 0 && self.up > 0 && self.north == 0,
            _ => false,
        }
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

/// A stored schematic's metadata. The `.schem` bytes stay in the database
/// until [`data`] fetches them.
#[derive(Debug, Clone)]
pub struct Schematic {
    pub id: String,
    pub category: String,
    /// The standard category's label, such as `RAM`; `None` for a category
    /// off the list.
    pub category_label: Option<String>,
    pub width: Option<i64>,
    pub orientation: String,
    /// `bin` or `hex`; `None` for a category off the list, such as a
    /// piston door.
    pub encoding: Option<String>,
    pub latency: Option<i64>,
    /// Whether every output bit settles on the same tick.
    pub synchronized: Option<bool>,
    /// `comb` or `seq`: the build's own, else its category's usual logic.
    pub logic: Option<String>,
    /// The build's own logic, where it says so; `None` follows its
    /// category's.
    pub own_logic: Option<String>,
    /// The fewest ticks from one input to the next that still gives right
    /// outputs; for a serial build, from one bit to the next.
    pub throughput: Option<i64>,
    /// For a threshold, the fewest inputs on that turn it on.
    pub at_least: Option<i64>,
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
    /// Of the stored file, so a URL derived from the file can name its
    /// version.
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
    pub fn tags_in<'a>(
        &'a self,
        kind: &'a str,
    ) -> impl Iterator<Item = &'a Tag> + 'a {
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
            .unwrap_or_else(|| words(&self.category))
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
        (self.encoding.as_deref() == Some("hex")).then(|| "Hex".to_owned())
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
            .chain(self.tags_in("restriction").map(Tag::label))
            .chain(self.tags_in("other").map(Tag::label))
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
                .unwrap_or_else(|| words(&self.design))
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
            .filter(|(direction, _)| direction.adds_bits(&self.orientation))
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

    /// `Combinational` or `Sequential`.
    pub fn logic_label(&self) -> Option<&str> {
        self.logic.as_deref().map(logic_label)
    }

    /// Whether a new input goes in before the last one's result comes out:
    /// a stateless, synchronized build whose throughput is below its
    /// latency.
    pub fn pipelined(&self) -> bool {
        self.logic.as_deref() == Some("comb")
            && self.synchronized == Some(true)
            && matches!((self.throughput, self.latency), (Some(t), Some(l)) if t < l)
    }

    pub fn orientation_label(&self) -> &str {
        match self.orientation.as_str() {
            "horz" => "Horizontal",
            "vert" => "Vertical",
            "diag" => "Diagonal",
            other => other,
        }
    }

    pub fn encoding_label(&self) -> Option<&str> {
        self.encoding.as_deref().map(|encoding| match encoding {
            "bin" => "Binary",
            "hex" => "Hex",
            other => other,
        })
    }
}

/// Each schematic's flags, by its id, in its category's order.
async fn flags_by_schematic(
    db: &SqlitePool,
) -> Result<HashMap<String, Vec<Flag>>, sqlx::Error> {
    let mut flags: HashMap<String, Vec<Flag>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_flag.schematic_id, flag.category, flag.name,
            flag.label
         FROM schematic_flag
         JOIN schematic ON schematic.id = schematic_flag.schematic_id
         JOIN flag ON flag.category = schematic.category
            AND flag.name = schematic_flag.name
         ORDER BY flag.position"
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

/// Every build's stacking directions, by its id, the most westward first.
async fn stacking_by_schematic(
    db: &SqlitePool,
) -> Result<HashMap<String, Vec<StackDirection>>, sqlx::Error> {
    let mut stacking: HashMap<String, Vec<StackDirection>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, west, up, north, max_copies
         FROM stack_direction ORDER BY west DESC, up DESC, north DESC"
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

/// What every category is named and searched by beyond its row: its
/// measures and its aliases.
struct Categories {
    measures: HashMap<String, Vec<Measure>>,
    aliases: HashMap<String, Vec<String>>,
}

impl Categories {
    async fn load(db: &SqlitePool) -> Result<Self, sqlx::Error> {
        let mut measures: HashMap<String, Vec<Measure>> = HashMap::new();
        for row in sqlx::query!(
            "SELECT category, measure FROM category_measure
             ORDER BY category, position"
        )
        .fetch_all(db)
        .await?
        {
            measures
                .entry(row.category)
                .or_default()
                .extend(Measure::named(&row.measure));
        }
        let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
        for row in sqlx::query!(
            "SELECT category, alias FROM category_alias ORDER BY alias"
        )
        .fetch_all(db)
        .await?
        {
            aliases.entry(row.category).or_default().push(row.alias);
        }
        Ok(Categories { measures, aliases })
    }

    fn measures(&self, category: &str) -> Vec<Measure> {
        self.measures.get(category).cloned().unwrap_or_default()
    }

    fn aliases(&self, category: &str) -> Vec<String> {
        self.aliases.get(category).cloned().unwrap_or_default()
    }
}

/// Every schematic's stored tags, by its id, by kind then name.
async fn tags_by_schematic(
    db: &SqlitePool,
) -> Result<HashMap<String, Vec<Tag>>, sqlx::Error> {
    let mut tags: HashMap<String, Vec<Tag>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_tag.schematic_id, tag.name, tag.token, tag.kind
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         ORDER BY tag.kind = 'other', tag.kind, tag.name"
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

/// Every schematic, ordered by category, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT schematic.id, schematic.category, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", throughput, at_least, bytes, torches, works_from, works_from_by, size_x, size_y, size_z, built_on, imported_by, sha256, added_at, edited_at, downloads,
            schematic.logic as own_logic,
            coalesce(schematic.logic, (SELECT logic FROM category
                WHERE category.name = schematic.category)) as "logic?: String",
            conventional as "conventional: bool", description, given_name, design,
            design.abbreviation as "design_abbreviation?: String",
            design.label as "design_given_label?: String",
            (SELECT label FROM category WHERE category.name =
                schematic.category) as "category_label?: String",
            user.username as "uploader?: String",
            coalesce(user.minecraft_name, user.username)
                as "uploader_name?: String",
            EXISTS (SELECT 1 FROM simulation
                WHERE simulation.schematic_id = schematic.id
                    AND simulation.sha256 = schematic.sha256
                    AND (json_extract(simulation.report, '$.wrong') > 0
                        OR json_extract(simulation.report, '$.outcome')
                            = 'broken'))
                as "fails_simulation!: bool"
         FROM schematic LEFT JOIN design ON design.category =
             schematic.category AND design.name = schematic.design
         LEFT JOIN user ON user.id = schematic.uploaded_by
         ORDER BY schematic.category, width, added_at"#
    )
    .fetch_all(db)
    .await?;
    let mut stacking = stacking_by_schematic(db).await?;
    let mut flags = flags_by_schematic(db).await?;
    let mut builders: HashMap<String, Vec<String>> = HashMap::new();
    let mut contributors: HashMap<String, Vec<String>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, role, name FROM credit
         ORDER BY schematic_id, position"
    )
    .fetch_all(db)
    .await?
    {
        let people = if row.role == Credit::Builder.as_str() {
            &mut builders
        } else {
            &mut contributors
        };
        people.entry(row.schematic_id).or_default().push(row.name);
    }
    let mut tags = tags_by_schematic(db).await?;
    let about = Categories::load(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| Schematic {
            measures: about.measures(&row.category),
            aliases: about.aliases(&row.category),
            tags: with_derived(
                tags.remove(&row.id),
                row.fails_simulation,
                row.torches,
                row.works_from.as_deref(),
            ),
            flags: flags.remove(&row.id).unwrap_or_default(),
            builders: builders.remove(&row.id).unwrap_or_default(),
            contributors: contributors.remove(&row.id).unwrap_or_default(),
            stacking: stacking.remove(&row.id).unwrap_or_default(),
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
            torches: row.torches,
            works_from: row.works_from,
            works_from_by: row.works_from_by,
            bytes: row.bytes,
            size: (row.size_x, row.size_y, row.size_z),
            built_on: row.built_on,
            imported_by: row.imported_by,
            conventional: row.conventional,
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
        })
        .collect())
}

/// A build's stored `tags`, if any, led by `fails_simulation` when it
/// fails, with `torchless` among its restrictions when its file has no
/// torches, and `alpha` when it works from Alpha.
fn with_derived(
    tags: Option<Vec<Tag>>,
    fails_simulation: bool,
    torches: Option<i64>,
    works_from: Option<&str>,
) -> Vec<Tag> {
    let mut tags = tags.unwrap_or_default();
    let alpha = works_from == Some(crate::works_from::RELEASES[0].name);
    for (has, name) in [(alpha, ALPHA), (torches == Some(0), TORCHLESS)] {
        if !has {
            continue;
        }
        // Restrictions come first, by name.
        let at = tags
            .iter()
            .position(|tag| tag.kind != "restriction" || *tag.name > *name)
            .unwrap_or(tags.len());
        tags.insert(at, Tag::derived(name, "restriction"));
    }
    if fails_simulation {
        tags.insert(0, Tag::derived(FAILS_SIMULATION, "failing"));
    }
    tags
}

pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"SELECT schematic.id, schematic.category, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", throughput, at_least, bytes, torches, works_from, works_from_by, size_x, size_y, size_z, built_on, imported_by, sha256, added_at, edited_at, downloads,
            schematic.logic as own_logic,
            coalesce(schematic.logic, (SELECT logic FROM category
                WHERE category.name = schematic.category)) as "logic?: String",
            conventional as "conventional: bool", description, given_name, design,
            design.abbreviation as "design_abbreviation?: String",
            design.label as "design_given_label?: String",
            (SELECT label FROM category WHERE category.name =
                schematic.category) as "category_label?: String",
            user.username as "uploader?: String",
            coalesce(user.minecraft_name, user.username)
                as "uploader_name?: String",
            EXISTS (SELECT 1 FROM simulation
                WHERE simulation.schematic_id = schematic.id
                    AND simulation.sha256 = schematic.sha256
                    AND (json_extract(simulation.report, '$.wrong') > 0
                        OR json_extract(simulation.report, '$.outcome')
                            = 'broken'))
                as "fails_simulation!: bool"
         FROM schematic LEFT JOIN design ON design.category =
             schematic.category AND design.name = schematic.design
         LEFT JOIN user ON user.id = schematic.uploaded_by
         WHERE schematic.id = ?"#,
        id
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let (tags, flags) = (stored_tags(db, id).await?, its_flags(db, id).await?);
    let builders = credits(db, id, Credit::Builder).await?;
    let contributors = credits(db, id, Credit::Contributor).await?;
    let about = Categories::load(db).await?;
    Ok(Some(Schematic {
        measures: about.measures(&row.category),
        aliases: about.aliases(&row.category),
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
        torches: row.torches,
        works_from: row.works_from.clone(),
        works_from_by: row.works_from_by,
        bytes: row.bytes,
        size: (row.size_x, row.size_y, row.size_z),
        stacking: stacking(db, id).await?,
        builders,
        contributors,
        built_on: row.built_on,
        imported_by: row.imported_by,
        conventional: row.conventional,
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
        tags: with_derived(
            Some(tags),
            row.fails_simulation,
            row.torches,
            row.works_from.as_deref(),
        ),
        flags,
    }))
}

/// Build `id`'s stored tags, by kind then name.
async fn stored_tags(
    db: &SqlitePool,
    id: &str,
) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        "SELECT tag.name, tag.token, tag.kind
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         WHERE schematic_tag.schematic_id = ?
         ORDER BY tag.kind = 'other', tag.kind, tag.name",
        id
    )
    .fetch_all(db)
    .await
}

/// Build `id`'s flags, in its category's order.
async fn its_flags(
    db: &SqlitePool,
    id: &str,
) -> Result<Vec<Flag>, sqlx::Error> {
    sqlx::query_as!(
        Flag,
        "SELECT flag.category, flag.name, flag.label
         FROM schematic_flag
         JOIN schematic ON schematic.id = schematic_flag.schematic_id
         JOIN flag ON flag.category = schematic.category
            AND flag.name = schematic_flag.name
         WHERE schematic_flag.schematic_id = ?
         ORDER BY flag.position",
        id
    )
    .fetch_all(db)
    .await
}

/// What a person did for a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credit {
    Builder,
    /// Helped, such as with the design or testing, without building it.
    Contributor,
}

impl Credit {
    pub fn as_str(self) -> &'static str {
        match self {
            Credit::Builder => "builder",
            Credit::Contributor => "contributor",
        }
    }
}

/// The people a build credits in one role, in the order given.
pub async fn credits<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    credit: Credit,
) -> Result<Vec<String>, sqlx::Error> {
    let role = credit.as_str();
    sqlx::query_scalar!(
        "SELECT name FROM credit WHERE schematic_id = ? AND role = ?
         ORDER BY position",
        id,
        role
    )
    .fetch_all(db)
    .await
}

/// Replaces the people a build credits in one role with `names`, in that
/// order. A name given twice, in any case, is credited once.
pub async fn set_credits(
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
    for (position, name) in names.iter().enumerate() {
        let position = i64::try_from(position).unwrap_or(i64::MAX);
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

/// Whether a name can be credited: a Minecraft name of letters, digits,
/// and underscores.
pub fn valid_player_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The directions a build stacks along: west first, then up, then north.
pub async fn stacking<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Vec<StackDirection>, sqlx::Error> {
    sqlx::query_as!(
        StackDirection,
        "SELECT west, up, north, max_copies FROM stack_direction
         WHERE schematic_id = ? ORDER BY west DESC, up DESC, north DESC",
        id
    )
    .fetch_all(db)
    .await
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

/// The stored `.schem` bytes.
pub async fn data<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!("SELECT data FROM schematic WHERE id = ?", id)
        .fetch_optional(db)
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

pub async fn id_by_sha256<'e, E: SqliteExecutor<'e>>(
    db: E,
    sha256: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT id FROM schematic WHERE sha256 = ?", sha256)
        .fetch_optional(db)
        .await
}

#[derive(Clone, Copy)]
pub struct New<'a> {
    pub category: &'a str,
    pub width: Option<i64>,
    pub orientation: &'a str,
    pub encoding: Option<&'a str>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub size: (i64, i64, i64),
    pub built_on: Option<&'a str>,
    pub imported_by: Option<&'a str>,
    pub conventional: bool,
    pub design: Option<&'a str>,
    pub description: Option<&'a str>,
    pub given_name: Option<&'a str>,
    /// `comb` or `seq`; `None` follows the category.
    pub logic: Option<&'a str>,
    pub throughput: Option<i64>,
    pub at_least: Option<i64>,
    pub bytes: Option<i64>,
    /// What is read from the file, such as its torches.
    pub facts: &'a FileFacts,
    /// Whether the file keeps where its uploader's `//paste` put it.
    pub keeps_paste: bool,
    pub uploaded_by: Option<uuid::Uuid>,
    pub sha256: &'a str,
    pub data: &'a [u8],
}

/// Inserts a schematic and returns its generated id.
pub async fn insert<'e, E: SqliteExecutor<'e>>(
    db: E,
    new: &New<'_>,
) -> Result<String, sqlx::Error> {
    let (x, y, z) = new.size;
    sqlx::query_scalar!(
        r#"INSERT INTO schematic (category, width, orientation, encoding,
            latency, synchronized, size_x, size_y, size_z, built_on,
            imported_by, conventional, design, description, given_name,
            logic, throughput, at_least, bytes, torches, works_from,
            works_from_by, keeps_paste, uploaded_by, sha256, data)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            ?, ?, ?, ?)
         RETURNING id as "id!""#,
        new.category,
        new.width,
        new.orientation,
        new.encoding,
        new.latency,
        new.synchronized,
        x,
        y,
        z,
        new.built_on,
        new.imported_by,
        new.conventional,
        new.design,
        new.description,
        new.given_name,
        new.logic,
        new.throughput,
        new.at_least,
        new.bytes,
        new.facts.torches,
        new.facts.works_from.release,
        new.facts.works_from.because,
        new.keeps_paste,
        new.uploaded_by,
        new.sha256,
        new.data,
    )
    .fetch_one(db)
    .await
}

/// An entry in the list of categories a build may have: one function, so
/// any of its builds could stand in for another.
#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    /// As people read it, such as `RAM` or `BCD to Binary`.
    pub label: String,
    /// The label for many builds, such as `Adders` or `RAM`.
    pub plural: String,
    /// Whether each build gives its own name, as bussing does.
    pub named: bool,
    /// The sidebar section it sits in, one of [`SECTIONS`]; `None` for
    /// Other, with the categories off the list.
    pub section: Option<String>,
    /// `comb` or `seq`, its builds' usual logic; `None` for none.
    pub logic: Option<String>,
    /// The numbers its builds are named and ranked by, in order, such as
    /// width then capacity for a memory, or throughput for a serial adder.
    pub measures: Vec<Measure>,
    /// Other words it is searched by, such as `inc`.
    pub aliases: Vec<String>,
}

impl Category {
    pub fn has(&self, measure: Measure) -> bool {
        self.measures.contains(&measure)
    }

    /// The standard category among `categories` that `typed` names, by its
    /// name or its label, whatever the capitals and spacing.
    pub fn find<'a>(
        categories: &'a [Category],
        typed: &str,
    ) -> Option<&'a Self> {
        let typed = typed.trim();
        categories.iter().find(|c| {
            c.label.eq_ignore_ascii_case(typed) || c.name == tag_name(typed)
        })
    }

    /// The name of the category `typed` names: a standard one's among
    /// `categories`, or else the text as the name of a category of the
    /// builder's own, empty when nothing is typed.
    pub fn name_of(categories: &[Category], typed: &str) -> String {
        Self::find(categories, typed)
            .map_or_else(|| tag_name(typed), |c| c.name.clone())
    }
}

/// The sidebar's sections of standard categories, in order: each stored
/// name, as `rsdb define-category --section` takes it, with its heading.
pub const SECTIONS: [(&str, &str); 7] = [
    ("arithmetic", "Arithmetic"),
    ("logic", "Logic"),
    ("gates", "Gates"),
    ("conversion", "Conversion"),
    ("memory", "Memory"),
    ("display", "Display"),
    ("wiring", "Wiring"),
];

/// A design within a category, such as the carry lookahead adder.
#[derive(Debug, Clone)]
pub struct Design {
    pub category: String,
    pub name: String,
    /// The usual short name, such as `CLA`.
    pub abbreviation: Option<String>,
    /// How people read it, such as `XOR`; `None` for the name's words.
    pub label: Option<String>,
}

impl Design {
    /// Such as `Carry Lookahead (CLA)`.
    pub fn label(&self) -> String {
        let words = self.label.clone().unwrap_or_else(|| words(&self.name));
        match &self.abbreviation {
            Some(short) => format!("{words} ({short})"),
            None => words,
        }
    }
}

/// Every standard category, alphabetically by label.
pub async fn categories(db: &SqlitePool) -> Result<Vec<Category>, sqlx::Error> {
    let about = Categories::load(db).await?;
    Ok(sqlx::query!(
        r#"SELECT name as "name!", label,
            coalesce(plural, label || 's') as "plural!: String",
            named as "named: bool", section, logic
         FROM category ORDER BY label COLLATE NOCASE"#
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|row| Category {
        measures: about.measures(&row.name),
        aliases: about.aliases(&row.name),
        name: row.name,
        label: row.label,
        plural: row.plural,
        named: row.named,
        section: row.section,
        logic: row.logic,
    })
    .collect())
}

/// Gives `category`, of the list or off it, another word to be searched
/// by, in the form a search term takes: lowercase, with spaces.
pub async fn define_alias<'e, E: SqliteExecutor<'e>>(
    db: E,
    alias: &str,
    category: &str,
) -> Result<(), sqlx::Error> {
    let alias = alias.trim().to_lowercase().replace('_', " ");
    sqlx::query!(
        "INSERT INTO category_alias (alias, category) VALUES (?, ?)",
        alias,
        category
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Everyone credited on a stored build, by name.
pub async fn all_people<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT DISTINCT name FROM credit ORDER BY name")
        .fetch_all(db)
        .await
}

/// Every design, grouped by category.
pub async fn designs<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Design>, sqlx::Error> {
    sqlx::query_as!(
        Design,
        "SELECT category, name, abbreviation, label FROM design
         ORDER BY category, name = 'other', name"
    )
    .fetch_all(db)
    .await
}

/// Checks a build's category and design, and returns the category. A
/// category is a standard one from the list, or any other name made of
/// lowercase words joined by underscores. A design is `other`, or one the
/// category lists. The `Err` is a sentence for the person.
pub async fn check_category(
    db: &SqlitePool,
    category: &str,
    design: &str,
) -> Result<Result<Category, String>, sqlx::Error> {
    let valid = |name: &str| {
        name.chars().any(|c| c.is_ascii_alphanumeric())
            && name.chars().all(|c| {
                c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || c == '_'
                    || c == '-'
            })
    };
    if !valid(category) {
        return Ok(Err(
            "A category's name is words, numbers, and hyphens, such as line \
             drawer or 8-bit horizontal diode."
                .to_owned(),
        ));
    }
    // The sidebar gathers every category off the list under `other`.
    if category == "other" {
        return Ok(Err("Name the category, rather than other.".to_owned()));
    }
    let found = categories(db)
        .await?
        .into_iter()
        .find(|f| f.name == category)
        .unwrap_or_else(|| Category {
            name: category.to_owned(),
            label: words(category),
            plural: words(category),
            named: false,
            section: None,
            logic: None,
            measures: Vec::new(),
            aliases: Vec::new(),
        });
    let listed = designs(db)
        .await?
        .iter()
        .any(|d| d.category == category && d.name == design);
    if design != "other" && !listed {
        return Ok(Err(format!(
            "{} is not a design of {}.",
            design.replace('_', " "),
            category.replace('_', " ")
        )));
    }
    Ok(Ok(found))
}

/// Adds a standard category. A plural that is the label with an `s` is
/// left for the database to make.
pub async fn define_category(
    db: &SqlitePool,
    category: &Category,
) -> Result<(), sqlx::Error> {
    let plural = (category.plural != format!("{}s", category.label))
        .then_some(category.plural.as_str());
    let mut tx = db.begin().await?;
    sqlx::query!(
        "INSERT INTO category (name, label, plural, named, section, logic)
         VALUES (?, ?, ?, ?, ?, ?)",
        category.name,
        category.label,
        plural,
        category.named,
        category.section,
        category.logic
    )
    .execute(&mut *tx)
    .await?;
    for (position, measure) in (1_i64..).zip(&category.measures) {
        let measure = measure.name();
        sqlx::query!(
            "INSERT INTO category_measure (category, measure, position)
             VALUES (?, ?, ?)",
            category.name,
            measure,
            position
        )
        .execute(&mut *tx)
        .await?;
    }
    for alias in &category.aliases {
        define_alias(&mut *tx, alias, &category.name).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn define_design<'e, E: SqliteExecutor<'e>>(
    db: E,
    design: &Design,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO design (category, name, abbreviation, label)
         VALUES (?, ?, ?, ?)",
        design.category,
        design.name,
        design.abbreviation,
        design.label
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A yes-or-no fact a category's builds record, such as an adder's carry
/// in or a shift register being bidirectional.
#[derive(Debug, Clone)]
pub struct Flag {
    pub category: String,
    /// Such as `carry_in`.
    pub name: String,
    /// Such as `Carry in`.
    pub label: String,
}

/// Every category's flags, each category's in its order.
pub async fn flags<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Flag>, sqlx::Error> {
    sqlx::query_as!(
        Flag,
        "SELECT category, name, label FROM flag ORDER BY category, position"
    )
    .fetch_all(db)
    .await
}

/// Adds a flag to the end of its category's.
pub async fn define_flag<'e, E: SqliteExecutor<'e>>(
    db: E,
    flag: &Flag,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO flag (category, name, label, position)
         VALUES (?, ?, ?, (SELECT coalesce(max(position), 0) + 1 FROM flag
            WHERE category = ?))",
        flag.category,
        flag.name,
        flag.label,
        flag.category
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The names among `names` that are flags of `category`, in its order; any
/// other means nothing for its builds.
pub async fn keep_flags<'e, E: SqliteExecutor<'e>>(
    db: E,
    category: &str,
    names: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    Ok(flags(db)
        .await?
        .into_iter()
        .filter(|flag| flag.category == category && names.contains(&flag.name))
        .map(|flag| flag.name)
        .collect())
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

/// Replaces a schematic's stored file, as `rsdb reread` rewrites it.
pub async fn set_file<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    data: &[u8],
    sha256: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET data = ?, sha256 = ? WHERE id = ?",
        data,
        sha256,
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

/// The ids of builds whose files keep where their uploader's `//paste` put
/// them.
pub async fn keeping_paste(
    db: &SqlitePool,
) -> Result<std::collections::HashSet<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT id FROM schematic WHERE keeps_paste")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect(),
    )
}

/// Sets what is read from a build's file, such as its torches.
pub async fn set_file_facts<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    facts: &FileFacts,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET torches = ?, works_from = ?, works_from_by = ?
         WHERE id = ?",
        facts.torches,
        facts.works_from.release,
        facts.works_from.because,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The ids of the builds whose files are not yet read for every fact.
pub async fn unread_files(db: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM schematic WHERE torches IS NULL OR works_from IS NULL"
    )
    .fetch_all(db)
    .await
}

/// Stores a schematic's size, as `rsdb reread` finds it.
/// Sets a width read from the file, which is not a person's edit.
pub async fn set_width<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    width: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE schematic SET width = ? WHERE id = ?", width, id)
        .execute(db)
        .await?;
    Ok(())
}

/// Sets an orientation read from the file, which is not a person's edit.
pub async fn set_orientation<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    orientation: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET orientation = ? WHERE id = ?",
        orientation,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets an encoding read from the file, which is not a person's edit.
pub async fn set_encoding<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    encoding: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET encoding = ? WHERE id = ?",
        encoding,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets a latency and whether it is synchronized as simulation measured
/// them, which is not a person's edit.
pub async fn set_timing<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    latency: i64,
    synchronized: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET latency = ?, synchronized = ? WHERE id = ?",
        latency,
        synchronized,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_size<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    (x, y, z): (i64, i64, i64),
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET size_x = ?, size_y = ?, size_z = ? WHERE id = ?",
        x,
        y,
        z,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets every field of checked `details`, clearing the empty ones.
pub async fn set_details<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    details: &crate::details::Details,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET category = ?, width = ?, orientation = ?,
            encoding = ?, latency = ?, synchronized = ?, design = ?,
            built_on = ?,
            description = ?, given_name = ?, logic = ?, throughput = ?,
            at_least = ?, bytes = ?,
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
        details.bytes,
        id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Deletes a schematic and its tag links.
pub async fn delete<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic WHERE id = ?", id)
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

/// Whether a typed name, once in the form [`tag_name`] gives, can name a
/// tag: lowercase letters, digits, and underscores.
pub fn valid_tag_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Replaces a build's tags with the ones `names` name. A name no tag has
/// yet makes a new tag under `other`.
pub async fn set_tags(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic_tag WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for name in names {
        if tag_id(&mut *tx, name).await?.is_none() {
            define_tag(&mut *tx, name, None, "other").await?;
        }
        let tag = tag_id(&mut *tx, name)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        attach(&mut *tx, id, &tag).await?;
    }
    Ok(())
}

/// The names in `names` that no tag has, as its name or short token.
pub async fn unknown_tags(
    db: &SqlitePool,
    names: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    let mut unknown = Vec::new();
    for name in names {
        if tag_id(db, name).await?.is_none() {
            unknown.push(name.clone());
        }
    }
    Ok(unknown)
}

/// Refuses `names` that no tag has unless `may_create`, as only an admin
/// may: the `Err` says which and why.
pub async fn check_new_tags(
    db: &SqlitePool,
    names: &[String],
    may_create: bool,
) -> Result<Result<(), String>, sqlx::Error> {
    if may_create {
        return Ok(Ok(()));
    }
    let unknown = unknown_tags(db, names).await?;
    Ok(match unknown.as_slice() {
        [] => Ok(()),
        [one] => Err(format!(
            "There is no tag {}, and only an admin can make a new one.",
            one.replace('_', " ")
        )),
        many => Err(format!(
            "There are no tags {}, and only an admin can make new ones.",
            many.iter()
                .map(|name| name.replace('_', " "))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    })
}

/// The tag named `name`, which may also be its short token.
pub async fn tag_id<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
) -> Result<Option<String>, sqlx::Error> {
    let name = tag_name(name);
    sqlx::query_scalar!(
        "SELECT id FROM tag WHERE name = ? OR token = ?",
        name,
        name
    )
    .fetch_optional(db)
    .await
}

pub async fn attach<'e, E: SqliteExecutor<'e>>(
    db: E,
    schematic_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT OR IGNORE INTO schematic_tag (schematic_id, tag_id)
         VALUES (?, ?)",
        schematic_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn detach<'e, E: SqliteExecutor<'e>>(
    db: E,
    schematic_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM schematic_tag WHERE schematic_id = ? AND tag_id = ?",
        schematic_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn all_tags<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        "SELECT name, token, kind FROM tag
         ORDER BY kind = 'other', kind, name"
    )
    .fetch_all(db)
    .await
}

pub async fn define_tag<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
    token: Option<&str>,
    kind: &str,
) -> Result<(), sqlx::Error> {
    let name = tag_name(name);
    sqlx::query!(
        "INSERT INTO tag (name, token, kind) VALUES (?, ?, ?)",
        name,
        token,
        kind
    )
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standard_category_is_typed_by_name_or_label() {
        let hex = Category {
            name: "hex_to_binary".to_owned(),
            label: "Hex to Binary".to_owned(),
            plural: "Hex to Binary".to_owned(),
            named: false,
            section: None,
            logic: None,
            measures: vec![Measure::Width],
            aliases: Vec::new(),
        };
        let categories = std::slice::from_ref(&hex);
        for typed in [
            "Hex to Binary",
            "hex to binary",
            "hex_to_binary",
            " HEX TO BINARY ",
        ] {
            assert_eq!(Category::name_of(categories, typed), "hex_to_binary");
        }
        assert_eq!(
            Category::name_of(categories, "8-Bit Horizontal Diode"),
            "8-bit_horizontal_diode"
        );
        assert!(Category::find(categories, "piston door").is_none());
    }

    fn tag(name: &str, token: Option<&str>, kind: &str) -> Tag {
        Tag {
            name: name.to_owned(),
            token: token.map(str::to_owned),
            kind: kind.to_owned(),
        }
    }

    fn cla() -> Schematic {
        Schematic {
            id: String::new(),
            category: "adder".to_owned(),
            category_label: Some("Adder".to_owned()),
            width: Some(8),
            orientation: "horz".to_owned(),
            encoding: Some("bin".to_owned()),
            latency: Some(8),
            synchronized: Some(false),
            logic: Some("comb".to_owned()),
            own_logic: None,
            throughput: None,
            at_least: None,
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
            tags: vec![tag("alpha", None, "restriction")],
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
        hex.encoding = Some("hex".to_owned());
        assert_eq!(hex.name(), "32-bit Hex Carry Lookahead Adder");
        assert_eq!(hex.title(), "32-bit alpha Hex Carry Lookahead Adder");
        assert_eq!(hex.filename(), "32_bit_Hex_Carry_Lookahead_Adder.schem");
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
    fn a_build_without_torches_is_torchless_among_its_restrictions() {
        let stored = vec![
            tag("alpha", None, "restriction"),
            tag("buffer", None, "other"),
        ];
        let names = |tags: Vec<Tag>| -> Vec<String> {
            tags.into_iter().map(|tag| tag.name).collect()
        };
        assert_eq!(
            names(with_derived(Some(stored.clone()), true, Some(0), None)),
            ["fails_simulation", "alpha", "torchless", "buffer"]
        );
        assert_eq!(
            names(with_derived(Some(stored.clone()), false, Some(3), None)),
            ["alpha", "buffer"]
        );
        assert_eq!(
            names(with_derived(Some(stored), false, None, None)),
            ["alpha", "buffer"],
            "an uncounted build is not torchless yet"
        );
        // Alpha is read from the file too, before torchless by name.
        let other = vec![tag("buffer", None, "other")];
        assert_eq!(
            names(with_derived(Some(other), false, Some(0), Some("Alpha"))),
            ["alpha", "torchless", "buffer"]
        );
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
    fn a_synchronized_combinational_build_faster_than_its_latency_is_pipelined()
    {
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
        let sequential = Schematic {
            logic: Some("seq".to_owned()),
            ..piped.clone()
        };
        assert!(!sequential.pipelined());
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
