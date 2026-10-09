use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

/// A tag's name is written in full words joined by underscores, such as
/// `carry_lookahead`.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    /// A short form, such as `comb`, that commands and search accept.
    pub token: Option<String>,
    /// `restriction`, `timing`, or `other`, which colours its badge.
    pub kind: String,
}

impl Tag {
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
    /// For a serial build, the ticks from one bit to the next.
    pub period: Option<i64>,
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
    /// such as `XOR`.
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
    /// Sorted by kind in glossary order (restriction, timing, other),
    /// then name.
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

    /// The width and period that lead a name, such as `8-bit` or
    /// `2-tick`.
    fn measures(&self) -> impl Iterator<Item = String> {
        let width = self.width.map(|w| format!("{w}-bit"));
        let period = self.period.map(|p| format!("{p}-tick"));
        width.into_iter().chain(period)
    }

    /// The width, design, and category, such as `8-bit Carry Lookahead
    /// Adder`, for views that show the tags beside it. A build with a given
    /// name, such as `8-bit Bus Crossing`, uses it for the category.
    pub fn name(&self) -> String {
        self.measures()
            .chain(self.design_label())
            .chain([self.what()])
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A readable name from the most identifying facts, such as
    /// `8-bit alpha Carry Lookahead Adder`.
    pub fn title(&self) -> String {
        self.measures()
            .chain(self.tags_in("restriction").map(Tag::label))
            .chain(self.tags_in("other").map(Tag::label))
            .chain(self.design_label())
            .chain([self.what()])
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

    /// The download filename: the build's name with dashes for spaces,
    /// such as `8-bit-Carry-Lookahead-Adder.schem`.
    pub fn filename(&self) -> String {
        format!("{}.schem", self.name().replace(' ', "-"))
    }

    /// The number of blocks inside the build's bounding box.
    pub fn volume(&self) -> i64 {
        let (x, y, z) = self.size;
        x * y * z
    }

    /// This build stacked into a box of inner `size`, with `copies` along
    /// each direction. Copies that add bits widen it. A stack's latency and
    /// timing are not its slice's, so they are unknown.
    #[must_use]
    pub fn stacked(
        &self,
        copies: &[(StackDirection, usize)],
        size: [i32; 3],
    ) -> Schematic {
        let factor: i64 = copies
            .iter()
            .filter(|(direction, _)| direction.adds_bits(&self.orientation))
            .map(|(_, n)| i64::try_from(*n).unwrap_or(i64::MAX))
            .product();
        Schematic {
            width: self.width.map(|w| w * factor),
            latency: None,
            synchronized: None,
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

/// Every schematic, ordered by category, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT schematic.id, schematic.category, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", period, size_x, size_y, size_z, built_on, imported_by, sha256, added_at, edited_at, downloads,
            conventional as "conventional: bool", description, given_name, design,
            design.abbreviation as "design_abbreviation?: String",
            design.label as "design_given_label?: String",
            (SELECT label FROM category WHERE category.name =
                schematic.category) as "category_label?: String",
            user.username as "uploader?: String",
            coalesce(user.minecraft_name, user.username)
                as "uploader_name?: String"
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
    Ok(rows
        .into_iter()
        .map(|row| Schematic {
            tags: tags.remove(&row.id).unwrap_or_default(),
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
            period: row.period,
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

pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"SELECT schematic.id, schematic.category, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", period, size_x, size_y, size_z, built_on, imported_by, sha256, added_at, edited_at, downloads,
            conventional as "conventional: bool", description, given_name, design,
            design.abbreviation as "design_abbreviation?: String",
            design.label as "design_given_label?: String",
            (SELECT label FROM category WHERE category.name =
                schematic.category) as "category_label?: String",
            user.username as "uploader?: String",
            coalesce(user.minecraft_name, user.username)
                as "uploader_name?: String"
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
    let tags = sqlx::query_as!(
        Tag,
        "SELECT tag.name, tag.token, tag.kind
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         WHERE schematic_tag.schematic_id = ?
         ORDER BY tag.kind = 'other', tag.kind, tag.name",
        id
    )
    .fetch_all(db)
    .await?;
    let flags = sqlx::query_as!(
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
    .await?;
    Ok(Some(Schematic {
        id: row.id,
        category: row.category,
        category_label: row.category_label,
        width: row.width,
        orientation: row.orientation,
        encoding: row.encoding,
        latency: row.latency,
        synchronized: row.synchronized,
        period: row.period,
        size: (row.size_x, row.size_y, row.size_z),
        stacking: stacking(db, id).await?,
        builders: credits(db, id, Credit::Builder).await?,
        contributors: credits(db, id, Credit::Contributor).await?,
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
        tags,
        flags,
    }))
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
    pub period: Option<i64>,
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
            period, uploaded_by, sha256, data)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        new.period,
        new.uploaded_by,
        new.sha256,
        new.data,
    )
    .fetch_one(db)
    .await
}

/// An entry in the list of categories a build may have.
#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    /// As people read it, such as `RAM` or `BCD to Binary`.
    pub label: String,
    /// The label for many builds, such as `Adders` or `RAM`.
    pub plural: String,
    /// Whether each build gives its own name, as bussing does.
    pub named: bool,
    /// Whether its designs all do the same job, so a leaderboard may rank
    /// them together, as adders' do and gates' do not.
    pub designs_interchangeable: bool,
    /// The sidebar section it sits in, one of [`SECTIONS`]; `None` for
    /// Other, with the categories off the list.
    pub section: Option<String>,
    /// Whether its builds are serial, each with a period: the ticks from
    /// one bit to the next.
    pub periodic: bool,
}

/// The sidebar's sections of standard categories, in order: each stored
/// name, as `rsdb define-category --section` takes it, with its heading.
pub const SECTIONS: [(&str, &str); 6] = [
    ("arithmetic", "Arithmetic"),
    ("logic", "Logic"),
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
pub async fn categories<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Category>, sqlx::Error> {
    sqlx::query_as!(
        Category,
        r#"SELECT name as "name!", label,
            coalesce(plural, label || 's') as "plural!: String",
            named as "named: bool",
            designs_interchangeable as "designs_interchangeable: bool",
            section,
            periodic as "periodic: bool"
         FROM category ORDER BY label COLLATE NOCASE"#
    )
    .fetch_all(db)
    .await
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
            designs_interchangeable: true,
            section: None,
            periodic: false,
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
pub async fn define_category<'e, E: SqliteExecutor<'e>>(
    db: E,
    category: &Category,
) -> Result<(), sqlx::Error> {
    let plural = (category.plural != format!("{}s", category.label))
        .then_some(category.plural.as_str());
    sqlx::query!(
        "INSERT INTO category (name, label, plural, named, section,
            periodic)
         VALUES (?, ?, ?, ?, ?, ?)",
        category.name,
        category.label,
        plural,
        category.named,
        category.section,
        category.periodic
    )
    .execute(db)
    .await?;
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

/// Stores a schematic's size, as `rsdb reread` finds it.
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

/// Every field a details form sets; `None` makes a field unknown.
pub struct Details<'a> {
    pub category: &'a str,
    pub width: Option<i64>,
    pub orientation: &'a str,
    pub encoding: Option<&'a str>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub design: Option<&'a str>,
    pub built_on: Option<&'a str>,
    pub description: Option<&'a str>,
    pub given_name: Option<&'a str>,
    pub period: Option<i64>,
}

/// Sets every field of a details form, clearing the empty ones.
pub async fn set_details<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    details: &Details<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET category = ?, width = ?, orientation = ?,
            encoding = ?, latency = ?, synchronized = ?, design = ?,
            built_on = ?,
            description = ?, given_name = ?, period = ?,
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
        details.period,
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

/// Fields to change; `None` keeps the stored value.
#[derive(Default)]
pub struct Changes<'a> {
    pub category: Option<&'a str>,
    pub width: Option<i64>,
    pub orientation: Option<&'a str>,
    pub encoding: Option<&'a str>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub built_on: Option<&'a str>,
    pub imported_by: Option<&'a str>,
    pub conventional: Option<bool>,
    pub design: Option<&'a str>,
    pub description: Option<&'a str>,
    pub given_name: Option<&'a str>,
    pub period: Option<i64>,
}

pub async fn update<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    changes: &Changes<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET
            category = coalesce(?, category),
            width = coalesce(?, width),
            orientation = coalesce(?, orientation),
            encoding = coalesce(?, encoding),
            latency = coalesce(?, latency),
            synchronized = coalesce(?, synchronized),
            built_on = coalesce(?, built_on),
            imported_by = coalesce(?, imported_by),
            conventional = coalesce(?, conventional),
            design = coalesce(?, design),
            description = coalesce(?, description),
            given_name = coalesce(?, given_name),
            period = coalesce(?, period),
            edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
        changes.category,
        changes.width,
        changes.orientation,
        changes.encoding,
        changes.latency,
        changes.synchronized,
        changes.built_on,
        changes.imported_by,
        changes.conventional,
        changes.design,
        changes.description,
        changes.given_name,
        changes.period,
        id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A field that may be unknown.
#[derive(PartialEq, Eq)]
pub enum Optional {
    Width,
    Latency,
    Synchronized,
    BuiltOn,
    ImportedBy,
    Description,
    Design,
    GivenName,
    Period,
}

/// Makes `fields` unknown.
pub async fn forget<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    fields: &[Optional],
) -> Result<(), sqlx::Error> {
    let has = |field| fields.contains(&field);
    let (width, latency, synchronized, built_on) = (
        has(Optional::Width),
        has(Optional::Latency),
        has(Optional::Synchronized),
        has(Optional::BuiltOn),
    );
    let imported_by = has(Optional::ImportedBy);
    let description = has(Optional::Description);
    let design = has(Optional::Design);
    let given_name = has(Optional::GivenName);
    let period = has(Optional::Period);
    sqlx::query!(
        "UPDATE schematic SET
            width = iif(?, NULL, width),
            latency = iif(?, NULL, latency),
            synchronized = iif(?, NULL, synchronized),
            built_on = iif(?, NULL, built_on),
            imported_by = iif(?, NULL, imported_by),
            description = iif(?, NULL, description),
            design = iif(?, 'other', design),
            given_name = iif(?, NULL, given_name),
            period = iif(?, NULL, period)
         WHERE id = ?",
        width,
        latency,
        synchronized,
        built_on,
        imported_by,
        description,
        design,
        given_name,
        period,
        id,
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
            period: None,
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
        assert_eq!(cla().filename(), "8-bit-Carry-Lookahead-Adder.schem");
    }

    #[test]
    fn only_a_mod_or_a_confirmed_builder_may_edit() {
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
        };
        let cla = cla();
        assert!(user(Some("EmbiZone"), true, Role::User).may_edit(&cla));
        assert!(!user(Some("embizone"), false, Role::User).may_edit(&cla));
        assert!(!user(Some("alex_you"), true, Role::User).may_edit(&cla));
        assert!(!user(None, false, Role::User).may_edit(&cla));
        assert!(user(None, false, Role::Mod).may_edit(&cla));
    }

    #[test]
    fn a_design_keeps_its_label() {
        let s = Schematic {
            category: "gate".to_owned(),
            category_label: Some("Gate".to_owned()),
            design: "nand_xnor".to_owned(),
            design_given_label: Some("NAND+XNOR".to_owned()),
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.name(), "8-bit NAND+XNOR Gate");
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
            period: None,
            builders: Vec::new(),
            built_on: None,
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.title(), "Adder");
        assert_eq!(s.filename(), "Adder.schem");
    }
}
