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
    /// The design within its category, such as `carry_lookahead`.
    pub design: Option<String>,
    /// The design's usual short name, such as `CLA`.
    pub design_abbreviation: Option<String>,
    /// Whether it has a carry in and a carry out; `None` when unknown or
    /// when its category has no carries.
    pub carry_in: Option<bool>,
    pub carry_out: Option<bool>,
    /// Free text about the build, such as how to use it.
    pub description: Option<String>,
    /// The username of the account that uploaded it through the site.
    pub uploader: Option<String>,
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

    /// The width, design, and category, such as `8-bit Carry Lookahead
    /// Adder`, for views that show the tags beside it. A build with a given
    /// name, such as `8-bit Bus Crossing`, uses it for the category.
    pub fn name(&self) -> String {
        self.width
            .map(|w| format!("{w}-bit"))
            .into_iter()
            .chain(self.design_label())
            .chain([self.what()])
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A readable name from the most identifying facts, such as
    /// `8-bit alpha Carry Lookahead Adder`.
    pub fn title(&self) -> String {
        self.width
            .map(|w| format!("{w}-bit"))
            .into_iter()
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
        self.design
            .as_deref()
            .filter(|design| *design != "other")
            .map(words)
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

    /// Who brought the build in: the uploading account, else whoever ran
    /// the import.
    pub fn importer(&self) -> Option<&str> {
        self.uploader.as_deref().or(self.imported_by.as_deref())
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

/// Every schematic, ordered by category, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, category, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", size_x, size_y, size_z, built_on, imported_by, sha256, added_at,
            conventional as "conventional: bool", description, given_name, design,
            carry_in as "carry_in: bool", carry_out as "carry_out: bool",
            (SELECT abbreviation FROM design WHERE design.category =
                schematic.category AND design.name = schematic.design)
                as "design_abbreviation?: String",
            (SELECT label FROM category WHERE category.name =
                schematic.category) as "category_label?: String",
            (SELECT username FROM user WHERE user.id = schematic.uploaded_by)
                as "uploader?: String"
         FROM schematic ORDER BY category, width, added_at"#
    )
    .fetch_all(db)
    .await?;
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
            size: (row.size_x, row.size_y, row.size_z),
            built_on: row.built_on,
            imported_by: row.imported_by,
            conventional: row.conventional,
            design: row.design,
            design_abbreviation: row.design_abbreviation,
            carry_in: row.carry_in,
            carry_out: row.carry_out,
            description: row.description,
            given_name: row.given_name,
            uploader: row.uploader,
            sha256: row.sha256,
            added_at: row.added_at,
        })
        .collect())
}

pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"SELECT id, category, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", size_x, size_y, size_z, built_on, imported_by, sha256, added_at,
            conventional as "conventional: bool", description, given_name, design,
            carry_in as "carry_in: bool", carry_out as "carry_out: bool",
            (SELECT abbreviation FROM design WHERE design.category =
                schematic.category AND design.name = schematic.design)
                as "design_abbreviation?: String",
            (SELECT label FROM category WHERE category.name =
                schematic.category) as "category_label?: String",
            (SELECT username FROM user WHERE user.id = schematic.uploaded_by)
                as "uploader?: String"
         FROM schematic WHERE id = ?"#,
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
    Ok(Some(Schematic {
        id: row.id,
        category: row.category,
        category_label: row.category_label,
        width: row.width,
        orientation: row.orientation,
        encoding: row.encoding,
        latency: row.latency,
        synchronized: row.synchronized,
        size: (row.size_x, row.size_y, row.size_z),
        stacking: stacking(db, id).await?,
        builders: credits(db, id, Credit::Builder).await?,
        contributors: credits(db, id, Credit::Contributor).await?,
        built_on: row.built_on,
        imported_by: row.imported_by,
        conventional: row.conventional,
        design: row.design,
        design_abbreviation: row.design_abbreviation,
        carry_in: row.carry_in,
        carry_out: row.carry_out,
        description: row.description,
        given_name: row.given_name,
        uploader: row.uploader,
        sha256: row.sha256,
        added_at: row.added_at,
        tags,
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
        sqlx::query!(
            "INSERT OR IGNORE INTO credit (schematic_id, role, name, position)
             VALUES (?, ?, ?, ?)",
            id,
            role,
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
    pub carry_in: Option<bool>,
    pub carry_out: Option<bool>,
    pub description: Option<&'a str>,
    pub given_name: Option<&'a str>,
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
            imported_by, conventional, design, carry_in, carry_out,
            description, given_name, uploaded_by, sha256, data)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        new.carry_in,
        new.carry_out,
        new.description,
        new.given_name,
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
    /// Whether its builds have carry lines worth recording.
    pub carries: bool,
    /// Whether each build gives its own name, as bussing does.
    pub named: bool,
}

/// A design within a category, such as the carry lookahead adder.
#[derive(Debug, Clone)]
pub struct Design {
    pub category: String,
    pub name: String,
    /// The usual short name, such as `CLA`.
    pub abbreviation: Option<String>,
}

impl Design {
    /// Such as `Carry Lookahead (CLA)`.
    pub fn label(&self) -> String {
        let words = words(&self.name);
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
            iif(plural = '', label || 's', plural) as "plural!: String",
            carries as "carries: bool", named as "named: bool"
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
        "SELECT category, name, abbreviation FROM design
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
            carries: false,
            named: false,
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

/// Adds a standard category to the end of the list. A plural that is the
/// label with an `s` is left for the database to make.
pub async fn define_category<'e, E: SqliteExecutor<'e>>(
    db: E,
    category: &Category,
) -> Result<(), sqlx::Error> {
    let plural = if category.plural == format!("{}s", category.label) {
        ""
    } else {
        &category.plural
    };
    sqlx::query!(
        "INSERT INTO category (name, label, plural, carries, named, position)
         VALUES (?, ?, ?, ?, ?,
            (SELECT coalesce(max(position), 0) + 1 FROM category))",
        category.name,
        category.label,
        plural,
        category.carries,
        category.named
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
        "INSERT INTO design (category, name, abbreviation) VALUES (?, ?, ?)",
        design.category,
        design.name,
        design.abbreviation
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Stores a schematic's carry lines, as read from its file.
pub async fn set_carries<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    carry_in: Option<bool>,
    carry_out: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET carry_in = ?, carry_out = ? WHERE id = ?",
        carry_in,
        carry_out,
        id
    )
    .execute(db)
    .await?;
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
    pub carry_in: Option<bool>,
    pub carry_out: Option<bool>,
    pub built_on: Option<&'a str>,
    pub description: Option<&'a str>,
    pub given_name: Option<&'a str>,
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
            carry_in = ?, carry_out = ?, built_on = ?,
            description = ?, given_name = ?
         WHERE id = ?",
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        details.design,
        details.carry_in,
        details.carry_out,
        details.built_on,
        details.description,
        details.given_name,
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
    pub carry_in: Option<bool>,
    pub carry_out: Option<bool>,
    pub description: Option<&'a str>,
    pub given_name: Option<&'a str>,
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
            carry_in = coalesce(?, carry_in),
            carry_out = coalesce(?, carry_out),
            description = coalesce(?, description),
            given_name = coalesce(?, given_name)
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
        changes.carry_in,
        changes.carry_out,
        changes.description,
        changes.given_name,
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
    Carries,
    GivenName,
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
    let carries = has(Optional::Carries);
    let given_name = has(Optional::GivenName);
    sqlx::query!(
        "UPDATE schematic SET
            width = iif(?, NULL, width),
            latency = iif(?, NULL, latency),
            synchronized = iif(?, NULL, synchronized),
            built_on = iif(?, NULL, built_on),
            imported_by = iif(?, NULL, imported_by),
            description = iif(?, NULL, description),
            design = iif(?, NULL, design),
            carry_in = iif(?, NULL, carry_in),
            carry_out = iif(?, NULL, carry_out),
            given_name = iif(?, NULL, given_name)
         WHERE id = ?",
        width,
        latency,
        synchronized,
        built_on,
        imported_by,
        description,
        design,
        carries,
        carries,
        given_name,
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
            size: (19, 11, 18),
            stacking: Vec::new(),
            builders: vec!["embizone".to_owned()],
            contributors: Vec::new(),
            built_on: Some("2014-09-06".to_owned()),
            imported_by: None,
            conventional: true,
            design: Some("carry_lookahead".to_owned()),
            design_abbreviation: None,
            carry_in: None,
            carry_out: None,
            description: None,
            given_name: None,
            uploader: None,
            sha256: String::new(),
            added_at: String::new(),
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
    fn a_category_keeps_its_label() {
        let s = Schematic {
            category: "ram".to_owned(),
            category_label: Some("RAM".to_owned()),
            design: Some("other".to_owned()),
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
            design: Some("other".to_owned()),
            ..cla()
        };
        assert_eq!(s.name(), "Maze Screen");
    }

    #[test]
    fn unknown_parts_are_left_out() {
        let s = Schematic {
            width: None,
            design: None,
            latency: None,
            synchronized: None,
            builders: Vec::new(),
            built_on: None,
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.title(), "Adder");
        assert_eq!(s.filename(), "Adder.schem");
    }
}
