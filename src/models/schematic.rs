use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

/// A tag's name is written in full words joined by underscores, such as
/// `carry_lookahead`.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    /// The filename abbreviation, when the name is too long for one.
    pub token: Option<String>,
    pub category: String,
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
    pub function: String,
    /// The standard function's label, such as `RAM`; `None` for a function
    /// off the list.
    pub function_label: Option<String>,
    pub width: Option<i64>,
    pub orientation: String,
    pub encoding: String,
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
    /// The design within its function, such as `carry_lookahead`.
    pub design: Option<String>,
    /// The design's usual short name, such as `CLA`.
    pub design_abbreviation: Option<String>,
    /// Whether it has a carry in and a carry out; `None` when unknown or
    /// when its function has no carries.
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
    /// Sorted by category in glossary order (restriction, timing, other),
    /// then name.
    pub tags: Vec<Tag>,
}

impl Schematic {
    pub fn tags_in<'a>(
        &'a self,
        category: &'a str,
    ) -> impl Iterator<Item = &'a Tag> + 'a {
        self.tags.iter().filter(move |tag| tag.category == category)
    }

    /// The width in words, such as `8 bits` or `1 bit`.
    pub fn width_label(&self) -> Option<String> {
        self.width
            .map(|w| format!("{w} {}", if w == 1 { "bit" } else { "bits" }))
    }

    /// The function as people read it, such as `RAM` or `Line Drawer`.
    pub fn function_label(&self) -> String {
        self.function_label
            .clone()
            .unwrap_or_else(|| words(&self.function))
    }

    /// The width, design, and function, such as `8-bit Carry Lookahead
    /// Adder`, for views that show the tags beside it.
    pub fn name(&self) -> String {
        self.width
            .map(|w| format!("{w}-bit"))
            .into_iter()
            .chain(self.design_label())
            .chain([self.function_label()])
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
            .chain([self.function_label()])
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

    pub fn encoding_label(&self) -> &str {
        match self.encoding.as_str() {
            "bin" => "binary",
            "hex" => "hex",
            other => other,
        }
    }
}

/// Every schematic, ordered by function, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, function, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", size_x, size_y, size_z, built_on, imported_by, sha256, added_at,
            conventional as "conventional: bool", description, design,
            carry_in as "carry_in: bool", carry_out as "carry_out: bool",
            (SELECT abbreviation FROM design WHERE design.function =
                schematic.function AND design.name = schematic.design)
                as "design_abbreviation?: String",
            (SELECT label FROM function WHERE function.name =
                schematic.function) as "function_label?: String",
            (SELECT username FROM user WHERE user.id = schematic.uploaded_by)
                as "uploader?: String"
         FROM schematic ORDER BY function, width, added_at"#
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
        "SELECT schematic_tag.schematic_id, tag.name, tag.token, tag.category
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         ORDER BY tag.category = 'other', tag.category, tag.name"
    )
    .fetch_all(db)
    .await?
    {
        tags.entry(row.schematic_id).or_default().push(Tag {
            name: row.name,
            token: row.token,
            category: row.category,
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
            function: row.function,
            function_label: row.function_label,
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
        r#"SELECT id, function, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", size_x, size_y, size_z, built_on, imported_by, sha256, added_at,
            conventional as "conventional: bool", description, design,
            carry_in as "carry_in: bool", carry_out as "carry_out: bool",
            (SELECT abbreviation FROM design WHERE design.function =
                schematic.function AND design.name = schematic.design)
                as "design_abbreviation?: String",
            (SELECT label FROM function WHERE function.name =
                schematic.function) as "function_label?: String",
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
        "SELECT tag.name, tag.token, tag.category
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         WHERE schematic_tag.schematic_id = ?
         ORDER BY tag.category = 'other', tag.category, tag.name",
        id
    )
    .fetch_all(db)
    .await?;
    Ok(Some(Schematic {
        id: row.id,
        function: row.function,
        function_label: row.function_label,
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
    pub function: &'a str,
    pub width: Option<i64>,
    pub orientation: &'a str,
    pub encoding: &'a str,
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
        r#"INSERT INTO schematic (function, width, orientation, encoding,
            latency, synchronized, size_x, size_y, size_z, built_on,
            imported_by, conventional, design, carry_in, carry_out,
            description, uploaded_by, sha256, data)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id as "id!""#,
        new.function,
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
        new.uploaded_by,
        new.sha256,
        new.data,
    )
    .fetch_one(db)
    .await
}

/// An entry in the list of functions a build may have.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    /// As people read it, such as `RAM` or `BCD to Binary`.
    pub label: String,
    /// Whether its builds have carry lines worth recording.
    pub carries: bool,
}

/// A design within a function, such as the carry lookahead adder.
#[derive(Debug, Clone)]
pub struct Design {
    pub function: String,
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

/// Every standard function, in the list's order.
pub async fn functions<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Function>, sqlx::Error> {
    sqlx::query_as!(
        Function,
        r#"SELECT name as "name!", label, carries as "carries: bool"
         FROM function ORDER BY position, name"#
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

/// Every design, grouped by function.
pub async fn designs<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Design>, sqlx::Error> {
    sqlx::query_as!(
        Design,
        "SELECT function, name, abbreviation FROM design
         ORDER BY function, name = 'other', name"
    )
    .fetch_all(db)
    .await
}

/// Checks a build's function and design, and returns the function. A
/// function is a standard one from the list, or any other name made of
/// lowercase words joined by underscores. A design is `other`, or one the
/// function lists. The `Err` is a sentence for the person.
pub async fn check_function(
    db: &SqlitePool,
    function: &str,
    design: &str,
) -> Result<Result<Function, String>, sqlx::Error> {
    let valid = |name: &str| {
        name.starts_with(|c: char| c.is_ascii_lowercase())
            && name.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
            })
    };
    if !valid(function) {
        return Ok(Err(
            "A function's name is lowercase words joined by underscores, \
             such as line_drawer."
                .to_owned(),
        ));
    }
    let found = functions(db)
        .await?
        .into_iter()
        .find(|f| f.name == function)
        .unwrap_or_else(|| Function {
            name: function.to_owned(),
            label: words(function),
            carries: false,
        });
    let listed = designs(db)
        .await?
        .iter()
        .any(|d| d.function == function && d.name == design);
    if design != "other" && !listed {
        return Ok(Err(format!(
            "{} is not a design of {}.",
            design.replace('_', " "),
            function.replace('_', " ")
        )));
    }
    Ok(Ok(found))
}

/// Adds a standard function to the end of the list.
pub async fn define_function<'e, E: SqliteExecutor<'e>>(
    db: E,
    function: &Function,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO function (name, label, carries, position)
         VALUES (?, ?, ?, (SELECT coalesce(max(position), 0) + 1 FROM function))",
        function.name,
        function.label,
        function.carries
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
        "INSERT INTO design (function, name, abbreviation) VALUES (?, ?, ?)",
        design.function,
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
    pub function: &'a str,
    pub width: Option<i64>,
    pub orientation: &'a str,
    pub encoding: &'a str,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub design: Option<&'a str>,
    pub carry_in: Option<bool>,
    pub carry_out: Option<bool>,
    pub built_on: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Sets every field of a details form, clearing the empty ones.
pub async fn set_details<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    details: &Details<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET function = ?, width = ?, orientation = ?,
            encoding = ?, latency = ?, synchronized = ?, design = ?,
            carry_in = ?, carry_out = ?, built_on = ?,
            description = ?
         WHERE id = ?",
        details.function,
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
    pub function: Option<&'a str>,
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
}

pub async fn update<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    changes: &Changes<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET
            function = coalesce(?, function),
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
            description = coalesce(?, description)
         WHERE id = ?",
        changes.function,
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
            carry_out = iif(?, NULL, carry_out)
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
        "SELECT name, token, category FROM tag
         ORDER BY category = 'other', category, name"
    )
    .fetch_all(db)
    .await
}

pub async fn define_tag<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
    token: Option<&str>,
    category: &str,
) -> Result<(), sqlx::Error> {
    let name = tag_name(name);
    sqlx::query!(
        "INSERT INTO tag (name, token, category) VALUES (?, ?, ?)",
        name,
        token,
        category
    )
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, token: Option<&str>, category: &str) -> Tag {
        Tag {
            name: name.to_owned(),
            token: token.map(str::to_owned),
            category: category.to_owned(),
        }
    }

    fn cla() -> Schematic {
        Schematic {
            id: String::new(),
            function: "adder".to_owned(),
            function_label: Some("Adder".to_owned()),
            width: Some(8),
            orientation: "horz".to_owned(),
            encoding: "bin".to_owned(),
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
            uploader: None,
            sha256: String::new(),
            added_at: String::new(),
            tags: vec![tag("alpha", None, "restriction")],
        }
    }

    #[test]
    fn title_reads_width_restrictions_design_then_function() {
        assert_eq!(cla().title(), "8-bit alpha Carry Lookahead Adder");
    }

    #[test]
    fn filename_is_the_name() {
        assert_eq!(cla().filename(), "8-bit-Carry-Lookahead-Adder.schem");
    }

    #[test]
    fn a_function_keeps_its_label() {
        let s = Schematic {
            function: "ram".to_owned(),
            function_label: Some("RAM".to_owned()),
            design: Some("other".to_owned()),
            ..cla()
        };
        assert_eq!(s.name(), "8-bit RAM");
    }

    #[test]
    fn a_function_off_the_list_is_capitalized() {
        let s = Schematic {
            function: "maze_screen".to_owned(),
            function_label: None,
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
